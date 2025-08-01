mod context;
mod local;

// // Include test modules so cargo test discovers them (they live next to this mod).
// // These modules contain integration-style tests using the real OpenAI provider.
// #[cfg(test)]
// mod local_integration_tests;
// #[cfg(test)]
// mod local_tool_tests;

use async_trait::async_trait;
use secrecy::SecretString;
use slipstream_metadata::AgentRef;
use typed_builder::TypedBuilder;
use uuid::Uuid;
use validator::{Validate, ValidationError};

pub use context::ExecutionContext;
pub use local::Local;

use crate::{ResultStream, StreamEvent, completer::StructuredOutput};

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
  #[builder(setter(into))]
  pub agent: AgentRef,
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
}

#[derive(Debug, Clone, Default)]
pub enum ExecutorConfig {
  #[default]
  Local,
  #[allow(dead_code)]
  // Placeholder for future executor types, to force the use of `ExecutorConfig::Local` by default
  Restate { url: String, api_key: SecretString },
}

#[async_trait]
pub trait Executor: Send + Sync {
  /// Executes the given agent with the provided parameters and returns a stream of events.
  async fn execute(
    &self,
    context: ExecutionContext,
    params: AgentRequest,
  ) -> ResultStream<StreamEvent>;
}
