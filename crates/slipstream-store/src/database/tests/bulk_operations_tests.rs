
use crate::streams::ResultStream;
use crate::{Database, DatabaseCommand, DatabaseOperation, ToDatabase};
use crate::{Result, config::Config};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
use futures::{StreamExt, TryStreamExt};
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

// Test data structure that implements ToDatabase
#[derive(Debug, Clone)]
struct TestItem {
  id: String,
  name: String,
  value: u64,
}

impl ToDatabase for TestItem {
  type MetaContext = ();
  type GraphContext = ();

  fn into_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    Ok(vec![
      ("id", kuzu::Value::String(self.id.clone())),
      ("name", kuzu::Value::String(self.name.clone())),
      ("value", kuzu::Value::Int64(self.value as i64)),
    ])
  }

  fn into_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
    let schema = Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::UInt64, false),
    ]));

    RecordBatch::try_new(
      schema,
      vec![
        Arc::new(StringArray::from(vec![self.id.clone()])) as ArrayRef,
        Arc::new(StringArray::from(vec![self.name.clone()])) as ArrayRef,
        Arc::new(UInt64Array::from(vec![self.value])) as ArrayRef,
      ],
    )
    .map_err(Into::into)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["id"]
  }

  fn meta_table_name() -> &'static str {
    "test_items"
  }

  fn graph_table_name() -> &'static str {
    "TestItem"
  }

  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    Arc::new(ArrowSchema::new(vec![
      Field::new("id", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("value", DataType::UInt64, false),
    ]))
  }
}

fn test_config(path: &std::path::Path) -> Config {
  Config::new_test(path.to_path_buf())
}

// Command for bulk saving test items
struct BulkSaveTestItems {
  table: &'static str,
  items: Vec<Arc<TestItem>>,
  cypher: &'static str,
}

impl DatabaseCommand for BulkSaveTestItems {
  type Output = usize;
  type SaveData = TestItem;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Mutation(crate::operations::MutationOperation::Bulk {
      table: self.table,
      data: self.items.clone(),
      graph_context: (),
      meta_context: (),
      cypher: self.cypher,
      transformer: Box::new(|count| count),
    })
  }
}

// Command for querying graph DB
struct QueryGraphItems {
  cypher: String,
}

impl DatabaseCommand for QueryGraphItems {
  type Output = ResultStream<String>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    use crate::transformers::graph_stream;

    DatabaseOperation::Query(crate::operations::QueryOperation::IndexOnly {
      query: crate::operations::GraphIndexQuery {
        cypher: self.cypher.clone(),
        params: vec![],
      },
      transformer: Box::new(graph_stream::extract_strings()),
    })
  }
}

// Command for querying meta DB
struct QueryMetaItemCount {
  table: &'static str,
}

impl DatabaseCommand for QueryMetaItemCount {
  type Output = ResultStream<usize>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    use crate::transformers::record_batch;

    DatabaseOperation::Query(crate::operations::QueryOperation::StoreOnly {
      query: crate::operations::PrimaryStoreQuery {
        table: self.table,
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      },
      transformer: Box::new(record_batch::count_rows()),
    })
  }
}

// Command for counting items in graph DB
struct CountGraphItems {
  cypher: String,
}

impl DatabaseCommand for CountGraphItems {
  type Output = ResultStream<usize>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    use crate::transformers::graph_stream;

    DatabaseOperation::Query(crate::operations::QueryOperation::IndexOnly {
      query: crate::operations::GraphIndexQuery {
        cypher: self.cypher.clone(),
        params: vec![],
      },
      transformer: Box::new(graph_stream::extract_count()),
    })
  }
}

// Migration command for creating test schema
struct CreateTestSchema;

impl DatabaseCommand for CreateTestSchema {
  type Output = ();
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: vec![
        "CREATE NODE TABLE IF NOT EXISTS TestItem (
                        id STRING,
                        name STRING,
                        value INT64,
                        PRIMARY KEY(id)
                    )",
      ],
      meta_setup: Box::new(|conn| {
        Box::pin(async move {
          // Create the test_items table
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("value", DataType::UInt64, false),
          ]));
          conn
            .create_empty_table("test_items", schema)
            .execute()
            .await?;
          Ok(())
        })
      }),
      transformer: Box::new(|_| ()),
    }
  }
}

// Command for querying test items with multiple return columns
struct QueryTestItems {
  cypher: String,
}

impl DatabaseCommand for QueryTestItems {
  type Output = ResultStream<(String, String, i64)>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Query(crate::operations::QueryOperation::IndexOnly {
      query: crate::operations::GraphIndexQuery {
        cypher: self.cypher.clone(),
        params: vec![],
      },
      transformer: Box::new(|stream| {
        Box::pin(async_stream::try_stream! {
            use futures::StreamExt;
            let mut pinned_stream = std::pin::pin!(stream);
            while let Some(result) = pinned_stream.next().await {
                let values = result?;
                if values.len() == 3 {
                    if let (
                        Some(kuzu::Value::String(id)),
                        Some(kuzu::Value::String(name)),
                        Some(kuzu::Value::Int64(value)),
                    ) = (values.get(0), values.get(1), values.get(2))
                    {
                        yield (id.clone(), name.clone(), *value);
                    }
                }
            }
        })
      }),
    })
  }
}

// Command for creating ContextualItem schema
struct CreateContextualItemSchema;

impl DatabaseCommand for CreateContextualItemSchema {
  type Output = ();
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: vec![
        "CREATE NODE TABLE IF NOT EXISTS ContextualItem (
                        id STRING,
                        category STRING,
                        score DOUBLE,
                        PRIMARY KEY(id)
                    )",
      ],
      meta_setup: Box::new(|conn| {
        Box::pin(async move {
          // Create the contextual_items table
          let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("category", DataType::Utf8, false),
            Field::new("adjusted_score", DataType::Float64, false),
          ]));
          conn
            .create_empty_table("contextual_items", schema)
            .execute()
            .await?;
          Ok(())
        })
      }),
      transformer: Box::new(|_| ()),
    }
  }
}

async fn setup_test_db() -> (Database, tempfile::TempDir) {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Create test schema using Migration command
  db.execute(CreateTestSchema)
    .await
    .expect("Failed to run migration");

  (db, temp_dir)
}

#[tokio::test]
async fn test_bulk_save_create_mode() {
  let (db, temp_dir) = setup_test_db().await;

  // Create test items
  let items: Vec<Arc<TestItem>> = (0..5)
    .map(|i| {
      Arc::new(TestItem {
        id: format!("item-{}", i),
        name: format!("Test Item {}", i),
        value: i as u64 * 10,
      })
    })
    .collect();

  // Bulk save command
  let bulk_save_cmd = BulkSaveTestItems {
    table: "test_items",
    items: items.clone(),
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  let saved_count = db
    .execute(bulk_save_cmd)
    .await
    .expect("Failed to bulk save");
  assert_eq!(saved_count, 5, "Should have saved 5 items");

  // Verify data was saved to both databases using Query commands

  // Check GraphDB
  let graph_query = QueryGraphItems {
    cypher: "MATCH (n:TestItem) RETURN n.id ORDER BY n.id".to_string(),
  };

  let graph_ids: Vec<String> = db
    .execute(graph_query)
    .await
    .expect("Failed to query graph")
    .try_collect()
    .await
    .expect("Failed to collect graph results");
  assert_eq!(graph_ids.len(), 5, "Should find 5 items in GraphDB");
  assert_eq!(
    graph_ids,
    vec!["item-0", "item-1", "item-2", "item-3", "item-4"]
  );

  // Check MetaDB
  let meta_query = QueryMetaItemCount {
    table: "test_items",
  };

  let mut meta_count_stream = db.execute(meta_query).await.expect("Failed to query meta");
  let meta_count = meta_count_stream
    .next()
    .await
    .expect("Should have count")
    .expect("Count should be Ok");
  assert_eq!(meta_count, 5, "Should find 5 items in MetaDB");

  // Drop database before temp_dir to avoid lingering handles
  drop(db);
  drop(temp_dir);
}

#[tokio::test]
async fn test_bulk_save_append_mode() {
  let (db, temp_dir) = setup_test_db().await;

  // Create append_test table
  struct CreateAppendTestTable;
  impl DatabaseCommand for CreateAppendTestTable {
    type Output = ();
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Migration {
        graph_ddl: vec![],
        meta_setup: Box::new(|conn| {
          Box::pin(async move {
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
              Field::new("value", DataType::UInt64, false),
            ]));
            conn
              .create_empty_table("append_test", schema)
              .execute()
              .await?;
            Ok(())
          })
        }),
        transformer: Box::new(|_| ()),
      }
    }
  }
  db.execute(CreateAppendTestTable)
    .await
    .expect("Failed to create append_test table");

  // First, create initial data
  let initial_items: Vec<Arc<TestItem>> = vec![Arc::new(TestItem {
    id: "existing-1".to_string(),
    name: "Existing Item".to_string(),
    value: 100,
  })];

  let create_cmd = BulkSaveTestItems {
    table: "append_test",
    items: initial_items,
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  db.execute(create_cmd)
    .await
    .expect("Failed to create initial data");

  // Now bulk append new items
  let new_items: Vec<Arc<TestItem>> = (0..3)
    .map(|i| {
      Arc::new(TestItem {
        id: format!("new-{}", i),
        name: format!("New Item {}", i),
        value: i as u64 * 20,
      })
    })
    .collect();

  let append_cmd = BulkSaveTestItems {
    table: "append_test",
    items: new_items,
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  let appended_count = db.execute(append_cmd).await.expect("Failed to bulk append");
  assert_eq!(appended_count, 3, "Should have appended 3 items");

  // Verify total count
  let count_query = CountGraphItems {
    cypher: "MATCH (n:TestItem) RETURN count(n)".to_string(),
  };

  let mut count_stream = db
    .execute(count_query)
    .await
    .expect("Failed to count items");
  let total_count = count_stream
    .next()
    .await
    .expect("Should have count")
    .expect("Count should be Ok");
  assert_eq!(
    total_count, 4,
    "Should have 4 total items (1 initial + 3 appended)"
  );

  drop(db);
  drop(temp_dir);
}

#[tokio::test]
async fn test_bulk_save_merge_insert_mode() {
  let (db, temp_dir) = setup_test_db().await;

  // Create merge_test table
  struct CreateMergeTestTable;
  impl DatabaseCommand for CreateMergeTestTable {
    type Output = ();
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Migration {
        graph_ddl: vec![],
        meta_setup: Box::new(|conn| {
          Box::pin(async move {
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
              Field::new("value", DataType::UInt64, false),
            ]));
            conn
              .create_empty_table("merge_test", schema)
              .execute()
              .await?;
            Ok(())
          })
        }),
        transformer: Box::new(|_| ()),
      }
    }
  }
  db.execute(CreateMergeTestTable)
    .await
    .expect("Failed to create merge_test table");

  // Create initial items
  let initial_items: Vec<Arc<TestItem>> = vec![
    Arc::new(TestItem {
      id: "merge-1".to_string(),
      name: "Original Name 1".to_string(),
      value: 10,
    }),
    Arc::new(TestItem {
      id: "merge-2".to_string(),
      name: "Original Name 2".to_string(),
      value: 20,
    }),
  ];

  let create_cmd = BulkSaveTestItems {
    table: "merge_test",
    items: initial_items,
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  db.execute(create_cmd)
    .await
    .expect("Failed to create initial data");

  // Now bulk merge - update existing and insert new
  let merge_items: Vec<Arc<TestItem>> = vec![
    Arc::new(TestItem {
      id: "merge-1".to_string(), // Existing ID - should update
      name: "Updated Name 1".to_string(),
      value: 100,
    }),
    Arc::new(TestItem {
      id: "merge-3".to_string(), // New ID - should insert
      name: "New Item 3".to_string(),
      value: 30,
    }),
  ];

  let merge_cmd = BulkSaveTestItems {
    table: "merge_test",
    items: merge_items,
    cypher: "MERGE (n:TestItem {id: $id}) SET n.name = $name, n.value = $value",
  };

  let merged_count = db.execute(merge_cmd).await.expect("Failed to bulk merge");
  assert_eq!(merged_count, 2, "Should have processed 2 items");

  // Verify the results
  let verify_query = QueryTestItems {
    cypher: "MATCH (n:TestItem) RETURN n.id, n.name, n.value ORDER BY n.id".to_string(),
  };

  let items: Vec<(String, String, i64)> = db
    .execute(verify_query)
    .await
    .expect("Failed to verify merge")
    .try_collect()
    .await
    .expect("Failed to collect results");
  assert_eq!(items.len(), 3, "Should have 3 total items");

  // Check that merge-1 was updated
  assert!(
    items
      .iter()
      .any(|(id, name, value)| { id == "merge-1" && name == "Updated Name 1" && *value == 100 }),
    "merge-1 should be updated"
  );

  // Check that merge-2 remained unchanged
  assert!(
    items
      .iter()
      .any(|(id, name, value)| { id == "merge-2" && name == "Original Name 2" && *value == 20 }),
    "merge-2 should be unchanged"
  );

  // Check that merge-3 was inserted
  assert!(
    items
      .iter()
      .any(|(id, name, value)| { id == "merge-3" && name == "New Item 3" && *value == 30 }),
    "merge-3 should be inserted"
  );

  drop(db);
  drop(temp_dir);
}

#[tokio::test]
async fn test_bulk_save_with_empty_data() {
  let (db, temp_dir) = setup_test_db().await;

  // Create empty_test table
  struct CreateEmptyTestTable;
  impl DatabaseCommand for CreateEmptyTestTable {
    type Output = ();
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Migration {
        graph_ddl: vec![],
        meta_setup: Box::new(|conn| {
          Box::pin(async move {
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
              Field::new("value", DataType::UInt64, false),
            ]));
            conn
              .create_empty_table("empty_test", schema)
              .execute()
              .await?;
            Ok(())
          })
        }),
        transformer: Box::new(|_| ()),
      }
    }
  }
  db.execute(CreateEmptyTestTable)
    .await
    .expect("Failed to create empty_test table");

  // Test bulk save with empty vector
  let empty_items: Vec<Arc<TestItem>> = vec![];

  let bulk_cmd = BulkSaveTestItems {
    table: "empty_test",
    items: empty_items,
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  let saved_count = db
    .execute(bulk_cmd)
    .await
    .expect("Failed to bulk save empty");
  assert_eq!(saved_count, 0, "Should have saved 0 items");

  drop(db);
  drop(temp_dir);
}

#[tokio::test]
async fn test_bulk_save_large_batch() {
  let (db, temp_dir) = setup_test_db().await;

  // Create large_batch_test table
  struct CreateLargeBatchTestTable;
  impl DatabaseCommand for CreateLargeBatchTestTable {
    type Output = ();
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Migration {
        graph_ddl: vec![],
        meta_setup: Box::new(|conn| {
          Box::pin(async move {
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
              Field::new("value", DataType::UInt64, false),
            ]));
            conn
              .create_empty_table("large_batch_test", schema)
              .execute()
              .await?;
            Ok(())
          })
        }),
        transformer: Box::new(|_| ()),
      }
    }
  }
  db.execute(CreateLargeBatchTestTable)
    .await
    .expect("Failed to create large_batch_test table");

  // Create a large batch of items
  let large_batch: Vec<Arc<TestItem>> = (0..100)
    .map(|i| {
      Arc::new(TestItem {
        id: Uuid::now_v7().to_string(),
        name: format!("Large Batch Item {}", i),
        value: i as u64,
      })
    })
    .collect();

  let bulk_cmd = BulkSaveTestItems {
    table: "large_batch_test",
    items: large_batch.clone(),
    cypher: "CREATE (:TestItem {id: $id, name: $name, value: $value})",
  };

  let saved_count = db
    .execute(bulk_cmd)
    .await
    .expect("Failed to bulk save large batch");
  assert_eq!(saved_count, 100, "Should have saved 100 items");

  // Verify count in both databases
  let graph_count_query = CountGraphItems {
    cypher: "MATCH (n:TestItem) RETURN count(n)".to_string(),
  };

  let mut count_stream = db
    .execute(graph_count_query)
    .await
    .expect("Failed to count in graph");
  let graph_count = count_stream
    .next()
    .await
    .expect("Should have count")
    .expect("Count should be Ok");
  assert_eq!(graph_count, 100, "GraphDB should have 100 items");

  let meta_count_query = QueryMetaItemCount {
    table: "large_batch_test",
  };

  let mut count_stream = db
    .execute(meta_count_query)
    .await
    .expect("Failed to count in meta");
  let meta_count = count_stream
    .next()
    .await
    .expect("Should have count")
    .expect("Count should be Ok");
  assert_eq!(meta_count, 100, "MetaDB should have 100 items");

  drop(db);
  drop(temp_dir);
}

#[tokio::test]
async fn test_bulk_save_with_contexts() {
  let (db, temp_dir) = setup_test_db().await;

  // Define a type with non-unit contexts
  #[derive(Debug, Clone)]
  struct ContextualItem {
    id: String,
    category: String,
    score: f64,
  }

  #[derive(Debug, Clone, Default)]
  struct GraphContext {
    prefix: String,
  }

  #[derive(Debug, Clone, Default)]
  struct MetaContext {
    multiplier: f64,
  }

  impl ToDatabase for ContextualItem {
    type GraphContext = GraphContext;
    type MetaContext = MetaContext;

    fn into_graph_value(
      &self,
      ctx: Self::GraphContext,
    ) -> Result<Vec<(&'static str, kuzu::Value)>> {
      Ok(vec![
        (
          "id",
          kuzu::Value::String(format!("{}-{}", ctx.prefix, self.id)),
        ),
        ("category", kuzu::Value::String(self.category.clone())),
        ("score", kuzu::Value::Double(self.score)),
      ])
    }

    fn into_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch> {
      let schema = Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("adjusted_score", DataType::Float64, false),
      ]));

      RecordBatch::try_new(
        schema,
        vec![
          Arc::new(StringArray::from(vec![self.id.clone()])) as ArrayRef,
          Arc::new(StringArray::from(vec![self.category.clone()])) as ArrayRef,
          Arc::new(arrow_array::Float64Array::from(vec![
            self.score * ctx.multiplier,
          ])) as ArrayRef,
        ],
      )
      .map_err(Into::into)
    }

    fn primary_key_columns() -> &'static [&'static str] {
      &["id"]
    }

    fn meta_table_name() -> &'static str {
      "contextual_items"
    }

    fn graph_table_name() -> &'static str {
      "ContextualItem"
    }

    fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
      Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("adjusted_score", DataType::Float64, false),
      ]))
    }
  }

  // Create test items
  let items: Vec<Arc<ContextualItem>> = vec![
    Arc::new(ContextualItem {
      id: "ctx-1".to_string(),
      category: "A".to_string(),
      score: 0.5,
    }),
    Arc::new(ContextualItem {
      id: "ctx-2".to_string(),
      category: "B".to_string(),
      score: 0.8,
    }),
  ];

  // First create the node table
  db.execute(CreateContextualItemSchema)
    .await
    .expect("Failed to create schema");

  // Create a command for bulk saving with contexts
  struct BulkSaveContextualItems {
    items: Vec<Arc<ContextualItem>>,
    graph_context: GraphContext,
    meta_context: MetaContext,
  }

  impl DatabaseCommand for BulkSaveContextualItems {
    type Output = usize;
    type SaveData = ContextualItem;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Mutation(crate::operations::MutationOperation::Bulk {
        table: "contextual_items",
        data: self.items.clone(),
        graph_context: self.graph_context.clone(),
        meta_context: self.meta_context.clone(),
        cypher: "CREATE (:ContextualItem {id: $id, category: $category, score: $score})",
        transformer: Box::new(|count| count),
      })
    }
  }

  // Bulk save with contexts
  let bulk_cmd = BulkSaveContextualItems {
    items,
    graph_context: GraphContext {
      prefix: "TEST".to_string(),
    },
    meta_context: MetaContext { multiplier: 2.0 },
  };

  let saved_count = db
    .execute(bulk_cmd)
    .await
    .expect("Failed to bulk save with contexts");
  assert_eq!(saved_count, 2, "Should have saved 2 items");

  // Verify the context was applied in GraphDB
  let graph_query = CountGraphItems {
    cypher: "MATCH (n:ContextualItem) WHERE n.id STARTS WITH 'TEST-' RETURN count(n)".to_string(),
  };

  let mut count_stream = db
    .execute(graph_query)
    .await
    .expect("Failed to verify context");
  let prefixed_count = count_stream
    .next()
    .await
    .expect("Should have count")
    .expect("Count should be Ok");
  assert_eq!(
    prefixed_count, 2,
    "All items should have TEST- prefix from context"
  );

  drop(db);
  drop(temp_dir);
}
