mod config;
mod openailike;

use async_trait::async_trait;

#[async_trait]
pub trait Completer {
  async fn complete(&self, prompt: &str) -> Result<String, CompleterError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CompleterError {
  #[error("Rate limit exceeded. Please try again later.")]
  RateLimitExceeded,
  #[error("LLM refusal: {0}")]
  LLMRefusal(String),
  #[error("LLM empty response: {0}")]
  LLMEmptyResponse(String),
}
