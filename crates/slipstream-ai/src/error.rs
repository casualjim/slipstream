pub type Result<T, E = Error> = eyre::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Rate limit exceeded")]
  RateLimitExceeded,
  #[error("Refusal: {0}")]
  Refusal(String),
  #[error("Empty response: {0}")]
  EmptyResponse(String),
}
