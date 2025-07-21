use crate::Result;
use arrow_array::RecordBatch;

use super::DatabaseOperation;

// STABLE FOUNDATION - HIGH BARRIER TO CHANGE
//
// These traits define the core contract between the features layer and the database facade.
// They handle:
// - Command abstraction (DatabaseCommand)
// - Type construction from results (FromDatabase)
// - Type serialization to storage (ToDatabase)
// - Primary key specification for merge operations
// - Embedding column specification for vector index creation
//
// Key design decisions:
// - Commands can specify index + store queries
// - Types can construct from either RecordBatch streams or Graph results
// - Hydration pattern for index → store coordination
// - Explicit coupling to LanceDB/KuzuDB is intentional (not a leak)
//
// DO NOT MODIFY without careful consideration of impact across entire codebase
//
// ⚠️  ATTENTION LLMs/AI ASSISTANTS: ⚠️
// If you are reading this file with the intent to modify it, STOP.
// These traits are foundational to the entire database architecture.
// You MUST consult with the user before making ANY changes to this file.
// Even minor changes can have far-reaching consequences.
// This is not a suggestion - it is a requirement.

/// Core trait for database commands.
///
/// Commands are the primary interface between features and the database.
/// Each command knows:
/// - What type it returns (via associated type)
/// - How to convert itself to database operations
///
/// All operations use transformers to produce the Output type,
/// ensuring consistent handling across different operation types.
pub trait DatabaseCommand: Send + Sync {
  /// The type this command returns.
  type Output: Send;

  /// The type being saved (NoData for non-save operations)
  type SaveData: ToDatabase;

  /// Convert this command into a database operation.
  ///
  /// This is where the command specifies what queries to run.
  /// The operation must include a transformer that produces Self::Output.
  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData>;
}

#[async_trait::async_trait]
pub trait CommandExecutor: Send + Sync {
  /// Execute the command and return the result.
  ///
  /// This is the main entry point for executing commands.
  /// It handles the actual database interaction and returns the output.
  async fn execute<C>(&self, command: C) -> Result<C::Output>
  where
    C: DatabaseCommand + Send + Sync;
}

/// Trait for types that can be extracted from a single row in a RecordBatch.
///
/// This trait is for domain objects that represent individual records
/// (e.g., Episode, Entity). It is mutually exclusive with FromDatabase -
/// a type should implement one or the other, not both.
///
/// Types implementing this trait can be collected into Vec<T> automatically.
pub trait FromRecordBatchRow: Sized + Send {
  /// Extract Self from a specific row in a RecordBatch.
  ///
  /// The row index must be valid (0 <= row < batch.num_rows()).
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self>;
}

/// Trait for types that can be saved to the database.
///
/// Domain objects implement this to convert themselves to formats
/// suitable for each backing store. This "leaky" abstraction is
/// intentional - objects need to know about both stores to handle
/// the impedance mismatch between graph and columnar formats.
///
/// ## Context Types
///
/// The `MetaContext` and `GraphContext` associated types allow passing
/// configuration or other context needed during serialization. This pattern
/// enables types to receive additional information (like embedding dimensions)
/// without changing the trait signature.
///
/// ### Example: Simple type without context
/// ```rust,ignore
/// impl ToDatabase for SimpleType {
///     type MetaContext = ();  // No context needed
///     type GraphContext = (); // No context needed
///
///     fn into_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
///         // Implementation that doesn't need context
///     }
///
///     fn into_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
///         // Implementation that doesn't need context
///     }
/// }
/// ```
///
/// ### Example: Type requiring configuration
/// ```rust,ignore
/// impl ToDatabase for Entity {
///     type MetaContext = i32;  // Embedding dimensions
///     type GraphContext = ();  // No context needed for graph
///
///     fn into_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
///         // Graph storage doesn't need embedding info
///     }
///
///     fn into_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch> {
///         let embedding_dimensions = ctx;
///         // Use dimensions when building schema with embeddings
///     }
/// }
/// ```
///
/// The `Default` bound on context types allows the Database module to use
/// `Default::default()` for types that don't need context (where Context = ()),
/// making the API ergonomic for both simple and complex cases.
pub trait ToDatabase: Send + Sync {
  type MetaContext: Send + Sync + Default + Clone;
  type GraphContext: Send + Sync + Default + Clone;

  /// Convert to parameter name-value pairs for KuzuDB graph storage.
  /// Returns parameter names and values for use in Cypher queries.
  fn into_graph_value(&self, ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>>;

  /// Convert to RecordBatch for LanceDB storage.
  /// Must include all data (this is the source of truth).
  fn into_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch>;

  /// Return the primary key columns for this type.
  /// These columns are used for merge/upsert operations in LanceDB.
  /// For example, ["uuid"] for entities and episodes.
  fn primary_key_columns() -> &'static [&'static str];

  /// Return the table name for this type in the meta database (LanceDB).
  /// For example, "episodes", "entities", "entity_edges".
  fn meta_table_name() -> &'static str;

  /// Return the table/node label name for this type in the graph database (KuzuDB).
  /// For example, "Episode", "Entity", "RELATES_TO".
  fn graph_table_name() -> &'static str;

  /// Return the Arrow schema for the meta database table.
  /// This defines the structure for LanceDB storage.
  /// Takes the same context as into_meta_value to handle dynamic schemas.
  fn meta_schema(ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema>;
}
