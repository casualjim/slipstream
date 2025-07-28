pub mod definitions;
pub mod messages;
pub mod registry;

/// The core result type for slipstream-core.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The main error type for slipstream-core, categorizing all major error cases.
/// All variants are structured and meaningful; there is no catch-all.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Resource was not found (e.g., agent/model/tool).
  #[error("Not found: {resource} ({id:?})")]
  NotFound {
    resource: &'static str,
    id: Option<String>,
  },

  /// Validation error for a specific field.
  #[error("{field}: {reason}")]
  Validation { field: &'static str, reason: String },

  /// Conflict error (e.g., duplicate or already exists).
  #[error("Conflict: {resource} ({id:?})")]
  Conflict {
    resource: &'static str,
    id: Option<String>,
  },

  /// Registry logic error, optionally includes an HTTP status code.
  #[error("{reason} (status: {status_code:?})")]
  Registry {
    reason: String,
    status_code: Option<u16>,
  },

  /// Serialization or deserialization error (serde/serde_json).
  #[error(transparent)]
  Serialization(#[from] serde_json::Error),

  /// IO error (file, network, etc).
  #[error(transparent)]
  Io(#[from] std::io::Error),
}
