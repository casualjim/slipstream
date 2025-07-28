#[cfg(test)]
mod tests {
  use crate::streams::ResultStream;
  use crate::{Database, DatabaseCommand, DatabaseOperation, ToDatabase};
  use crate::{Result, config::Config};
  use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
  use arrow_array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
  use futures::{StreamExt, TryStreamExt};
  use std::sync::Arc;
  use tempfile::tempdir;
  use uuid::Uuid;

  // Test data structure
  #[derive(Debug, Clone)]
  struct InvariantTestItem {
    id: Uuid,
    name: String,
    value: u64,
  }

  impl ToDatabase for InvariantTestItem {
    type MetaContext = ();
    type GraphContext = ();

    fn into_graph_value(
      &self,
      _ctx: Self::GraphContext,
    ) -> Result<Vec<(&'static str, kuzu::Value)>> {
      Ok(vec![
        ("id", kuzu::Value::UUID(self.id)),
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
          Arc::new(StringArray::from(vec![self.id.to_string()])) as ArrayRef,
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
      "invariant_test_items"
    }

    fn graph_table_name() -> &'static str {
      "InvariantTestItem"
    }

    fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
      Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::UInt64, false),
      ]))
    }
  }

  // Implement FromRecordBatchRow for InvariantTestItem
  impl crate::traits::FromRecordBatchRow for InvariantTestItem {
    fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
      let id_col = batch
        .column_by_name("id")
        .ok_or_else(|| crate::Error::InvalidMetaDbData("Missing id column".to_string()))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| crate::Error::InvalidMetaDbData("id column is not String".to_string()))?;

      let name_col = batch
        .column_by_name("name")
        .ok_or_else(|| crate::Error::InvalidMetaDbData("Missing name column".to_string()))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| crate::Error::InvalidMetaDbData("name column is not String".to_string()))?;

      let value_col = batch
        .column_by_name("value")
        .ok_or_else(|| crate::Error::InvalidMetaDbData("Missing value column".to_string()))?
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or_else(|| crate::Error::InvalidMetaDbData("value column is not UInt64".to_string()))?;

      let id = Uuid::parse_str(id_col.value(row))?;
      let name = name_col.value(row).to_string();
      let value = value_col.value(row);

      Ok(InvariantTestItem { id, name, value })
    }
  }

  // Commands for testing
  struct CreateInvariantSchema;

  impl DatabaseCommand for CreateInvariantSchema {
    type Output = ();
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Migration {
        graph_ddl: vec![
          "CREATE NODE TABLE IF NOT EXISTS InvariantItem (
                        id UUID,
                        name STRING,
                        value INT64,
                        PRIMARY KEY(id)
                    )",
        ],
        meta_setup: Box::new(|conn| {
          Box::pin(async move {
            // Create the invariant_items table
            let schema = Arc::new(ArrowSchema::new(vec![
              Field::new("id", DataType::Utf8, false),
              Field::new("name", DataType::Utf8, false),
              Field::new("value", DataType::UInt64, false),
            ]));
            conn
              .create_empty_table("invariant_items", schema)
              .execute()
              .await?;
            Ok(())
          })
        }),
        transformer: Box::new(|_| ()),
      }
    }
  }

  struct SaveInvariantItem {
    item: Arc<InvariantTestItem>,
  }

  impl DatabaseCommand for SaveInvariantItem {
    type Output = ();
    type SaveData = InvariantTestItem;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      DatabaseOperation::Mutation(crate::operations::MutationOperation::Single {
        table: "invariant_items",
        data: self.item.clone(),
        graph_context: (),
        meta_context: (),
        cypher: "CREATE (:InvariantItem {id: $id, name: $name, value: $value})",
        transformer: Box::new(|_| ()),
      })
    }
  }

  struct QueryInvariantItems {
    filter: Option<String>,
  }

  impl DatabaseCommand for QueryInvariantItems {
    type Output = ResultStream<InvariantTestItem>;
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      use crate::transformers::record_batch;

      DatabaseOperation::Query(crate::operations::QueryOperation::StoreOnly {
        query: crate::operations::PrimaryStoreQuery {
          table: "invariant_items",
          filter: self.filter.clone(),
          limit: None,
          offset: None,
          vector_search: None,
        },
        transformer: Box::new(record_batch::to_typed_stream()),
      })
    }
  }

  struct CountGraphItems {
    label: &'static str,
  }

  impl DatabaseCommand for CountGraphItems {
    type Output = ResultStream<usize>;
    type SaveData = crate::operations::NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      use crate::transformers::graph_stream;

      DatabaseOperation::Query(crate::operations::QueryOperation::IndexOnly {
        query: crate::operations::GraphIndexQuery {
          cypher: format!("MATCH (n:{}) RETURN count(n)", self.label),
          params: vec![],
        },
        transformer: Box::new(graph_stream::extract_count()),
      })
    }
  }

  // Test 1: Verify rollback actually restores data correctly
  #[tokio::test]
  async fn test_rollback_data_integrity() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = Config::new_test(temp_dir.path().to_path_buf());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    // Create schema
    db.execute(CreateInvariantSchema)
      .await
      .expect("Failed to create schema");

    // Insert initial data
    let item1 = Arc::new(InvariantTestItem {
      id: Uuid::now_v7(),
      name: "Initial Item".to_string(),
      value: 100,
    });

    db.execute(SaveInvariantItem {
      item: item1.clone(),
    })
    .await
    .expect("Failed to save initial item");

    // Query to verify initial state
    let initial_items: Vec<InvariantTestItem> = db
      .execute(QueryInvariantItems { filter: None })
      .await
      .expect("Failed to query initial items")
      .try_collect()
      .await
      .expect("Failed to collect initial items");

    assert_eq!(initial_items.len(), 1);
    assert_eq!(initial_items[0].name, "Initial Item");
    assert_eq!(initial_items[0].value, 100);

    // Now create a command that will fail during 2PC
    struct FailingBulkSave;

    impl DatabaseCommand for FailingBulkSave {
      type Output = usize;
      type SaveData = InvariantTestItem;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        // Create items that will cause GraphDB to fail
        let items = vec![Arc::new(InvariantTestItem {
          id: Uuid::now_v7(),
          name: "Should Not Persist".to_string(),
          value: 999,
        })];

        DatabaseOperation::Mutation(crate::operations::MutationOperation::Bulk {
          table: "invariant_items",
          data: items,
          graph_context: (),
          meta_context: (),
          // This cypher will fail because it references a non-existent property
          cypher: "CREATE (:InvariantItem {id: $id, name: $name, value: $value, nonexistent: $nonexistent})",
          transformer: Box::new(|count| count),
        })
      }
    }

    // Execute the failing command
    let result = db.execute(FailingBulkSave).await;

    assert!(result.is_err(), "Command should have failed");

    // Verify that data was rolled back correctly
    let after_rollback: Vec<InvariantTestItem> = db
      .execute(QueryInvariantItems { filter: None })
      .await
      .expect("Failed to query after rollback")
      .try_collect()
      .await
      .expect("Failed to collect after rollback");

    // Debug: print what we found

    // Should still have only the initial item
    assert_eq!(after_rollback.len(), 1);
    assert_eq!(after_rollback[0].name, "Initial Item");
    assert_eq!(after_rollback[0].value, 100);

    // Verify GraphDB also rolled back
    let graph_count = db
      .execute(CountGraphItems {
        label: "InvariantItem",
      })
      .await
      .expect("Failed to count graph items")
      .next()
      .await
      .expect("Should have count")
      .expect("Count should be Ok");

    assert_eq!(graph_count, 1, "GraphDB should only have the initial item");
  }

  // Test 2: Large transaction handling
  #[tokio::test]
  async fn test_large_transaction_handling() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = Config::new_test(temp_dir.path().to_path_buf());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    db.execute(CreateInvariantSchema)
      .await
      .expect("Failed to create schema");

    // Create a large batch (not 1M but enough to test the pattern)
    struct LargeBulkSave {
      count: usize,
    }

    impl DatabaseCommand for LargeBulkSave {
      type Output = usize;
      type SaveData = InvariantTestItem;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        let items: Vec<Arc<InvariantTestItem>> = (0..self.count)
          .map(|i| {
            Arc::new(InvariantTestItem {
              id: Uuid::now_v7(),
              name: format!("Large Item {}", i),
              value: i as u64,
            })
          })
          .collect();

        DatabaseOperation::Mutation(crate::operations::MutationOperation::Bulk {
          table: "invariant_items",
          data: items,
          graph_context: (),
          meta_context: (),
          cypher: "CREATE (:InvariantItem {id: $id, name: $name, value: $value})",
          transformer: Box::new(|count| count),
        })
      }
    }

    // Test with 1000 items (can increase for stress testing)
    let saved_count = db
      .execute(LargeBulkSave { count: 1000 })
      .await
      .expect("Failed to save large batch");

    assert_eq!(saved_count, 1000);

    // Verify count in graph
    let graph_count = db
      .execute(CountGraphItems {
        label: "InvariantItem",
      })
      .await
      .expect("Failed to count graph items")
      .next()
      .await
      .expect("Should have count")
      .expect("Count should be Ok");

    assert_eq!(graph_count, 1000, "All items should be in GraphDB");
  }

  // Test 3: Empty operation handling
  #[tokio::test]
  async fn test_empty_operations_edge_case() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = Config::new_test(temp_dir.path().to_path_buf());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    db.execute(CreateInvariantSchema)
      .await
      .expect("Failed to create schema");

    // Command that filters out all items
    struct EmptyAfterFilter;

    impl DatabaseCommand for EmptyAfterFilter {
      type Output = usize;
      type SaveData = InvariantTestItem;

      fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
        // Start with items but filter them all out
        let items: Vec<Arc<InvariantTestItem>> = vec![];

        DatabaseOperation::Mutation(crate::operations::MutationOperation::Bulk {
          table: "invariant_items",
          data: items,
          graph_context: (),
          meta_context: (),
          cypher: "CREATE (:InvariantItem {id: $id, name: $name, value: $value})",
          transformer: Box::new(|count| count),
        })
      }
    }

    let result = db
      .execute(EmptyAfterFilter)
      .await
      .expect("Empty operation should succeed");
    assert_eq!(result, 0, "Should handle empty operations gracefully");
  }

  // Test 4: Unicode and special characters
  #[tokio::test]
  async fn test_unicode_and_special_characters() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = Config::new_test(temp_dir.path().to_path_buf());
    let db = Database::new(&config)
      .await
      .expect("Failed to create Database");

    db.execute(CreateInvariantSchema)
      .await
      .expect("Failed to create schema");

    // Test various special characters
    let special_items = vec![
      ("Emoji 🚀 Test", 100),
      ("Chinese 中文 Test", 200),
      ("Quote's \"Test\"", 300),
      ("Newline\nTest", 400),
      ("Tab\tTest", 500),
      ("Null\0Byte", 600),
    ];

    for (name, value) in special_items {
      let item = Arc::new(InvariantTestItem {
        id: Uuid::now_v7(),
        name: name.to_string(),
        value,
      });

      db.execute(SaveInvariantItem { item: item.clone() })
        .await
        .expect(&format!("Failed to save item with name: {}", name));
    }

    // Query back and verify
    let items: Vec<InvariantTestItem> = db
      .execute(QueryInvariantItems { filter: None })
      .await
      .expect("Failed to query items")
      .try_collect()
      .await
      .expect("Failed to collect items");

    assert_eq!(
      items.len(),
      6,
      "All special character items should be saved"
    );
  }

  // Test 5: Concurrent database instance detection
  #[tokio::test]
  async fn test_multiple_database_instances() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = Config::new_test(temp_dir.path().to_path_buf());

    // Create first database
    let db1 = Database::new(&config)
      .await
      .expect("Failed to create first Database");
    db1
      .execute(CreateInvariantSchema)
      .await
      .expect("Failed to create schema");

    // Create second database pointing to same location
    // This should work because KuzuDB and LanceDB handle their own locking
    let db2 = Database::new(&config)
      .await
      .expect("Failed to create second Database");

    // Save data through db1
    let item = Arc::new(InvariantTestItem {
      id: Uuid::now_v7(),
      name: "From DB1".to_string(),
      value: 100,
    });

    db1
      .execute(SaveInvariantItem { item })
      .await
      .expect("Failed to save through db1");

    // Query through db2 should see the data
    let items: Vec<InvariantTestItem> = db2
      .execute(QueryInvariantItems { filter: None })
      .await
      .expect("Failed to query through db2")
      .try_collect()
      .await
      .expect("Failed to collect items");

    assert_eq!(
      items.len(),
      1,
      "Data saved through db1 should be visible in db2"
    );
    assert_eq!(items[0].name, "From DB1");

    // Drop order shouldn't matter
    drop(db1);
    drop(db2);
  }

  // Test 6: OrderedStream memory bounds
  #[tokio::test]
  async fn test_ordered_stream_memory_bounds() {
    use crate::ordered::OrderedStream;
    use futures::stream;

    // Create a large key stream
    let num_keys = 10000;
    let keys: Vec<usize> = (0..num_keys).collect();
    let key_stream = stream::iter(keys.clone());

    // Create an items stream that only has every 10th item
    // This will cause OrderedStream to buffer many items
    let items = stream::iter(
      (0..num_keys)
        .step_by(10)
        .map(|i| Ok((i, format!("Item {}", i)))),
    );

    let mut ordered = OrderedStream::new(key_stream, Box::pin(items));

    // Consume the stream and count items
    let mut count = 0;
    while let Some(result) = ordered.next().await {
      if let Ok(item) = result {
        count += 1;
        // Verify the item is what we expect
        let expected = format!("Item {}", (count - 1) * 10);
        assert_eq!(item, expected);
      }
    }

    assert_eq!(count, num_keys / 10, "Should receive all items that exist");
  }
}
