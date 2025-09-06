use axum::response::IntoResponse;
use axum::{
  Router,
  extract::State,
  http::{HeaderMap, StatusCode},
  routing::post,
};
use bytes::Bytes;
use color_eyre::eyre::{Result, bail, eyre};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};

#[derive(Clone)]
struct AppState {
  nats_subject: String,
  nats_client: async_nats::Client,
  bucket_filter: Option<String>,
  webhook_secret: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2Event<'a> {
  #[serde(borrow)]
  event_type: Option<&'a str>,
  #[serde(borrow)]
  bucket_name: Option<&'a str>,
  #[serde(borrow)]
  bucket_id: Option<&'a str>,
  #[serde(borrow)]
  object_name: Option<&'a str>,
  #[serde(borrow)]
  content_type: Option<&'a str>,
  content_length: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;
  dotenvy::dotenv().ok();

  init_tracing();

  let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
  let nats_subject = std::env::var("NATS_SINK_TOPIC")
    .or_else(|_| std::env::var("NATS_SINK_SUBJECT"))
    .unwrap_or_else(|_| "slipstream.events.backblaze".to_string());
  let bucket_filter = std::env::var("AWS_S3_BUCKET").ok();
  let webhook_secret = std::env::var("B2_WEBHOOK_SECRET")
    .or_else(|_| std::env::var("BACKBLAZE_WEBHOOK_SECRET"))
    .ok();
  let addr: SocketAddr = std::env::var("PORT")
    .unwrap_or_else(|_| "8080".to_string())
    .parse()
    .map(|port: u16| ([0, 0, 0, 0], port).into())
    .unwrap_or_else(|_| ([0, 0, 0, 0], 8080).into());

  let nats_client = async_nats::connect(nats_url.clone())
    .await
    .map_err(|e| eyre!("failed to connect to NATS at {nats_url}: {e}"))?;
  info!(subject = %nats_subject, "Connected to NATS");

  let state = Arc::new(AppState {
    nats_subject,
    nats_client,
    bucket_filter,
    webhook_secret,
  });

  let app = Router::new()
    .route("/healthz", post(healthz).get(healthz))
    .route("/webhook", post(handle_webhook));

  info!(%addr, "Listening for Backblaze B2 webhooks");

  let listener = tokio::net::TcpListener::bind(addr).await?;
  axum::serve(listener, app.with_state(state))
    .with_graceful_shutdown(shutdown_signal())
    .await?;
  Ok(())
}

async fn shutdown_signal() {
  let ctrl_c = async {
    signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    signal::unix::signal(signal::unix::SignalKind::terminate())
      .expect("failed to install signal handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
    _ = ctrl_c => {
      info!("Received Ctrl+C, shutting down gracefully");
    },
    _ = terminate => {
      info!("Received terminate signal, shutting down gracefully");
    },
  }
}

fn init_tracing() {
  use tracing_subscriber::{EnvFilter, fmt};
  let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new("info,hyper=info,axum::rejection=trace"));
  fmt().with_env_filter(filter).compact().init();
}

async fn healthz() -> impl IntoResponse {
  (StatusCode::OK, "ok")
}

async fn handle_webhook(
  State(state): State<Arc<AppState>>,
  headers: HeaderMap,
  body: Bytes,
) -> impl IntoResponse {
  // Verify signature if configured
  if let Some(secret) = &state.webhook_secret {
    if let Err(e) = verify_signature(&headers, &body, secret) {
      warn!(error=%e, "signature verification failed");
      return (StatusCode::UNAUTHORIZED, "invalid signature");
    }
  }

  // Optionally filter by bucket name if set
  if let Some(expected) = &state.bucket_filter {
    match serde_json::from_slice::<serde_json::Value>(&body) {
      Ok(v) => {
        let matches = v
          .get("bucketName")
          .and_then(|b| b.as_str())
          .map(|bn| bn == expected)
          .or_else(|| {
            v.get("bucket_name")
              .and_then(|b| b.as_str())
              .map(|bn| bn == expected)
          })
          .unwrap_or(true); // default to true if field missing
        if !matches {
          warn!(expected=%expected, "ignoring event for different bucket");
          return (StatusCode::ACCEPTED, "ignored: bucket filter");
        }
      }
      Err(e) => {
        warn!(error=%e, "failed to parse payload for bucket filter; accepting anyway");
      }
    }
  }

  // Publish raw event to NATS subject
  let subject = state.nats_subject.clone();
  let payload = body.clone();
  if let Err(e) = state.nats_client.publish(subject.clone(), payload).await {
    error!(%subject, error=%e, "failed to publish to NATS");
    return (StatusCode::INTERNAL_SERVER_ERROR, "publish failed");
  }

  (StatusCode::OK, "ok")
}

fn verify_signature(headers: &HeaderMap, body: &Bytes, secret: &str) -> Result<()> {
  // Backblaze documents the signature header as `X-Bz-Event-Notification-Signature`.
  // Some examples prefix the digest with `v1=`; accept either form.
  let header_name =
    axum::http::header::HeaderName::from_static("x-bz-event-notification-signature");
  let given = headers
    .get(&header_name)
    .ok_or_else(|| eyre!("missing signature header"))?;
  let given = given
    .to_str()
    .map_err(|_| eyre!("invalid signature header"))?;

  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
  mac.update(body);
  let expected = hex::encode(mac.finalize().into_bytes());

  // Accept raw hex or prefixed form like "v1=hex"
  let ok = given.trim() == expected
    || given
      .trim()
      .strip_prefix("v1=")
      .map(|s| s == expected)
      .unwrap_or(false);
  if ok {
    Ok(())
  } else {
    bail!("signature mismatch")
  }
}
