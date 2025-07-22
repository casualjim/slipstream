use aide::{
  axum::ApiRouter,
  openapi::{Info, OpenApi},
};

use crate::app::AppState;

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
  ApiRouter::new().with_state(app)
}

async fn health_check() -> &'static str {
  "OK"
}
