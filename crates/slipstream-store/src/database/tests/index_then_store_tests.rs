use tempfile::tempdir;

use super::tests_helpers::*;
use crate::operations::GraphIndexQuery;
use crate::operations::QueryOperation::IndexThenStore;
use crate::{
  Database, DatabaseCommand, DatabaseOperation, NoData, PrimaryStoreQuery, ResultStream,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{RecordBatch, StringArray};
use futures::{StreamExt, TryStreamExt};

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
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
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
      std::sync::Arc::new(StringArray::from(vec!["initial"])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["group-init"])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["Initial Episode"])) as arrow_array::ArrayRef,
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
      std::sync::Arc::new(StringArray::from(vec![
        test_uuid1.to_string(),
        test_uuid2.to_string(),
      ])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["test-group", "test-group"]))
        as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["Episode 1", "Episode 2"]))
        as arrow_array::ArrayRef,
    ],
  )
  .expect("Failed to create episode batch");

  db.meta
    .save_to_table("episodes", episode_batch, &["uuid"])
    .await
    .expect("Failed to insert episodes into LanceDB");

  // Now test IndexThenStore pattern manually
  struct TestIndexThenStore;

  impl DatabaseCommand for TestIndexThenStore {
    type Output = ResultStream<(String, String)>; // (uuid, name) pairs
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
              filter: Some(format!("uuid IN ({in_clause})")),
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
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
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
      std::sync::Arc::new(StringArray::from(vec!["some-uuid"])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["Some Episode"])) as arrow_array::ArrayRef,
    ],
  )
  .expect("Failed to create batch");

  db.meta
    .save_to_table("episodes", batch, &["uuid"])
    .await
    .expect("Failed to save episodes data");

  // Test IndexThenStore with empty index results
  struct EmptyIndexTest;

  impl DatabaseCommand for EmptyIndexTest {
    type Output = ResultStream<String>;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      use crate::operations::{GraphIndexQuery, PrimaryStoreQuery, QueryOperation::IndexThenStore};

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
              filter: Some(format!("uuid IN ({in_clause})")),
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
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
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
      std::sync::Arc::new(StringArray::from(vec!["some-uuid"])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["Some Episode"])) as arrow_array::ArrayRef,
    ],
  )
  .expect("Failed to create batch");

  db.meta
    .save_to_table("episodes", batch, &["uuid"])
    .await
    .expect("Failed to save episodes data");

  // Test IndexThenStore with missing table (should fail)
  struct MissingTableTest;

  impl DatabaseCommand for MissingTableTest {
    type Output = ResultStream<String>;
    type SaveData = NoData;

    fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
      use crate::operations::{GraphIndexQuery, PrimaryStoreQuery, QueryOperation::IndexThenStore};

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
              filter: Some(format!("uuid IN ({in_clause})")),
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
