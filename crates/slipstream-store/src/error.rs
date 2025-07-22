/// A specialized [`Result`] type for slipstream-store operations.
///
/// This type alias is used throughout the slipstream-store crate to represent
/// results that may return a central [`Error`] type. By default, the error type
/// is [`Error`], but it can be overridden if needed.
///
/// # Type Parameters
///
/// * `T` - The type of the value returned on success.
/// * `E` - The error type, which defaults to [`Error`].
///
/// # Examples
///
/// ```rust
/// use slipstream_store::error::Result;
///
/// fn do_something() -> Result<()> {
///     // Your logic here
///     Ok(())
/// }
/// ```
pub type Result<T, E = Error> = eyre::Result<T, E>;

/// Central error type for slipstream-store operations.
///
/// Encapsulates all error variants that may arise from interacting with
/// the graph database (Kuzu), meta database (LanceDB), IO, Arrow, and
/// serialization layers. Provides granular error reporting for database
/// state, data integrity, and transaction management, supporting
/// transparent conversion from underlying error types.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Error from the graphdb (Kuzu)
  #[error("graph: {0}")]
  Kuzu(#[from] kuzu::Error),

  /// Error from the meta db (LanceDB)
  #[error("meta: {0}")]
  LanceDB(#[from] lancedb::Error),

  /// IO Error
  #[error("IO error: {0}")]
  IoError(#[from] std::io::Error),

  /// Arrow error from Apache Arrow operations
  #[error("Arrow error: {0}")]
  Arrow(#[from] arrow::error::ArrowError),

  /// Generic error with a custom message
  #[error("{0}")]
  Generic(String),

  /// Error indicating that the graph database is closed
  #[error("Graph database is closed")]
  GraphDatabaseClosed,
  /// Error indicating that the meta database is closed
  #[error("Meta database is closed")]
  MetaDatabaseClosed,

  /// Error indicating invalid data from the graph database
  #[error("Invalid graph database data: {0}")]
  InvalidGraphDbData(String),

  /// Error indicating invalid data from the meta database (LanceDB)
  #[error("Invalid meta database data: {0}")]
  InvalidMetaDbData(String),

  /// JSON serialization/deserialization errors
  #[error(transparent)]
  SerdeJson(#[from] serde_json::Error),

  /// UUID parsing errors
  #[error(transparent)]
  UuidError(#[from] uuid::Error),

  /// Jiff timestamp errors
  #[error(transparent)]
  JiffError(#[from] jiff::Error),

  /// Transaction already committed
  #[error("Transaction has already been committed")]
  TransactionAlreadyCommitted,
}
