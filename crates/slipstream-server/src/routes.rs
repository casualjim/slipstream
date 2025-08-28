use crate::add_messages_service;
use crate::app::AppState;
use crate::error::AppError;
use crate::models::AddMessages;
use aide::{
  axum::ApiRouter,
  openapi::{Info, OpenApi},
};
use axum::{
  Json,
  extract::State,
  response::IntoResponse,
  routing::{get, post},
};
use slipstream_restate::axum::{RestateService, create_restate_endpoint};

pub fn openapi() -> OpenApi {
  OpenApi {
    info: Info {
      title: "Slipstream API".to_string(),
      description: Some("High-performance agentic memory storage".to_string()),
      version: "1.0.0".to_string(),
      ..Default::default()
    },
    ..Default::default()
  }
}

pub fn router(app: AppState) -> ApiRouter<AppState> {
  let restate_service = RestateService::new(create_restate_endpoint());

  ApiRouter::new()
    .route("/api/v1/messages", post(handle_add_messages))
    .nest_service("/restate", restate_service) // Restate service invocation
    .with_state(app)
}

async fn handle_add_messages(
  app: State<AppState>,
  Json(payload): Json<AddMessages>,
) -> Result<impl IntoResponse, AppError> {
  tracing::info!(
    "Received add messages request for group_id: {}",
    payload.group_id
  );
  tracing::info!("Number of messages: {}", payload.messages.len());

  // Call the service function
  add_messages_service(&app, payload).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::{body::Body, http::{Request, StatusCode}};
  use tower::ServiceExt;

  #[tokio::test]
  async fn api_router_does_not_expose_healthz() {
    let app_state = AppState::new().await.unwrap();

    let mut api = crate::routes::openapi();
    let router = crate::routes::router(app_state).finish_api(&mut api);

    let res = router
      .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
  }
}
