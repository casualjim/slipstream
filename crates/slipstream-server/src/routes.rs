use crate::add_messages_service;
use crate::app::AppState;
use crate::error::AppError;
use crate::models::AddMessages;
use crate::services::uploads::upload_files;
use aide::{
  axum::ApiRouter,
  openapi::{Info, OpenApi},
};
use axum::{
  Json,
  extract::{Multipart, Path, State},
  http::StatusCode,
  response::IntoResponse,
  routing::post,
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
    .route(
      "/api/v1/workspaces/{workspace_id}/uploads",
      post(handle_upload).layer(axum::extract::DefaultBodyLimit::max(55 * 1024 * 1024)),
    )
    .nest_service("/restate", restate_service) // Restate service invocation
    .with_state(app)
}

// Thin handler – delegates to service layer
async fn handle_upload(
  State(app): State<AppState>,
  Path(workspace_id): Path<String>,
  multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
  let results = upload_files(&app, &workspace_id, multipart).await?;
  Ok((StatusCode::CREATED, Json(results)))
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
