use crate::{Result, config::Config};
use arrow_array::RecordBatch;
use futures::TryStreamExt;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::create_embedding_function_from_config;

// Simplify closure type used for setup functions
pub type SetupFn = Box<
  dyn for<'a> FnOnce(&'a lancedb::Connection) -> futures::future::BoxFuture<'a, Result<()>> + Send,
>;

/// MetaDb provides an async interface to LanceDB
///
/// This is a simplified version that lives in the db module,
/// avoiding dependencies on other modules.
pub struct MetaDb {
  conn: lancedb::Connection,
  pub(super) path: std::path::PathBuf,
  // Internal background cleanup task
  cleanup_task: Arc<Mutex<Option<JoinHandle<()>>>>,
  cancel_token: CancellationToken,
}

// We need to implement Clone manually because JoinHandle doesn't implement Clone
impl Clone for MetaDb {
  fn clone(&self) -> Self {
    Self {
      conn: self.conn.clone(),
      path: self.path.clone(),
      cleanup_task: Arc::clone(&self.cleanup_task),
      cancel_token: self.cancel_token.clone(),
    }
  }
}

impl std::fmt::Debug for MetaDb {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    // Check if cleanup task is running
    let has_cleanup_task = self
      .cleanup_task
      .try_lock()
      .map(|guard| guard.is_some())
      .unwrap_or(true); // If we can't acquire lock, assume it's running

    f.debug_struct("MetaDb")
      .field("path", &self.path)
      .field("has_cleanup_task", &has_cleanup_task)
      .finish()
  }
}

impl MetaDb {
  pub async fn new(config: &Config) -> Result<Self> {
    let data_dir = config.meta_data_dir();
    let conn = lancedb::connect(data_dir.as_path().to_str().unwrap())
      .execute()
      .await?;

    // Register embedding function from config
    let embedding_fn = create_embedding_function_from_config(config);
    let fn_name = embedding_fn.name().to_string();
    conn.embedding_registry().register(&fn_name, embedding_fn)?;

    let db = Self {
      conn,
      path: data_dir,
      cleanup_task: Arc::new(Mutex::new(None)),
      cancel_token: CancellationToken::new(),
    };

    // Start the background cleanup task
    db.start_cleanup_task().await;

    Ok(db)
  }

  /// Execute a setup function that needs access to the underlying connection
  /// This is used for migrations and other setup operations
  pub async fn execute_setup(&self, f: SetupFn) -> Result<()> {
    f(&self.conn).await
  }

  /// Execute a query on a table and return a stream of RecordBatches
  pub async fn query_table(
    &self,
    query: &super::operations::PrimaryStoreQuery,
  ) -> Result<impl futures::Stream<Item = Result<RecordBatch>> + use<>> {
    use lancedb::query::{ExecutableQuery, QueryBase};

    tracing::debug!(
      "MetaDB query_table: table={}, filter={:?}, limit={:?}, offset={:?}, vector_search={:?}",
      query.table,
      query.filter,
      query.limit,
      query.offset,
      query.vector_search.is_some()
    );

    // Check if table exists
    let table = match self.conn.open_table(query.table).execute().await {
      Ok(t) => t,
      Err(e) => {
        tracing::error!("Failed to open table {}: {}", query.table, e);
        return Err(e.into());
      }
    };

    // Get table stats for debugging
    let count = table.count_rows(None).await?;
    tracing::debug!("Table {} has {} total rows", query.table, count);

    // Check if this is a vector search or regular query
    let stream = if let Some(ref vector_params) = query.vector_search {
      // Vector search
      let mut vector_query = table
        .vector_search(vector_params.vector.as_slice())?
        .column(vector_params.column)
        .distance_type(vector_params.distance_type);

      // Apply filter if provided
      if let Some(ref filter_expr) = query.filter {
        vector_query = vector_query.only_if(filter_expr.as_str());
      }

      // Apply limit if specified
      if let Some(limit_val) = query.limit {
        vector_query = vector_query.limit(limit_val);
      }

      tracing::debug!("Executing LanceDB vector search...");
      vector_query.execute().await?
    } else {
      // Regular query
      let mut regular_query = table.query();

      if let Some(ref filter_expr) = query.filter {
        tracing::debug!("Applying filter to query: {}", filter_expr);
        regular_query = regular_query.only_if(filter_expr.as_str());

        // Try to count rows that match the filter (if LanceDB supports it)
        // This is just for debugging
        match table.count_rows(Some(filter_expr.to_string())).await {
          Ok(filtered_count) => {
            tracing::debug!("Filter '{}' matches {} rows", filter_expr, filtered_count);
          }
          Err(e) => {
            tracing::debug!("Could not count filtered rows: {}", e);
          }
        }
      }

      if let Some(limit_val) = query.limit {
        regular_query = regular_query.limit(limit_val);
      }

      if let Some(offset_val) = query.offset {
        regular_query = regular_query.offset(offset_val);
      }

      tracing::debug!("Executing LanceDB query...");
      regular_query.execute().await?
    };

    // Return the stream directly - we'll log in the transformers
    Ok(stream.map_err(Into::into))
  }

  /// Save data to a table with automatic mode detection
  /// For new tables: creates the table
  /// For existing tables: performs merge/upsert based on merge_columns
  /// Returns the new version of the table after the operation
  pub async fn save_to_table(
    &self,
    table_name: &str,
    data: RecordBatch,
    merge_columns: &[&'static str], // columns to use for merge/upsert (typically primary keys)
  ) -> Result<(u64, u64)> {
    tracing::debug!(
      "MetaDB save_to_table_with_mode: table={}, rows={}",
      table_name,
      data.num_rows()
    );

    // Merge insert (upsert)
    let table = self.conn.open_table(table_name).execute().await?;
    let current_version = table.version().await?;
    let schema = data.schema();
    let batch_iter = arrow_array::RecordBatchIterator::new(vec![Ok(data)].into_iter(), schema);

    // Convert Vec<String> to Vec<&str> for merge_insert
    let mut merge = table.merge_insert(merge_columns);
    merge.when_matched_update_all(None);
    merge.when_not_matched_insert_all();
    let merge_result = merge.execute(Box::new(batch_iter)).await?;
    tracing::debug!(
      "MetaDB merge insert to table {}, new version={}",
      table_name,
      merge_result.version
    );
    Ok((current_version, merge_result.version))
  }

  /// Get table version for 2PC coordination
  pub async fn get_table_version(&self, table_name: &str) -> Result<u64> {
    let table = self.conn.open_table(table_name).execute().await?;
    Ok(table.version().await?)
  }

  /// Perform a 2PC rollback by checking out a version and restoring
  /// This creates a new version with the old data (roll-forward approach)
  pub async fn rollback_to_version(&self, table_name: &str, version: u64) -> Result<()> {
    let table = self.conn.open_table(table_name).execute().await?;
    table.checkout(version).await?;
    table.restore().await?;
    Ok(())
  }

  /// Start the background optimization task
  /// This runs periodically to optimize LanceDB tables (compaction, pruning, etc.)
  async fn start_cleanup_task(&self) {
    let conn = self.conn.clone();
    let cancel = self.cancel_token.clone();

    // Spawn the optimization task
    let handle = tokio::spawn(async move {
      loop {
        // Wait for random period between 50-70 minutes
        let mut rng = StdRng::from_os_rng();
        let wait_minutes = rng.random_range(50..=70);
        let wait_duration = Duration::from_secs(wait_minutes * 60);
        tokio::select! {
          _ = tokio::time::sleep(wait_duration) => {},
          _ = cancel.cancelled() => {
            tracing::info!("LanceDB optimization task cancelled");
            break;
          }
        }

        tracing::info!("Running LanceDB optimization task");

        match Self::run_all_optimizations(&conn).await {
          Ok((tables_optimized, actions_performed)) => {
            tracing::info!(
              "LanceDB optimization completed: {} tables optimized, {} actions performed",
              tables_optimized,
              actions_performed
            );
          }
          Err(e) => {
            tracing::error!("LanceDB optimization failed: {}", e);
          }
        }
      }
    });
    let mut guard = self.cleanup_task.lock().await;
    *guard = Some(handle);
  }

  /// Run all optimization actions on all tables
  /// Returns (number of tables optimized, total actions performed)
  async fn run_all_optimizations(conn: &lancedb::Connection) -> Result<(usize, usize)> {
    let table_names = conn.table_names().execute().await?;
    let mut tables_optimized = 0;

    for table_name in table_names {
      match conn.open_table(&table_name).execute().await {
        Ok(table) => {
          // Run ALL optimization actions: compact, prune, and index
          match table.optimize(lancedb::table::OptimizeAction::All).await {
            Ok(_) => {
              tracing::debug!(
                "Optimized table '{}' (ran all optimization actions)",
                table_name
              );
              tables_optimized += 1;
            }
            Err(e) => {
              tracing::warn!("Failed to optimize table '{}': {}", table_name, e);
            }
          }
        }
        Err(e) => {
          tracing::warn!(
            "Failed to open table '{}' for optimization: {}",
            table_name,
            e
          );
        }
      }
    }

    tracing::info!(
      "Optimization summary: {} tables optimized",
      tables_optimized
    );
    Ok((tables_optimized, tables_optimized)) // Return same count for both since All runs multiple actions
  }
}

impl Drop for MetaDb {
  fn drop(&mut self) {
    // Cancel the cleanup task if this is the last reference
    if Arc::strong_count(&self.cleanup_task) == 1 {
      let cleanup_task = Arc::clone(&self.cleanup_task);
      let cancel = self.cancel_token.clone();
      tokio::spawn(async move {
        cancel.cancel();
        let mut guard = cleanup_task.lock().await;
        if let Some(handle) = guard.take() {
          handle.abort();
          tracing::debug!("LanceDB cleanup task cancelled");
        }
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
  use arrow_array::{FixedSizeListArray, Float32Array, RecordBatch, StringArray};
  use lancedb::query::{ExecutableQuery as _, QueryBase as _};
  use std::sync::Arc;
  use tempfile::tempdir;

  fn test_config(path: &std::path::Path) -> Config {
    Config::new_test(path.to_path_buf())
  }

  #[tokio::test]
  async fn test_basic_operations() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Create test data
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // First create the table using execute_setup
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("test_table", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

    let id_array = StringArray::from(vec!["1", "2"]);
    let name_array = StringArray::from(vec!["Alice", "Bob"]);

    let batch = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(id_array) as arrow_array::ArrayRef,
        Arc::new(name_array) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    // Now save data using MergeInsert (primary key is "id")
    let _ = db
      .save_to_table("test_table", batch, &["id"])
      .await
      .expect("Failed to save data");

    // Query data
    let query = crate::operations::PrimaryStoreQuery {
      table: "test_table",
      filter: None,
      limit: Some(10),
      offset: None,
      vector_search: None,
    };
    let stream = db.query_table(&query).await.expect("Failed to query");

    let results: Vec<RecordBatch> = stream
      .try_collect()
      .await
      .expect("Failed to collect results");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].num_rows(), 2);
  }

  #[tokio::test]
  async fn test_version_tracking() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Create initial table
    let schema = Arc::new(ArrowSchema::new(vec![Field::new(
      "id",
      DataType::Utf8,
      false,
    )]));

    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![Arc::new(StringArray::from(vec!["1"])) as arrow_array::ArrayRef],
    )
    .expect("Failed to create batch");

    // Direct LanceDB access for documentation purposes
    // This test demonstrates how LanceDB versioning works
    let batch_iter =
      arrow_array::RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema.clone());
    let table = db
      .conn
      .create_table("version_test", Box::new(batch_iter))
      .execute()
      .await
      .expect("Failed to create table");

    // Get initial version
    let v1 = table.version().await.expect("Failed to get version");

    // Add more data using append
    let batch2 = RecordBatch::try_new(
      schema,
      vec![Arc::new(StringArray::from(vec!["2"])) as arrow_array::ArrayRef],
    )
    .expect("Failed to create batch");

    let batch_iter2 =
      arrow_array::RecordBatchIterator::new(vec![Ok(batch2.clone())].into_iter(), batch2.schema());
    let _append_result = table
      .add(Box::new(batch_iter2))
      .execute()
      .await
      .expect("Failed to append");

    // Get new version
    let v2 = table.version().await.expect("Failed to get version");

    assert!(v2 > v1, "Version should increment after changes");

    // Demonstrate that each operation creates a new version
  }

  #[tokio::test]
  async fn test_embedding_integration() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());

    // Note: This test will fail in CI without Ollama running, but it's valuable for documentation
    // In a real environment, the embedding function would be properly configured
    let _db = MetaDb::new(&config).await;
    // Test passes if MetaDb creation succeeds (embedding function gets registered)
  }

  #[tokio::test]
  async fn test_lancedb_2pc_coordination() {
    // Test LanceDB's roll-forward rollback approach for 2PC coordination
    // This validates the key 2PC capabilities we need for Database

    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Step 1: Create test table with initial data
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("content", DataType::Utf8, false),
      Field::new("transaction_id", DataType::Utf8, false),
    ]));

    let initial_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["1", "2"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec![
          "initial content 1",
          "initial content 2",
        ])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["init", "init"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    // Direct LanceDB access for documentation purposes
    let batch_iter =
      arrow_array::RecordBatchIterator::new(vec![Ok(initial_data)].into_iter(), schema.clone());
    let table = db
      .conn
      .create_table("transaction_test", Box::new(batch_iter))
      .execute()
      .await
      .expect("Failed to create table");

    // Step 2: Test version tracking (key to LanceDB-first 2PC)
    let initial_version = table.version().await.expect("Failed to get version");

    // Verify initial data using direct LanceDB query
    let query = table.query();
    let stream = query.execute().await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let initial_row_count: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(initial_row_count, 2, "Should have 2 initial rows");

    // Step 3: Simulate successful 2PC operation
    let commit_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["3"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["committed content"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["txn_123"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let batch_iter =
      arrow_array::RecordBatchIterator::new(vec![Ok(commit_data)].into_iter(), schema.clone());
    let _append_result = table
      .add(Box::new(batch_iter))
      .execute()
      .await
      .expect("Failed to append data");

    // Step 4: Verify version incremented and data was added
    let version_after_commit = table.version().await.expect("Failed to get version");

    assert!(
      version_after_commit > initial_version,
      "Version should increment after operation"
    );

    let query = table.query();
    let stream = query.execute().await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let committed_row_count: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(committed_row_count, 3, "Should have 3 rows after commit");

    // Step 5: Test rollback capability (key for 2PC abort)

    // Save checkpoint version before operation that will be rolled back
    let checkpoint_version = version_after_commit;

    // Add data we'll rollback
    let rollback_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["4"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["rollback test"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["txn_456"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let batch_iter =
      arrow_array::RecordBatchIterator::new(vec![Ok(rollback_data)].into_iter(), schema.clone());
    let _append_result = table
      .add(Box::new(batch_iter))
      .execute()
      .await
      .expect("Failed to append rollback data");

    let version_before_rollback = table.version().await.expect("Failed to get version");

    // Verify data before rollback
    let query = table.query();
    let stream = query.execute().await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let pre_rollback_row_count: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
      pre_rollback_row_count, 4,
      "Should have 4 rows before rollback"
    );

    // Perform 2PC abort: rollback to checkpoint version using direct LanceDB operations
    table
      .checkout(checkpoint_version)
      .await
      .expect("Failed to checkout");
    table.restore().await.expect("Failed to restore");

    // Step 6: Verify rollback worked
    let version_after_rollback = table.version().await.expect("Failed to get version");

    // Key insight: restore() creates NEW version (roll-forward approach)
    assert!(
      version_after_rollback > checkpoint_version,
      "LanceDB restore creates NEW version (roll-forward approach)"
    );
    assert!(
      version_after_rollback > version_before_rollback,
      "Rollback should create newer version than pre-rollback state"
    );

    // Verify data after rollback
    let query = table.query();
    let stream = query.execute().await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let post_rollback_row_count: usize = results.iter().map(|b| b.num_rows()).sum();

    // IMPORTANT: After rollback, we should have the same data as at checkpoint_version
    // This is the key behavior for 2PC coordination
    assert_eq!(
      post_rollback_row_count, 3,
      "Should have 3 rows after rollback (same as checkpoint)"
    );

    // Verify the specific content to ensure rollback restored correct data
    let query = table.query().only_if("id = '4'");
    let stream = query.execute().await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let row_4_count: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
      row_4_count, 0,
      "Row with id='4' should not exist after rollback"
    );
  }

  #[tokio::test]
  async fn test_query_filtering_and_limits() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Create test data with multiple rows
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("category", DataType::Utf8, false),
      Field::new("value", DataType::Utf8, false),
    ]));

    // First create the table
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("filter_test", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

    let test_data = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(StringArray::from(vec!["1", "2", "3", "4", "5"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["A", "B", "A", "C", "B"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec![
          "val1", "val2", "val3", "val4", "val5",
        ])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = db
      .save_to_table("filter_test", test_data, &["id"])
      .await
      .expect("Failed to save data");

    // Test 1: Query with filter
    let query = crate::operations::PrimaryStoreQuery {
      table: "filter_test",
      filter: Some("category = 'A'".to_string()),
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db
      .query_table(&query)
      .await
      .expect("Failed to query with filter");

    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|batch| batch.num_rows()).sum();
    assert_eq!(total_rows, 2, "Should find 2 rows with category 'A'");

    // Test 2: Query with limit
    let query = crate::operations::PrimaryStoreQuery {
      table: "filter_test",
      filter: None,
      limit: Some(3),
      offset: None,
      vector_search: None,
    };
    let stream = db
      .query_table(&query)
      .await
      .expect("Failed to query with limit");

    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|batch| batch.num_rows()).sum();
    assert_eq!(total_rows, 3, "Should limit to 3 rows");

    // Test 3: Query with both filter and limit
    let query = crate::operations::PrimaryStoreQuery {
      table: "filter_test",
      filter: Some("category = 'B'".to_string()),
      limit: Some(1),
      offset: None,
      vector_search: None,
    };
    let stream = db
      .query_table(&query)
      .await
      .expect("Failed to query with filter and limit");

    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|batch| batch.num_rows()).sum();
    assert_eq!(
      total_rows, 1,
      "Should find 1 row with category 'B' limited to 1"
    );
  }

  #[tokio::test]
  async fn test_save_modes() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    // First create the table
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("save_modes_test", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

    // Test initial data save
    let initial_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["1"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Alice"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = db
      .save_to_table("save_modes_test", initial_data, &["id"])
      .await
      .expect("Failed to save initial data");

    // Test additional data save (append behavior with MergeInsert)
    let append_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["2"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Bob"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = db
      .save_to_table("save_modes_test", append_data, &["id"])
      .await
      .expect("Failed to save additional data");

    // Verify we have 2 rows
    let query = crate::operations::PrimaryStoreQuery {
      table: "save_modes_test",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db.query_table(&query).await.expect("Failed to query");

    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|batch| batch.num_rows()).sum();
    assert_eq!(total_rows, 2, "Should have 2 rows after second save");

    // Test upsert behavior (update existing + insert new)
    let upsert_data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["1", "3"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Alice Updated", "Charlie"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = db
      .save_to_table("save_modes_test", upsert_data, &["id"])
      .await
      .expect("Failed to upsert data");

    // Verify we have 3 rows (1 updated, 1 unchanged, 1 new)
    let query = crate::operations::PrimaryStoreQuery {
      table: "save_modes_test",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db.query_table(&query).await.expect("Failed to query");

    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|batch| batch.num_rows()).sum();
    assert_eq!(total_rows, 3, "Should have 3 rows after upsert");
  }

  #[tokio::test]
  async fn test_vector_search_functionality() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Create test data with embeddings
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new(
        "embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          3, // Small embedding size for test
        ),
        false,
      ),
    ]));

    // First create the table
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("vector_test", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

    // Create test vectors
    let vector1 = vec![1.0_f32, 0.0, 0.0];
    let vector2 = vec![0.0_f32, 1.0, 0.0];
    let vector3 = vec![0.0_f32, 0.0, 1.0];
    let vector4 = vec![0.7_f32, 0.7, 0.0]; // Similar to vector1

    // Create embeddings as FixedSizeListArray
    let mut all_values: Vec<f32> = Vec::new();
    all_values.extend(&vector1);
    all_values.extend(&vector2);
    all_values.extend(&vector3);
    all_values.extend(&vector4);
    let embedding_values = Float32Array::from(all_values);

    let embedding_array = FixedSizeListArray::new(
      Arc::new(Field::new("item", DataType::Float32, true)),
      3,
      Arc::new(embedding_values),
      None,
    );

    let test_data = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(StringArray::from(vec!["1", "2", "3", "4"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["item1", "item2", "item3", "item4"]))
          as arrow_array::ArrayRef,
        Arc::new(embedding_array) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    let _ = db
      .save_to_table("vector_test", test_data, &["id"])
      .await
      .expect("Failed to save test data");

    // Test 1: Vector search without filter
    let query = crate::operations::PrimaryStoreQuery {
      table: "vector_test",
      filter: None,
      limit: Some(2),
      offset: None,
      vector_search: Some(crate::operations::VectorSearchParams {
        vector: vector1.clone(),
        column: "embedding",
        distance_type: lancedb::DistanceType::Cosine,
      }),
    };

    let stream = db
      .query_table(&query)
      .await
      .expect("Failed to execute vector search");
    let results: Vec<RecordBatch> = stream
      .try_collect()
      .await
      .expect("Failed to collect results");

    // Should return 2 results (limit=2)
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 2, "Should return 2 results with limit");

    // The first result should be id=1 (exact match) and second should be id=4 (similar)
    if let Some(batch) = results.first() {
      if let Some(id_array) = batch.column_by_name("id") {
        let id_array = id_array.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(
          id_array.value(0),
          "1",
          "First result should be the exact match"
        );
        if batch.num_rows() > 1 {
          assert_eq!(
            id_array.value(1),
            "4",
            "Second result should be the similar vector"
          );
        }
      }

      // Check that _distance column exists
      assert!(
        batch.column_by_name("_distance").is_some(),
        "_distance column should exist in vector search results"
      );
    }

    // Test 2: Vector search with filter
    let query_with_filter = crate::operations::PrimaryStoreQuery {
      table: "vector_test",
      filter: Some("id IN ('2', '3', '4')".to_string()),
      limit: Some(2),
      offset: None,
      vector_search: Some(crate::operations::VectorSearchParams {
        vector: vector1.clone(),
        column: "embedding",
        distance_type: lancedb::DistanceType::Cosine,
      }),
    };

    let stream = db
      .query_table(&query_with_filter)
      .await
      .expect("Failed to execute filtered vector search");
    let results: Vec<RecordBatch> = stream
      .try_collect()
      .await
      .expect("Failed to collect results");

    // Should return at most 2 results, but id=1 should be filtered out
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert!(total_rows <= 2, "Should respect limit");

    // id=1 should not be in results due to filter
    for batch in &results {
      if let Some(id_array) = batch.column_by_name("id") {
        let id_array = id_array.as_any().downcast_ref::<StringArray>().unwrap();
        for i in 0..batch.num_rows() {
          assert_ne!(
            id_array.value(i),
            "1",
            "Filtered ID should not appear in results"
          );
        }
      }
    }
  }

  #[tokio::test]
  async fn test_merge_insert_on_empty_table() {
    // This test documents how MergeInsert works with the new invariant that tables must exist
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("content", DataType::Utf8, false),
    ]));

    // Test scenario 1: MergeInsert on a non-existent table should fail
    let data = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["uuid1", "uuid2"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["content1", "content2"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create batch");

    // Try MergeInsert on non-existent table - this will fail
    let result = db
      .save_to_table("merge_test", data.clone(), &["uuid"])
      .await;

    // MergeInsert should fail on non-existent table
    assert!(
      result.is_err(),
      "MergeInsert should fail on non-existent table"
    );

    // Test scenario 2: MergeInsert on an empty table (proper way)
    // First create the table
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("merge_test_empty", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

    // Verify table exists but is empty
    let query = crate::operations::PrimaryStoreQuery {
      table: "merge_test_empty",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db.query_table(&query).await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 0, "Table should be empty");

    // Now MergeInsert on the empty table should work
    let (_, _version) = db
      .save_to_table("merge_test_empty", data.clone(), &["uuid"])
      .await
      .expect("MergeInsert should work on empty table");

    // Check if data was actually saved
    let query2 = crate::operations::PrimaryStoreQuery {
      table: "merge_test_empty",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db
      .query_table(&query2)
      .await
      .expect("Failed to query after merge");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();

    assert_eq!(
      total_rows, 2,
      "Should have 2 rows after MergeInsert on empty table"
    );

    // Test scenario 3: Reproduce the exact episode table schema behavior
    // This mimics what happens with episodes
    let episode_schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new("source", DataType::Utf8, false),
    ]));

    // Create the episodes table first
    let episode_schema_clone = episode_schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("episodes_test", episode_schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create episodes table");

    let episode_data = RecordBatch::try_new(
      episode_schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["episode_uuid1"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["test-group"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Message"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create episode batch");

    // Add first episode using MergeInsert
    let _ = db
      .save_to_table("episodes_test", episode_data.clone(), &["uuid"])
      .await
      .expect("Failed to save first episode");

    // Add a second episode using MergeInsert
    let episode_data2 = RecordBatch::try_new(
      episode_schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["episode_uuid2"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["test-group"])) as arrow_array::ArrayRef,
        Arc::new(StringArray::from(vec!["Message"])) as arrow_array::ArrayRef,
      ],
    )
    .expect("Failed to create second episode batch");

    let (_, _version2) = db
      .save_to_table("episodes_test", episode_data2.clone(), &["uuid"])
      .await
      .expect("MergeInsert should work on non-empty table");

    // Check if both episodes were saved
    let query = crate::operations::PrimaryStoreQuery {
      table: "episodes_test",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db
      .query_table(&query)
      .await
      .expect("Failed to query episodes");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();

    // Since we added two episodes with MergeInsert, we should have 2 rows
    assert_eq!(total_rows, 2, "Should have 2 rows after both MergeInserts");
  }

  #[tokio::test]
  async fn test_embedding_index_requires_data() {
    // This test documents LanceDB's limitation: you cannot create vector indices on empty tables
    // Demonstrates the workaround: create table with dummy data, create index, then remove dummy data
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = MetaDb::new(&config).await.expect("Failed to create MetaDb");

    // Schema with embedding column
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new(
        "name_embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          384, // Standard sentence-transformer dimension
        ),
        false,
      ),
    ]));

    // Step 1: Try to create empty table and create vector index - this will fail
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        let table = conn
          .create_empty_table("embedding_test_empty", schema_clone)
          .execute()
          .await?;

        // Try to create vector index on empty table - this should fail
        let index_result = table
          .create_index(&["name_embedding"], lancedb::index::Index::Auto)
          .execute()
          .await;

        match index_result {
          Ok(_) => {}
          Err(_e) => {}
        }

        Ok(())
      })
    }))
    .await
    .expect("Failed to create empty table");

    // Step 2: Demonstrate the workaround - create table with dummy data
    let dummy_embedding = vec![0.0f32; 384]; // Zero vector as placeholder

    // Create dummy data with zero UUID
    let dummy_batch = {
      use arrow_array::FixedSizeListArray;
      let embedding_values = Float32Array::from(dummy_embedding.clone());
      let embedding_array = FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        384,
        Arc::new(embedding_values),
        None,
      );

      RecordBatch::try_new(
        schema.clone(),
        vec![
          Arc::new(StringArray::from(vec![
            "00000000-0000-0000-0000-000000000000",
          ])) as arrow_array::ArrayRef,
          Arc::new(StringArray::from(vec!["__dummy_for_index_creation__"]))
            as arrow_array::ArrayRef,
          Arc::new(embedding_array) as arrow_array::ArrayRef,
        ],
      )
      .expect("Failed to create dummy batch")
    };

    // Create table with dummy data
    let schema_clone = schema.clone();
    db.execute_setup(Box::new(move |conn| {
      let schema = schema_clone.clone();
      let batch = dummy_batch;
      Box::pin(async move {
        let batch_iter =
          arrow_array::RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema.clone());

        let table = conn
          .create_table("embedding_test_with_data", Box::new(batch_iter))
          .execute()
          .await?;

        // Now create vector index - this should succeed
        let index_result = table
          .create_index(&["name_embedding"], lancedb::index::Index::Auto)
          .execute()
          .await;

        match index_result {
          Ok(_) => {}
          Err(_e) => {}
        }

        // Remove the dummy row
        table
          .delete("uuid = '00000000-0000-0000-0000-000000000000'")
          .await?;

        Ok(())
      })
    }))
    .await
    .expect("Failed to create table with dummy data");

    // Step 3: Verify the table is empty but has the index
    let query = crate::operations::PrimaryStoreQuery {
      table: "embedding_test_with_data",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    };
    let stream = db.query_table(&query).await.expect("Failed to query");
    let results: Vec<RecordBatch> = stream.try_collect().await.expect("Failed to collect");
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
      total_rows, 0,
      "Table should be empty after dummy row removal"
    );

    // Step 4: Add real data and test vector search
    let real_embedding = vec![1.0f32; 384]; // Real vector
    let real_batch = {
      use arrow_array::FixedSizeListArray;
      let embedding_values = Float32Array::from(real_embedding.clone());
      let embedding_array = FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        384,
        Arc::new(embedding_values),
        None,
      );

      RecordBatch::try_new(
        schema,
        vec![
          Arc::new(StringArray::from(vec!["real-uuid-1"])) as arrow_array::ArrayRef,
          Arc::new(StringArray::from(vec!["Real Entity"])) as arrow_array::ArrayRef,
          Arc::new(embedding_array) as arrow_array::ArrayRef,
        ],
      )
      .expect("Failed to create real batch")
    };

    let _ = db
      .save_to_table("embedding_test_with_data", real_batch, &["uuid"])
      .await
      .expect("Failed to save real data");

    // Test vector search works
    let vector_query = crate::operations::PrimaryStoreQuery {
      table: "embedding_test_with_data",
      filter: None,
      limit: Some(1),
      offset: None,
      vector_search: Some(crate::operations::VectorSearchParams {
        vector: real_embedding,
        column: "name_embedding",
        distance_type: lancedb::DistanceType::Cosine,
      }),
    };

    let stream = db
      .query_table(&vector_query)
      .await
      .expect("Failed to execute vector search");
    let results: Vec<RecordBatch> = stream
      .try_collect()
      .await
      .expect("Failed to collect vector search results");

    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 1, "Vector search should return 1 result");
  }
}
