mod local;
use std::sync::Arc;

use async_trait::async_trait;
use slipstream_core::messages::Aggregator;
use typed_builder::TypedBuilder;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::{Result, agent::Agent, completer::StructuredOutput, events::StreamEvent};

fn validate_uuid_not_nil(uuid: &Uuid) -> Result<(), ValidationError> {
  if uuid.is_nil() {
    Err(ValidationError::new("Run ID cannot be nil"))
  } else {
    Ok(())
  }
}

#[derive(Debug, TypedBuilder, Validate)]
pub struct AgentRequest {
  #[builder(default = Uuid::now_v7(), setter(into))]
  #[validate(custom(function = "validate_uuid_not_nil", message = "Run ID cannot be nil"))]
  pub run_id: Uuid,
  pub agent: Arc<dyn Agent>,
  pub session: Aggregator,
  #[builder(default, setter(into))]
  pub tool_choice: Option<String>,
  #[builder(default, setter(into))]
  pub structured_output: Option<StructuredOutput>,
  #[builder(default, setter(into))]
  pub stream: bool,
  #[builder(default, setter(into))]
  pub max_turns: Option<usize>,
  #[builder(default, setter(into))]
  pub context_variables: serde_json::Value,
  #[builder(default, setter(into))]
  pub reasoning_effort: crate::ReasoningEffort,
  pub sender: tokio::sync::broadcast::Sender<StreamEvent>,
}

pub type AgentResponse = Result<String>;

#[async_trait]
pub trait Executor: Send + Sync {
  /// Executes the given agent with the provided parameters.
  async fn execute(&self, params: AgentRequest) -> AgentResponse;
}
