use super::tests_helpers::*;
use crate::Database;
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{Int64Array, RecordBatch, StringArray};
use futures::{StreamExt, TryStreamExt};
use tempfile::tempdir;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[tokio::test]
async fn test_high_load_mixed_operations() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let actor = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Setup
  actor
    .graph
    .execute_write(
      "CREATE NODE TABLE LoadTest (id INT64, type STRING, PRIMARY KEY(id))",
      vec![],
    )
    .await
    .expect("Failed to create schema");

  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("type", DataType::Utf8, false),
  ]));

  // Create the table using execute_setup
  let schema_for_create = schema.clone();
  actor
    .meta
    .execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("load_test", schema_for_create)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

  // Launch 100 mixed operations
  let mut handles = vec![];

  for i in 0..100 {
    let actor_clone = actor.clone();
    let schema_clone = schema.clone();

    let handle = tokio::spawn(async move {
      match i % 4 {
        0 => {
          // Graph write
          actor_clone
            .graph
            .execute_write(
              "CREATE (:LoadTest {id: $id, type: $type})",
              vec![
                ("id", kuzu::Value::Int64(i)),
                ("type", kuzu::Value::String("graph_write".to_string())),
              ],
            )
            .await
            .map(|_| "graph_write")
        }
        1 => {
          // Graph read
          let receiver = actor_clone
            .graph
            .execute_query(
              "MATCH (n:LoadTest) WHERE n.id < $limit RETURN count(n)",
              vec![("limit", kuzu::Value::Int64(i))],
            )
            .await?;

          let mut stream = UnboundedReceiverStream::new(receiver);
          let _ = stream.next().await;
          Ok("graph_read")
        }
        2 => {
          // Meta write
          let batch = RecordBatch::try_new(
            schema_clone,
            vec![
              std::sync::Arc::new(Int64Array::from(vec![i])) as arrow_array::ArrayRef,
              std::sync::Arc::new(StringArray::from(vec!["meta_write"])) as arrow_array::ArrayRef,
            ],
          )?;

          actor_clone
            .meta
            .save_to_table("load_test", batch, &["id"])
            .await
            .map(|_| "meta_write")
        }
        _ => {
          // Meta read
          let filter = format!("id < {i}");
          let stream = actor_clone
            .meta
            .query_table(&crate::PrimaryStoreQuery {
              table: "load_test",
              filter: Some(filter),
              limit: Some(10),
              offset: None,
              vector_search: None,
            })
            .await?;

          let _ = stream.try_collect::<Vec<_>>().await?;
          Ok("meta_read")
        }
      }
    });
    handles.push(handle);
  }

  // Collect results
  let results: Vec<_> = futures::future::join_all(handles).await;

  // Count successes by type
  let mut success_counts = std::collections::HashMap::new();
  for op_type in results.into_iter().flatten().flatten() {
    *success_counts.entry(op_type).or_insert(0) += 1;
  }

  // All operations should succeed
  let total_success: usize = success_counts.values().sum();
  assert_eq!(total_success, 100, "All 100 operations should succeed");
}
