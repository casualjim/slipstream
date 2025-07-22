use crate::nodes::Concept;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use lancedb::embeddings::EmbeddingDefinition;
use lancedb::index::Index;
use lancedb::index::scalar::FtsIndexBuilder;
use slipstream_store::{DatabaseCommand, DatabaseOperation, NoData, Result, ToDatabase};
use std::sync::Arc;

/// Migration-specific schema for Concept table creation
/// This schema excludes the embedding column so LanceDB can add it via EmbeddingDefinition
fn concept_migration_schema() -> Arc<Schema> {
  Arc::new(Schema::new(vec![
    Field::new("uuid", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("group_id", DataType::Utf8, false),
    Field::new(
      "labels",
      DataType::List(Arc::new(Field::new("item", DataType::Utf8, false))),
      false,
    ),
    Field::new("attributes", DataType::Utf8, false),
    Field::new("summary", DataType::Utf8, false),
    Field::new(
      "created_at",
      DataType::Timestamp(TimeUnit::Microsecond, None),
      false,
    ),
    // Note: name_embedding column will be added by LanceDB via EmbeddingDefinition
  ]))
}

/// Command to run migration for the Concept node type (formerly Entity)
pub struct RunConceptMigration {
  pub embedding_function_name: String,
}

impl DatabaseCommand for RunConceptMigration {
  type Output = ();
  type SaveData = NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    let embedding_function_name = self.embedding_function_name.clone();

    DatabaseOperation::Migration {
      graph_ddl: vec![
        // Create the Concept node table in KuzuDB
        // Only store minimal fields needed for graph operations
        r#"
                CREATE NODE TABLE IF NOT EXISTS Concept (
                    uuid UUID,
                    name STRING,
                    group_id STRING,
                    created_at TIMESTAMP,
                    PRIMARY KEY (uuid)
                )"#
          .into(),
      ],

      meta_setup: Box::new(move |conn| {
        Box::pin(create_concepts_lance_table(conn, embedding_function_name))
      }),
      transformer: Box::new(|_| ()), // No transformation needed
    }
  }
}

/// Internal function to create the concepts table in LanceDB
async fn create_concepts_lance_table(
  conn: &lancedb::Connection,
  embedding_function_name: String,
) -> Result<()> {
  let table_name = Concept::meta_table_name();

  // Check if table already exists
  let tables = conn.table_names().execute().await?;
  if tables.contains(&table_name.to_string()) {
    tracing::debug!("Concepts table already exists, skipping creation");
    return Ok(());
  }

  // Create the table with embedding definition
  // This tells LanceDB to automatically generate embeddings for the "name" column
  // and store them in the "name_embedding" column using the registered embedding function
  // Note: We use a migration-specific schema without the embedding column
  let schema = concept_migration_schema();

  // Create a minimal batch with dummy data - LanceDB requires at least one batch
  // to create a table with embeddings. We'll create and then immediately delete this row.
  use arrow::array::StringBuilder;
  use arrow::buffer::OffsetBuffer;
  use arrow_array::{
    ArrayRef, ListArray, RecordBatch, RecordBatchIterator, TimestampMicrosecondArray,
  };

  let mut uuid_builder = StringBuilder::new();
  let mut name_builder = StringBuilder::new();
  let mut group_id_builder = StringBuilder::new();
  let mut attributes_builder = StringBuilder::new();
  let mut summary_builder = StringBuilder::new();

  // Add dummy data
  uuid_builder.append_value("00000000-0000-0000-0000-000000000000");
  name_builder.append_value("__dummy_concept_for_table_creation__");
  group_id_builder.append_value("__dummy__");
  attributes_builder.append_value("{}"); // Empty JSON object
  summary_builder.append_value("Dummy concept used for table creation");

  // Create empty labels array
  let mut labels_values_builder = StringBuilder::new();
  let labels_array = ListArray::new(
    std::sync::Arc::new(arrow::datatypes::Field::new(
      "item",
      arrow::datatypes::DataType::Utf8,
      false,
    )),
    OffsetBuffer::new(vec![0i32, 0].into()),
    std::sync::Arc::new(labels_values_builder.finish()) as ArrayRef,
    None,
  );

  let created_at = TimestampMicrosecondArray::from(vec![0i64]);

  // Create batch with migration schema (no embedding column yet)
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      std::sync::Arc::new(uuid_builder.finish()),
      std::sync::Arc::new(name_builder.finish()),
      std::sync::Arc::new(group_id_builder.finish()),
      std::sync::Arc::new(labels_array),
      std::sync::Arc::new(attributes_builder.finish()),
      std::sync::Arc::new(summary_builder.finish()),
      std::sync::Arc::new(created_at),
    ],
  )?;

  let batch_iter = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema.clone());

  let table = conn
    .create_table(table_name, Box::new(batch_iter))
    .add_embedding(EmbeddingDefinition::new(
      "name",                   // source column
      &embedding_function_name, // registered embedding function
      Some("name_embedding"),   // destination column
    ))?
    .execute()
    .await?;

  // Delete the dummy row
  table
    .delete("uuid = '00000000-0000-0000-0000-000000000000'")
    .await?;

  // Create scalar indices for fast lookup
  for col in ["uuid", "group_id", "created_at", "name"] {
    if let Err(e) = table.create_index(&[col][..], Index::Auto).execute().await {
      tracing::warn!("Failed to create scalar index for {}: {}", col, e);
    }
  }

  // Create fulltext index for text search on name
  // This will help with substring matching for entity deduplication
  if let Err(e) = table
    .create_index(&["name"], Index::FTS(FtsIndexBuilder::default()))
    .execute()
    .await
  {
    tracing::warn!("Failed to create fulltext index on name: {}", e);
  }

  // Note: Vector index for name_embedding will be created later when we have data
  // LanceDB requires data to exist before creating vector indices

  tracing::info!(
    "Created Concept table with embedding function '{}' and indices",
    embedding_function_name
  );
  Ok(())
}
