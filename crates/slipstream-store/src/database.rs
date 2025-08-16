// Ensure moved tests in src/database are compiled as unit tests
#[cfg(test)]
mod tests;

use std::sync::Arc;

use futures::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::Result;
use crate::config::Config;
use crate::graph::GraphDb;
use crate::meta::MetaDb;
use crate::operations::{self, DatabaseOperation};
use crate::traits::{DatabaseCommand, ToDatabase};

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
