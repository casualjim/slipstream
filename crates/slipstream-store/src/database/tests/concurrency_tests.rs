use std::sync::Arc;

use super::tests_helpers::{
  CountGraphItems, CountTableRows, CreateSchema, init_logging, test_config,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{Int64Array, RecordBatch, StringArray};
use futures::TryStreamExt as _;

use crate::{Database, meta::MetaDb};

#[tokio::test]
async fn test_concurrent_meta_writes() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Create table first
  let schema = Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("value", DataType::Utf8, false),
  ]));

  // Create the table using execute_setup
  let schema_for_create = schema.clone();
  db.execute(CreateSchema {
    graph_ddl: vec![],
    meta_table: "concurrent_test",
    meta_schema: schema_for_create,
  })
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

  let _ = db
    .execute_2pc(
      "concurrent_test",
      move |meta: &MetaDb| {
        let batch = initial_batch;
        Box::pin(async move {
          let (_rv, _v) = meta
            .save_to_table("concurrent_test", batch, &["id"])
            .await?;
          Ok(((), None))
        })
      },
      vec![],
    )
    .await;

  // Spawn 10 concurrent write operations to LanceDB
  let mut handles = vec![];

  for i in 1..=10 {
    let actor_clone = db.clone();
    let schema_clone = schema.clone();

    let handle = tokio::spawn(async move {
      let batch = RecordBatch::try_new(
        schema_clone,
        vec![
          Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
          Arc::new(StringArray::from(vec![format!("value_{i}")])) as arrow_array::ArrayRef,
        ],
      )?;

      actor_clone
        .execute_2pc(
          "concurrent_test",
          move |meta: &MetaDb| {
            let batch = batch;
            Box::pin(async move {
              let (_rv, _v) = meta
                .save_to_table("concurrent_test", batch, &["id"])
                .await?;
              Ok(((), None))
            })
          },
          vec![],
        )
        .await?;
      Ok::<i64, crate::Error>(i)
    });
    handles.push(handle);
  }

  // All writes should succeed due to MVCC
  let results: Vec<_> = futures::future::join_all(handles).await;
  for result in results.iter() {
    assert!(result.is_ok(), "Write failed: {result:?}");
  }

  // Verify all data was written
  let total_rows: usize = db
    .execute(CountTableRows {
      table: "concurrent_test",
    })
    .await
    .expect("query")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(total_rows, 11, "Should have initial + 10 concurrent writes");
}

#[tokio::test]
async fn test_concurrent_graph_writes() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup schema
  db.execute(CreateSchema {
    graph_ddl: vec!["CREATE NODE TABLE ConcurrentNode (id INT64, name STRING, PRIMARY KEY(id))"],
    meta_table: "__noop__",
    meta_schema: Arc::new(ArrowSchema::new(Vec::<Field>::new())),
  })
  .await
  .expect("Failed to create schema");

  // Spawn 10 concurrent write operations to KuzuDB
  let mut handles = vec![];

  for i in 1..=10 {
    let actor_clone = db.clone();

    let handle = tokio::spawn(async move {
      actor_clone
        .execute_2pc(
          "__noop__",
          move |_meta: &MetaDb| Box::pin(async move { Ok(((), None)) }),
          vec![(
            "CREATE (:ConcurrentNode {id: $id, name: $name})".to_string(),
            vec![
              ("id", kuzu::Value::Int64(i)),
              ("name", kuzu::Value::String(format!("Node{i}"))),
            ],
          )],
        )
        .await
    });
    handles.push(handle);
  }

  // All writes should succeed (KuzuDB serializes them)
  let results: Vec<_> = futures::future::join_all(handles).await;
  for result in results.iter() {
    assert!(result.is_ok(), "Write task failed: {result:?}");
    assert!(
      result.as_ref().unwrap().is_ok(),
      "Write inner failed: {:?}",
      result.as_ref().unwrap()
    );
  }

  // Verify all nodes were created
  let count = db
    .execute(CountGraphItems {
      label: "ConcurrentNode",
    })
    .await
    .expect("count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);
  assert_eq!(count, 10, "Should have created 10 nodes");
}

#[tokio::test]
async fn test_concurrent_2pc_operations() {
  init_logging();
  let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup schemas
  db.execute(CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE Entity2PC (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
    ],
    meta_table: "entities_2pc",
    meta_schema: Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ])),
  })
  .await
  .expect("Failed to create schema");

  // MetaDb table is created via CreateSchema above; no direct internal access here

  // Spawn multiple concurrent 2PC operations
  let mut handles = vec![];

  for i in 0..20 {
    let actor_clone = db.clone();

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
                  Arc::new(StringArray::from(vec![entity_id.to_string()])) as arrow_array::ArrayRef,
                  Arc::new(StringArray::from(vec![format!("Entity{i}")])) as arrow_array::ArrayRef,
                  Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
                ],
              )?;

              let (rollback_version, _new_version) =
                meta.save_to_table("entities_2pc", batch, &["id"]).await?;
              Ok((entity_id, Some(("entities_2pc", rollback_version))))
            })
          },
          vec![(
            "CREATE (:Entity2PC {id: $id, name: $name, value: $value})".to_string(),
            vec![
              ("id", kuzu::Value::UUID(entity_id)),
              ("name", kuzu::Value::String(format!("Entity{i}"))),
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
  let graph_count = db
    .execute(CountGraphItems { label: "Entity2PC" })
    .await
    .expect("count")
    .try_collect::<Vec<_>>()
    .await
    .expect("collect")
    .into_iter()
    .next()
    .unwrap_or(0);

  assert!(
    graph_count > 0,
    "Should have at least some successful 2PC operations"
  );
}
