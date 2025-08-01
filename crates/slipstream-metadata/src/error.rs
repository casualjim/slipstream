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

  /// Invalid reference error, optionally includes an ID.c
  #[error("Invalid reference [{kind}]: {reason}")]
  InvalidRef {
    kind: &'static str,
    reason: &'static str,
  },

  /// Serialization or deserialization error (serde/serde_json).
  #[error(transparent)]
  Serialization(#[from] serde_json::Error),

  /// IO error (file, network, etc).
  #[error(transparent)]
  Io(#[from] std::io::Error),

  /// Validation error.
  #[error(transparent)]
  Validation(#[from] validator::ValidationErrors),

  /// Enum kind error.
  #[error("Unknown {0}: {1}")]
  Unknown(EnumKind, String),

  /// Invalid semantic version string
  #[error(transparent)]
  InvalidVersion(#[from] semver::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumKind {
  ApiDialect,
  ModelProvider,
  ToolProvider,
  Modality,
  ReasoningEffort,
  ModelCapability,
}

impl std::fmt::Display for EnumKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EnumKind::ApiDialect => write!(f, "API Dialect"),
      EnumKind::ModelProvider => write!(f, "Model Provider"),
      EnumKind::ToolProvider => write!(f, "Tool Provider"),
      EnumKind::Modality => write!(f, "Modality"),
      EnumKind::ReasoningEffort => write!(f, "Reasoning Effort"),
      EnumKind::ModelCapability => write!(f, "Model Capability"),
    }
  }
}
