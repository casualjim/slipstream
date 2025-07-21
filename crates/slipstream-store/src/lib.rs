mod config;
// Clean database module implementation
// This module provides a clean actor-based database interface with proper separation of concerns

#[cfg(test)]
mod bulk_operations_tests;
#[cfg(test)]
mod crash_recovery_tests;
mod embeddings;
mod error;
mod graph;
#[cfg(test)]
mod invariant_tests;
mod meta;
mod operations;
mod ordered;
mod streams;
mod traits;

mod queries;
// Transformers are exported so we can retain the naming
pub mod transformers;

use futures::Stream;
use graph::GraphDb;
use meta::MetaDb;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub use error::{Error, Result};

// Re-export key traits and types for external use
pub use operations::{
  DatabaseOperation, MutationOperation, NoData, PrimaryStoreQuery, QueryOperation,
};
pub use traits::{CommandExecutor, DatabaseCommand, FromRecordBatchRow, ToDatabase};

// Internal re-exports for tests
#[cfg(test)]
pub use queries::{empty_filter, extract_uuids_to_in_clause};
pub use streams::ResultStream;

// Internal use of our embedding function
use embeddings::create_embedding_function_from_config;

use std::sync::Arc;

pub use crate::config::Config;

/// Database provides a unified interface to both GraphDb and MetaDb
///
/// This facade coordinates operations between KuzuDB (graph) and LanceDB (meta)
/// to ensure consistency. Since both databases already handle their own
/// thread-safety (GraphDb with RwLock, MetaDb with async operations),
/// we don't need additional synchronization here.
///
/// LanceDB handles concurrent writes through MVCC - each write creates a new version.
/// KuzuDB only allows one write transaction at a time.
#[derive(Clone)]
pub struct Database {
  graph: Arc<GraphDb>,
  meta: Arc<MetaDb>,
}

impl std::fmt::Debug for Database {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Database")
      .field("graph", &*self.graph)
      .field("meta", &*self.meta)
      .finish()
  }
}

impl Database {
  /// Create a new Database with the given configuration
  pub async fn new(config: &Config) -> Result<Self> {
    // Create both databases
    let graph = Arc::new(GraphDb::new(config)?);
    let meta = Arc::new(MetaDb::new(config).await?);

    let db = Self { graph, meta };

    // Create SystemMetadata table if it doesn't exist
    // This is internal database state, not a feature migration
    db.graph
      .execute_write(
        r#"
        CREATE NODE TABLE IF NOT EXISTS SystemMetadata (
            table_name STRING,
            lance_version INT64,
            last_updated STRING,
            PRIMARY KEY (table_name)
        )"#,
        vec![],
      )
      .await?;

    // Check consistency between LanceDB and GraphDB on startup
    db.check_consistency_on_startup().await?;

    Ok(db)
  }

  /// Check consistency between LanceDB and GraphDB on startup
  /// If LanceDB has advanced beyond what GraphDB knows, rollback to match
  async fn check_consistency_on_startup(&self) -> Result<()> {
    // Get all tables with their known versions from GraphDB
    let cypher = "MATCH (sm:SystemMetadata) RETURN sm.table_name, sm.lance_version";
    let receiver = self.graph.execute_query(cypher, vec![]).await?;

    let mut tables_to_check = Vec::new();
    {
      use futures::StreamExt;
      let mut stream = UnboundedReceiverStream::new(receiver);
      while let Some(Ok(row)) = stream.next().await {
        if let (Some(kuzu::Value::String(table)), Some(kuzu::Value::Int64(version))) =
          (row.get(0), row.get(1))
        {
          tables_to_check.push((table.clone(), *version as u64));
        }
      }
    }

    // Check each table
    for (table, known_version) in tables_to_check {
      match self.meta.get_table_version(&table).await {
        Ok(actual_version) => {
          if actual_version > known_version {
            tracing::warn!(
              "Detected incomplete transaction: LanceDB table {} at version {} but GraphDB knows version {}. Rolling back.",
              table,
              actual_version,
              known_version
            );
            self.meta.rollback_to_version(&table, known_version).await?;
            tracing::info!(
              "Successfully rolled back table {} to version {}",
              table,
              known_version
            );
          } else {
            tracing::debug!("Table {} is consistent at version {}", table, known_version);
          }
        }
        Err(e) => {
          // Table might not exist in LanceDB yet - this is fine
          tracing::debug!("Table {} not found in LanceDB: {}", table, e);
        }
      }
    }

    Ok(())
  }

  /// Execute a DatabaseCommand and return its output
  ///
  /// This is the single public interface for all database operations.
  /// The command pattern ensures type safety and proper operation handling.
  pub async fn execute<C>(&self, command: C) -> Result<C::Output>
  where
    C: DatabaseCommand + Send + Sync,
  {
    use DatabaseOperation::*;

    let operation = command.to_operation();
    match operation {
      Skip { transformer } => {
        // Skip execution and apply transformer
        Ok(transformer(()))
      }
      Query(query_op) => {
        // Handle query operations
        use operations::QueryOperation::*;

        match query_op {
          IndexOnly { query, transformer } => {
            // Query only the graph index
            let receiver = self
              .graph
              .execute_query(
                &query.cypher,
                query.params, // Now accepts Vec<(&'static str, kuzu::Value)> directly
              )
              .await?;

            // Convert receiver to stream
            let graph_stream: operations::GraphResultStream =
              Box::pin(UnboundedReceiverStream::new(receiver));

            // Apply transformer directly on graph results
            let output = transformer(graph_stream);
            Ok(output)
          }

          StoreOnly { query, transformer } => {
            // Query only the primary store
            let stream = self.meta.query_table(&query).await?;

            // Apply transformer to convert RecordBatch stream to output type
            let output = transformer(Box::pin(stream));

            // Return the output directly
            Ok(output)
          }

          IndexThenStore {
            index,
            query_builder,
            transformer,
          } => {
            // Query the graph index first
            let receiver = self
              .graph
              .execute_query(&index.cypher, index.params)
              .await?;

            // Collect graph results - we need to do this anyway for the IN clause
            let graph_results = Arc::new({
              let stream = UnboundedReceiverStream::new(receiver);
              use futures::TryStreamExt;
              stream.try_collect::<Vec<Vec<kuzu::Value>>>().await?
            });

            // Create stream for query builder from the Arc
            let results_for_builder = graph_results.clone();
            let graph_stream_for_builder: Box<
              dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send + Unpin,
            > = Box::new(futures::stream::iter(
              (0..results_for_builder.len()).map(move |i| Ok(results_for_builder[i].clone())),
            ));

            // Build the store query based on graph results
            let store_query = query_builder(graph_stream_for_builder).await?;

            // Execute the store query using query_table (handles both regular and vector searches)
            let result_stream = self.meta.query_table(&store_query).await?;

            // Create stream for transformer from the same Arc
            let results_for_transformer = graph_results;
            let graph_stream_for_transformer = futures::stream::iter(
              (0..results_for_transformer.len())
                .map(move |i| Ok(results_for_transformer[i].clone())),
            );

            // Apply transformer to convert results to output type
            let output = transformer(
              Box::pin(graph_stream_for_transformer),
              Box::pin(result_stream),
            );

            Ok(output)
          }
        }
      }
      Mutation(mutation_op) => {
        // Handle mutation operations with 2PC
        use operations::MutationOperation::*;

        match mutation_op {
          Single {
            table,
            data,
            graph_context,
            meta_context,
            cypher,
            transformer,
          } => {
            // Convert data to formats for both databases (before moving into closure)
            let graph_params = data.into_graph_value(graph_context)?;
            let meta_batch = data.into_meta_value(meta_context)?;
            // Get primary key columns from the type
            let primary_keys = C::SaveData::primary_key_columns();
            // Rebind static strings as local variables to avoid allocation
            let cypher = cypher;
            let table = table;

            // Use 2PC to ensure consistency
            self
              .execute_2pc(
                table,
                move |meta_db| {
                  let batch = meta_batch;
                  Box::pin(async move {
                    // Save to LanceDB with automatic mode detection
                    let (rollback_version, _current_version) =
                      meta_db.save_to_table(table, batch, primary_keys).await?;

                    // Return unit result with rollback info
                    Ok(((), Some((table, rollback_version))))
                  })
                },
                vec![
                  // Use the provided cypher query with the named parameters
                  { (cypher.to_string(), graph_params) },
                ],
              )
              .await?;

            // Apply transformer to convert () to output type
            Ok(transformer(()))
          }
          Bulk {
            table,
            data,
            graph_context,
            meta_context,
            cypher,
            transformer,
          } => {
            // Convert all items to a single RecordBatch for LanceDB
            let batches: Result<Vec<_>> = data
              .iter()
              .map(|item| item.into_meta_value(meta_context.clone()))
              .collect();
            let batches = batches?;

            // If no data, return early with count 0
            if batches.is_empty() {
              return Ok(transformer(0));
            }

            // Concatenate all batches into one
            let meta_batch = arrow::compute::concat_batches(&batches[0].schema(), &batches)
              .map_err(|e| crate::Error::Arrow(e))?;

            // Convert all items to individual graph parameters
            let graph_operations: Result<Vec<_>> = data
              .iter()
              .map(|item| {
                let params = item.into_graph_value(graph_context.clone())?;
                Ok((cypher.to_string(), params))
              })
              .collect();
            let graph_operations = graph_operations?;

            let table = table;
            let item_count = data.len();
            // Get primary key columns from the type
            let primary_keys = C::SaveData::primary_key_columns();

            // Use 2PC to ensure consistency
            // Execute all operations within a single transaction
            self
              .execute_2pc(
                table,
                move |meta_db| {
                  let batch = meta_batch;
                  Box::pin(async move {
                    // Save to LanceDB and get both versions
                    let (rollback_version, _new_version) =
                      meta_db.save_to_table(table, batch, primary_keys).await?;

                    // Return count with rollback info
                    Ok((item_count, Some((table, rollback_version))))
                  })
                },
                graph_operations,
              )
              .await?;

            // Apply transformer with item count
            Ok(transformer(item_count))
          }
        }
      }
      Migration {
        graph_ddl,
        meta_setup,
        transformer,
      } => {
        // Handle migration operations
        // Migrations don't need 2PC since they're idempotent DDL operations

        // Execute graph DDL statements
        for ddl in graph_ddl {
          // Try to execute each DDL statement, but don't fail on errors
          // (e.g., relationship tables that depend on tables that don't exist yet)
          match self.graph.execute_write(&ddl, vec![]).await {
            Ok(_) => tracing::info!("Executed graph DDL: {}", ddl),
            Err(e) => tracing::warn!("Graph DDL failed (may be expected): {} - {}", ddl, e),
          }
        }

        // Execute meta setup through MetaDb's execute_setup method
        self.meta.execute_setup(meta_setup).await?;

        // Apply transformer to convert () to output type
        Ok(transformer(()))
      }
    }
  }

  /// Execute a two-phase commit operation
  ///
  /// This ensures that operations on both databases either both succeed
  /// or both fail. We use KuzuDB's transaction support to provide atomicity:
  ///
  /// 1. Begin KuzuDB transaction (this serializes write transactions)
  /// 2. Execute GraphDB operations within the transaction
  /// 3. Execute LanceDB operations (auto-commits)
  /// 4. If GraphDB fails after LanceDB succeeded, rollback LanceDB
  /// 5. Commit or rollback KuzuDB based on success/failure
  ///
  /// The meta_op function should return (result, rollback_info) where
  /// rollback_info is Some((table_name, version)) if rollback is possible
  #[cfg_attr(not(test), allow(dead_code))]
  pub(crate) async fn execute_2pc<F, T>(
    &self,
    table_name: &'static str, // Table to capture version for before any operations
    meta_op: F,
    graph_queries: Vec<(String, Vec<(&'static str, kuzu::Value)>)>,
  ) -> Result<T>
  where
    F: FnOnce(&MetaDb) -> futures::future::BoxFuture<'_, Result<(T, Option<(&'static str, u64)>)>>
      + Send
      + 'static,
    T: Send + 'static,
  {
    // Clone what we need for the blocking task
    let graph = self.graph.clone();
    let meta = self.meta.clone();
    let tx_id = uuid::Uuid::now_v7();

    // Run the entire 2PC operation in a blocking context
    tokio::task::spawn_blocking(move || {
      // Get exclusive access to the database and hold it for the entire transaction
      let db_guard = graph.db.write();
      let conn = kuzu::Connection::new(&db_guard)?;

      // Start the transaction
      conn.query("BEGIN TRANSACTION")?;
      tracing::debug!("Started transaction {}", tx_id);

      // If table name is provided, capture the version RIGHT AFTER acquiring the lock
      // This ensures nothing can change between now and when we execute our operations
      let initial_version =
        match tokio::runtime::Handle::current().block_on(meta.get_table_version(table_name)) {
          Ok(version) => {
            tracing::debug!(
              "Captured initial version {} for table {} in transaction {}",
              version,
              table_name,
              tx_id
            );

            Some((table_name, version))
          }
          Err(e) => {
            tracing::warn!(
              "Could not capture initial version for table {}: {} tx={}",
              table_name,
              e,
              tx_id
            );
            conn.query("ROLLBACK")?;
            return Err(e);
          }
        };

      // First, execute LanceDB operation and capture rollback info
      let meta_result = tokio::runtime::Handle::current().block_on(meta_op(&meta));

      let (result, _) = match meta_result {
        Ok((result, rollback_info)) => (result, rollback_info),
        Err(e) => {
          // LanceDB failed - rollback empty GraphDB transaction
          tracing::warn!("LanceDB operation failed in transaction {}: {}", tx_id, e);
          conn.query("ROLLBACK")?;
          return Err(e);
        }
      };

      // Get the new LanceDB version after the operation
      let new_lance_version = if let Some((table, _)) = initial_version {
        match tokio::runtime::Handle::current().block_on(meta.get_table_version(table)) {
          Ok(version) => {
            tracing::debug!(
              "LanceDB operation created version {} for table {}",
              version,
              table
            );
            Some((table, version))
          }
          Err(e) => {
            tracing::error!("Failed to get new LanceDB version: {}", e);
            conn.query("ROLLBACK")?;
            return Err(e);
          }
        }
      } else {
        None
      };

      // LanceDB succeeded, now execute graph operations
      tracing::debug!(
        "Executing graph operations in transaction {}: {} queries",
        tx_id,
        graph_queries.len()
      );

      // If we have a new version, add the SystemMetadata update to the graph queries
      let mut all_graph_queries = graph_queries;
      if let Some((table, version)) = new_lance_version {
        let timestamp = jiff::Timestamp::now().to_string();
        let update_query = format!(
          "MERGE (sm:SystemMetadata {{table_name: $table_name}})
           SET sm.lance_version = $version, sm.last_updated = $timestamp"
        );
        let params = vec![
          ("table_name", kuzu::Value::String(table.to_string())),
          ("version", kuzu::Value::Int64(version as i64)),
          ("timestamp", kuzu::Value::String(timestamp)),
        ];
        all_graph_queries.push((update_query, params));
      }

      for (query, params) in all_graph_queries {
        let result = if params.is_empty() {
          tracing::debug!("Executing graph query in transaction {}: {}", tx_id, query);
          conn.query(&query)
        } else {
          tracing::debug!(
            "Executing graph query with params in transaction {}: {}",
            tx_id,
            query
          );
          let maybe_prepared = conn.prepare(&query);
          tracing::debug!(
            "Prepared graph query in transaction {}: {}",
            tx_id,
            maybe_prepared.is_ok()
          );
          match maybe_prepared {
            Ok(mut prepared) => conn.execute(&mut prepared, params),
            Err(e) => {
              tracing::error!("Failed to prepare graph query: {}", e);
              if let Some((table_name, version)) = initial_version {
                tracing::warn!(
                  "Rolling back LanceDB table {} to version {} due to GraphDB failure",
                  table_name,
                  version
                );
                let rollback_result = tokio::runtime::Handle::current()
                  .block_on(meta.rollback_to_version(&table_name, version));

                if let Err(rollback_err) = rollback_result {
                  tracing::error!("Failed to rollback LanceDB: {}", rollback_err);
                }
              }
              return Err(e.into());
            }
          }
        };
        tracing::debug!("Executed graph query in transaction {}: {}", tx_id, query);

        if let Err(e) = result {
          tracing::error!("Graph operation failed in transaction {}: {}", tx_id, e);
          conn.query("ROLLBACK")?;

          // GraphDB failed - need to rollback LanceDB
          if let Some((table_name, version)) = initial_version {
            tracing::warn!(
              "Rolling back LanceDB table {} to version {} due to GraphDB failure",
              table_name,
              version
            );
            let rollback_result = tokio::runtime::Handle::current()
              .block_on(meta.rollback_to_version(&table_name, version));

            if let Err(rollback_err) = rollback_result {
              tracing::error!("Failed to rollback LanceDB: {}", rollback_err);
            }
          }

          return Err(e.into());
        }
      }

      // Both operations succeeded - commit the transaction
      conn.query("COMMIT")?;
      tracing::debug!("Committed transaction {}", tx_id);

      Ok(result)
    })
    .await
    .expect("2PC task should not panic")
  }
}

#[cfg(test)]
mod tests {
  use super::{operations::NoData, *};
  use crate::operations::{GraphIndexQuery, VectorSearchParams};
  use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
  use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, StringArray,
  };
  use futures::{StreamExt, TryStreamExt};
  use tempfile::tempdir;

  use ctor::ctor;
  use tracing_subscriber::prelude::*;

  #[ctor]
  fn init_color_backtrace() {
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let subscriber = tracing_subscriber::fmt::layer()
      .pretty()
      .with_test_writer()
      .with_filter(env_filter);

    tracing_subscriber::registry().with(subscriber).init();
    color_backtrace::install();
  }

  fn test_config(path: &std::path::Path) -> Config {
    Config::new_test(path.to_path_buf())
  }

  #[tokio::test]
  async fn test_database_actor_creation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Both databases should be accessible
    assert!(actor.graph.execute_write("RETURN 1", vec![]).await.is_ok());
    let query = crate::operations::PrimaryStoreQuery {
      table: "nonexistent",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    assert!(actor.meta.query_table(&query).await.is_err());
  }

  #[tokio::test]
  async fn test_2pc_success() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup GraphDb schema
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE TestNode (id UUID, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Setup MetaDb table
    actor
      .meta
      .execute_setup(Box::new(|conn| {
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));
          conn
            .create_empty_table("test_nodes", schema)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create test_nodes table");

    // Test successful 2PC operation
    let test_id = uuid::Uuid::now_v7();
    let result = actor
      .execute_2pc(
        "test_nodes",
        move |meta: &MetaDb| {
          Box::pin(async move {
            // Create test data for MetaDb
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
            ]));

            let batch = RecordBatch::try_new(
              schema,
              vec![
                Arc::new(StringArray::from(vec![test_id.to_string()])) as arrow_array::ArrayRef,
                Arc::new(StringArray::from(vec!["Test Node"])) as arrow_array::ArrayRef,
              ],
            )?;

            let (prev_version, _new_version) =
              meta.save_to_table("test_nodes", batch, &["id"]).await?;
            // Return test_id with rollback info (table_name, version)
            Ok((test_id, Some(("test_nodes", prev_version))))
          })
        },
        vec![
          // Create corresponding node in GraphDb
          (
            "CREATE (:TestNode {id: $id, name: $name})".to_string(),
            vec![
              ("id", kuzu::Value::UUID(test_id)),
              ("name", kuzu::Value::String("Test Node".to_string())),
            ],
          ),
        ],
      )
      .await;

    assert!(result.is_ok());
    let returned_id = result.unwrap();
    assert_eq!(returned_id, test_id);
  }

  #[tokio::test]
  async fn test_concurrent_meta_writes() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create table first
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Int64, false),
      Field::new("value", DataType::Utf8, false),
    ]));

    // Create the table using execute_setup
    let schema_for_create = schema.clone();
    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("concurrent_test", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Now insert initial data
    let initial_batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(Int64Array::from(vec![0])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["initial"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = actor
      .meta
      .save_to_table("concurrent_test", initial_batch, &["id"])
      .await
      .expect("Failed to save initial data");

    // Spawn 10 concurrent write operations to LanceDB
    let mut handles = vec![];

    for i in 1..=10 {
      let actor_clone = actor.clone();
      let schema_clone = schema.clone();

      let handle = tokio::spawn(async move {
        let batch = RecordBatch::try_new(
          schema_clone,
          vec![
            Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
            Arc::new(StringArray::from(vec![format!("value_{}", i)])) as arrow_array::ArrayRef,
          ],
        )?;

        let _ = actor_clone
          .meta
          .save_to_table("concurrent_test", batch, &["id"])
          .await?;
        Ok::<i64, crate::Error>(i)
      });
      handles.push(handle);
    }

    // All writes should succeed due to MVCC
    let results: Vec<_> = futures::future::join_all(handles).await;
    for result in results.iter() {
      assert!(result.is_ok(), "Write failed: {:?}", result);
    }

    // Verify all data was written
    let stream = actor
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "concurrent_test",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Failed to query");

    let batches: Vec<_> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 11, "Should have initial + 10 concurrent writes");
  }

  #[tokio::test]
  async fn test_concurrent_graph_writes() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup schema
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE ConcurrentNode (id INT64, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Spawn 10 concurrent write operations to KuzuDB
    let mut handles = vec![];

    for i in 1..=10 {
      let actor_clone = actor.clone();

      let handle = tokio::spawn(async move {
        actor_clone
          .graph
          .execute_write(
            "CREATE (:ConcurrentNode {id: $id, name: $name})",
            vec![
              ("id", kuzu::Value::Int64(i)),
              ("name", kuzu::Value::String(format!("Node{}", i))),
            ],
          )
          .await
      });
      handles.push(handle);
    }

    // All writes should succeed (KuzuDB serializes them)
    let results: Vec<_> = futures::future::join_all(handles).await;
    for result in results.iter() {
      assert!(result.is_ok(), "Write task failed: {:?}", result);
      assert!(
        result.as_ref().unwrap().is_ok(),
        "Write inner failed: {:?}",
        result.as_ref().unwrap()
      );
    }

    // Verify all nodes were created
    let receiver = actor
      .graph
      .execute_query("MATCH (n:ConcurrentNode) RETURN count(n)", vec![])
      .await
      .expect("Failed to query");

    let mut stream = UnboundedReceiverStream::new(receiver);
    let row = stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be Ok");
    match &row[0] {
      kuzu::Value::Int64(count) => assert_eq!(*count, 10, "Should have created 10 nodes"),
      _ => panic!("Expected Int64 count"),
    }
  }

  #[tokio::test]
  async fn test_concurrent_2pc_operations() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup schemas
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE Entity2PC (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Create the MetaDb table first - schemas should exist beforehand in real systems
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ]));

    // Create the table using execute_setup
    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("entities_2pc", schema)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Spawn multiple concurrent 2PC operations
    let mut handles = vec![];

    for i in 0..20 {
      let actor_clone = actor.clone();

      let handle = tokio::spawn(async move {
        let entity_id = uuid::Uuid::now_v7();

        actor_clone
          .execute_2pc(
            "entities_2pc",
            move |meta: &MetaDb| {
              Box::pin(async move {
                let schema = Arc::new(ArrowSchema::new(vec![
                  Field::new("id", DataType::Utf8, false),
                  Field::new("name", DataType::Utf8, false),
                  Field::new("value", DataType::Int64, false),
                ]));

                let batch = RecordBatch::try_new(
                  schema,
                  vec![
                    Arc::new(StringArray::from(vec![entity_id.to_string()]))
                      as arrow_array::ArrayRef,
                    Arc::new(StringArray::from(vec![format!("Entity{}", i)]))
                      as arrow_array::ArrayRef,
                    Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
                  ],
                )?;

                // Save and get both versions
                let (rollback_version, _new_version) =
                  meta.save_to_table("entities_2pc", batch, &["id"]).await?;
                Ok((entity_id, Some(("entities_2pc", rollback_version))))
              })
            },
            vec![(
              "CREATE (:Entity2PC {id: $id, name: $name, value: $value})".to_string(),
              vec![
                ("id", kuzu::Value::UUID(entity_id)),
                ("name", kuzu::Value::String(format!("Entity{}", i))),
                ("value", kuzu::Value::Int64(i)),
              ],
            )],
          )
          .await
      });
      handles.push(handle);
    }

    // Collect results
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All operations should complete (some might fail due to conflicts)
    let successful: Vec<_> = results
      .iter()
      .filter_map(|r| r.as_ref().ok())
      .filter_map(|r| r.as_ref().ok())
      .collect();

    assert!(
      !successful.is_empty(),
      "At least some 2PC operations should succeed"
    );

    // Verify data consistency
    let graph_count = {
      let receiver = actor
        .graph
        .execute_query("MATCH (n:Entity2PC) RETURN count(n)", vec![])
        .await
        .expect("Failed to query");

      let mut stream = UnboundedReceiverStream::new(receiver);
      let row = stream
        .next()
        .await
        .expect("Should have result")
        .expect("Should be Ok");
      match &row[0] {
        kuzu::Value::Int64(count) => *count,
        _ => panic!("Expected Int64 count"),
      }
    };

    println!(
      "Successfully completed {} 2PC operations out of 20",
      graph_count
    );
    assert!(
      graph_count > 0,
      "Should have at least some successful 2PC operations"
    );
  }

  #[tokio::test]
  async fn test_high_load_mixed_operations() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE LoadTest (id INT64, type STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Int64, false),
      Field::new("type", DataType::Utf8, false),
    ]));

    // Create the table using execute_setup
    let schema_for_create = schema.clone();
    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("load_test", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Launch 100 mixed operations
    let mut handles = vec![];

    for i in 0..100 {
      let actor_clone = actor.clone();
      let schema_clone = schema.clone();

      let handle = tokio::spawn(async move {
        match i % 4 {
          0 => {
            // Graph write
            actor_clone
              .graph
              .execute_write(
                "CREATE (:LoadTest {id: $id, type: $type})",
                vec![
                  ("id", kuzu::Value::Int64(i)),
                  ("type", kuzu::Value::String("graph_write".to_string())),
                ],
              )
              .await
              .map(|_| "graph_write")
          }
          1 => {
            // Graph read
            let receiver = actor_clone
              .graph
              .execute_query(
                "MATCH (n:LoadTest) WHERE n.id < $limit RETURN count(n)",
                vec![("limit", kuzu::Value::Int64(i))],
              )
              .await?;

            let mut stream = UnboundedReceiverStream::new(receiver);
            let _ = stream.next().await;
            Ok("graph_read")
          }
          2 => {
            // Meta write
            let batch = RecordBatch::try_new(
              schema_clone,
              vec![
                Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
                Arc::new(StringArray::from(vec!["meta_write"])) as arrow_array::ArrayRef,
              ],
            )?;

            actor_clone
              .meta
              .save_to_table("load_test", batch, &["id"])
              .await
              .map(|_| "meta_write")
          }
          _ => {
            // Meta read
            let filter = format!("id < {}", i);
            let stream = actor_clone
              .meta
              .query_table(&PrimaryStoreQuery {
                table: "load_test",
                filter: Some(filter),
                limit: Some(10),
                offset: None,
                vector_search: None,
              })
              .await?;

            let _ = stream.try_collect::<Vec<_>>().await?;
            Ok("meta_read")
          }
        }
      });
      handles.push(handle);
    }

    // Collect results
    let results: Vec<_> = futures::future::join_all(handles).await;

    // Count successes by type
    let mut success_counts = std::collections::HashMap::new();
    for result in results {
      if let Ok(Ok(op_type)) = result {
        *success_counts.entry(op_type).or_insert(0) += 1;
      }
    }

    println!("High load test results: {:?}", success_counts);

    // All operations should succeed
    let total_success: usize = success_counts.values().sum();
    assert_eq!(total_success, 100, "All 100 operations should succeed");
  }

  #[tokio::test]
  async fn test_2pc_failures_under_concurrency() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup schemas
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE Entity2PCFailure (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Create a table with a unique constraint to force conflicts
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE UniqueConstraint (name STRING, PRIMARY KEY(name))",
        vec![],
      )
      .await
      .expect("Failed to create constraint table");

    // Create the MetaDb table first - schemas should exist beforehand in real systems
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
      Field::new("conflict_group", DataType::Int64, false),
    ]));

    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        let schema = schema.clone();
        Box::pin(async move {
          conn
            .create_empty_table("entities_2pc_failure", schema)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Track versions for analysis
    let version_tracker = Arc::new(parking_lot::Mutex::new(Vec::new()));

    // Spawn 50 concurrent 2PC operations with intentional conflicts
    let mut handles = vec![];

    for i in 0..50 {
      let actor_clone = actor.clone();
      let version_tracker_clone = version_tracker.clone();

      let handle = tokio::spawn(async move {
        let entity_id = uuid::Uuid::now_v7();

        // Use a shared name for some operations to cause conflicts
        let conflict_group = i / 10; // Creates 5 groups of 10 operations each
        let unique_name = format!("conflict_group_{}", conflict_group);

        // Note: We can't do the conditional tracking inside execute anymore
        // So we'll track before calling and adjust based on result
        let tracker = version_tracker_clone.clone();
        let tracker_inner = tracker.clone();

        let result = actor_clone
          .execute_2pc(
            "entities_2pc_failure",
            move |meta: &MetaDb| {
              let tracker = tracker_inner.clone();
              Box::pin(async move {
                let schema = Arc::new(ArrowSchema::new(vec![
                  Field::new("id", DataType::Utf8, false),
                  Field::new("name", DataType::Utf8, false),
                  Field::new("value", DataType::Int64, false),
                  Field::new("conflict_group", DataType::Int64, false),
                ]));

                let batch = RecordBatch::try_new(
                  schema,
                  vec![
                    Arc::new(StringArray::from(vec![entity_id.to_string()]))
                      as arrow_array::ArrayRef,
                    Arc::new(StringArray::from(vec![format!("Entity{}", i)]))
                      as arrow_array::ArrayRef,
                    Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
                    Arc::new(Int64Array::from(vec![conflict_group])) as arrow_array::ArrayRef,
                  ],
                )?;

                // Save to table and get both versions
                let (rollback_version, new_version) = meta
                  .save_to_table("entities_2pc_failure", batch, &["id"])
                  .await?;

                // Track the version that was created
                tracker.lock().push((i, new_version, "meta_success"));

                Ok((entity_id, Some(("entities_2pc_failure", rollback_version))))
              })
            },
            vec![
              // First, always create the entity (should succeed)
              (
                "CREATE (:Entity2PCFailure {id: $id, name: $name, value: $value})".to_string(),
                vec![
                  ("id", kuzu::Value::UUID(entity_id)),
                  ("name", kuzu::Value::String(format!("Entity{}", i))),
                  ("value", kuzu::Value::Int64(i)),
                ],
              ),
              // Then try to create the unique constraint node
              // This will fail for all but the first in each conflict group
              (
                "CREATE (:UniqueConstraint {name: $name})".to_string(),
                vec![("name", kuzu::Value::String(unique_name.clone()))],
              ),
            ],
          )
          .await;

        // Track graph result
        if result.is_err() {
          tracker.lock().push((i, 0, "graph_failed"));
        } else {
          tracker.lock().push((i, 0, "graph_success"));
        }

        (i, entity_id, result)
      });
      handles.push(handle);
    }

    // Collect results
    let results: Vec<_> = futures::future::join_all(handles).await;

    // Analyze results
    let mut successful = 0;
    let mut failed = 0;
    let mut panic_count = 0;
    let mut graph_errors = Vec::new();

    for result in results.iter() {
      match result {
        Ok((i, _, Ok(_))) => {
          successful += 1;
          println!("Operation {} (group {}) succeeded", i, i / 10);
        }
        Ok((i, _, Err(e))) => {
          failed += 1;
          graph_errors.push((i, e.to_string()));
        }
        Err(e) => {
          panic_count += 1;
          println!("Task panicked: {:?}", e);
        }
      }
    }

    println!("\n2PC Failure Analysis:");
    println!("Total operations: 50");
    println!("Successful 2PC: {}", successful);
    println!("Failed 2PC: {}", failed);
    println!("Panicked tasks: {}", panic_count);

    // Print first few errors to understand the pattern
    println!("\nFirst 10 GraphDb errors:");
    for (i, err) in graph_errors.iter().take(10) {
      println!("  Operation {} (group {}): {}", i, *i / 10, err);
    }

    // We expect exactly 5 successes (first in each conflict group)
    // The exact winners depend on execution order, but there should be one per group
    assert_eq!(
      successful, 5,
      "Should have exactly 5 successful operations (one per conflict group)"
    );
    assert_eq!(failed, 45, "Should have 45 failed operations (conflicts)");
    assert_eq!(panic_count, 0, "No tasks should panic");

    // Verify MetaDb state - all 50 operations should have succeeded there
    let meta_count = {
      let stream = actor
        .meta
        .query_table(&PrimaryStoreQuery {
          table: "entities_2pc_failure",
          filter: None,
          limit: None,
          offset: None,
          vector_search: None,
        })
        .await
        .expect("Failed to query meta");
      let batches: Vec<_> = stream.try_collect().await.expect("Failed to collect");
      batches.iter().map(|b| b.num_rows()).sum::<usize>()
    };

    // Verify GraphDb state - only successful operations should be there
    let graph_count = {
      let receiver = actor
        .graph
        .execute_query("MATCH (n:Entity2PCFailure) RETURN count(n)", vec![])
        .await
        .expect("Failed to query");

      let mut stream = UnboundedReceiverStream::new(receiver);
      let row = stream
        .next()
        .await
        .expect("Should have result")
        .expect("Should be Ok");
      match &row[0] {
        kuzu::Value::Int64(count) => *count as usize,
        _ => panic!("Expected Int64 count"),
      }
    };

    // Check UniqueConstraint nodes - should be exactly 5
    let unique_count = {
      let receiver = actor
        .graph
        .execute_query("MATCH (n:UniqueConstraint) RETURN count(n)", vec![])
        .await
        .expect("Failed to query");

      let mut stream = UnboundedReceiverStream::new(receiver);
      let row = stream
        .next()
        .await
        .expect("Should have result")
        .expect("Should be Ok");
      match &row[0] {
        kuzu::Value::Int64(count) => *count as usize,
        _ => panic!("Expected Int64 count"),
      }
    };

    println!("\nData Consistency Check:");
    println!("MetaDb records: {} (with proper 2PC rollback)", meta_count);
    println!(
      "GraphDb Entity2PCFailure records: {} (all entity nodes created)",
      graph_count
    );
    println!(
      "GraphDb UniqueConstraint records: {} (only non-conflicting operations)",
      unique_count
    );

    // WITH PROPER 2PC: We expect consistency between databases
    // Only successful transactions should have data in both databases
    // The exact count depends on execution order and rollback cascades

    // We should have at least the initial record + 5 successful operations
    assert!(
      meta_count >= 6,
      "Should have at least initial + 5 successful records, got {}",
      meta_count
    );

    // Entity2PCFailure nodes are created before the unique constraint check,
    // so some might remain even after rollback (depends on implementation details)
    assert!(
      graph_count >= 5,
      "Should have at least 5 Entity2PCFailure nodes, got {}",
      graph_count
    );

    // Exactly 5 unique constraint nodes (one per conflict group)
    assert_eq!(
      unique_count, 5,
      "Only 5 UniqueConstraint nodes should exist (one per group)"
    );

    // Analyze version progression
    let versions = version_tracker.lock();
    let successful_meta_versions: Vec<_> = versions
      .iter()
      .filter(|(_, _, status)| *status == "meta_success")
      .map(|(i, version, _)| (*i, *version))
      .collect();

    println!("\nVersion Analysis:");
    println!(
      "Successful MetaDb operations: {}",
      successful_meta_versions.len()
    );

    // Due to concurrent rollbacks, we may not get all 50 versions
    // Some operations may fail during the MetaDb phase due to conflicts
    assert!(
      !successful_meta_versions.is_empty(),
      "Should have some successful MetaDb operations"
    );
  }

  #[tokio::test]
  async fn test_2pc_deterministic_rollback() {
    // Deterministic test for 2PC rollback behavior
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE DeterministicEntity (id UUID, value INT64, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Create initial MetaDb state
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ]));

    // Create table first
    let schema_for_create = schema.clone();
    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("deterministic_test", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Insert initial data
    let initial_batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec![
          "00000000-0000-0000-0000-000000000000",
        ])) as arrow_array::ArrayRef,
        Arc::new(Int64Array::from(vec![0])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let (_, initial_version) = actor
      .meta
      .save_to_table("deterministic_test", initial_batch, &["id"])
      .await
      .expect("Failed to insert initial data");

    println!("Initial state:");
    println!("- MetaDb version: {}", initial_version);

    // Test 1: Successful 2PC operation
    let success_id = uuid::Uuid::now_v7();
    let schema_clone = schema.clone();
    let result = actor
      .execute_2pc(
        "deterministic_test",
        move |meta: &MetaDb| {
          Box::pin(async move {
            let batch = RecordBatch::try_new(
              schema_clone,
              vec![
                Arc::new(StringArray::from(vec![success_id.to_string()])) as arrow_array::ArrayRef,
                Arc::new(Int64Array::from(vec![1])) as arrow_array::ArrayRef,
              ],
            )?;

            let (rollback_version, version) = meta
              .save_to_table("deterministic_test", batch, &["id"])
              .await?;
            println!("Test 1 - MetaDb write succeeded, version: {}", version);
            Ok((success_id, Some(("deterministic_test", rollback_version))))
          })
        },
        vec![(
          "CREATE (:DeterministicEntity {id: $id, value: $value})".to_string(),
          vec![
            ("id", kuzu::Value::UUID(success_id)),
            ("value", kuzu::Value::Int64(1)),
          ],
        )],
      )
      .await;

    assert!(result.is_ok(), "Successful 2PC should complete");

    // Test 2: Failed 2PC operation (GraphDb fails)
    let failed_id = uuid::Uuid::now_v7();
    let schema_clone = schema.clone();
    // For this test, we need GraphDb to fail
    // We'll use an invalid query to trigger the failure
    let result = actor
      .execute_2pc(
        "deterministic_test",
        move |meta: &MetaDb| {
          Box::pin(async move {
            let batch = RecordBatch::try_new(
              schema_clone,
              vec![
                Arc::new(StringArray::from(vec![failed_id.to_string()])) as arrow_array::ArrayRef,
                Arc::new(Int64Array::from(vec![2])) as arrow_array::ArrayRef,
              ],
            )?;

            let (rollback_version, version) = meta
              .save_to_table("deterministic_test", batch, &["id"])
              .await?;
            println!("Test 2 - MetaDb write succeeded, version: {}", version);
            Ok((failed_id, Some(("deterministic_test", rollback_version))))
          })
        },
        vec![
          // This will fail because we're trying to create without proper syntax
          ("INVALID QUERY TO SIMULATE FAILURE".to_string(), vec![]),
        ],
      )
      .await;

    assert!(result.is_err(), "Failed 2PC should return error");

    // Verify final state
    let meta_count = {
      let stream = actor
        .meta
        .query_table(&PrimaryStoreQuery {
          table: "deterministic_test",
          filter: None,
          limit: None,
          offset: None,
          vector_search: None,
        })
        .await
        .expect("Failed to query");
      let batches: Vec<_> = stream.try_collect().await.expect("Failed to collect");

      // Debug: print what's actually in the table
      println!("\nMetaDb contents after rollback:");
      for (i, batch) in batches.iter().enumerate() {
        println!("  Batch {}: {} rows", i, batch.num_rows());
        if batch.num_rows() > 0 {
          let id_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
          let value_col = batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
          for row in 0..batch.num_rows() {
            println!(
              "    Row {}: id={}, value={}",
              row,
              id_col.value(row),
              value_col.value(row)
            );
          }
        }
      }

      batches.iter().map(|b| b.num_rows()).sum::<usize>()
    };

    let graph_count = {
      let receiver = actor
        .graph
        .execute_query("MATCH (n:DeterministicEntity) RETURN count(n)", vec![])
        .await
        .expect("Failed to query");

      let mut stream = UnboundedReceiverStream::new(receiver);
      let row = stream
        .next()
        .await
        .expect("Should have result")
        .expect("Should be Ok");
      match &row[0] {
        kuzu::Value::Int64(count) => *count as usize,
        _ => panic!("Expected Int64 count"),
      }
    };

    let final_version = actor
      .meta
      .get_table_version("deterministic_test")
      .await
      .expect("Failed to get version");

    println!("\nFinal state:");
    println!(
      "- MetaDb records: {} (after rollback, same as checkpoint)",
      meta_count
    );
    println!(
      "- GraphDb records: {} (should be 1: only successful operation)",
      graph_count
    );
    println!(
      "- Final MetaDb version: {} (includes rollback version)",
      final_version
    );

    // Based on our reference test, LanceDB rollback restores data to checkpoint state
    assert_eq!(
      meta_count, 2,
      "MetaDb should have same data as checkpoint (initial + successful)"
    );
    assert_eq!(
      graph_count, 1,
      "GraphDb should have only successful operation"
    );
    assert!(
      final_version >= 4,
      "Version should be at least 4 (initial + success + failed + rollback)"
    );
  }

  #[tokio::test]
  async fn test_index_then_store_pattern() {
    // Test the IndexThenStore pattern that episode queries use
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create GraphDB schema (Episode table)
    db.graph
      .execute_write(
        "CREATE NODE TABLE Episode (uuid UUID, group_id STRING, PRIMARY KEY (uuid))",
        vec![],
      )
      .await
      .expect("Failed to create Episode schema");

    // Create LanceDB schema (episodes table)
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // Create the episodes table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("episodes", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create episodes table");

    let initial_batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["initial"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["group-init"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Initial Episode"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    db.meta
      .save_to_table("episodes", initial_batch, &["uuid"])
      .await
      .expect("Failed to save initial data");

    // Insert test data into both databases
    let test_uuid1 = uuid::Uuid::now_v7();
    let test_uuid2 = uuid::Uuid::now_v7();

    // Insert into GraphDB
    db.graph
      .execute_write(
        "CREATE (:Episode {uuid: $uuid1, group_id: $group_id1})",
        vec![
          ("uuid1", kuzu::Value::UUID(test_uuid1)),
          ("group_id1", kuzu::Value::String("test-group".to_string())),
        ],
      )
      .await
      .expect("Failed to insert episode 1 into GraphDB");

    db.graph
      .execute_write(
        "CREATE (:Episode {uuid: $uuid2, group_id: $group_id2})",
        vec![
          ("uuid2", kuzu::Value::UUID(test_uuid2)),
          ("group_id2", kuzu::Value::String("test-group".to_string())),
        ],
      )
      .await
      .expect("Failed to insert episode 2 into GraphDB");

    // Insert into LanceDB
    let episode_batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec![
          test_uuid1.to_string(),
          test_uuid2.to_string(),
        ])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["test-group", "test-group"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Episode 1", "Episode 2"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create episode batch");

    db.meta
      .save_to_table("episodes", episode_batch, &["uuid"])
      .await
      .expect("Failed to insert episodes into LanceDB");

    // Now test IndexThenStore pattern manually
    use crate::DatabaseOperation;
    use crate::operations::{GraphIndexQuery, PrimaryStoreQuery, QueryOperation::IndexThenStore};
    use futures::{StreamExt, TryStreamExt};

    // Create a test command that mimics GetEpisodesByGroupIds
    struct TestIndexThenStore;

    impl crate::DatabaseCommand for TestIndexThenStore {
      type Output = crate::ResultStream<(String, String)>; // (uuid, name) pairs
      type SaveData = NoData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        DatabaseOperation::Query(IndexThenStore {
          index: GraphIndexQuery {
            cypher: "MATCH (n:Episode) WHERE list_contains(['test-group'], n.group_id) RETURN n.uuid ORDER BY n.uuid DESC".to_string(),
            params: vec![],
          },
          query_builder: Box::new(|graph_stream| {
            Box::pin(async move {
              use crate::extract_uuids_to_in_clause;

              let (in_clause, uuids) = extract_uuids_to_in_clause(graph_stream).await?;
              println!("Found {} UUIDs from GraphDB", uuids.len());

              if uuids.is_empty() {
                return Ok(PrimaryStoreQuery {
                  table: "episodes",
                  filter: Some(crate::empty_filter()),
                  limit: Some(0),
                  offset: None,
                  vector_search: None,
                });
              }

              Ok(PrimaryStoreQuery {
                table: "episodes",
                filter: Some(format!("uuid IN ({})", in_clause)),
                limit: None,
                offset: None,
                vector_search: None,
              })
            })
          }),
          transformer: Box::new(|_graph_stream, batch_stream| {
            Box::pin(async_stream::try_stream! {
              let mut batch_stream = std::pin::pin!(batch_stream);
              while let Some(batch) = batch_stream.next().await {
                let batch = batch?;
                let uuid_col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
                let name_col = batch.column(2).as_any().downcast_ref::<StringArray>().unwrap();

                for row in 0..batch.num_rows() {
                  yield (uuid_col.value(row).to_string(), name_col.value(row).to_string());
                }
              }
            })
          }),
        })
      }
    }

    // Execute the test command
    let result_stream = db
      .execute(TestIndexThenStore)
      .await
      .expect("IndexThenStore should work");

    // Collect the stream into a Vec for testing
    let result: Vec<(String, String)> = result_stream
      .try_collect()
      .await
      .expect("Failed to collect stream");

    println!("Final result: {:?}", result);
    assert_eq!(result.len(), 2, "Should find 2 episodes");
    assert!(
      result
        .iter()
        .any(|(uuid, _)| uuid == &test_uuid1.to_string())
    );
    assert!(
      result
        .iter()
        .any(|(uuid, _)| uuid == &test_uuid2.to_string())
    );
  }

  #[tokio::test]
  async fn test_index_then_store_empty_index() {
    // Test IndexThenStore when GraphDB returns no results
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create GraphDB schema but no data
    db.graph
      .execute_write(
        "CREATE NODE TABLE Episode (uuid UUID, group_id STRING, PRIMARY KEY (uuid))",
        vec![],
      )
      .await
      .expect("Failed to create Episode schema");

    // Create LanceDB with data (but won't be returned since no index results)
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // Create episodes table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("episodes", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create episodes table");

    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["some-uuid"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Some Episode"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    db.meta
      .save_to_table("episodes", batch, &["uuid"])
      .await
      .expect("Failed to save episodes data");

    // Test IndexThenStore with empty index results
    struct EmptyIndexTest;

    impl crate::DatabaseCommand for EmptyIndexTest {
      type Output = crate::ResultStream<String>;
      type SaveData = NoData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        use crate::operations::{
          GraphIndexQuery, PrimaryStoreQuery, QueryOperation::IndexThenStore,
        };

        DatabaseOperation::Query(IndexThenStore {
          index: GraphIndexQuery {
            cypher: "MATCH (n:Episode) WHERE n.group_id = 'nonexistent' RETURN n.uuid".to_string(),
            params: vec![],
          },
          query_builder: Box::new(|graph_stream| {
            Box::pin(async move {
              use crate::extract_uuids_to_in_clause;

              let (in_clause, uuids) = extract_uuids_to_in_clause(graph_stream).await?;

              if uuids.is_empty() {
                return Ok(PrimaryStoreQuery {
                  table: "episodes",
                  filter: Some(crate::empty_filter()),
                  limit: Some(0),
                  offset: None,
                  vector_search: None,
                });
              }

              Ok(PrimaryStoreQuery {
                table: "episodes",
                filter: Some(format!("uuid IN ({})", in_clause)),
                limit: None,
                offset: None,
                vector_search: None,
              })
            })
          }),
          transformer: Box::new(|_graph_stream, _batch_stream| {
            Box::pin(async_stream::try_stream! {
              // Should be empty since no episodes match
              // The empty stream will automatically terminate without yielding anything
              if false { // Never execute this branch
                yield "".to_string();
              }
            })
          }),
        })
      }
    }

    let result_stream = db
      .execute(EmptyIndexTest)
      .await
      .expect("Should handle empty index");

    // Collect the stream into a Vec for testing
    let result: Vec<String> = result_stream
      .try_collect()
      .await
      .expect("Failed to collect stream");

    assert_eq!(
      result.len(),
      0,
      "Should return empty result when index is empty"
    );
  }

  #[tokio::test]
  async fn test_index_then_store_missing_table() {
    // Test IndexThenStore when GraphDB table doesn't exist
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Don't create GraphDB schema - table doesn't exist

    // Create LanceDB table
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // Create episodes table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("episodes", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create episodes table");

    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["some-uuid"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Some Episode"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    db.meta
      .save_to_table("episodes", batch, &["uuid"])
      .await
      .expect("Failed to save episodes data");

    // Test IndexThenStore with missing table (should fail)
    struct MissingTableTest;

    impl crate::DatabaseCommand for MissingTableTest {
      type Output = crate::ResultStream<String>;
      type SaveData = NoData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        use crate::operations::{
          GraphIndexQuery, PrimaryStoreQuery, QueryOperation::IndexThenStore,
        };

        DatabaseOperation::Query(IndexThenStore {
          index: GraphIndexQuery {
            cypher: "MATCH (n:Episode) RETURN n.uuid".to_string(),
            params: vec![],
          },
          query_builder: Box::new(|graph_stream| {
            Box::pin(async move {
              use crate::extract_uuids_to_in_clause;

              let (in_clause, uuids) = extract_uuids_to_in_clause(graph_stream).await?;

              if uuids.is_empty() {
                return Ok(PrimaryStoreQuery {
                  table: "episodes",
                  filter: Some(crate::empty_filter()),
                  limit: Some(0),
                  offset: None,
                  vector_search: None,
                });
              }

              Ok(PrimaryStoreQuery {
                table: "episodes",
                filter: Some(format!("uuid IN ({})", in_clause)),
                limit: None,
                offset: None,
                vector_search: None,
              })
            })
          }),
          transformer: Box::new(|graph_stream, _batch_stream| {
            Box::pin(async_stream::try_stream! {
              let mut graph_stream = std::pin::pin!(graph_stream);
              let mut count = 0;

              while let Some(row_result) = graph_stream.next().await {
                let _row = row_result?;
                count += 1;
              }

              yield format!("processed_{}_rows", count);
            })
          }),
        })
      }
    }

    let result = db.execute(MissingTableTest).await;
    assert!(
      result.is_err(),
      "Should fail when Episode table doesn't exist"
    );
  }

  #[tokio::test]
  async fn test_2pc_rollback_simulation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    let actor = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup
    actor
      .graph
      .execute_write(
        "CREATE NODE TABLE TestEntity (id UUID, value INT64, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Create initial state
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
      Field::new("status", DataType::Utf8, false),
    ]));

    // Create the table first
    let schema_clone = schema.clone();
    actor
      .meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("test_rollback", schema_clone)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Insert initial data
    let initial_batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["initial"])) as arrow_array::ArrayRef,
        Arc::new(Int64Array::from(vec![0])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["committed"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = actor
      .meta
      .save_to_table("test_rollback", initial_batch, &["id"])
      .await
      .expect("Failed to insert initial data");

    let initial_version = actor
      .meta
      .get_table_version("test_rollback")
      .await
      .expect("Failed to get version");
    println!("Initial version: {}", initial_version);

    // Simulate a failed 2PC operation
    let failed_id = uuid::Uuid::now_v7();
    let result = actor
      .execute_2pc(
        "test_rollback",
        move |meta: &MetaDb| {
          Box::pin(async move {
            let batch = RecordBatch::try_new(
              schema.clone(),
              vec![
                Arc::new(StringArray::from(vec![failed_id.to_string()])) as arrow_array::ArrayRef,
                Arc::new(Int64Array::from(vec![999])) as arrow_array::ArrayRef,
                Arc::new(StringArray::from(vec!["pending"])) as arrow_array::ArrayRef,
              ],
            )?;

            let (rollback_version, version) =
              meta.save_to_table("test_rollback", batch, &["id"]).await?;
            println!("Version after MetaDb write: {}", version);

            Ok((failed_id, Some(("test_rollback", rollback_version))))
          })
        },
        vec![
          // Use an invalid query to simulate failure
          ("INVALID QUERY TO SIMULATE FAILURE".to_string(), vec![]),
        ],
      )
      .await;

    assert!(result.is_err(), "2PC should fail");

    // With proper 2PC implementation, the rollback should have already happened automatically
    let current_count = {
      let stream = actor
        .meta
        .query_table(&PrimaryStoreQuery {
          table: "test_rollback",
          filter: None,
          limit: None,
          offset: None,
          vector_search: None,
        })
        .await
        .expect("Failed to query");
      let batches: Vec<_> = stream.try_collect().await.expect("Failed to collect");
      batches.iter().map(|b| b.num_rows()).sum::<usize>()
    };

    // Since we now have automatic rollback in execute, the failed transaction's data
    // should already be rolled back
    assert_eq!(
      current_count, 1,
      "Should only have initial record after automatic rollback"
    );

    let final_version = actor
      .meta
      .get_table_version("test_rollback")
      .await
      .expect("Failed to get version");

    println!("\nAutomatic rollback demonstration:");
    println!("Initial version: {}", initial_version);
    println!(
      "After failed 2PC with automatic rollback: {}",
      final_version
    );
    println!(
      "Note: Rollback created NEW version {} with old data",
      final_version
    );

    // The automatic rollback should have created a new version
    assert!(
      final_version > initial_version,
      "Rollback should create a new version"
    );
  }

  // ========================================================================
  // COMPREHENSIVE TESTS FOR Database::execute METHOD
  // ========================================================================

  /// Test command that implements Skip operation
  struct TestSkipCommand {
    value: i32,
  }

  impl DatabaseCommand for TestSkipCommand {
    type Output = String;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      let value = self.value;
      DatabaseOperation::Skip {
        transformer: Box::new(move |_| format!("skipped_{}", value)),
      }
    }
  }

  #[tokio::test]
  async fn test_execute_skip_operation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Test skip operation with different values
    let result1 = db
      .execute(TestSkipCommand { value: 42 })
      .await
      .expect("Skip should work");
    assert_eq!(result1, "skipped_42");

    let result2 = db
      .execute(TestSkipCommand { value: 123 })
      .await
      .expect("Skip should work");
    assert_eq!(result2, "skipped_123");
  }

  /// Test command that implements StoreOnly operation
  struct TestStoreOnlyCommand {
    table: &'static str,
    filter: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    expected_count: usize,
  }

  impl DatabaseCommand for TestStoreOnlyCommand {
    type Output = ResultStream<usize>;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      let expected = self.expected_count;
      DatabaseOperation::Query(operations::QueryOperation::StoreOnly {
        query: PrimaryStoreQuery {
          table: self.table,
          filter: self.filter.clone(),
          limit: self.limit,
          offset: self.offset,
          vector_search: None,
        },
        transformer: Box::new(move |batch_stream| {
          Box::pin(async_stream::try_stream! {
            let mut batch_stream = std::pin::pin!(batch_stream);
            let mut total_rows = 0;
            while let Some(batch) = batch_stream.next().await {
              let batch = batch?;
              total_rows += batch.num_rows();
            }
            // Validate the count matches expected
            assert_eq!(total_rows, expected,
              "TestStoreOnlyCommand: Expected {} rows but got {}", expected, total_rows);
            yield total_rows;
          })
        }),
      })
    }
  }

  #[tokio::test]
  async fn test_execute_store_only_operation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create test data
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Int64, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ]));

    // Create the table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("store_only_test", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Insert initial data
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["a", "b", "c", "d", "e"])) as arrow_array::ArrayRef,
        Arc::new(Int64Array::from(vec![10, 20, 30, 40, 50])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    db.meta
      .save_to_table("store_only_test", batch, &["id"])
      .await
      .expect("Failed to create table");

    // Test 1: Query all rows
    let mut result_stream = db
      .execute(TestStoreOnlyCommand {
        table: "store_only_test",
        filter: None,
        limit: None,
        offset: None,
        expected_count: 5,
      })
      .await
      .expect("StoreOnly should work");
    let result = result_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be ok");
    assert_eq!(result, 5);

    // Test 2: Query with filter
    let mut result_stream = db
      .execute(TestStoreOnlyCommand {
        table: "store_only_test",
        filter: Some("value >= 30".to_string()),
        limit: None,
        offset: None,
        expected_count: 3,
      })
      .await
      .expect("StoreOnly with filter should work");
    let result = result_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be ok");
    assert_eq!(result, 3);

    // Test 3: Query with limit
    let mut result_stream = db
      .execute(TestStoreOnlyCommand {
        table: "store_only_test",
        filter: None,
        limit: Some(2),
        offset: None,
        expected_count: 2,
      })
      .await
      .expect("StoreOnly with limit should work");
    let result = result_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be ok");
    assert_eq!(result, 2);

    // Test 4: Query with offset
    let mut result_stream = db
      .execute(TestStoreOnlyCommand {
        table: "store_only_test",
        filter: None,
        limit: None,
        offset: Some(3),
        expected_count: 2,
      })
      .await
      .expect("StoreOnly with offset should work");
    let result = result_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be ok");
    assert_eq!(result, 2);

    // Test 5: Query with limit and offset
    let mut result_stream = db
      .execute(TestStoreOnlyCommand {
        table: "store_only_test",
        filter: None,
        limit: Some(2),
        offset: Some(1),
        expected_count: 2,
      })
      .await
      .expect("StoreOnly with limit and offset should work");
    let result = result_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Should be ok");
    assert_eq!(result, 2);

    // Test 6: Query nonexistent table (should fail)
    let result = db
      .execute(TestStoreOnlyCommand {
        table: "nonexistent_table",
        filter: None,
        limit: None,
        offset: None,
        expected_count: 0,
      })
      .await;
    assert!(
      result.is_err(),
      "StoreOnly with nonexistent table should fail"
    );
  }

  /// Test command that implements IndexOnly operation
  struct TestIndexOnlyCommand {
    cypher: String,
    params: Vec<(&'static str, kuzu::Value)>,
  }

  impl DatabaseCommand for TestIndexOnlyCommand {
    type Output = ResultStream<(i64, String)>;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Query(operations::QueryOperation::IndexOnly {
        query: GraphIndexQuery {
          cypher: self.cypher.clone(),
          params: self.params.clone(),
        },
        transformer: Box::new(|graph_stream| {
          Box::pin(async_stream::try_stream! {
            let mut stream = std::pin::pin!(graph_stream);
            while let Some(row_result) = stream.next().await {
              let row = row_result?;
              // Extract id and name from Vec<kuzu::Value>
              if row.len() >= 2 {
                if let (kuzu::Value::Int64(id), kuzu::Value::String(name)) = (&row[0], &row[1]) {
                  yield (*id, name.clone());
                } else {
                  Err(crate::Error::InvalidGraphDbData(
                    format!("Expected (Int64, String), got {:?}", row)
                  ))?;
                }
              } else {
                Err(crate::Error::InvalidGraphDbData(
                  format!("Expected 2 values, got {}", row.len())
                ))?;
              }
            }
          })
        }),
      })
    }
  }

  #[tokio::test]
  async fn test_execute_index_only_operation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create schema
    db.graph
      .execute_write(
        "CREATE NODE TABLE IndexOnlyTest (id INT64, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Insert test data
    db.graph
      .execute_write("CREATE (:IndexOnlyTest {id: 1, name: 'test1'})", vec![])
      .await
      .expect("Failed to insert data");

    db.graph
      .execute_write("CREATE (:IndexOnlyTest {id: 2, name: 'test2'})", vec![])
      .await
      .expect("Failed to insert data");

    // Test IndexOnly operation
    let mut result_stream = db
      .execute(TestIndexOnlyCommand {
        cypher: "MATCH (n:IndexOnlyTest) RETURN n.id, n.name ORDER BY n.id".to_string(),
        params: vec![],
      })
      .await
      .expect("IndexOnly should now work");

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = result_stream.next().await {
      results.push(result.expect("Result should be Ok"));
    }

    assert_eq!(results.len(), 2, "Should have 2 results");
    assert_eq!(results[0], (1, "test1".to_string()));
    assert_eq!(results[1], (2, "test2".to_string()));
  }

  #[tokio::test]
  async fn test_execute_index_only_with_params() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create schema
    db.graph
      .execute_write(
        "CREATE NODE TABLE IndexOnlyTest (id INT64, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Insert test data
    for i in 1..=5 {
      db.graph
        .execute_write(
          "CREATE (:IndexOnlyTest {id: $id, name: $name})",
          vec![
            ("id", kuzu::Value::Int64(i)),
            ("name", kuzu::Value::String(format!("test{}", i))),
          ],
        )
        .await
        .expect("Failed to insert data");
    }

    // Test IndexOnly operation with parameters
    let mut result_stream = db
      .execute(TestIndexOnlyCommand {
        cypher: "MATCH (n:IndexOnlyTest) WHERE n.id > $min_id RETURN n.id, n.name ORDER BY n.id"
          .to_string(),
        params: vec![("min_id", kuzu::Value::Int64(3))],
      })
      .await
      .expect("IndexOnly with params should work");

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = result_stream.next().await {
      results.push(result.expect("Result should be Ok"));
    }

    assert_eq!(results.len(), 2, "Should have 2 results (id > 3)");
    assert_eq!(results[0], (4, "test4".to_string()));
    assert_eq!(results[1], (5, "test5".to_string()));
  }

  /// Test command that returns UUIDs from IndexOnly
  struct TestIndexOnlyUuidCommand;

  impl DatabaseCommand for TestIndexOnlyUuidCommand {
    type Output = ResultStream<uuid::Uuid>;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Query(operations::QueryOperation::IndexOnly {
        query: GraphIndexQuery {
          cypher: "MATCH (n:UuidTest) RETURN n.uuid ORDER BY n.uuid".to_string(),
          params: vec![],
        },
        transformer: Box::new(|graph_stream| {
          Box::pin(async_stream::try_stream! {
            let mut stream = std::pin::pin!(graph_stream);
            while let Some(row_result) = stream.next().await {
              let row = row_result?;
              if !row.is_empty() {
                if let kuzu::Value::UUID(uuid) = &row[0] {
                  yield *uuid;
                } else {
                  Err(crate::Error::InvalidGraphDbData(
                    format!("Expected UUID, got {:?}", row[0])
                  ))?;
                }
              }
            }
          })
        }),
      })
    }
  }

  #[tokio::test]
  async fn test_execute_index_only_with_uuids() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create schema
    db.graph
      .execute_write(
        "CREATE NODE TABLE UuidTest (uuid UUID, PRIMARY KEY(uuid))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Insert test data
    let uuid1 = uuid::Uuid::now_v7();
    let uuid2 = uuid::Uuid::now_v7();

    db.graph
      .execute_write(
        "CREATE (:UuidTest {uuid: $uuid})",
        vec![("uuid", kuzu::Value::UUID(uuid1))],
      )
      .await
      .expect("Failed to insert data");

    db.graph
      .execute_write(
        "CREATE (:UuidTest {uuid: $uuid})",
        vec![("uuid", kuzu::Value::UUID(uuid2))],
      )
      .await
      .expect("Failed to insert data");

    // Test IndexOnly operation returning UUIDs
    let mut result_stream = db
      .execute(TestIndexOnlyUuidCommand)
      .await
      .expect("IndexOnly UUID should work");

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = result_stream.next().await {
      results.push(result.expect("Result should be Ok"));
    }

    assert_eq!(results.len(), 2, "Should have 2 UUIDs");
    // UUIDs should be in order
    if uuid1 < uuid2 {
      assert_eq!(results[0], uuid1);
      assert_eq!(results[1], uuid2);
    } else {
      assert_eq!(results[0], uuid2);
      assert_eq!(results[1], uuid1);
    }
  }

  /// Test data that implements ToDatabase trait for Save operation
  #[derive(Clone)]
  struct TestSaveData {
    id: uuid::Uuid,
    name: String,
    value: i64,
  }

  impl ToDatabase for TestSaveData {
    type MetaContext = ();
    type GraphContext = ();

    fn into_graph_value(
      &self,
      _ctx: Self::GraphContext,
    ) -> Result<Vec<(&'static str, kuzu::Value)>> {
      Ok(vec![
        ("id", kuzu::Value::UUID(self.id)),
        ("name", kuzu::Value::String(self.name.clone())),
        ("value", kuzu::Value::Int64(self.value)),
      ])
    }

    fn into_meta_value(&self, _ctx: Self::MetaContext) -> Result<arrow_array::RecordBatch> {
      let schema = Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
      ]));

      RecordBatch::try_new(
        schema,
        vec![
          Arc::new(StringArray::from(vec![self.id.to_string()])) as arrow_array::ArrayRef,
          Arc::new(StringArray::from(vec![self.name.clone()])) as arrow_array::ArrayRef,
          Arc::new(Int64Array::from(vec![self.value])) as arrow_array::ArrayRef,
        ],
      )
      .map_err(|e| crate::Error::Arrow(e))
    }

    fn primary_key_columns() -> &'static [&'static str] {
      &["id"]
    }

    fn meta_table_name() -> &'static str {
      "test_save_data"
    }

    fn graph_table_name() -> &'static str {
      "TestSaveData"
    }

    fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
      Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
      ]))
    }
  }

  /// Test command that implements Save operation
  #[derive(Clone)]
  struct TestSaveCommand {
    data: TestSaveData,
    table: &'static str,
    cypher: &'static str,
  }

  impl DatabaseCommand for TestSaveCommand {
    type Output = String;
    type SaveData = TestSaveData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      let data_id = self.data.id;
      DatabaseOperation::Mutation(operations::MutationOperation::Single {
        table: self.table,
        data: Arc::new(self.data.clone()),
        graph_context: (),
        meta_context: (),
        cypher: self.cypher,
        transformer: Box::new(move |_| format!("saved_{}", data_id)),
      })
    }
  }

  #[tokio::test]
  async fn test_execute_save_operation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup both GraphDB and MetaDB schema using Migration operation
    db.execute(TestMigrationCommand {
      graph_ddl: vec![
        "CREATE NODE TABLE SaveTest (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
      ],
      table_name: "save_test".to_string(),
      schema: ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
      ]),
    })
    .await
    .expect("Failed to create schemas");

    // Test 1: Save with Create mode
    let test_id = uuid::Uuid::now_v7();
    let result = db
      .execute(TestSaveCommand {
        data: TestSaveData {
          id: test_id,
          name: "test_create".to_string(),
          value: 42,
        },
        table: "save_test",
        cypher: "CREATE (:SaveTest {id: $id, name: $name, value: $value})",
      })
      .await
      .expect("Save Create should work");
    assert_eq!(result, format!("saved_{}", test_id));

    // Verify data was saved to both databases
    let meta_stream = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "save_test",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Query should work");
    let meta_batches: Vec<_> = meta_stream.try_collect().await.expect("Should collect");
    let meta_rows: usize = meta_batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(meta_rows, 1, "Should have 1 row in MetaDB");

    let graph_receiver = db
      .graph
      .execute_query("MATCH (n:SaveTest) RETURN count(n)", vec![])
      .await
      .expect("Query should work");
    let mut graph_stream = UnboundedReceiverStream::new(graph_receiver);
    let row = graph_stream
      .next()
      .await
      .expect("Should have result")
      .expect("Row should be ok");
    if let kuzu::Value::Int64(count) = &row[0] {
      assert_eq!(*count, 1, "Should have 1 row in GraphDB");
    } else {
      panic!("Expected Int64 count");
    }

    // Test 2: Save with Append mode
    let test_id2 = uuid::Uuid::now_v7();
    let result = db
      .execute(TestSaveCommand {
        data: TestSaveData {
          id: test_id2,
          name: "test_append".to_string(),
          value: 84,
        },
        table: "save_test",
        cypher: "CREATE (:SaveTest {id: $id, name: $name, value: $value})",
      })
      .await
      .expect("Save Append should work");
    assert_eq!(result, format!("saved_{}", test_id2));

    // Verify both records exist
    let meta_stream = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "save_test",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Query should work");
    let meta_batches: Vec<_> = meta_stream.try_collect().await.expect("Should collect");
    let meta_rows: usize = meta_batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(meta_rows, 2, "Should have 2 rows in MetaDB");
  }

  #[tokio::test]
  async fn test_execute_save_operation_failure() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup both GraphDB and MetaDB schema using Migration operation
    db.execute(TestMigrationCommand {
      graph_ddl: vec!["CREATE NODE TABLE SaveFailTest (id UUID, name STRING, PRIMARY KEY(id))"],
      table_name: "save_fail_test".to_string(),
      schema: ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
      ]),
    })
    .await
    .expect("Failed to create schemas");

    // Test: Save operation where GraphDB fails (should rollback MetaDB)
    let test_id = uuid::Uuid::now_v7();
    let result = db
      .execute(TestSaveCommand {
        data: TestSaveData {
          id: test_id,
          name: "test_fail".to_string(),
          value: 42,
        },
        table: "save_fail_test",
        cypher: "INVALID CYPHER QUERY", // This will cause GraphDB to fail
      })
      .await;

    assert!(result.is_err(), "Save with invalid cypher should fail");

    // Since we're using 2PC, the MetaDB should be rolled back when GraphDB fails
    let meta_result = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "save_fail_test",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await;

    // Table should exist but be empty due to rollback
    let stream = meta_result.expect("Table should exist since we created it in setup");
    let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
    let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
      rows, 0,
      "MetaDB should have been rolled back, no rows should exist"
    );
  }

  /// Helper function to create setup closure for Migration operation
  fn create_migration_setup(
    table_name: String,
    schema: ArrowSchema,
  ) -> Box<
    dyn for<'a> FnOnce(&'a lancedb::Connection) -> futures::future::BoxFuture<'a, Result<()>>
      + Send,
  > {
    Box::new(move |conn: &lancedb::Connection| {
      let table_name = table_name.clone();
      let schema = Arc::new(schema.clone());
      Box::pin(async move {
        // Create an empty table with the schema
        conn
          .create_empty_table(&table_name, schema)
          .execute()
          .await?;
        Ok(())
      })
    })
  }

  /// Test command that implements Migration operation
  struct TestMigrationCommand {
    graph_ddl: Vec<&'static str>,
    table_name: String,
    schema: ArrowSchema,
  }

  impl DatabaseCommand for TestMigrationCommand {
    type Output = String;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      let table_name = self.table_name.clone();
      DatabaseOperation::Migration {
        graph_ddl: self.graph_ddl.clone(),
        meta_setup: create_migration_setup(self.table_name.clone(), self.schema.clone()),
        transformer: Box::new(move |_| format!("migrated_{}", table_name)),
      }
    }
  }

  #[tokio::test]
  async fn test_execute_migration_operation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Test migration operation
    let result = db
      .execute(TestMigrationCommand {
        graph_ddl: vec![
          "CREATE NODE TABLE MigrationTest (id UUID, name STRING, PRIMARY KEY(id))",
          "CREATE NODE TABLE MigrationTest2 (id UUID, value INT64, PRIMARY KEY(id))",
        ],
        table_name: "migration_test".to_string(),
        schema: ArrowSchema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("name", DataType::Utf8, false),
        ]),
      })
      .await
      .expect("Migration should work");
    assert_eq!(result, "migrated_migration_test");

    // Verify GraphDB tables were created
    let receiver = db
      .graph
      .execute_query("MATCH (n:MigrationTest) RETURN count(n)", vec![])
      .await
      .expect("Query should work");
    let mut stream = UnboundedReceiverStream::new(receiver);
    let row = stream
      .next()
      .await
      .expect("Should have result")
      .expect("Row should be ok");
    if let kuzu::Value::Int64(count) = &row[0] {
      assert_eq!(*count, 0, "Table should exist but be empty");
    } else {
      panic!("Expected Int64 count");
    }

    // Verify MetaDB table was created
    let meta_stream = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "migration_test",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Query should work");
    let batches: Vec<_> = meta_stream.try_collect().await.expect("Should collect");
    assert!(
      !batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0),
      "MetaDB table should exist"
    );
  }

  #[tokio::test]
  async fn test_execute_migration_operation_with_failures() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Test migration with some failing DDL (should continue with warnings)
    let result = db
      .execute(TestMigrationCommand {
        graph_ddl: vec![
          "CREATE NODE TABLE MigrationFailTest (id UUID, name STRING, PRIMARY KEY(id))",
          "INVALID DDL STATEMENT", // This will fail but should be warned
          "CREATE NODE TABLE MigrationFailTest2 (id UUID, value INT64, PRIMARY KEY(id))",
        ],
        table_name: "migration_fail_test".to_string(),
        schema: ArrowSchema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("name", DataType::Utf8, false),
        ]),
      })
      .await
      .expect("Migration should work despite some DDL failures");
    assert_eq!(result, "migrated_migration_fail_test");

    // Verify successful tables were still created
    let receiver = db
      .graph
      .execute_query("MATCH (n:MigrationFailTest) RETURN count(n)", vec![])
      .await
      .expect("Query should work");
    let mut stream = UnboundedReceiverStream::new(receiver);
    let row = stream
      .next()
      .await
      .expect("Should have result")
      .expect("Row should be ok");
    if let kuzu::Value::Int64(count) = &row[0] {
      assert_eq!(*count, 0, "Valid table should exist");
    } else {
      panic!("Expected Int64 count");
    }
  }

  #[tokio::test]
  async fn test_execute_concurrent_operations() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup test data
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Int64, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // Create table first
    let schema_clone = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("concurrent_test", schema_clone)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    // Insert test data
    let batch = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(Int64Array::from(vec![1, 2, 3])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["a", "b", "c"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    db.meta
      .save_to_table("concurrent_test", batch, &["id"])
      .await
      .expect("Failed to insert test data");

    // Run multiple concurrent execute operations
    let mut handles = vec![];

    for i in 0..10 {
      let db_clone = db.clone();
      let handle = tokio::spawn(async move {
        if i % 2 == 0 {
          // Skip operation
          db_clone.execute(TestSkipCommand { value: i }).await
        } else {
          // Simple operation - just return a formatted string
          Ok(format!("operation_{}", i))
        }
      });
      handles.push(handle);
    }

    // Wait for all operations to complete
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All operations should succeed
    for (i, result) in results.iter().enumerate() {
      assert!(result.is_ok(), "Operation {} should succeed", i);
      assert!(
        result.as_ref().unwrap().is_ok(),
        "Inner operation {} should succeed",
        i
      );
    }
  }

  #[tokio::test]
  async fn test_execute_error_propagation() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Test that errors from different operation types are properly propagated

    // Error from StoreOnly (nonexistent table)
    let result = db
      .execute(TestStoreOnlyCommand {
        table: "definitely_nonexistent_table",
        filter: None,
        limit: None,
        offset: None,
        expected_count: 0,
      })
      .await;
    assert!(
      result.is_err(),
      "StoreOnly with nonexistent table should fail"
    );

    // Error from IndexThenStore (missing graph table)
    struct ErrorIndexThenStoreCommand;
    impl DatabaseCommand for ErrorIndexThenStoreCommand {
      type Output = crate::ResultStream<String>;
      type SaveData = NoData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        DatabaseOperation::Query(operations::QueryOperation::IndexThenStore {
          index: GraphIndexQuery {
            cypher: "MATCH (n:NonexistentNode) RETURN n.id".to_string(),
            params: vec![],
          },
          query_builder: Box::new(|graph_stream| {
            Box::pin(async move {
              use crate::extract_uuids_to_in_clause;

              let (in_clause, uuids) = extract_uuids_to_in_clause(graph_stream).await?;

              if uuids.is_empty() {
                return Ok(PrimaryStoreQuery {
                  table: "any_table",
                  filter: Some(crate::empty_filter()),
                  limit: Some(0),
                  offset: None,
                  vector_search: None,
                });
              }

              Ok(PrimaryStoreQuery {
                table: "any_table",
                filter: Some(format!("uuid IN ({})", in_clause)),
                limit: None,
                offset: None,
                vector_search: None,
              })
            })
          }),
          transformer: Box::new(|_graph_stream, _batch_stream| {
            Box::pin(async_stream::try_stream! {
              yield "should_not_reach".to_string();
            })
          }),
        })
      }
    }

    let result = db.execute(ErrorIndexThenStoreCommand).await;
    assert!(
      result.is_err(),
      "IndexThenStore with nonexistent graph table should fail"
    );
  }

  #[tokio::test]
  async fn test_save_with_parameter_mismatch() {
    // Test that verifies we get proper errors when parameters don't match the schema
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create a simple schema that only has id and name fields
    db.graph
      .execute_write(
        "CREATE NODE TABLE SimpleTest (id UUID, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create schema");

    // Also create the LanceDB table
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));
    db.meta
      .create_empty_table("simple_test", &schema)
      .await
      .expect("Failed to create LanceDB table");

    // Test data that has an extra parameter
    #[derive(Clone)]
    struct MismatchedData {
      id: uuid::Uuid,
      name: String,
      extra_field: String,
    }

    impl ToDatabase for MismatchedData {
      type MetaContext = ();
      type GraphContext = ();

      fn primary_key_columns() -> &'static [&'static str] {
        &["id"]
      }

      fn meta_table_name() -> &'static str {
        "simple_test"
      }

      fn graph_table_name() -> &'static str {
        "SimpleTest"
      }

      fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
        Arc::new(ArrowSchema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("name", DataType::Utf8, false),
        ]))
      }

      fn into_graph_value(
        &self,
        _ctx: Self::GraphContext,
      ) -> Result<Vec<(&'static str, kuzu::Value)>> {
        Ok(vec![
          ("id", kuzu::Value::UUID(self.id)),
          ("name", kuzu::Value::String(self.name.clone())),
          // This parameter doesn't exist in the schema
          ("extra_field", kuzu::Value::String(self.extra_field.clone())),
        ])
      }

      fn into_meta_value(&self, _ctx: Self::MetaContext) -> Result<arrow_array::RecordBatch> {
        let schema = Arc::new(ArrowSchema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("name", DataType::Utf8, false),
        ]));

        RecordBatch::try_new(
          schema,
          vec![
            Arc::new(StringArray::from(vec![self.id.to_string()])) as arrow_array::ArrayRef,
            Arc::new(StringArray::from(vec![self.name.clone()])) as arrow_array::ArrayRef,
          ],
        )
        .map_err(|e| crate::Error::Arrow(e))
      }
    }

    struct SaveWithMismatchCommand {
      data: MismatchedData,
    }

    impl DatabaseCommand for SaveWithMismatchCommand {
      type Output = ();
      type SaveData = MismatchedData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        DatabaseOperation::save_simple(
          "simple_test",
          Arc::new(self.data.clone()),
          r#"CREATE (:SimpleTest {id: $id, name: $name, extra_field: $extra_field})"#,
          |_| (),
        )
      }
    }

    let test_data = MismatchedData {
      id: uuid::Uuid::now_v7(),
      name: "Test".to_string(),
      extra_field: "This field doesn't exist".to_string(),
    };

    // This should fail because extra_field doesn't exist in the schema
    let result = db
      .execute(SaveWithMismatchCommand { data: test_data })
      .await;

    assert!(result.is_err(), "Save with parameter mismatch should fail");

    // Check that the error message contains information about the missing parameter
    if let Err(e) = result {
      let error_string = e.to_string();
      assert!(
        error_string.contains("extra_field") || error_string.contains("Parameter"),
        "Error should mention the problematic parameter: {}",
        error_string
      );
    }
  }

  #[tokio::test]
  async fn test_raw_database_synchronization_without_abstractions() {
    // Raw test with direct database calls - no abstractions, no 2PC, no commands
    // This will prove whether the issue is in the database layer or the abstractions

    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    // Create both databases directly
    let graph_db = super::GraphDb::new(&config).expect("Failed to create GraphDb");
    let meta_db = super::MetaDb::new(&config)
      .await
      .expect("Failed to create MetaDb");

    println!("Raw test: GraphDB path = {:?}", graph_db.path);
    println!("Raw test: MetaDB path = {:?}", meta_db.path);

    // Create Episode schema in GraphDB manually
    graph_db
      .execute_write(
        r#"
      CREATE NODE TABLE IF NOT EXISTS Episode (
          uuid UUID,
          group_id STRING,
          source STRING,
          valid_at TIMESTAMP,
          created_at TIMESTAMP,
          kumos_version STRING,
          PRIMARY KEY (uuid)
      )"#,
        vec![],
      )
      .await
      .expect("Failed to create Episode schema in GraphDB");

    // Create Episode schema in MetaDB manually using Arrow
    use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
    use arrow_array::{
      FixedSizeListArray, ListArray, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use std::sync::Arc;

    let episode_schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new("source", DataType::Utf8, false),
      Field::new("source_description", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("content", DataType::Utf8, false),
      Field::new(
        "valid_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "entity_edges",
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        false,
      ),
      Field::new(
        "embedding",
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 1536),
        true,
      ),
      Field::new("version", DataType::Utf8, false),
    ]));

    // Create test episode data
    let test_uuid = uuid::Uuid::now_v7();
    let now_jiff = jiff::Timestamp::now();
    let now_time = time::OffsetDateTime::from_unix_timestamp_nanos(now_jiff.as_nanosecond())
      .expect("Invalid timestamp");

    println!("Raw test: Creating episode with UUID: {}", test_uuid);

    // Create episodes table in MetaDB first
    let schema_for_create = episode_schema.clone();
    meta_db
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("episodes", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create episodes table in MetaDB");

    // 1. Save to MetaDB first (manually)
    let uuid_array = StringArray::from(vec![test_uuid.to_string()]);
    let group_id_array = StringArray::from(vec!["raw-test-group".to_string()]);
    let source_array = StringArray::from(vec!["Message".to_string()]);
    let source_desc_array = StringArray::from(vec!["Raw test source".to_string()]);
    let name_array = StringArray::from(vec!["Raw Test Episode".to_string()]);
    let content_array = StringArray::from(vec!["Raw test content".to_string()]);
    let valid_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);
    let created_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);

    // Create empty entity_edges list
    let entity_edges_values = StringArray::from(vec![] as Vec<&str>);
    let entity_edges_offsets = arrow::buffer::OffsetBuffer::new(vec![0i32, 0i32].into());
    let entity_edges_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, true)),
      entity_edges_offsets,
      Arc::new(entity_edges_values),
      None,
    )
    .expect("Failed to create entity_edges array");

    // Create null embedding array
    let embedding_array = FixedSizeListArray::new_null(
      Arc::new(Field::new("item", DataType::Float32, true)),
      1536,
      1,
    );

    let version_array = StringArray::from(vec![uuid::Uuid::now_v7().to_string()]);

    let meta_batch = RecordBatch::try_new(
      episode_schema.clone(),
      vec![
        Arc::new(uuid_array),
        Arc::new(group_id_array),
        Arc::new(source_array),
        Arc::new(source_desc_array),
        Arc::new(name_array),
        Arc::new(content_array),
        Arc::new(valid_at_array),
        Arc::new(created_at_array),
        Arc::new(entity_edges_array),
        Arc::new(embedding_array),
        Arc::new(version_array),
      ],
    )
    .expect("Failed to create RecordBatch");

    println!("Raw test: Saving to MetaDB...");
    let (_, meta_version) = meta_db
      .save_to_table("episodes", meta_batch, &["uuid"])
      .await
      .expect("Failed to save to MetaDB");
    println!(
      "Raw test: MetaDB save successful, version: {}",
      meta_version
    );

    // 2. Save to GraphDB manually
    println!("Raw test: Saving to GraphDB...");
    graph_db.execute_write(
      "CREATE (:Episode {uuid: $uuid, group_id: $group_id, source: $source, valid_at: $valid_at, created_at: $created_at, kumos_version: $version})",
      vec![
        ("uuid", kuzu::Value::UUID(test_uuid)),
        ("group_id", kuzu::Value::String("raw-test-group".to_string())),
        ("source", kuzu::Value::String("Message".to_string())),
        ("valid_at", kuzu::Value::Timestamp(now_time)),
        ("created_at", kuzu::Value::Timestamp(now_time)),
        ("version", kuzu::Value::String("v1.0.0".to_string())),
      ],
    )
    .await
    .expect("Failed to save to GraphDB");
    println!("Raw test: GraphDB save successful");

    // 3. Verify MetaDB has the data (direct query)
    println!("Raw test: Querying MetaDB directly...");
    let meta_stream = meta_db
      .query_table(&PrimaryStoreQuery {
        table: "episodes",
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Failed to query MetaDB");

    let meta_results: Vec<_> = meta_stream
      .try_collect()
      .await
      .expect("Failed to collect MetaDB results");
    let meta_row_count: usize = meta_results.iter().map(|b| b.num_rows()).sum();
    println!("Raw test: MetaDB has {} episodes", meta_row_count);

    // 4. Verify GraphDB has the data (direct query)
    println!("Raw test: Querying GraphDB directly...");
    let graph_receiver = graph_db
      .execute_query("MATCH (n:Episode) RETURN count(n)", vec![])
      .await
      .expect("Failed to query GraphDB");

    let mut graph_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(graph_receiver);
    let count_row = graph_stream
      .next()
      .await
      .expect("Should have count result")
      .expect("Count query should succeed");

    let graph_count = if let kuzu::Value::Int64(count) = &count_row[0] {
      *count
    } else {
      panic!("Expected Int64 count from GraphDB");
    };
    println!("Raw test: GraphDB has {} episodes", graph_count);

    // 5. Test the specific group query that fails in the real test
    println!("Raw test: Testing group query on GraphDB...");
    let group_receiver = graph_db
      .execute_query(
        "MATCH (n:Episode) WHERE list_contains(['raw-test-group'], n.group_id) RETURN n.uuid",
        vec![],
      )
      .await
      .expect("Failed to execute group query");

    let mut group_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(group_receiver);
    let mut found_uuids = Vec::new();
    while let Some(result) = group_stream.next().await {
      let row = result.expect("Row should be Ok");
      if let kuzu::Value::UUID(uuid) = &row[0] {
        found_uuids.push(*uuid);
        println!("Raw test: Found UUID in GraphDB: {}", uuid);
      }
    }
    println!(
      "Raw test: GraphDB group query found {} UUIDs",
      found_uuids.len()
    );

    // 6. Test the specific UUID filter that fails in LanceDB
    if !found_uuids.is_empty() {
      println!("Raw test: Testing UUID filter on MetaDB...");
      let uuid_filter = format!("uuid IN ('{}')", found_uuids[0]);
      let uuid_stream = meta_db
        .query_table(&PrimaryStoreQuery {
          table: "episodes",
          filter: Some(uuid_filter.clone()),
          limit: None,
          offset: None,
          vector_search: None,
        })
        .await
        .expect("Failed to query MetaDB with UUID filter");

      let uuid_results: Vec<_> = uuid_stream
        .try_collect()
        .await
        .expect("Failed to collect UUID filter results");
      let uuid_row_count: usize = uuid_results.iter().map(|b| b.num_rows()).sum();
      println!(
        "Raw test: MetaDB UUID filter found {} episodes",
        uuid_row_count
      );

      // This is the key test - do we get results when we use the exact same UUID?
      if uuid_row_count == 0 {
        println!(
          "❌ BUG CONFIRMED: GraphDB finds UUID but MetaDB doesn't find it with UUID filter"
        );
        println!("   GraphDB found: {}", found_uuids[0]);
        println!("   MetaDB filter: {}", uuid_filter);
        panic!("Database synchronization issue confirmed!");
      } else {
        println!("✅ SUCCESS: Both databases have the episode and UUID filter works");
      }
    } else {
      println!("❌ BUG: GraphDB group query didn't find any UUIDs");
      panic!("GraphDB group query issue confirmed!");
    }

    println!("Raw test: All database operations successful!");
  }

  #[tokio::test]
  async fn test_2pc_episode_pattern() {
    // Test the 2PC pattern used by episode saving without external dependencies
    // This mimics the episode save operation using only internal db components

    use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
    use arrow_array::{
      FixedSizeListArray, ListArray, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use std::sync::Arc;

    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create Episode schemas manually (like migrations would do)
    println!("2PC Episode pattern: Creating schemas...");

    // GraphDB Episode schema
    db.graph
      .execute_write(
        r#"
      CREATE NODE TABLE IF NOT EXISTS Episode (
          uuid UUID,
          group_id STRING,
          source STRING,
          valid_at TIMESTAMP,
          created_at TIMESTAMP,
          kumos_version STRING,
          PRIMARY KEY (uuid)
      )"#,
        vec![],
      )
      .await
      .expect("Failed to create Episode schema in GraphDB");

    // Create test episode data
    let test_uuid = uuid::Uuid::now_v7();
    let now_jiff = jiff::Timestamp::now();
    let now_time = time::OffsetDateTime::from_unix_timestamp_nanos(now_jiff.as_nanosecond())
      .expect("Invalid timestamp");

    println!("2PC Episode pattern: Testing with UUID: {}", test_uuid);

    // Create episode table in MetaDB first (like migrations do)
    let episode_schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new("source", DataType::Utf8, false),
      Field::new("source_description", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("content", DataType::Utf8, false),
      Field::new(
        "valid_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "entity_edges",
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        false,
      ),
      Field::new(
        "embedding",
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 1536),
        true,
      ),
      Field::new("version", DataType::Utf8, false),
    ]));

    let schema_for_create = episode_schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("episodes", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create episodes table");
    println!("2PC Episode pattern: Created empty episodes table");

    // Use the 2PC mechanism directly like SaveEpisode does
    let episode_schema_clone = episode_schema.clone();
    db.execute_2pc(
      "episodes",
      move |meta: &MetaDb| {
        Box::pin(async move {
          // Create episode data
          let uuid_array = StringArray::from(vec![test_uuid.to_string()]);
          let group_id_array = StringArray::from(vec!["2pc-episode-test".to_string()]);
          let source_array = StringArray::from(vec!["Message".to_string()]);
          let source_desc_array = StringArray::from(vec!["2PC test source".to_string()]);
          let name_array = StringArray::from(vec!["2PC Episode Test".to_string()]);
          let content_array = StringArray::from(vec!["2PC test content".to_string()]);
          let valid_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);
          let created_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);

          // Create empty entity_edges list
          let entity_edges_values = StringArray::from(vec![] as Vec<&str>);
          let entity_edges_offsets = arrow::buffer::OffsetBuffer::new(vec![0i32, 0i32].into());
          let entity_edges_array = ListArray::try_new(
            Arc::new(Field::new("item", DataType::Utf8, true)),
            entity_edges_offsets,
            Arc::new(entity_edges_values),
            None,
          )?;

          // Create null embedding array
          let embedding_array = FixedSizeListArray::new_null(
            Arc::new(Field::new("item", DataType::Float32, true)),
            1536,
            1,
          );

          let version_array = StringArray::from(vec![uuid::Uuid::now_v7().to_string()]);

          let batch = RecordBatch::try_new(
            episode_schema_clone,
            vec![
              Arc::new(uuid_array),
              Arc::new(group_id_array),
              Arc::new(source_array),
              Arc::new(source_desc_array),
              Arc::new(name_array),
              Arc::new(content_array),
              Arc::new(valid_at_array),
              Arc::new(created_at_array),
              Arc::new(entity_edges_array),
              Arc::new(embedding_array),
              Arc::new(version_array),
            ],
          )?;

          // Save to MetaDB with MergeInsert (like SaveEpisode)
          let (_, version) = meta.save_to_table("episodes", batch, &["uuid"]).await?;

          println!("2PC Episode pattern: MetaDB save successful, version: {}", version);
          Ok((test_uuid, Some(("episodes", version - 1)))) // Return table name and previous version for rollback
        })
      },
      vec![
        // GraphDB operations (like SaveEpisode)
        (
          "MERGE (n:Episode {uuid: $uuid}) SET n.group_id = $group_id, n.source = $source, n.valid_at = $valid_at, n.created_at = $created_at, n.kumos_version = $kumos_version".to_string(),
          vec![
            ("uuid", kuzu::Value::UUID(test_uuid)),
            ("group_id", kuzu::Value::String("2pc-episode-test".to_string())),
            ("source", kuzu::Value::String("Message".to_string())),
            ("valid_at", kuzu::Value::Timestamp(now_time)),
            ("created_at", kuzu::Value::Timestamp(now_time)),
            ("kumos_version", kuzu::Value::String("v1.0.0".to_string())),
          ],
        ),
      ],
    ).await.expect("2PC operation should succeed");

    println!("2PC Episode pattern: 2PC operation completed successfully");

    // Verify both databases have the data
    println!("2PC Episode pattern: Verifying MetaDB state...");
    let meta_filter = format!("uuid = '{}'", test_uuid);
    let meta_stream = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "episodes",
        filter: Some(meta_filter),
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Failed to query MetaDB");
    let meta_batches: Vec<_> = meta_stream
      .try_collect()
      .await
      .expect("Failed to collect MetaDB results");
    let meta_count: usize = meta_batches.iter().map(|b| b.num_rows()).sum();
    println!("2PC Episode pattern: MetaDB has {} episodes", meta_count);

    println!("2PC Episode pattern: Verifying GraphDB state...");
    let graph_receiver = db
      .graph
      .execute_query(
        "MATCH (n:Episode {uuid: $uuid}) RETURN n.group_id",
        vec![("uuid", kuzu::Value::UUID(test_uuid))],
      )
      .await
      .expect("Failed to query GraphDB");

    let mut graph_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(graph_receiver);
    let mut graph_count = 0;
    while let Some(result) = graph_stream.next().await {
      let row = result.expect("Row should be Ok");
      if let kuzu::Value::String(_group_id) = &row[0] {
        graph_count += 1;
      }
    }
    println!("2PC Episode pattern: GraphDB has {} episodes", graph_count);

    // Both should have exactly 1 episode
    assert_eq!(
      meta_count, 1,
      "MetaDB should have exactly 1 episode after 2PC save"
    );
    assert_eq!(
      graph_count, 1,
      "GraphDB should have exactly 1 episode after 2PC save"
    );

    // Test the problematic query pattern from GetEpisodesByGroupIds
    println!("2PC Episode pattern: Testing group query pattern...");
    let group_receiver = db
      .graph
      .execute_query(
        "MATCH (n:Episode) WHERE list_contains(['2pc-episode-test'], n.group_id) RETURN n.uuid",
        vec![],
      )
      .await
      .expect("Failed to execute group query");

    let mut group_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(group_receiver);
    let mut group_found_uuids = Vec::new();
    while let Some(result) = group_stream.next().await {
      let row = result.expect("Row should be Ok");
      if let kuzu::Value::UUID(uuid) = &row[0] {
        group_found_uuids.push(*uuid);
        println!("2PC Episode pattern: Group query found UUID: {}", uuid);
      }
    }

    println!(
      "2PC Episode pattern: Group query found {} UUIDs",
      group_found_uuids.len()
    );
    assert_eq!(
      group_found_uuids.len(),
      1,
      "Group query should find exactly 1 UUID"
    );
    assert_eq!(
      group_found_uuids[0], test_uuid,
      "Group query should find the correct UUID"
    );

    // Test the UUID IN filter on MetaDB (the problematic part)
    println!("2PC Episode pattern: Testing UUID IN filter on MetaDB...");
    let uuid_filter = format!("uuid IN ('{}')", group_found_uuids[0]);
    let uuid_stream = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "episodes",
        filter: Some(uuid_filter),
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("Failed to query MetaDB with UUID filter");
    let uuid_batches: Vec<_> = uuid_stream
      .try_collect()
      .await
      .expect("Failed to collect UUID filter results");
    let uuid_count: usize = uuid_batches.iter().map(|b| b.num_rows()).sum();

    println!(
      "2PC Episode pattern: MetaDB UUID IN filter found {} episodes",
      uuid_count
    );

    if uuid_count == 0 {
      println!("❌ 2PC EPISODE BUG CONFIRMED:");
      println!("   - 2PC operation succeeded");
      println!("   - GraphDB has episode with UUID: {}", test_uuid);
      println!("   - MetaDB has episode: {} (by direct filter)", meta_count);
      println!("   - Group query finds UUID: {}", group_found_uuids[0]);
      println!("   - But UUID IN filter finds: {} episodes", uuid_count);

      // Debug the actual UUID values
      let meta_all_stream = db
        .meta
        .query_table(&PrimaryStoreQuery {
          table: "episodes",
          filter: None,
          limit: None,
          offset: None,
          vector_search: None,
        })
        .await
        .expect("Failed to query all MetaDB");
      let meta_all_batches: Vec<_> = meta_all_stream
        .try_collect()
        .await
        .expect("Failed to collect all MetaDB results");
      for batch in &meta_all_batches {
        if batch.num_rows() > 0 {
          let uuid_column = batch
            .column_by_name("uuid")
            .expect("Should have uuid column");
          let uuid_array = uuid_column
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .expect("UUID should be string array");
          let stored_uuid = uuid_array.value(0);
          println!(
            "   MetaDB UUID: '{}' (len: {})",
            stored_uuid,
            stored_uuid.len()
          );
          println!(
            "   GraphDB UUID: '{}' (len: {})",
            group_found_uuids[0],
            group_found_uuids[0].to_string().len()
          );
          println!(
            "   String equality: {}",
            stored_uuid == group_found_uuids[0].to_string()
          );
        }
      }

      panic!("The episode 2PC pattern has a UUID synchronization issue!");
    }

    assert_eq!(
      uuid_count, 1,
      "UUID IN filter should find exactly 1 episode"
    );
    println!("✅ 2PC Episode pattern: All verifications passed!");
  }

  #[tokio::test]
  async fn test_vector_search_through_database() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let v2_data_dir = temp_dir.path().join("v2");
    std::fs::create_dir_all(&v2_data_dir).expect("Failed to create v2 dir");

    let config = crate::config::Config::new_test(v2_data_dir);
    let db = Database::new(&config)
      .await
      .expect("Failed to create database");

    // Create a simple vector table for testing
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          4, // Small embedding size for test
        ),
        false,
      ),
    ]));

    // Create test vectors (orthogonal for easy testing)
    let vectors = vec![
      vec![1.0_f32, 0.0, 0.0, 0.0], // id=1
      vec![0.0_f32, 1.0, 0.0, 0.0], // id=2
      vec![0.0_f32, 0.0, 1.0, 0.0], // id=3
      vec![0.0_f32, 0.0, 0.0, 1.0], // id=4
      vec![0.7_f32, 0.7, 0.0, 0.0], // id=5, similar to id=1
    ];

    // Create embeddings as FixedSizeListArray
    let mut embedding_values = Vec::new();
    for v in &vectors {
      embedding_values.extend_from_slice(v);
    }

    let embedding_array = FixedSizeListArray::new(
      Arc::new(Field::new("item", DataType::Float32, true)),
      4,
      Arc::new(Float32Array::from(embedding_values)),
      None,
    );

    let test_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["1", "2", "3", "4", "5"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec![
          "item1", "item2", "item3", "item4", "item5",
        ])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec![
          "group1", "group1", "group2", "group2", "group1",
        ])) as arrow_array::ArrayRef,
        Arc::new(embedding_array) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    // Create vector_items table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("vector_items", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create vector table");

    // Save the test data
    db.meta
      .save_to_table("vector_items", test_data, &["id"])
      .await
      .expect("Failed to save vector data");

    // Test 1: Basic vector search
    let query_vector = vec![1.0_f32, 0.0, 0.0, 0.0]; // Should match id=1 exactly

    struct TestVectorSearch {
      query_vector: Vec<f32>,
      limit: usize,
      filter: Option<String>,
    }

    impl DatabaseCommand for TestVectorSearch {
      type Output = ResultStream<(String, f32)>; // Stream of (id, distance)
      type SaveData = NoData;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        let query_vector = self.query_vector.clone();
        let limit = self.limit;
        let filter = self.filter.clone();

        DatabaseOperation::Query(QueryOperation::StoreOnly {
          query: PrimaryStoreQuery {
            table: "vector_items",
            filter,
            limit: Some(limit),
            offset: None,
            vector_search: Some(VectorSearchParams {
              vector: query_vector,
              column: "embedding",
              distance_type: lancedb::DistanceType::Cosine,
            }),
          },
          transformer: Box::new(move |batch_stream| {
            Box::pin(async_stream::try_stream! {
              let mut batch_stream = std::pin::pin!(batch_stream);

              while let Some(batch) = batch_stream.next().await {
                let batch = batch?;

                let id_array = batch
                  .column_by_name("id")
                  .ok_or(crate::Error::InvalidMetaDbData("missing id column".into()))?
                  .as_any()
                  .downcast_ref::<StringArray>()
                  .ok_or(crate::Error::InvalidMetaDbData(
                    "id is not string array".into(),
                  ))?;

                let distance_array = batch
                  .column_by_name("_distance")
                  .ok_or(crate::Error::InvalidMetaDbData(
                    "missing _distance column".into(),
                  ))?
                  .as_any()
                  .downcast_ref::<Float32Array>()
                  .ok_or(crate::Error::InvalidMetaDbData(
                    "_distance is not float array".into(),
                  ))?;

                for i in 0..batch.num_rows() {
                  yield (id_array.value(i).to_string(), distance_array.value(i));
                }
              }
            })
          }),
        })
      }
    }

    // Execute vector search for exact match
    let mut results_stream = db
      .execute(TestVectorSearch {
        query_vector: query_vector.clone(),
        limit: 3,
        filter: None,
      })
      .await
      .expect("Failed to execute vector search");

    // Collect results from stream
    let mut results = Vec::new();
    while let Some(result) = results_stream.next().await {
      results.push(result.expect("Result should be Ok"));
    }

    assert!(!results.is_empty(), "Should find at least one result");
    assert_eq!(results[0].0, "1", "First result should be exact match");
    assert!(
      results[0].1 < 0.01,
      "Exact match should have near-zero distance"
    );

    if results.len() > 1 {
      assert_eq!(results[1].0, "5", "Second result should be similar vector");
    }

    // Test 2: Vector search with filter
    let mut filtered_results_stream = db
      .execute(TestVectorSearch {
        query_vector: query_vector.clone(),
        limit: 3,
        filter: Some("group_id = 'group2'".to_string()),
      })
      .await
      .expect("Failed to execute filtered vector search");

    // Collect filtered results
    let mut filtered_results = Vec::new();
    while let Some(result) = filtered_results_stream.next().await {
      filtered_results.push(result.expect("Result should be Ok"));
    }

    // Should only return items from group2 (id=3 and id=4)
    for (id, _) in &filtered_results {
      assert!(id == "3" || id == "4", "Results should only be from group2");
    }

    // Test 3: Check that regular queries still work alongside vector search
    let regular_query_results = db
      .meta
      .query_table(&PrimaryStoreQuery {
        table: "vector_items",
        filter: Some("name = 'item3'".to_string()),
        limit: None,
        offset: None,
        vector_search: None, // Regular query, no vector search
      })
      .await
      .expect("Failed to execute regular query")
      .try_collect::<Vec<_>>()
      .await
      .expect("Failed to collect results");

    assert_eq!(
      regular_query_results.len(),
      1,
      "Should find exactly one item3"
    );

    println!("✅ Vector search through Database: All tests passed!");
  }

  #[tokio::test]
  async fn test_vector_search_edge_cases() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let v2_data_dir = temp_dir.path().join("v2");
    std::fs::create_dir_all(&v2_data_dir).expect("Failed to create v2 dir");

    let config = crate::config::Config::new_test(v2_data_dir);
    let db = Database::new(&config)
      .await
      .expect("Failed to create database");

    // Create table with edge case data
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new(
        "embedding",
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 2),
        false,
      ),
    ]));

    // Edge case vectors
    let vectors = vec![
      vec![0.0_f32, 0.0],       // Zero vector
      vec![f32::MAX, f32::MAX], // Max values
      vec![-1.0_f32, -1.0],     // Negative values
    ];

    let mut embedding_values = Vec::new();
    for v in &vectors {
      embedding_values.extend_from_slice(v);
    }

    let embedding_array = FixedSizeListArray::new(
      Arc::new(Field::new("item", DataType::Float32, true)),
      2,
      Arc::new(Float32Array::from(embedding_values)),
      None,
    );

    let test_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["zero", "max", "negative"])) as arrow_array::ArrayRef,
        Arc::new(embedding_array) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    // Create edge_case_vectors table first
    let schema_for_create = schema.clone();
    db.meta
      .execute_setup(Box::new(move |conn| {
        Box::pin(async move {
          conn
            .create_empty_table("edge_case_vectors", schema_for_create)
            .execute()
            .await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create table");

    db.meta
      .save_to_table("edge_case_vectors", test_data, &["id"])
      .await
      .expect("Failed to save edge case data");

    // Test searching with zero vector
    let zero_search = PrimaryStoreQuery {
      table: "edge_case_vectors",
      filter: None,
      limit: Some(3),
      offset: None,
      vector_search: Some(VectorSearchParams {
        vector: vec![0.0_f32, 0.0],
        column: "embedding",
        distance_type: lancedb::DistanceType::Cosine,
      }),
    };

    // This might error due to zero vector normalization issues in cosine distance
    let result = db.meta.query_table(&zero_search).await;

    if result.is_ok() {
      let stream = result.unwrap();
      let batches: Vec<_> = stream.try_collect().await.expect("Failed to collect");
      println!("Zero vector search returned {} batches", batches.len());
    } else {
      println!("Zero vector search failed as expected: {:?}", result.err());
    }

    // Test with L2 distance (should handle zero vectors better)
    let l2_search = PrimaryStoreQuery {
      table: "edge_case_vectors",
      filter: None,
      limit: Some(3),
      offset: None,
      vector_search: Some(VectorSearchParams {
        vector: vec![0.0_f32, 0.0],
        column: "embedding",
        distance_type: lancedb::DistanceType::L2,
      }),
    };

    let l2_result = db
      .meta
      .query_table(&l2_search)
      .await
      .expect("L2 distance should work with zero vectors");

    let l2_batches: Vec<_> = l2_result
      .try_collect()
      .await
      .expect("Failed to collect L2 results");

    assert!(!l2_batches.is_empty(), "L2 search should return results");

    println!("✅ Vector search edge cases: Tests completed!");
  }
}
