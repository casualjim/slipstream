use axum::{body::Body, extract::Request, response::Response};
use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{FutureExt, TryStreamExt};
use http::header::CONTENT_TYPE;
use http::{HeaderName, HeaderValue, response};
use restate_sdk::endpoint::{self, Endpoint, InputReceiver, OutputSender};
use restate_sdk_shared_core::Header;
use std::convert::Infallible;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::sync::mpsc;
use tower::Service as TowerService;
use tracing::{debug, warn};

use crate::greeter::{Greeter as _, GreeterImpl};
use crate::ratelimit::{RateLimiter as _, RateLimiterImpl};

#[allow(clippy::declare_interior_mutable_const)]
const X_RESTATE_SERVER: HeaderName = HeaderName::from_static("x-restate-server");
const X_RESTATE_SERVER_VALUE: HeaderValue = HeaderValue::from_static("restate-sdk-rust/0.6.0");

/// Creates a Restate endpoint with all bound services
pub fn create_restate_endpoint() -> Endpoint {
  Endpoint::builder()
    .bind(GreeterImpl.serve())
    .bind(RateLimiterImpl.serve())
    .build()
}

/// Tower service that wraps Restate Endpoint directly
#[derive(Clone)]
pub struct RestateService {
  endpoint: Endpoint,
}

impl RestateService {
  pub fn new(endpoint: Endpoint) -> Self {
    Self { endpoint }
  }
}

impl Default for RestateService {
  fn default() -> Self {
    Self::new(create_restate_endpoint())
  }
}

impl TowerService<Request> for RestateService {
  type Response = Response;
  type Error = Infallible;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    // Always ready - endpoint handles its own readiness
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request) -> Self::Future {
    let endpoint = self.endpoint.clone();

    Box::pin(async move {
      let (parts, body) = req.into_parts();

      // First try to resolve the endpoint
      let endpoint_response = match endpoint.resolve(parts.uri.path(), parts.headers) {
        Ok(res) => res,
        Err(err) => {
          debug!("Error when trying to handle incoming request: {err}");
          return Ok(
            Response::builder()
              .status(err.status_code())
              .header(CONTENT_TYPE, "text/plain")
              .header(X_RESTATE_SERVER, X_RESTATE_SERVER_VALUE)
              .body(Body::from(err.to_string()))
              .expect("Headers should be valid"),
          );
        }
      };

      match endpoint_response {
        endpoint::Response::ReplyNow {
          status_code,
          headers,
          body: response_body,
        } => {
          let response_builder = response_builder_from_response_head(status_code, headers);
          Ok(
            response_builder
              .body(Body::from(response_body))
              .expect("Headers should be valid"),
          )
        }
        endpoint::Response::BidiStream {
          status_code,
          headers,
          handler,
        } => {
          // Convert axum body to input receiver
          let input_receiver =
            InputReceiver::from_stream(body.into_data_stream().map_err(Into::into));

          let (output_tx, output_rx) = mpsc::unbounded_channel();
          let output_sender = OutputSender::from_channel(output_tx);

          let handler_fut = Box::pin(handler.handle(input_receiver, output_sender));

          let response_builder = response_builder_from_response_head(status_code, headers);

          // Create a streaming body from the output receiver
          let streaming_body = RestateStreamingBody {
            fut: Some(handler_fut),
            output_rx,
            end_stream: false,
          };

          Ok(
            response_builder
              .body(Body::new(streaming_body))
              .expect("Headers should be valid"),
          )
        }
      }
    })
  }
}

fn response_builder_from_response_head(
  status_code: u16,
  headers: Vec<Header>,
) -> response::Builder {
  let mut response_builder = Response::builder()
    .status(status_code)
    .header(X_RESTATE_SERVER, X_RESTATE_SERVER_VALUE);

  for header in headers {
    response_builder = response_builder.header(header.key.deref(), header.value.deref());
  }

  response_builder
}

/// Streaming body implementation for bidirectional streams
pub struct RestateStreamingBody {
  fut: Option<BoxFuture<'static, Result<(), endpoint::Error>>>,
  output_rx: mpsc::UnboundedReceiver<Bytes>,
  end_stream: bool,
}

impl http_body::Body for RestateStreamingBody {
  type Data = Bytes;
  type Error = Infallible;

  fn poll_frame(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
    // First try to consume the runner future
    if let Some(mut fut) = self.fut.take() {
      match fut.poll_unpin(cx) {
        Poll::Ready(res) => {
          if let Err(e) = res {
            warn!("Handler failure: {e:?}")
          }
          self.output_rx.close();
        }
        Poll::Pending => {
          self.fut = Some(fut);
        }
      }
    }

    if let Some(out) = ready!(self.output_rx.poll_recv(cx)) {
      Poll::Ready(Some(Ok(http_body::Frame::data(out))))
    } else {
      self.end_stream = true;
      Poll::Ready(None)
    }
  }

  fn is_end_stream(&self) -> bool {
    self.end_stream
  }
}

pub fn add(left: u64, right: u64) -> u64 {
  left + right
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::Body;
  use axum::http::{Method, Request, StatusCode};
  use http_body_util::BodyExt;
  use tower::ServiceExt;

  #[test]
  fn it_works() {
    let result = 2 + 2;
    assert_eq!(result, 4);
  }

  #[tokio::test]
  async fn test_restate_service_discovery() {
    // Create a GET request to the discovery endpoint
    let request = Request::builder()
      .method(Method::GET)
      .uri("/discover")
      .body(Body::empty())
      .unwrap();

    // Create the service and call it
    let response = RestateService::default().oneshot(request).await.unwrap();

    // Should get a 200 response
    if response.status() != StatusCode::OK {
      let status = response.status();
      let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
      let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
      panic!("Expected 200 but got {}: {}", status, body_str);
    }

    // Discovery should return service metadata
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    // Should contain service definitions
    assert!(!body_str.is_empty());
  }

  #[tokio::test]
  async fn test_restate_service_invalid_path() {
    // Create a request to a non-existent service
    let request = Request::builder()
      .method(Method::POST)
      .uri("/invoke/NonExistent/method")
      .header("content-type", "application/json")
      .body(Body::from(r#"{}"#))
      .unwrap();

    // Create the service and call it
    let response = RestateService::default().oneshot(request).await.unwrap();

    // Should get a 4xx or 5xx status
    assert!(response.status().is_client_error() || response.status().is_server_error());
  }
}
