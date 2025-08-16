use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{Int64Array, RecordBatch, StringArray};
use futures::TryStreamExt;

use super::tests_helpers::{
  CountGraphItems, CountTableRows, CreateSchema, init_logging, test_config,
};
use crate::{Database, meta::MetaDb};

// Helpers to use only public crate APIs in tests

// --- test_2pc_success ---
#[tokio::test]
async fn test_2pc_success() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup schema via public execute API
  let schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
  ]));
  db.execute(CreateSchema {
    graph_ddl: vec!["CREATE NODE TABLE TestNode (id UUID, name STRING, PRIMARY KEY(id))"],
    meta_table: "test_nodes",
    meta_schema: schema,
  })
  .await
  .expect("Failed to create schema");

  // Test successful 2PC operation
  let test_id = uuid::Uuid::now_v7();
  let result = db
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

// --- test_2pc_failures_under_concurrency ---
#[tokio::test]
async fn test_2pc_failures_under_concurrency() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup schemas via public execute API
  let meta_schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("value", DataType::Int64, false),
    Field::new("conflict_group", DataType::Int64, false),
  ]));

  db.execute(CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE Entity2PCFailure (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
      "CREATE NODE TABLE UniqueConstraint (name STRING, PRIMARY KEY(name))",
    ],
    meta_table: "entities_2pc_failure",
    meta_schema,
  })
  .await
  .expect("Failed to setup schemas");

  // Track versions for analysis
  let version_tracker = Arc::new(parking_lot::Mutex::new(Vec::new()));

  // Spawn 50 concurrent 2PC operations with intentional conflicts
  let mut handles = vec![];

  for i in 0..50 {
    let actor_clone = db.clone();
    let version_tracker_clone = version_tracker.clone();

    let handle = tokio::spawn(async move {
      let entity_id = uuid::Uuid::now_v7();

      // Use a shared name for some operations to cause conflicts
      let conflict_group = i / 10; // Creates 5 groups of 10 operations each
      let unique_name = format!("conflict_group_{}", conflict_group);

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
                  Arc::new(StringArray::from(vec![entity_id.to_string()])) as arrow_array::ArrayRef,
                  Arc::new(StringArray::from(vec![format!("Entity{}", i)]))
                    as arrow_array::ArrayRef,
                  Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
                  Arc::new(Int64Array::from(vec![conflict_group])) as arrow_array::ArrayRef,
                ],
              )?;

              let (rollback_version, new_version) = meta
                .save_to_table("entities_2pc_failure", batch, &["id"])
                .await?;

              tracker.lock().push((i, new_version, "meta_success"));

              Ok((entity_id, Some(("entities_2pc_failure", rollback_version))))
            })
          },
          vec![
            (
              "CREATE (:Entity2PCFailure {id: $id, name: $name, value: $value})".to_string(),
              vec![
                ("id", kuzu::Value::UUID(entity_id)),
                ("name", kuzu::Value::String(format!("Entity{}", i))),
                ("value", kuzu::Value::Int64(i)),
              ],
            ),
            (
              "CREATE (:UniqueConstraint {name: $name})".to_string(),
              vec![("name", kuzu::Value::String(unique_name.clone()))],
            ),
          ],
        )
        .await;

      if result.is_err() {
        tracker.lock().push((i, 0, "graph_failed"));
      } else {
        tracker.lock().push((i, 0, "graph_success"));
      }

      (i, entity_id, result)
    });
    handles.push(handle);
  }

  let results: Vec<_> = futures::future::join_all(handles).await;

  let mut successful = 0;
  let mut failed = 0;
  let mut panic_count = 0;

  for result in results.iter() {
    match result {
      Ok((_, _, Ok(_))) => successful += 1,
      Ok((_, _, Err(_))) => failed += 1,
      Err(_) => panic_count += 1,
    }
  }

  assert_eq!(successful, 5);
  assert_eq!(failed, 45);
  assert_eq!(panic_count, 0);

  let graph_count = db
    .execute(CountGraphItems {
      label: "Entity2PCFailure",
    })
    .await
    .expect("Failed to count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert!(graph_count >= 5);
}

// --- test_2pc_deterministic_rollback ---
#[tokio::test]
async fn test_2pc_deterministic_rollback() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup
  // Setup via Migration
  let schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("value", DataType::Int64, false),
  ]));
  db.execute(CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE DeterministicEntity (id UUID, value INT64, PRIMARY KEY(id))",
    ],
    meta_table: "deterministic_test",
    meta_schema: schema.clone(),
  })
  .await
  .expect("Failed to create schema");

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
  // Save via 2PC with no graph queries
  let _ = db
    .execute_2pc(
      "deterministic_test",
      move |meta: &MetaDb| {
        let batch = initial_batch;
        Box::pin(async move {
          let (_rollback_version, _v) = meta
            .save_to_table("deterministic_test", batch, &["id"])
            .await?;
          Ok(((), None))
        })
      },
      vec![],
    )
    .await;

  // Test 1: Successful 2PC operation
  let success_id = uuid::Uuid::now_v7();
  let schema_clone = schema.clone();
  let result = db
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

          let (rollback_version, _version) = meta
            .save_to_table("deterministic_test", batch, &["id"])
            .await?;

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
  let result = db
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

          let (rollback_version, _version) = meta
            .save_to_table("deterministic_test", batch, &["id"])
            .await?;

          Ok((failed_id, Some(("deterministic_test", rollback_version))))
        })
      },
      vec![("INVALID QUERY TO SIMULATE FAILURE".to_string(), vec![])],
    )
    .await;

  assert!(result.is_err(), "Failed 2PC should return error");

  // Verify final state
  // Count meta rows via public query API
  let meta_count: usize = db
    .execute(CountTableRows {
      table: "deterministic_test",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);

  let graph_count = db
    .execute(CountGraphItems {
      label: "DeterministicEntity",
    })
    .await
    .expect("Failed to count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);

  // Read version through 2PC meta access (no graph ops)
  let final_version: u64 = db
    .execute_2pc(
      "deterministic_test",
      move |meta: &MetaDb| {
        Box::pin(async move { Ok((meta.get_table_version("deterministic_test").await?, None)) })
      },
      vec![],
    )
    .await
    .expect("get_version");

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

// --- test_2pc_rollback_simulation ---
#[tokio::test]
async fn test_2pc_rollback_simulation() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup via Migration
  let schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("value", DataType::Int64, false),
    Field::new("status", DataType::Utf8, false),
  ]));
  let schema_clone = schema.clone();
  db.execute(CreateSchema {
    graph_ddl: vec!["CREATE NODE TABLE TestEntity (id UUID, value INT64, PRIMARY KEY(id))"],
    meta_table: "test_rollback",
    meta_schema: schema_clone,
  })
  .await
  .expect("Failed to create schema");

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

  let _ = db
    .execute_2pc(
      "test_rollback",
      move |meta: &MetaDb| {
        let batch = initial_batch;
        Box::pin(async move {
          let (_rv, _v) = meta.save_to_table("test_rollback", batch, &["id"]).await?;
          Ok(((), None))
        })
      },
      vec![],
    )
    .await;

  let initial_version: u64 = db
    .execute_2pc(
      "test_rollback",
      move |meta: &MetaDb| {
        Box::pin(async move { Ok((meta.get_table_version("test_rollback").await?, None)) })
      },
      vec![],
    )
    .await
    .expect("get_version");

  // Simulate a failed 2PC operation
  let failed_id = uuid::Uuid::now_v7();
  let result = db
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

          let (rollback_version, _version) =
            meta.save_to_table("test_rollback", batch, &["id"]).await?;

          Ok((failed_id, Some(("test_rollback", rollback_version))))
        })
      },
      vec![("INVALID QUERY TO SIMULATE FAILURE".to_string(), vec![])],
    )
    .await;

  assert!(result.is_err(), "2PC should fail");

  // With proper 2PC implementation, the rollback should have already happened automatically
  let current_count: usize = db
    .execute(CountTableRows {
      table: "test_rollback",
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
    current_count, 1,
    "Should only have initial record after automatic rollback"
  );

  let final_version: u64 = db
    .execute_2pc(
      "test_rollback",
      move |meta: &MetaDb| {
        Box::pin(async move { Ok((meta.get_table_version("test_rollback").await?, None)) })
      },
      vec![],
    )
    .await
    .expect("get_version");

  assert!(
    final_version > initial_version,
    "Rollback should create a new version"
  );
}
