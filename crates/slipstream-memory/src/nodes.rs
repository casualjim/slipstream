//! Node types for the Kumos knowledge graph
//!
//! This module defines the core node types:
//! - Interaction (formerly Episode): Temporal data points
//! - Concept (formerly Entity): Real-world entities with embeddings
//! - Theme (formerly Community): Clusters of related concepts

use arrow::array::{
  Array, ArrayRef, RecordBatch, StringArray, StringBuilder, TimestampMicrosecondArray,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use slipstream_store::{Error, Result};
use slipstream_store::{FromRecordBatchRow, ToDatabase};
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

/// Episode type enum matching Graphiti's EpisodeType
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
  Message,
  Json,
  Text,
}

impl Default for ContentType {
  fn default() -> Self {
    ContentType::Text
  }
}

impl Display for ContentType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ContentType::Message => write!(f, "message"),
      ContentType::Json => write!(f, "json"),
      ContentType::Text => write!(f, "text"),
    }
  }
}

/// Interaction node (formerly Episode) - represents temporal data points
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Interaction {
  pub uuid: Uuid,
  pub name: String,
  pub content: String,
  pub source: ContentType,
  pub source_description: String,
  pub group_id: String,
  pub labels: Vec<String>,
  pub created_at: Timestamp,
  pub valid_at: Timestamp,
  pub reference_edges: Vec<String>, // formerly entity_edges
  pub version: Uuid,                // kumos_version
}

impl ToDatabase for Interaction {
  type MetaContext = ();
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    // Only include properties used in WHERE, ORDER BY, or graph traversal queries
    Ok(vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      ("group_id", kuzu::Value::String(self.group_id.clone())),
      (
        "valid_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(self.valid_at.as_nanosecond()).map_err(
            |e| slipstream_store::Error::Generic(format!("Invalid valid_at timestamp: {}", e)),
          )?,
        ),
      ),
      (
        "created_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(self.created_at.as_nanosecond()).map_err(
            |e| slipstream_store::Error::Generic(format!("Invalid created_at timestamp: {}", e)),
          )?,
        ),
      ),
    ])
  }

  fn as_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
    use arrow_array::ListArray;

    // Create arrays for each field
    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);
    let source_array = StringArray::from(vec![serde_json::to_string(&self.source)?]);
    let source_desc_array = StringArray::from(vec![self.source_description.clone()]);
    let name_array = StringArray::from(vec![self.name.clone()]);
    let content_array = StringArray::from(vec![self.content.clone()]);

    // Convert timestamps to microseconds
    let valid_at_array = TimestampMicrosecondArray::from(vec![self.valid_at.as_microsecond()]);
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    // Create labels list array
    let mut labels_builder = StringBuilder::new();
    for label in &self.labels {
      labels_builder.append_value(label);
    }
    let labels_values = labels_builder.finish();
    let labels_offsets = vec![0i32, self.labels.len() as i32];
    let labels_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, true)),
      OffsetBuffer::new(labels_offsets.into()),
      Arc::new(labels_values) as ArrayRef,
      None,
    )
    .map_err(|e| {
      slipstream_store::Error::Generic(format!("Failed to create labels array: {}", e))
    })?;

    // Create reference_edges list array
    let mut list_builder = StringBuilder::new();
    for edge in &self.reference_edges {
      list_builder.append_value(edge);
    }
    let values = list_builder.finish();
    let offsets = vec![0i32, self.reference_edges.len() as i32];
    let reference_edges_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, true)),
      OffsetBuffer::new(offsets.into()),
      Arc::new(values) as ArrayRef,
      None,
    )
    .map_err(|e| slipstream_store::Error::Generic(format!("Failed to create list array: {}", e)))?;

    // Version array
    let version_array = StringArray::from(vec![self.version.to_string()]);

    // Create RecordBatch - use the schema from meta_schema()
    let schema = Self::meta_schema(());
    let batch = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(uuid_array),
        Arc::new(group_id_array),
        Arc::new(source_array),
        Arc::new(source_desc_array),
        Arc::new(name_array),
        Arc::new(content_array),
        Arc::new(valid_at_array),
        Arc::new(created_at_array),
        Arc::new(labels_array),
        Arc::new(reference_edges_array),
        Arc::new(version_array),
      ],
    )
    .map_err(|e| {
      slipstream_store::Error::Generic(format!("Failed to create RecordBatch: {}", e))
    })?;

    Ok(batch)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "interactions"
  }

  fn graph_table_name() -> &'static str {
    "Interaction"
  }

  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    static SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();
    SCHEMA
      .get_or_init(|| {
        Arc::new(Schema::new(vec![
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
            "labels",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
          ),
          Field::new(
            "reference_edges",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
          ),
          Field::new("version", DataType::Utf8, false),
        ]))
      })
      .clone()
  }
}

impl FromRecordBatchRow for Interaction {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    // Get columns
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let name_array = batch
      .column_by_name("name")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing name column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid name type".to_string()))?;

    let content_array = batch
      .column_by_name("content")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing content column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid content type".to_string()))?;

    let source_array = batch
      .column_by_name("source")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing source column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid source type".to_string()))?;

    let source_description_array = batch
      .column_by_name("source_description")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing source_description column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid source_description type".to_string()))?;

    let group_id_array = batch
      .column_by_name("group_id")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing group_id column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid group_id type".to_string()))?;

    let created_at_array = batch
      .column_by_name("created_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing created_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid created_at type".to_string()))?;

    let valid_at_array = batch
      .column_by_name("valid_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing valid_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid valid_at type".to_string()))?;

    let labels_array = batch
      .column_by_name("labels")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing labels column".to_string()))?
      .as_any()
      .downcast_ref::<arrow_array::ListArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels type".to_string()))?;

    let reference_edges_array = batch
      .column_by_name("reference_edges")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing reference_edges column".to_string()))?
      .as_any()
      .downcast_ref::<arrow_array::ListArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid reference_edges type".to_string()))?;

    let version_array = batch
      .column_by_name("version")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing version column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid version type".to_string()))?;

    // Parse labels from list array
    let labels_list = labels_array.value(row);
    let labels_strings = labels_list
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels values type".to_string()))?;

    let mut labels = Vec::new();
    for i in 0..labels_strings.len() {
      labels.push(labels_strings.value(i).to_string());
    }

    // Parse reference edges from list array
    let edges_list = reference_edges_array.value(row);
    let edges_strings = edges_list
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid reference edges values type".to_string()))?;

    let mut reference_edges = Vec::new();
    for i in 0..edges_strings.len() {
      reference_edges.push(edges_strings.value(i).to_string());
    }

    // Parse source as EpisodeType
    let source: ContentType = serde_json::from_str(source_array.value(row))?;

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      name: name_array.value(row).to_string(),
      content: content_array.value(row).to_string(),
      source,
      source_description: source_description_array.value(row).to_string(),
      group_id: group_id_array.value(row).to_string(),
      labels,
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
      valid_at: Timestamp::from_microsecond(valid_at_array.value(row))?,
      reference_edges,
      version: Uuid::parse_str(version_array.value(row))?,
    })
  }
}

impl Default for Interaction {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      name: String::new(),
      content: String::new(),
      source: ContentType::default(),
      source_description: String::new(),
      group_id: String::new(),
      labels: Vec::new(),
      created_at: Timestamp::now(),
      valid_at: Timestamp::now(),
      reference_edges: Vec::new(),
      version: Uuid::now_v7(),
    }
  }
}

impl Display for Interaction {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Interaction(uuid={}, name='{}')", self.uuid, self.name)
  }
}

/// Concept node (formerly Entity) - represents real-world entities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Concept {
  pub uuid: Uuid,
  pub name: String,
  pub labels: Vec<String>,
  pub attributes: HashMap<String, serde_json::Value>,
  pub group_id: String,
  pub created_at: Timestamp,
  pub name_embedding: Option<Vec<f32>>,
  pub summary: String,
}

impl ToDatabase for Concept {
  type MetaContext = i32; // embedding dimensions
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    // GraphDB only stores minimal fields for graph operations
    Ok(vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      ("name", kuzu::Value::String(self.name.clone())),
      ("group_id", kuzu::Value::String(self.group_id.clone())),
      (
        "created_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(self.created_at.as_nanosecond()).map_err(
            |e| slipstream_store::Error::Generic(format!("Invalid created_at timestamp: {}", e)),
          )?,
        ),
      ),
    ])
  }

  fn as_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch> {
    use arrow_array::ListArray;

    let embedding_dimensions = ctx;

    // Create arrays for each field
    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let name_array = StringArray::from(vec![self.name.clone()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);

    // Serialize attributes to JSON string
    let attributes_json = serde_json::to_string(&self.attributes)?;
    let attributes_array = StringArray::from(vec![attributes_json]);

    let summary_array = StringArray::from(vec![self.summary.clone()]);

    // Convert timestamp to microseconds
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    // Build labels list array
    let mut list_builder = StringBuilder::new();
    for label in &self.labels {
      list_builder.append_value(label);
    }
    let values = list_builder.finish();
    let offsets = vec![0i32, self.labels.len() as i32];
    let labels_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, false)),
      OffsetBuffer::new(offsets.into()),
      Arc::new(values) as ArrayRef,
      None,
    )?;

    // Build embedding array - use actual embedding if available
    let mut embedding_builder = arrow_array::builder::FixedSizeListBuilder::new(
      arrow_array::builder::Float32Builder::new(),
      embedding_dimensions,
    );

    if let Some(embedding) = &self.name_embedding {
      embedding_builder.values().append_slice(embedding);
      embedding_builder.append(true);
    } else {
      // If entity is missing embeddings, LanceDB will generate them
      // For now provide zeros
      embedding_builder
        .values()
        .append_slice(&vec![0.0f32; embedding_dimensions as usize]);
      embedding_builder.append(true);
    }

    // Get schema with the correct embedding dimensions
    let schema = Self::meta_schema(embedding_dimensions);
    let batch = RecordBatch::try_new(
      schema,
      vec![
        Arc::new(uuid_array),
        Arc::new(name_array),
        Arc::new(group_id_array),
        Arc::new(labels_array),
        Arc::new(attributes_array),
        Arc::new(summary_array),
        Arc::new(created_at_array),
        Arc::new(embedding_builder.finish()),
      ],
    )?;

    Ok(batch)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "concepts"
  }

  fn graph_table_name() -> &'static str {
    "Concept"
  }

  fn meta_schema(ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    let embedding_dimensions = ctx;
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
      Field::new(
        "name_embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          embedding_dimensions,
        ),
        false,
      ),
    ]))
  }
}

impl FromRecordBatchRow for Concept {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let name_array = batch
      .column_by_name("name")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing name column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid name type".to_string()))?;

    let group_id_array = batch
      .column_by_name("group_id")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing group_id column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid group_id type".to_string()))?;

    let summary_array = batch
      .column_by_name("summary")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing summary column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid summary type".to_string()))?;

    let created_at_array = batch
      .column_by_name("created_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing created_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid created_at type".to_string()))?;

    let labels_array = batch
      .column_by_name("labels")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing labels column".to_string()))?
      .as_any()
      .downcast_ref::<arrow_array::ListArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels type".to_string()))?;

    let attributes_array = batch
      .column_by_name("attributes")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing attributes column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid attributes type".to_string()))?;

    // Parse labels
    let labels_list = labels_array.value(row);
    let labels_strings = labels_list
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels values type".to_string()))?;

    let mut labels = Vec::new();
    for i in 0..labels_strings.len() {
      labels.push(labels_strings.value(i).to_string());
    }

    // Parse attributes from JSON
    let attributes_json = attributes_array.value(row);
    let attributes: HashMap<String, serde_json::Value> = serde_json::from_str(attributes_json)?;

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      name: name_array.value(row).to_string(),
      labels,
      attributes,
      group_id: group_id_array.value(row).to_string(),
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
      name_embedding: None, // Embeddings are not returned in queries
      summary: summary_array.value(row).to_string(),
    })
  }
}

impl Default for Concept {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      name: String::new(),
      labels: Vec::new(),
      attributes: HashMap::new(),
      group_id: String::new(),
      created_at: Timestamp::now(),
      name_embedding: None,
      summary: String::new(),
    }
  }
}

impl Display for Concept {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Concept(uuid={}, name='{}')", self.uuid, self.name)
  }
}

/// Theme node (formerly Community) - represents clusters of related concepts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
  pub uuid: Uuid,
  pub name: String,
  pub summary: String,
  pub group_id: String,
  pub labels: Vec<String>,
  pub created_at: Timestamp,
  pub name_embedding: Option<Vec<f32>>,
}

impl ToDatabase for Theme {
  type MetaContext = i32; // embedding dimensions
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    // Only include properties used in WHERE, ORDER BY, or graph traversal queries
    Ok(vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      ("group_id", kuzu::Value::String(self.group_id.clone())),
      (
        "created_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(self.created_at.as_nanosecond()).map_err(
            |e| slipstream_store::Error::Generic(format!("Invalid created_at timestamp: {}", e)),
          )?,
        ),
      ),
    ])
  }

  fn as_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch> {
    use arrow_array::ListArray;

    let embedding_dimensions = ctx;

    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let name_array = StringArray::from(vec![self.name.clone()]);
    let summary_array = StringArray::from(vec![self.summary.clone()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    // Create labels list array
    let mut labels_builder = StringBuilder::new();
    for label in &self.labels {
      labels_builder.append_value(label);
    }
    let labels_values = labels_builder.finish();
    let labels_offsets = vec![0i32, self.labels.len() as i32];
    let labels_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, true)),
      OffsetBuffer::new(labels_offsets.into()),
      Arc::new(labels_values) as ArrayRef,
      None,
    )
    .map_err(|e| {
      slipstream_store::Error::Generic(format!("Failed to create labels array: {}", e))
    })?;

    // Build embedding array
    let mut embedding_builder = arrow_array::builder::FixedSizeListBuilder::new(
      arrow_array::builder::Float32Builder::new(),
      embedding_dimensions,
    );

    if let Some(embedding) = &self.name_embedding {
      embedding_builder.values().append_slice(embedding);
      embedding_builder.append(true);
    } else {
      embedding_builder
        .values()
        .append_slice(&vec![0.0f32; embedding_dimensions as usize]);
      embedding_builder.append(true);
    }

    let schema = Self::meta_schema(embedding_dimensions);

    Ok(RecordBatch::try_new(
      schema,
      vec![
        Arc::new(uuid_array),
        Arc::new(name_array),
        Arc::new(summary_array),
        Arc::new(group_id_array),
        Arc::new(labels_array),
        Arc::new(created_at_array),
        Arc::new(embedding_builder.finish()),
      ],
    )?)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "themes"
  }

  fn graph_table_name() -> &'static str {
    "Theme"
  }

  fn meta_schema(ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    let embedding_dimensions = ctx;
    Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("summary", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "labels",
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        false,
      ),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "name_embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          embedding_dimensions,
        ),
        false,
      ),
    ]))
  }
}

impl FromRecordBatchRow for Theme {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let name_array = batch
      .column_by_name("name")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing name column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid name type".to_string()))?;

    let summary_array = batch
      .column_by_name("summary")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing summary column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid summary type".to_string()))?;

    let group_id_array = batch
      .column_by_name("group_id")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing group_id column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid group_id type".to_string()))?;

    let labels_array = batch
      .column_by_name("labels")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing labels column".to_string()))?
      .as_any()
      .downcast_ref::<arrow_array::ListArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels type".to_string()))?;

    let created_at_array = batch
      .column_by_name("created_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing created_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid created_at type".to_string()))?;

    // Parse labels from list array
    let labels_list = labels_array.value(row);
    let labels_strings = labels_list
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid labels values type".to_string()))?;

    let mut labels = Vec::new();
    for i in 0..labels_strings.len() {
      labels.push(labels_strings.value(i).to_string());
    }

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      name: name_array.value(row).to_string(),
      summary: summary_array.value(row).to_string(),
      group_id: group_id_array.value(row).to_string(),
      labels,
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
      name_embedding: None, // Embeddings are not returned in queries
    })
  }
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      name: String::new(),
      summary: String::new(),
      group_id: String::new(),
      labels: Vec::new(),
      created_at: Timestamp::now(),
      name_embedding: None,
    }
  }
}

impl Display for Theme {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Theme(uuid={}, name='{}')", self.uuid, self.name)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jiff::Timestamp;
  use uuid::Uuid;

  #[test]
  fn test_interaction_to_database() {
    let interaction = Interaction {
      uuid: Uuid::new_v4(),
      name: "Test Interaction".to_string(),
      content: "Test content".to_string(),
      source: ContentType::Text,
      source_description: "Test source".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["test".to_string()],
      created_at: Timestamp::now(),
      valid_at: Timestamp::now(),
      reference_edges: vec!["edge1".to_string(), "edge2".to_string()],
      version: Uuid::new_v4(),
    };

    // Test graph conversion
    let graph_values = interaction.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 4); // Only covering index properties
    assert_eq!(graph_values[0].0, "uuid");
    assert_eq!(graph_values[1].0, "group_id");

    // Test meta conversion
    let batch = interaction.as_meta_value(()).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 11);
  }

  #[test]
  fn test_concept_to_database() {
    let concept = Concept {
      uuid: Uuid::new_v4(),
      name: "Test Concept".to_string(),
      labels: vec!["Person".to_string(), "Engineer".to_string()],
      attributes: HashMap::from([
        ("age".to_string(), serde_json::json!(30)),
        ("location".to_string(), serde_json::json!("New York")),
      ]),
      group_id: "test-group".to_string(),
      created_at: Timestamp::now(),
      name_embedding: Some(vec![0.1; 384]),
      summary: "A test concept".to_string(),
    };

    // Test graph conversion
    let graph_values = concept.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 4); // uuid, name, group_id, created_at for graph

    // Test meta conversion with embedding dimensions
    let batch = concept.as_meta_value(384).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 8);
  }

  #[test]
  fn test_theme_to_database() {
    let theme = Theme {
      uuid: Uuid::new_v4(),
      name: "Engineering Team".to_string(),
      summary: "A theme about engineering".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["theme".to_string(), "team".to_string()],
      created_at: Timestamp::now(),
      name_embedding: None,
    };

    // Test graph conversion
    let graph_values = theme.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 3); // Only covering index properties

    // Test meta conversion with embedding dimensions
    let batch = theme.as_meta_value(384).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 7);
  }

  #[test]
  fn test_primary_keys() {
    assert_eq!(Interaction::primary_key_columns(), &["uuid"]);
    assert_eq!(Concept::primary_key_columns(), &["uuid"]);
    assert_eq!(Theme::primary_key_columns(), &["uuid"]);
  }

  #[test]
  fn test_concept_with_no_embedding() {
    let concept = Concept {
      uuid: Uuid::new_v4(),
      name: "Test Concept".to_string(),
      labels: vec!["Entity".to_string()],
      attributes: HashMap::new(),
      group_id: "test-group".to_string(),
      created_at: Timestamp::now(),
      name_embedding: None, // No embedding
      summary: "Test".to_string(),
    };

    // Should still work with zeros
    let batch = concept.as_meta_value(384).unwrap();
    assert_eq!(batch.num_rows(), 1);
  }
}
