pub mod messages;

/// The core result type for slipstream-core.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The main error type for slipstream-core, categorizing all major error cases.
/// All variants are structured and meaningful; there is no catch-all.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Serialization or deserialization error (serde/serde_json).
  #[error(transparent)]
  Serialization(#[from] serde_json::Error),

  /// IO error (file, network, etc).
  #[error(transparent)]
  Io(#[from] std::io::Error),

  /// Validation error.
  #[error(transparent)]
  Validation(#[from] validator::ValidationErrors),
}
