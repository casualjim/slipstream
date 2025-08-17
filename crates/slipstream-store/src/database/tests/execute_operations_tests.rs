use tempfile::tempdir;

use super::tests_helpers::*;
use crate::operations::GraphIndexQuery;
use crate::{
  Database, DatabaseCommand, DatabaseOperation, NoData, PrimaryStoreQuery, QueryOperation,
  ResultStream,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{Int64Array, RecordBatch, StringArray};
use futures::StreamExt;

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
      transformer: Box::new(move |_| format!("skipped_{value}")),
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
    DatabaseOperation::Query(QueryOperation::StoreOnly {
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
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
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
      std::sync::Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["a", "b", "c", "d", "e"]))
        as arrow_array::ArrayRef,
      std::sync::Arc::new(Int64Array::from(vec![10, 20, 30, 40, 50])) as arrow_array::ArrayRef,
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
    DatabaseOperation::Query(QueryOperation::IndexOnly {
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
                  format!("Expected (Int64, String), got {row:?}")
                ))?;
              }
            } else {
              let len = row.len();
              Err(crate::Error::InvalidGraphDbData(
                format!("Expected 2 values, got {len}")
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
          ("name", kuzu::Value::String(format!("test{i}"))),
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
    DatabaseOperation::Query(QueryOperation::IndexOnly {
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
                let v = &row[0];
                Err(crate::Error::InvalidGraphDbData(
                  format!("Expected UUID, got {v:?}")
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

// ==== Save operation tests ====

/// Test data that implements ToDatabase trait for Save operation
#[derive(Clone)]
struct TestSaveData {
  id: uuid::Uuid,
  name: String,
  value: i64,
}

impl crate::ToDatabase for TestSaveData {
  type MetaContext = ();
  type GraphContext = ();

  fn as_graph_value(
    &self,
    _ctx: Self::GraphContext,
  ) -> crate::Result<Vec<(&'static str, kuzu::Value)>> {
    Ok(vec![
      ("id", kuzu::Value::UUID(self.id)),
      ("name", kuzu::Value::String(self.name.clone())),
      ("value", kuzu::Value::Int64(self.value)),
    ])
  }

  fn as_meta_value(&self, _ctx: Self::MetaContext) -> crate::Result<arrow_array::RecordBatch> {
    let schema = std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ]));

    arrow_array::RecordBatch::try_new(
      schema,
      vec![
        std::sync::Arc::new(StringArray::from(vec![self.id.to_string()])) as arrow_array::ArrayRef,
        std::sync::Arc::new(StringArray::from(vec![self.name.clone()])) as arrow_array::ArrayRef,
        std::sync::Arc::new(Int64Array::from(vec![self.value])) as arrow_array::ArrayRef,
      ],
    )
    .map_err(crate::Error::from)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["id"]
  }
  fn meta_table_name() -> &'static str {
    "save_test"
  }
  fn graph_table_name() -> &'static str {
    "SaveTest"
  }
  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ]))
  }
}

/// Test command that implements Save operation via save_simple
#[derive(Clone)]
struct TestSaveCommand {
  data: TestSaveData,
}

impl DatabaseCommand for TestSaveCommand {
  type Output = String;
  type SaveData = TestSaveData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    let id = self.data.id;
    DatabaseOperation::save_simple(
      "save_test",
      std::sync::Arc::new(self.data.clone()),
      "CREATE (:SaveTest {id: $id, name: $name, value: $value})",
      Box::new(move |_| format!("saved_{id}")),
    )
  }
}

#[tokio::test]
async fn test_execute_save_operation() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup both GraphDB and MetaDB schema using public CreateSchema helper
  db.execute(super::tests_helpers::CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE SaveTest (id UUID, name STRING, value INT64, PRIMARY KEY(id))",
    ],
    meta_table: "save_test",
    meta_schema: std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ])),
  })
  .await
  .expect("Failed to create schemas");

  // Save two rows
  let id1 = uuid::Uuid::now_v7();
  let out1 = db
    .execute(TestSaveCommand {
      data: TestSaveData {
        id: id1,
        name: "test_create".into(),
        value: 42,
      },
    })
    .await
    .expect("save should work");
  assert_eq!(out1, format!("saved_{id1}"));

  let id2 = uuid::Uuid::now_v7();
  let out2 = db
    .execute(TestSaveCommand {
      data: TestSaveData {
        id: id2,
        name: "test_append".into(),
        value: 84,
      },
    })
    .await
    .expect("second save should work");
  assert_eq!(out2, format!("saved_{id2}"));

  // Verify meta rows
  let mut count_stream = db
    .execute(super::tests_helpers::CountTableRows { table: "save_test" })
    .await
    .expect("count");
  let meta_rows = count_stream.next().await.unwrap().unwrap();
  assert_eq!(meta_rows, 2, "Should have 2 rows in MetaDB");

  // Verify graph rows
  let mut gcount_stream = db
    .execute(super::tests_helpers::CountGraphItems { label: "SaveTest" })
    .await
    .expect("gcount");
  let graph_rows = gcount_stream.next().await.unwrap().unwrap();
  assert_eq!(graph_rows, 2, "Should have 2 nodes in GraphDB");
}

#[tokio::test]
async fn test_execute_save_operation_failure() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup schema
  db.execute(super::tests_helpers::CreateSchema {
    graph_ddl: vec!["CREATE NODE TABLE SaveFailTest (id UUID, name STRING, PRIMARY KEY(id))"],
    meta_table: "save_fail_test",
    meta_schema: std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::Int64, false),
    ])),
  })
  .await
  .expect("Failed to create schemas");

  // Command that uses invalid cypher to force GraphDB failure
  #[derive(Clone)]
  struct SaveFailCmd {
    data: TestSaveData,
  }
  impl DatabaseCommand for SaveFailCmd {
    type Output = ();
    type SaveData = TestSaveData;
    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::save_simple(
        "save_fail_test",
        std::sync::Arc::new(self.data.clone()),
        "INVALID CYPHER QUERY",
        Box::new(|_| ()),
      )
    }
  }

  let id = uuid::Uuid::now_v7();
  let result = db
    .execute(SaveFailCmd {
      data: TestSaveData {
        id,
        name: "test_fail".into(),
        value: 42,
      },
    })
    .await;
  assert!(result.is_err(), "Save with invalid cypher should fail");

  // Meta should be rolled back to empty
  let mut count_stream = db
    .execute(super::tests_helpers::CountTableRows {
      table: "save_fail_test",
    })
    .await
    .expect("count");
  let rows = count_stream.next().await.unwrap().unwrap();
  assert_eq!(rows, 0, "MetaDB should be empty after rollback");
}

// ==== Migration operation tests ====

#[tokio::test]
async fn test_execute_migration_operation() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  db.execute(super::tests_helpers::CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE MigrationTest (id UUID, name STRING, PRIMARY KEY(id))",
      "CREATE NODE TABLE MigrationTest2 (id UUID, value INT64, PRIMARY KEY(id))",
    ],
    meta_table: "migration_test",
    meta_schema: std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ])),
  })
  .await
  .expect("Migration should work");

  // Verify Graph table exists (count query works and returns 0)
  let mut g = db
    .execute(super::tests_helpers::CountGraphItems {
      label: "MigrationTest",
    })
    .await
    .expect("graph count");
  assert_eq!(g.next().await.unwrap().unwrap(), 0usize);

  // Verify Meta table exists
  let mut m = db
    .execute(super::tests_helpers::CountTableRows {
      table: "migration_test",
    })
    .await
    .expect("meta count");
  assert_eq!(m.next().await.unwrap().unwrap(), 0usize);
}

#[tokio::test]
async fn test_execute_migration_operation_with_failures() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  db.execute(super::tests_helpers::CreateSchema {
    graph_ddl: vec![
      "CREATE NODE TABLE MigrationFailTest (id UUID, name STRING, PRIMARY KEY(id))",
      "INVALID DDL STATEMENT",
      "CREATE NODE TABLE MigrationFailTest2 (id UUID, value INT64, PRIMARY KEY(id))",
    ],
    meta_table: "migration_fail_test",
    meta_schema: std::sync::Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
    ])),
  })
  .await
  .expect("Migration should proceed despite some DDL failures");

  // Verify valid tables were created
  let mut g = db
    .execute(super::tests_helpers::CountGraphItems {
      label: "MigrationFailTest",
    })
    .await
    .expect("graph count");
  assert_eq!(g.next().await.unwrap().unwrap(), 0usize);
}

// ==== Execute concurrency and error propagation tests ====

#[tokio::test]
async fn test_execute_concurrent_operations() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup a simple table to exercise meta path as well
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("name", DataType::Utf8, false),
  ]));
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
    .expect("create table");

  // Spawn multiple concurrent execute operations
  let mut handles = vec![];
  for i in 0..10 {
    let dbc = db.clone();
    handles.push(tokio::spawn(async move {
      if i % 2 == 0 {
        dbc.execute(TestSkipCommand { value: i }).await
      } else {
        Ok(format!("operation_{i}"))
      }
    }));
  }

  let results: Vec<_> = futures::future::join_all(handles).await;
  for (i, r) in results.iter().enumerate() {
    assert!(r.is_ok(), "task {i} should succeed");
    assert!(r.as_ref().unwrap().is_ok(), "inner {i} ok");
  }
}

#[tokio::test]
async fn test_execute_error_propagation() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

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
      DatabaseOperation::Query(QueryOperation::IndexThenStore {
        index: GraphIndexQuery {
          cypher: "MATCH (n:NonexistentNode) RETURN n.id".to_string(),
          params: vec![],
        },
        query_builder: Box::new(|graph_stream| {
          Box::pin(async move {
            use crate::{empty_filter, extract_uuids_to_in_clause};
            let (in_clause, uuids) = extract_uuids_to_in_clause(graph_stream).await?;
            if uuids.is_empty() {
              return Ok(PrimaryStoreQuery {
                table: "any_table",
                filter: Some(empty_filter()),
                limit: Some(0),
                offset: None,
                vector_search: None,
              });
            }
            Ok(PrimaryStoreQuery {
              table: "any_table",
              filter: Some(format!("uuid IN ({in_clause})")),
              limit: None,
              offset: None,
              vector_search: None,
            })
          })
        }),
        transformer: Box::new(|_g, _b| {
          Box::pin(async_stream::try_stream! { yield "should_not_reach".to_string(); })
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

// ==== Save with parameter mismatch ====

#[tokio::test]
async fn test_save_with_parameter_mismatch() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Create simple schema
  db.graph
    .execute_write(
      "CREATE NODE TABLE SimpleTest (id UUID, name STRING, PRIMARY KEY(id))",
      vec![],
    )
    .await
    .expect("create graph schema");

  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
  ]));
  let schema_clone = schema.clone();
  db.meta
    .execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("simple_test", schema_clone)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("create meta table");

  #[derive(Clone)]
  struct MismatchedData {
    id: uuid::Uuid,
    name: String,
    extra_field: String,
  }
  impl crate::ToDatabase for MismatchedData {
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
      std::sync::Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
      ]))
    }
    fn as_graph_value(
      &self,
      _ctx: Self::GraphContext,
    ) -> crate::Result<Vec<(&'static str, kuzu::Value)>> {
      Ok(vec![
        ("id", kuzu::Value::UUID(self.id)),
        ("name", kuzu::Value::String(self.name.clone())),
        ("extra_field", kuzu::Value::String(self.extra_field.clone())),
      ])
    }
    fn as_meta_value(&self, _ctx: Self::MetaContext) -> crate::Result<arrow_array::RecordBatch> {
      let schema = std::sync::Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
      ]));
      arrow_array::RecordBatch::try_new(
        schema,
        vec![
          std::sync::Arc::new(StringArray::from(vec![self.id.to_string()]))
            as arrow_array::ArrayRef,
          std::sync::Arc::new(StringArray::from(vec![self.name.clone()])) as arrow_array::ArrayRef,
        ],
      )
      .map_err(crate::Error::from)
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
        std::sync::Arc::new(self.data.clone()),
        r#"CREATE (:SimpleTest {id: $id, name: $name, extra_field: $extra_field})"#,
        Box::new(|_| ()),
      )
    }
  }

  let data = MismatchedData {
    id: uuid::Uuid::now_v7(),
    name: "Test".into(),
    extra_field: "nope".into(),
  };
  let result = db.execute(SaveWithMismatchCommand { data }).await;
  assert!(result.is_err(), "Save with parameter mismatch should fail");
}
