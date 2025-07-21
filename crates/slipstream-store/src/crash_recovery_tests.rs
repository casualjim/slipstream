use super::*;
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{RecordBatch, StringArray};
use futures::TryStreamExt;
use std::sync::Arc;
use tempfile::tempdir;

fn test_config(path: &std::path::Path) -> Config {
  Config::new_test(path.to_path_buf())
}

async fn setup_test_database() -> (Database, tempfile::TempDir, Config) {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup test table schema
  db.graph
    .execute_write(
      "CREATE NODE TABLE IF NOT EXISTS TestCrashNode (id UUID, name STRING, PRIMARY KEY(id))",
      vec![],
    )
    .await
    .expect("Failed to create graph schema");

  db.meta
    .execute_setup(Box::new(|conn| {
      Box::pin(async move {
        let schema = Arc::new(ArrowSchema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("name", DataType::Utf8, false),
        ]));
        conn
          .create_empty_table("test_crash_nodes", schema)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create meta table");

  (db, temp_dir, config)
}

#[tokio::test]
async fn test_consistency_check_with_no_mismatch() {
  let (db, _temp_dir, _config) = setup_test_database().await;

  // Execute a successful transaction to establish baseline
  let test_id = uuid::Uuid::now_v7();
  let test_name = "test_node".to_string();

  let result = db
    .execute_2pc(
      "test_crash_nodes",
      move |meta: &MetaDb| {
        let id_str = test_id.to_string();
        let name = test_name.clone();
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id_str])),
              Arc::new(StringArray::from(vec![name])),
            ],
          )?;

          let (rollback_version, _new_version) = meta
            .save_to_table("test_crash_nodes", batch, &["id"])
            .await?;

          Ok((42u32, Some(("test_crash_nodes", rollback_version))))
        })
      },
      vec![(
        "CREATE (:TestCrashNode {id: $id, name: $name})".to_string(),
        vec![
          ("id", kuzu::Value::UUID(test_id)),
          ("name", kuzu::Value::String("test_node".to_string())),
        ],
      )],
    )
    .await
    .expect("2PC should succeed");

  assert_eq!(result, 42);

  // Check consistency - should find no issues
  db.check_consistency_on_startup()
    .await
    .expect("Consistency check should succeed");

  // Verify data is still there
  let stream = db
    .meta
    .query_table(&operations::PrimaryStoreQuery {
      table: "test_crash_nodes",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Should query table");

  let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 1, "Should still have the data");
}

#[tokio::test]
async fn test_successful_2pc_updates_system_metadata() {
  let (db, _temp_dir, _config) = setup_test_database().await;

  let test_id = uuid::Uuid::now_v7();
  let test_name = "test_node".to_string();

  // Execute successful 2PC
  let _result = db
    .execute_2pc(
      "test_crash_nodes",
      move |meta: &MetaDb| {
        let id_str = test_id.to_string();
        let name = test_name.clone();
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id_str])),
              Arc::new(StringArray::from(vec![name])),
            ],
          )?;

          let (rollback_version, new_version) = meta
            .save_to_table("test_crash_nodes", batch, &["id"])
            .await?;

          Ok((new_version, Some(("test_crash_nodes", rollback_version))))
        })
      },
      vec![(
        "CREATE (:TestCrashNode {id: $id, name: $name})".to_string(),
        vec![
          ("id", kuzu::Value::UUID(test_id)),
          ("name", kuzu::Value::String("test_node".to_string())),
        ],
      )],
    )
    .await
    .expect("2PC should succeed");

  // Verify SystemMetadata was updated
  let cypher = "MATCH (sm:SystemMetadata {table_name: $table_name}) RETURN sm.lance_version";
  let params = vec![(
    "table_name",
    kuzu::Value::String("test_crash_nodes".to_string()),
  )];
  let receiver = db
    .graph
    .execute_query(cypher, params)
    .await
    .expect("Query should succeed");

  let mut found_version = false;
  {
    use futures::StreamExt;
    let mut stream = UnboundedReceiverStream::new(receiver);
    if let Some(Ok(row)) = stream.next().await {
      if let Some(kuzu::Value::Int64(_version)) = row.get(0) {
        found_version = true;
      }
    }
  }

  assert!(found_version, "SystemMetadata should have been created");
}

#[tokio::test]
async fn test_failed_graphdb_transaction_leaves_no_system_metadata() {
  let (db, _temp_dir, _config) = setup_test_database().await;

  let test_id = uuid::Uuid::now_v7();

  // Execute a 2PC that will fail on the GraphDB side
  let result = db
    .execute_2pc(
      "test_crash_nodes",
      move |meta: &MetaDb| {
        let id_str = test_id.to_string();
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id_str])),
              Arc::new(StringArray::from(vec!["test_node".to_string()])),
            ],
          )?;

          let (rollback_version, _new_version) = meta
            .save_to_table("test_crash_nodes", batch, &["id"])
            .await?;

          Ok((42u32, Some(("test_crash_nodes", rollback_version))))
        })
      },
      vec![(
        // This GraphDB query will fail due to invalid syntax
        "INVALID QUERY SYNTAX".to_string(),
        vec![
          ("id", kuzu::Value::UUID(test_id)),
          ("name", kuzu::Value::String("test_node".to_string())),
        ],
      )],
    )
    .await;

  // The 2PC should fail
  assert!(
    result.is_err(),
    "2PC should fail due to invalid GraphDB query"
  );

  // Verify SystemMetadata was NOT created (transaction rolled back)
  let cypher = "MATCH (sm:SystemMetadata {table_name: $table_name}) RETURN sm.lance_version";
  let params = vec![(
    "table_name",
    kuzu::Value::String("test_crash_nodes".to_string()),
  )];
  let receiver = db
    .graph
    .execute_query(cypher, params)
    .await
    .expect("Query should succeed");

  let mut found_version = false;
  {
    use futures::StreamExt;
    let mut stream = UnboundedReceiverStream::new(receiver);
    if let Some(Ok(row)) = stream.next().await {
      if let Some(kuzu::Value::Int64(_version)) = row.get(0) {
        found_version = true;
      }
    }
  }

  assert!(
    !found_version,
    "SystemMetadata should not exist after failed transaction"
  );

  // Verify no data was persisted in LanceDB (was rolled back)
  let stream = db
    .meta
    .query_table(&operations::PrimaryStoreQuery {
      table: "test_crash_nodes",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Should query table");

  let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 0, "No data should exist after rollback");

  // Explicitly drop the database to clean up background tasks
  drop(db);
}

#[tokio::test]
async fn test_consistency_check_detects_and_fixes_incomplete_transaction() {
  let (db, _temp_dir, config) = setup_test_database().await;

  let test_id = uuid::Uuid::now_v7();

  // First, insert some data with a successful transaction
  let baseline_version = db
    .execute_2pc(
      "test_crash_nodes",
      move |meta: &MetaDb| {
        let id_str = test_id.to_string();
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id_str])),
              Arc::new(StringArray::from(vec!["initial_data".to_string()])),
            ],
          )?;

          let (_rollback_version, new_version) = meta
            .save_to_table("test_crash_nodes", batch, &["id"])
            .await?;

          Ok((new_version, Some(("test_crash_nodes", new_version))))
        })
      },
      vec![(
        "CREATE (:TestCrashNode {id: $id, name: $name})".to_string(),
        vec![
          ("id", kuzu::Value::UUID(test_id)),
          ("name", kuzu::Value::String("initial_data".to_string())),
        ],
      )],
    )
    .await
    .expect("Initial 2PC should succeed");

  // Now simulate an incomplete transaction by writing directly to LanceDB
  // without updating GraphDB (simulating a crash after LanceDB write)
  let crash_id = uuid::Uuid::now_v7();
  let schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
  ]));

  let crash_batch = RecordBatch::try_new(
    schema,
    vec![
      Arc::new(StringArray::from(vec![crash_id.to_string()])),
      Arc::new(StringArray::from(vec!["crash_data".to_string()])),
    ],
  )
  .expect("Should create crash batch");

  let (_rollback_version, post_crash_version) = db
    .meta
    .save_to_table("test_crash_nodes", crash_batch, &["id"])
    .await
    .expect("LanceDB operation should succeed");

  // Verify that the "crash data" exists before recovery
  assert!(
    post_crash_version > baseline_version,
    "Version should have incremented after crash data insertion"
  );

  // Drop the database and recreate to simulate restart
  drop(db);
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database after crash");

  // The consistency check in new() should have detected and fixed the issue

  // Verify that the table was rolled back
  let final_version = db
    .meta
    .get_table_version("test_crash_nodes")
    .await
    .expect("Should get final version");

  // After rollback, we should have a new version (roll-forward approach)
  assert!(
    final_version > post_crash_version,
    "Rollback should have created a new version"
  );

  // Verify we only have the baseline data
  let stream = db
    .meta
    .query_table(&operations::PrimaryStoreQuery {
      table: "test_crash_nodes",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Should query table");

  let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();

  assert_eq!(
    total_rows, 1,
    "Should only have baseline data after recovery"
  );
}

#[tokio::test]
async fn test_consistency_check_on_startup_with_multiple_tables() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  // Create initial database and set up multiple tables
  {
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Setup first table
    db.graph
      .execute_write(
        "CREATE NODE TABLE IF NOT EXISTS Table1 (id UUID, value INT64, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create graph schema");

    db.meta
      .execute_setup(Box::new(|conn| {
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
          ]));
          conn.create_empty_table("table1", schema).execute().await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create meta table");

    // Setup second table
    db.graph
      .execute_write(
        "CREATE NODE TABLE IF NOT EXISTS Table2 (id UUID, name STRING, PRIMARY KEY(id))",
        vec![],
      )
      .await
      .expect("Failed to create graph schema");

    db.meta
      .execute_setup(Box::new(|conn| {
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));
          conn.create_empty_table("table2", schema).execute().await?;
          Ok(())
        })
      }))
      .await
      .expect("Failed to create meta table");

    // Insert data into first table
    let id1 = uuid::Uuid::now_v7();
    db.execute_2pc(
      "table1",
      move |meta: &MetaDb| {
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id1.to_string()])),
              Arc::new(arrow_array::Int64Array::from(vec![100])),
            ],
          )?;

          let (_rollback_version, new_version) =
            meta.save_to_table("table1", batch, &["id"]).await?;

          Ok((new_version, Some(("table1", new_version))))
        })
      },
      vec![(
        "CREATE (:Table1 {id: $id, value: $value})".to_string(),
        vec![
          ("id", kuzu::Value::UUID(id1)),
          ("value", kuzu::Value::Int64(100)),
        ],
      )],
    )
    .await
    .expect("First table 2PC should succeed");

    // Insert data into second table
    let id2 = uuid::Uuid::now_v7();
    db.execute_2pc(
      "table2",
      move |meta: &MetaDb| {
        Box::pin(async move {
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
          ]));

          let batch = RecordBatch::try_new(
            schema,
            vec![
              Arc::new(StringArray::from(vec![id2.to_string()])),
              Arc::new(StringArray::from(vec!["test_name".to_string()])),
            ],
          )?;

          let (_rollback_version, new_version) =
            meta.save_to_table("table2", batch, &["id"]).await?;

          Ok((new_version, Some(("table2", new_version))))
        })
      },
      vec![(
        "CREATE (:Table2 {id: $id, name: $name})".to_string(),
        vec![
          ("id", kuzu::Value::UUID(id2)),
          ("name", kuzu::Value::String("test_name".to_string())),
        ],
      )],
    )
    .await
    .expect("Second table 2PC should succeed");

    // Now add some uncommitted data to table2 only
    let crash_id = uuid::Uuid::now_v7();
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ]));

    let crash_batch = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(StringArray::from(vec![crash_id.to_string()])),
        Arc::new(StringArray::from(vec!["crash_data".to_string()])),
      ],
    )
    .expect("Should create crash batch");

    db.meta
      .save_to_table("table2", crash_batch, &["id"])
      .await
      .expect("Crash data should save");
  }

  // Now create a new database instance - it should perform consistency check on startup
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database with recovery");

  // Verify table1 is unchanged (1 row)
  let stream = db
    .meta
    .query_table(&operations::PrimaryStoreQuery {
      table: "table1",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Should query table");

  let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 1, "Table1 should be unchanged");

  // Verify table2 was rolled back (only 1 row, not 2)
  let stream = db
    .meta
    .query_table(&operations::PrimaryStoreQuery {
      table: "table2",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Should query table");

  let batches: Vec<_> = stream.try_collect().await.expect("Should collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 1, "Table2 should have been rolled back");
}
