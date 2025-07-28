use std::num::ParseIntError;
use validator::ValidationErrors;

pub type Result<T, E = Error> = eyre::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Rate limit exceeded")]
  RateLimitExceeded,
  #[error("Refusal: {0}")]
  Refusal(String),
  #[error("openai: {0}")]
  OpenAI(#[from] async_openai::error::OpenAIError),
  #[error("{0}")]
  Validation(#[from] ValidationErrors),
  #[error("serde: {0}")]
  Serde(#[from] serde_json::Error),
  #[error("No choices returned from AI model")]
  NoChoices,
  #[error("Function name '{0}' is not a valid identifier")]
  FunctionNotFound(String),
  #[error("Function name '{0}' does not have a handler defined")]
  FunctionHandlerNotFound(String),
  #[error("io: {0}")]
  Io(#[from] std::io::Error),
  #[error("Agent tool error: {0}")]
  AgentTool(String),
  #[error("{0}")]
  ParseInt(#[from] ParseIntError),
  #[error("Unknown provider: {0}")]
  UnknownProvider(String),
  #[error("{0}")]
  Core(#[from] slipstream_core::Error),
}

#[cfg(test)]
mod tests {
  use ctor::ctor;

  #[ctor]
  fn init_test_env() {
    color_backtrace::install();
  }
}
