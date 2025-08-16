use super::tests_helpers::{AppendRows, CountSystemMetadataForTable, CountTableRows, CreateSchema};
use crate::{Config, Database, meta::MetaDb};

use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{RecordBatch, StringArray};
use futures::TryStreamExt;
use std::sync::Arc;
use tempfile::tempdir;
// no extra imports

fn test_config(path: &std::path::Path) -> Config {
  Config::new_test(path.to_path_buf())
}

async fn setup_test_database() -> (Database, tempfile::TempDir, Config) {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup test table schema via public migration command
  db.execute(CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE IF NOT EXISTS TestCrashNode (id UUID, name STRING, PRIMARY KEY(id))",
    ],
    meta_table: "test_crash_nodes",
    meta_schema: Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ])),
  })
  .await
  .expect("Failed to create schema");

  (db, temp_dir, config)
}

#[tokio::test]
async fn test_consistency_check_with_no_mismatch() {
  let (db, _temp_dir, config) = setup_test_database().await;

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

  // Restart database to run consistency check on startup, then verify data still present
  drop(db);
  let db = Database::new(&config).await.expect("recreate db");
  let total_rows: usize = db
    .execute(CountTableRows {
      table: "test_crash_nodes",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
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

  // Verify SystemMetadata was updated via helper
  let count: usize = db
    .execute(CountSystemMetadataForTable {
      table: "test_crash_nodes",
    })
    .await
    .expect("count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert!(count >= 1, "SystemMetadata should have been created");
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
  let count: usize = db
    .execute(CountSystemMetadataForTable {
      table: "test_crash_nodes",
    })
    .await
    .expect("count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(
    count, 0,
    "SystemMetadata should not exist after failed transaction"
  );

  // Verify no data was persisted in LanceDB (was rolled back)
  let total_rows: usize = db
    .execute(CountTableRows {
      table: "test_crash_nodes",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(total_rows, 0, "No data should exist after rollback");

  // Explicitly drop the database to clean up background tasks
  drop(db);
}

#[tokio::test]
async fn test_consistency_check_detects_and_fixes_incomplete_transaction() {
  let (db, _temp_dir, config) = setup_test_database().await;

  let test_id = uuid::Uuid::now_v7();

  // First, insert some data with a successful transaction
  let _baseline_version = db
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

  // Append crash data via public API (Lance-only change)
  db.execute(AppendRows {
    table: "test_crash_nodes",
    batch: crash_batch.clone(),
  })
  .await
  .expect("LanceDB operation should succeed");

  // Verify that the "crash data" exists before recovery (rows increased)
  let rows_before_restart: usize = db
    .execute(CountTableRows {
      table: "test_crash_nodes",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(
    rows_before_restart, 2,
    "Should have baseline + crash rows before restart"
  );

  // Drop the database and recreate to simulate restart
  drop(db);
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database after crash");

  // The consistency check in new() should have detected and fixed the issue

  // Verify we only have the baseline data
  let total_rows: usize = db
    .execute(CountTableRows {
      table: "test_crash_nodes",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);

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
    db.execute(CreateSchema {
      graph_ddl: vec![
        "CREATE NODE TABLE IF NOT EXISTS Table1 (id UUID, value INT64, PRIMARY KEY(id))",
      ],
      meta_table: "table1",
      meta_schema: Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
      ])),
    })
    .await
    .expect("Failed to create schema");

    // Setup second table
    db.execute(CreateSchema {
      graph_ddl: vec![
        "CREATE NODE TABLE IF NOT EXISTS Table2 (id UUID, name STRING, PRIMARY KEY(id))",
      ],
      meta_table: "table2",
      meta_schema: Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
      ])),
    })
    .await
    .expect("Failed to create schema");

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

    db.execute(AppendRows {
      table: "table2",
      batch: crash_batch,
    })
    .await
    .expect("Crash data should save");
  }

  // Now create a new database instance - it should perform consistency check on startup
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database with recovery");

  // Verify table1 is unchanged (1 row)
  let total_rows: usize = db
    .execute(CountTableRows { table: "table1" })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(total_rows, 1, "Table1 should be unchanged");

  // Verify table2 was rolled back (only 1 row, not 2)
  let total_rows: usize = db
    .execute(CountTableRows { table: "table2" })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(total_rows, 1, "Table2 should have been rolled back");
}
