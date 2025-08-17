use tempfile::tempdir;

use crate::{Database, PrimaryStoreQuery};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
use arrow_array::{FixedSizeListArray, RecordBatch, StringArray, TimestampMicrosecondArray};
use futures::{StreamExt, TryStreamExt};
use std::sync::Arc;

use super::tests_helpers::test_config;

#[tokio::test]
async fn test_2pc_episode_pattern() {
  // Migrated from legacy lib.rs: exercises 2PC flow like SaveEpisode
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());
  let db = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Graph schema
  db.graph
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

  // Prepare data & meta schema
  let test_uuid = uuid::Uuid::now_v7();
  let now_jiff = jiff::Timestamp::now();
  let now_time = time::OffsetDateTime::from_unix_timestamp_nanos(now_jiff.as_nanosecond())
    .expect("Invalid timestamp");

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

  // Create meta table
  let schema_for_create = episode_schema.clone();
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

  // Execute 2PC similar to SaveEpisode, writing Lance first then Graph and SystemMetadata
  let episode_schema_clone = episode_schema.clone();
  db
    .execute_2pc(
      "episodes",
      move |meta| {
        Box::pin(async move {
          let uuid_array = StringArray::from(vec![test_uuid.to_string()]);
          let group_id_array = StringArray::from(vec!["2pc-episode-test".to_string()]);
          let source_array = StringArray::from(vec!["Message".to_string()]);
          let source_desc_array = StringArray::from(vec!["2PC test source".to_string()]);
          let name_array = StringArray::from(vec!["2PC Episode Test".to_string()]);
          let content_array = StringArray::from(vec!["2PC test content".to_string()]);
          let valid_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);
          let created_at_array = TimestampMicrosecondArray::from(vec![now_jiff.as_microsecond()]);

          let entity_edges_offsets = arrow::buffer::OffsetBuffer::new(vec![0i32, 0i32].into());
          let entity_edges_values = StringArray::from(vec![] as Vec<&str>);
          let entity_edges_array = arrow_array::ListArray::try_new(
            Arc::new(Field::new("item", DataType::Utf8, true)),
            entity_edges_offsets,
            Arc::new(entity_edges_values),
            None,
          )?;

          let embedding_array = FixedSizeListArray::new_null(
            Arc::new(Field::new("item", DataType::Float32, true)),
            1536,
            1,
          );

          let version_array = StringArray::from(vec![uuid::Uuid::now_v7().to_string()]);

          let batch = RecordBatch::try_new(
            episode_schema_clone,
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
          )?;

          let (rollback_version, _new_version) = meta.save_to_table("episodes", batch, &["uuid"]).await?;
          Ok(((), Some(("episodes", rollback_version))))
        })
      },
      vec![
        (
          "MERGE (n:Episode {uuid: $uuid}) SET n.group_id = $group_id, n.source = $source, n.valid_at = $valid_at, n.created_at = $created_at, n.kumos_version = $kumos_version".to_string(),
          vec![
            ("uuid", kuzu::Value::UUID(test_uuid)),
            ("group_id", kuzu::Value::String("2pc-episode-test".to_string())),
            ("source", kuzu::Value::String("Message".to_string())),
            ("valid_at", kuzu::Value::Timestamp(now_time)),
            ("created_at", kuzu::Value::Timestamp(now_time)),
            ("kumos_version", kuzu::Value::String("v1.0.0".to_string())),
          ],
        ),
      ],
    )
    .await
    .expect("2PC operation should succeed");

  // Verify both sides have the data
  let meta_filter = format!("uuid = '{test_uuid}'");
  let meta_stream = db
    .meta
    .query_table(&PrimaryStoreQuery {
      table: "episodes",
      filter: Some(meta_filter),
      limit: None,
      offset: None,
      vector_search: None,
    })
    .await
    .expect("Failed to query MetaDB");
  let meta_batches: Vec<_> = meta_stream.try_collect().await.expect("collect meta");
  let meta_count: usize = meta_batches.iter().map(|b| b.num_rows()).sum();

  let graph_receiver = db
    .graph
    .execute_query(
      "MATCH (n:Episode {uuid: $uuid}) RETURN n.group_id",
      vec![("uuid", kuzu::Value::UUID(test_uuid))],
    )
    .await
    .expect("Failed to query GraphDB");
  let mut graph_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(graph_receiver);
  let mut graph_count = 0;
  while let Some(row) = graph_stream.next().await {
    let row = row.expect("ok");
    if let kuzu::Value::String(_) = &row[0] {
      graph_count += 1;
    }
  }

  assert_eq!(meta_count, 1);
  assert_eq!(graph_count, 1);

  // Group id query and UUID IN filter
  let group_receiver = db
    .graph
    .execute_query(
      "MATCH (n:Episode) WHERE list_contains(['2pc-episode-test'], n.group_id) RETURN n.uuid",
      vec![],
    )
    .await
    .expect("group query");
  let mut group_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(group_receiver);
  let mut uuids = Vec::new();
  while let Some(row) = group_stream.next().await {
    let row = row.expect("ok");
    if let kuzu::Value::UUID(u) = &row[0] {
      uuids.push(*u);
    }
  }
  assert_eq!(uuids.len(), 1);
  assert_eq!(uuids[0], test_uuid);

  let u0 = uuids[0];
  let uuid_filter = format!("uuid IN ('{u0}')");
  let uuid_stream = db
    .meta
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
  let uuid_count: usize = uuid_batches.iter().map(|b| b.num_rows()).sum();
  assert_eq!(uuid_count, 1);
}
