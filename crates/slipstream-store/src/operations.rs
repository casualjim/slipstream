use std::pin::Pin;

use super::ToDatabase;
use crate::{Result, ResultStream};
use arrow_array::RecordBatch;
use futures::Stream;
use std::sync::Arc;

/// A dummy type that implements ToDatabase with unit contexts
/// This allows us to have a default for the D parameter
pub struct NoData;

impl ToDatabase for NoData {
  type MetaContext = ();
  type GraphContext = ();

  fn into_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    unreachable!("NoData should never be used")
  }

  fn into_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
    unreachable!("NoData should never be used")
  }

  fn primary_key_columns() -> &'static [&'static str] {
    unreachable!("NoData should never be used")
  }

  fn meta_table_name() -> &'static str {
    unreachable!("NoData should never be used")
  }

  fn graph_table_name() -> &'static str {
    unreachable!("NoData should never be used")
  }

  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    unreachable!("NoData should never be used")
  }
}

pub enum DatabaseOperation<T, D: ToDatabase = NoData> {
  /// Run a query that returns data
  Query(QueryOperation<T>),

  /// Execute a mutation (single or bulk save)
  Mutation(MutationOperation<T, D>),

  /// Run migrations on both databases
  Migration {
    graph_ddl: Vec<&'static str>,
    meta_setup: Box<
      dyn for<'b> FnOnce(&'b lancedb::Connection) -> futures::future::BoxFuture<'b, Result<()>>
        + Send,
    >,
    transformer: Box<dyn FnOnce(()) -> T + Send>,
  },

  /// Skip execution and return default value
  /// Useful when queries would be empty (e.g., no UUIDs to look up)
  Skip {
    transformer: Box<dyn FnOnce(()) -> T + Send>,
  },
}

/// Type alias for graph query result stream - owned for easier lifetime management
pub type GraphResultStream = ResultStream<Vec<kuzu::Value>>;

/// Type alias for stream transformer - transforms RecordBatch streams to output type T
/// The transformer returns T directly, where T might be a ResultStream or other type
pub type StreamTransformer<T> = Box<dyn FnOnce(ResultStream<RecordBatch>) -> T + Send>;

/// Type alias for graph stream transformer - transforms graph results to output type T
pub type GraphStreamTransformer<T> = Box<dyn FnOnce(GraphResultStream) -> T + Send>;

impl<T, D: ToDatabase> DatabaseOperation<T, D> {
  /// Convenience constructor for single save operations where contexts are ()
  /// This handles the common case where no context is needed
  pub fn save_simple(
    table: &'static str,
    data: Arc<D>,
    cypher: &'static str,
    transformer: impl FnOnce(()) -> T + Send + 'static,
  ) -> Self
  where
    D::GraphContext: Default,
    D::MetaContext: Default,
  {
    DatabaseOperation::Mutation(MutationOperation::Single {
      table,
      data,
      graph_context: Default::default(),
      meta_context: Default::default(),
      cypher,
      transformer: Box::new(transformer),
    })
  }
}

/// Explicit query patterns - no invalid states possible!
pub enum QueryOperation<T> {
  /// Query only the graph index (KuzuDB)
  IndexOnly {
    query: GraphIndexQuery,
    transformer: GraphStreamTransformer<T>,
  },

  /// Query only the primary store (LanceDB)
  StoreOnly {
    query: PrimaryStoreQuery,
    transformer: StreamTransformer<T>,
  },

  /// Query graph index first, then use results to build and execute store query
  /// The transformer receives both the graph results and the store results
  IndexThenStore {
    index: GraphIndexQuery,
    query_builder: Box<
      dyn FnOnce(
          Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send + Unpin>,
        ) -> std::pin::Pin<
          Box<dyn std::future::Future<Output = Result<PrimaryStoreQuery>> + Send>,
        > + Send,
    >,
    transformer: Box<
      dyn FnOnce(
          Pin<Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send>>, // graph results
          Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send>>,      // store results
        ) -> T
        + Send,
    >,
  },
}

/// Mutation patterns for saving data
pub enum MutationOperation<T, D: ToDatabase> {
  /// Save a single item to both databases
  Single {
    table: &'static str,
    data: Arc<D>,
    graph_context: D::GraphContext,
    meta_context: D::MetaContext,
    cypher: &'static str,
    transformer: Box<dyn FnOnce(()) -> T + Send>,
  },

  /// Save multiple items in a single transaction
  Bulk {
    table: &'static str,
    data: Vec<Arc<D>>,
    graph_context: D::GraphContext,
    meta_context: D::MetaContext,
    cypher: &'static str, // Same cypher as single, will be executed for each item
    transformer: Box<dyn FnOnce(usize) -> T + Send>, // Returns number of items saved
  },
}

pub struct GraphIndexQuery {
  pub cypher: String, // These are often built dynamically with format!()
  pub params: Vec<(&'static str, kuzu::Value)>, // Parameter names are usually static
}

pub struct PrimaryStoreQuery {
  pub table: &'static str,
  pub filter: Option<String>, // This needs to be String since it's dynamically built
  pub limit: Option<usize>,
  pub offset: Option<usize>,
  pub vector_search: Option<VectorSearchParams>,
}

pub struct VectorSearchParams {
  pub vector: Vec<f32>,
  pub column: &'static str,
  pub distance_type: lancedb::DistanceType,
}
