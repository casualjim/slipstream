use super::tests_helpers::*;
use crate::{Database, PrimaryStoreQuery};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_array::{FixedSizeListArray, Float32Array, Int64Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use tempfile::tempdir;

#[tokio::test]
async fn test_vector_search_through_database() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Create a table with vector column
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("name", DataType::Utf8, false),
    Field::new(
      "embedding",
      DataType::FixedSizeList(
        std::sync::Arc::new(Field::new("item", DataType::Float32, false)),
        4,
      ),
      false,
    ),
  ]));

  let schema_for_create = schema.clone();
  db.meta
    .execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("vec_test", schema_for_create)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("Failed to create table");

  // Build embeddings array (2 rows)
  let values: Vec<f32> = vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
  let value_array = Float32Array::from(values);
  let field = std::sync::Arc::new(Field::new("item", DataType::Float32, false));
  let embedding = FixedSizeListArray::try_new(
    field,
    4,
    std::sync::Arc::new(value_array) as arrow_array::ArrayRef,
    None,
  )
  .expect("fixed size list");

  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      std::sync::Arc::new(Int64Array::from(vec![1, 2])) as arrow_array::ArrayRef,
      std::sync::Arc::new(StringArray::from(vec!["a", "b"])) as arrow_array::ArrayRef,
      std::sync::Arc::new(embedding) as arrow_array::ArrayRef,
    ],
  )
  .expect("batch");

  db.meta
    .save_to_table("vec_test", batch, &["id"])
    .await
    .expect("save");

  // Query with vector search params
  let params = crate::operations::VectorSearchParams {
    vector: vec![1.0, 0.0, 0.0, 0.0],
    column: "embedding",
    distance_type: lancedb::DistanceType::Cosine,
  };

  let stream = db
    .meta
    .query_table(&PrimaryStoreQuery {
      table: "vec_test",
      filter: None,
      limit: Some(1),
      offset: None,
      vector_search: Some(params),
    })
    .await
    .expect("query");

  let batches: Vec<_> = stream.try_collect().await.expect("collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 1, "Should return nearest neighbor");
}

#[tokio::test]
async fn test_vector_search_edge_cases() {
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config).await.expect("db");

  // Create minimal table with vector
  let schema = std::sync::Arc::new(ArrowSchema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new(
      "embedding",
      DataType::FixedSizeList(
        std::sync::Arc::new(Field::new("item", DataType::Float32, false)),
        4,
      ),
      false,
    ),
  ]));
  let schema_for_create = schema.clone();
  db.meta
    .execute_setup(Box::new(move |conn| {
      Box::pin(async move {
        conn
          .create_empty_table("vec_edge", schema_for_create)
          .execute()
          .await?;
        Ok(())
      })
    }))
    .await
    .expect("create");

  // No data case
  let params = crate::operations::VectorSearchParams {
    vector: vec![0.0, 0.0, 0.0, 0.0],
    column: "embedding",
    distance_type: lancedb::DistanceType::L2,
  };
  let stream = db
    .meta
    .query_table(&PrimaryStoreQuery {
      table: "vec_edge",
      filter: None,
      limit: None,
      offset: None,
      vector_search: Some(params),
    })
    .await
    .expect("query");
  let batches: Vec<_> = stream.try_collect().await.expect("collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(total_rows, 0, "Empty table returns zero rows");

  // Mismatched k larger than dataset
  // Insert one row
  let values: Vec<f32> = vec![0.0, 0.0, 1.0, 0.0];
  let value_array = Float32Array::from(values);
  let field = std::sync::Arc::new(Field::new("item", DataType::Float32, false));
  let embedding = FixedSizeListArray::try_new(
    field,
    4,
    std::sync::Arc::new(value_array) as arrow_array::ArrayRef,
    None,
  )
  .expect("fixed size list");
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      std::sync::Arc::new(Int64Array::from(vec![1])) as arrow_array::ArrayRef,
      std::sync::Arc::new(embedding) as arrow_array::ArrayRef,
    ],
  )
  .expect("batch");
  db.meta
    .save_to_table("vec_edge", batch, &["id"])
    .await
    .expect("save");

  let params = crate::operations::VectorSearchParams {
    vector: vec![0.0, 0.0, 1.0, 0.0],
    column: "embedding",
    distance_type: lancedb::DistanceType::Cosine,
  };
  let stream = db
    .meta
    .query_table(&PrimaryStoreQuery {
      table: "vec_edge",
      filter: None,
      limit: None,
      offset: None,
      vector_search: Some(params),
    })
    .await
    .expect("query");
  let batches: Vec<_> = stream.try_collect().await.expect("collect");
  let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(
    total_rows, 1,
    "k larger than dataset should cap at available rows"
  );
}
