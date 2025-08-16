use tempfile::tempdir;

use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
use arrow_array::{
  FixedSizeListArray, ListArray, RecordBatch, StringArray, TimestampMicrosecondArray,
};
use futures::{StreamExt, TryStreamExt};
use std::sync::Arc;

use super::tests_helpers::test_config;
use crate::PrimaryStoreQuery;

#[tokio::test]
async fn test_raw_database_synchronization_without_abstractions() {
  // Raw test using direct MetaDb and GraphDb APIs migrated out of lib.rs
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  // Create both databases directly
  let graph_db = crate::graph::GraphDb::new(&config).expect("Failed to create GraphDb");
  let meta_db = crate::meta::MetaDb::new(&config)
    .await
    .expect("Failed to create MetaDb");

  // Create Episode schema in GraphDB
  graph_db
    .execute_write(
      r#"
      CREATE NODE TABLE IF NOT EXISTS Episode (
          uuid UUID,
          group_id STRING,
          source STRING,
          valid_at TIMESTAMP,
          created_at TIMESTAMP,
          kumos_version STRING,
          PRIMARY KEY (uuid)
      )"#,
      vec![],
    )
    .await
    .expect("Failed to create Episode schema in GraphDB");

  // Create Episode schema in MetaDB using Arrow
  let episode_schema = Arc::new(ArrowSchema::new(vec![
    Field::new("uuid", DataType::Utf8, false),
    Field::new("group_id", DataType::Utf8, false),
    Field::new("source", DataType::Utf8, false),
    Field::new("source_description", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("content", DataType::Utf8, false),
    Field::new(
      "valid_at",
      DataType::Timestamp(TimeUnit::Microsecond, None),
      false,
    ),
    Field::new(
      "created_at",
      DataType::Timestamp(TimeUnit::Microsecond, None),
      false,
    ),
    Field::new(
      "entity_edges",
      DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
      false,
    ),
    Field::new(
      "embedding",
      DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 1536),
      true,
    ),
    Field::new("version", DataType::Utf8, false),
  ]));

  let test_uuid = uuid::Uuid::now_v7();
  let now_jiff = jiff::Timestamp::now();
  let now_time = time::OffsetDateTime::from_unix_timestamp_nanos(now_jiff.as_nanosecond())
    .expect("Invalid timestamp");

  // Create episodes table in MetaDB
  let schema_for_create = episode_schema.clone();
  meta_db
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
    .expect("Failed to create episodes table in MetaDB");

  // 1. Save to MetaDB
  let uuid_array = StringArray::from(vec![test_uuid.to_string()]);
  let group_id_array = StringArray::from(vec!["raw-test-group".to_string()]);
  let source_array = StringArray::from(vec!["Message".to_string()]);
  let source_desc_array = StringArray::from(vec!["Raw test source".to_string()]);
  let name_array = StringArray::from(vec!["Raw Test Episode".to_string()]);
  let content_array = StringArray::from(vec!["Raw test content".to_string()]);
  let valid_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);
  let created_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);

  let entity_edges_values = StringArray::from(vec![] as Vec<&str>);
  let entity_edges_offsets = arrow::buffer::OffsetBuffer::new(vec![0i32, 0i32].into());
  let entity_edges_array = ListArray::try_new(
    Arc::new(Field::new("item", DataType::Utf8, true)),
    entity_edges_offsets,
    Arc::new(entity_edges_values),
    None,
  )
  .expect("Failed to create entity_edges array");

  let embedding_array = FixedSizeListArray::new_null(
    Arc::new(Field::new("item", DataType::Float32, true)),
    1536,
    1,
  );

  let version_array = StringArray::from(vec![uuid::Uuid::now_v7().to_string()]);

  let meta_batch = RecordBatch::try_new(
    episode_schema.clone(),
    vec![
      Arc::new(uuid_array),
      Arc::new(group_id_array),
      Arc::new(source_array),
      Arc::new(source_desc_array),
      Arc::new(name_array),
      Arc::new(content_array),
      Arc::new(valid_at_array),
      Arc::new(created_at_array),
      Arc::new(entity_edges_array),
      Arc::new(embedding_array),
      Arc::new(version_array),
    ],
  )
  .expect("Failed to create RecordBatch");

  let _ = meta_db
    .save_to_table("episodes", meta_batch, &["uuid"])
    .await
    .expect("save meta");

  // 2. Save to GraphDB
  graph_db
    .execute_write(
      "CREATE (:Episode {uuid: $uuid, group_id: $group_id, source: $source, valid_at: $valid_at, created_at: $created_at, kumos_version: $version})",
      vec![
        ("uuid", kuzu::Value::UUID(test_uuid)),
        ("group_id", kuzu::Value::String("raw-test-group".to_string())),
        ("source", kuzu::Value::String("Message".to_string())),
        ("valid_at", kuzu::Value::Timestamp(now_time)),
        ("created_at", kuzu::Value::Timestamp(now_time)),
        ("version", kuzu::Value::String("v1.0.0".to_string())),
      ],
    )
    .await
    .expect("Failed to save to GraphDB");

  // 3. Verify MetaDB
  let meta_stream = meta_db
    .query_table(&PrimaryStoreQuery {
      table: "episodes",
      filter: None,
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("query meta");
  let meta_batches: Vec<_> = meta_stream.try_collect().await.expect("collect meta");
  let meta_row_count: usize = meta_batches.iter().map(|b| b.num_rows()).sum();

  // 4. Verify GraphDB
  let graph_receiver = graph_db
    .execute_query("MATCH (n:Episode) RETURN count(n)", vec![])
    .await
    .expect("query graph");
  let mut graph_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(graph_receiver);
  let count_row = graph_stream.next().await.expect("count row").expect("ok");
  let graph_count = if let kuzu::Value::Int64(c) = &count_row[0] {
    *c
  } else {
    panic!("expected count")
  };

  // 5. Group query
  let group_receiver = graph_db
    .execute_query(
      "MATCH (n:Episode) WHERE list_contains(['raw-test-group'], n.group_id) RETURN n.uuid",
      vec![],
    )
    .await
    .expect("group query");
  let mut group_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(group_receiver);
  let mut found_uuids = Vec::new();
  while let Some(row) = group_stream.next().await {
    let row = row.expect("ok");
    if let kuzu::Value::UUID(u) = &row[0] {
      found_uuids.push(*u);
    }
  }

  // 6. UUID IN filter in Meta
  if !found_uuids.is_empty() {
    let uuid_filter = format!("uuid IN ('{}')", found_uuids[0]);
    let uuid_stream = meta_db
      .query_table(&PrimaryStoreQuery {
        table: "episodes",
        filter: Some(uuid_filter),
        limit: None,
        offset: None,
        vector_search: None,
      })
      .await
      .expect("uuid filter");
    let uuid_batches: Vec<_> = uuid_stream
      .try_collect()
      .await
      .expect("collect uuid batches");
    let uuid_row_count: usize = uuid_batches.iter().map(|b| b.num_rows()).sum();
    if uuid_row_count == 0 {
      panic!("Database synchronization issue confirmed!");
    }
  } else {
    panic!("GraphDB group query issue confirmed!");
  }

  assert_eq!(meta_row_count, 1);
  assert_eq!(graph_count, 1);
}
