use std::time::Duration;

use ::serde::{Deserialize, Serialize};
use restate_sdk::prelude::*;
use schemars::JsonSchema;

use crate::handlers::{send_notification, send_reminder};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GreeterArgs {
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GreeterResponse {
  pub message: String,
}

#[restate_sdk::service]
pub trait Greeter {
  async fn greet(name: Json<GreeterArgs>) -> Result<Json<GreeterResponse>, HandlerError>;
}

pub struct GreeterImpl;

impl Greeter for GreeterImpl {
  async fn greet(
    &self,
    mut ctx: Context<'_>,
    Json(args): Json<GreeterArgs>,
  ) -> Result<Json<GreeterResponse>, HandlerError> {
    let greeting_id = ctx.rand_uuid().to_string();

    ctx
      .run(|| send_notification(&greeting_id, &args.name))
      .name("notification")
      .await?;

    ctx.sleep(Duration::from_secs(1)).await?;

    ctx
      .run(|| send_reminder(&greeting_id, &args.name))
      .name("reminder")
      .await?;

    Ok(Json(GreeterResponse {
      message: format!("You said hi to {}", args.name),
    }))
  }
}
