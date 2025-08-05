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
  ApiRouter::new()
    .route("/api/v1/messages", post(handle_add_messages))
    .route("/healthz", get(health_check)) // Add health check endpoin
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

async fn health_check() -> &'static str {
  "OK"
}
