//! Edge types for the Kumos knowledge graph
//!
//! This module defines the core edge types:
//! - Mentions (formerly MENTIONS): Links from Interactions to Concepts
//! - Relates (formerly RELATES_TO): Facts connecting Concepts
//! - Includes (formerly HAS_MEMBER): Links from Themes to Concepts

use arrow::array::{
  Array, ArrayRef, RecordBatch, StringArray, StringBuilder, TimestampMicrosecondArray,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use slipstream_store::FromRecordBatchRow;
use slipstream_store::ToDatabase;
use slipstream_store::{Error, Result};
use std::fmt::{self, Display};
use std::sync::Arc;
use uuid::Uuid;

/// Mentions edge (formerly MENTIONS) - links Interactions to Concepts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mentions {
  pub uuid: Uuid,
  pub source_interaction_uuid: Uuid, // formerly source_node_uuid for Episode
  pub target_concept_uuid: Uuid,     // formerly target_node_uuid for Entity
  pub group_id: String,
  pub created_at: Timestamp,
}

impl ToDatabase for Mentions {
  type MetaContext = ();
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    Ok(vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      (
        "source_uuid",
        kuzu::Value::UUID(self.source_interaction_uuid),
      ),
      ("target_uuid", kuzu::Value::UUID(self.target_concept_uuid)),
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

  fn as_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let source_array = StringArray::from(vec![self.source_interaction_uuid.to_string()]);
    let target_array = StringArray::from(vec![self.target_concept_uuid.to_string()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    let schema = Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("source_interaction_uuid", DataType::Utf8, false),
      Field::new("target_concept_uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
    ]));

    Ok(RecordBatch::try_new(
      schema,
      vec![
        Arc::new(uuid_array),
        Arc::new(source_array),
        Arc::new(target_array),
        Arc::new(group_id_array),
        Arc::new(created_at_array),
      ],
    )?)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "mentions_edges"
  }

  fn graph_table_name() -> &'static str {
    "MENTIONS"
  }

  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("source_interaction_uuid", DataType::Utf8, false),
      Field::new("target_concept_uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
    ]))
  }
}

impl FromRecordBatchRow for Mentions {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let source_array = batch
      .column_by_name("source_interaction_uuid")
      .ok_or_else(|| {
        Error::InvalidMetaDbData("Missing source_interaction_uuid column".to_string())
      })?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| {
        Error::InvalidMetaDbData("Invalid source_interaction_uuid type".to_string())
      })?;

    let target_array = batch
      .column_by_name("target_concept_uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing target_concept_uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid target_concept_uuid type".to_string()))?;

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

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      source_interaction_uuid: Uuid::parse_str(source_array.value(row))?,
      target_concept_uuid: Uuid::parse_str(target_array.value(row))?,
      group_id: group_id_array.value(row).to_string(),
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
    })
  }
}

impl Default for Mentions {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      source_interaction_uuid: Uuid::now_v7(),
      target_concept_uuid: Uuid::now_v7(),
      group_id: String::new(),
      created_at: Timestamp::now(),
    }
  }
}

impl Display for Mentions {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "Mentions(uuid={}, {} -> {})",
      self.uuid, self.source_interaction_uuid, self.target_concept_uuid
    )
  }
}

/// Relates edge (formerly RELATES_TO) - represents facts between Concepts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relates {
  pub uuid: Uuid,
  pub source_concept_uuid: Uuid, // formerly source_node_uuid
  pub target_concept_uuid: Uuid, // formerly target_node_uuid
  pub fact: String,
  pub name: String,
  pub group_id: String,
  pub interactions: Vec<Uuid>, // formerly episodes
  pub created_at: Timestamp,
  pub valid_at: Option<Timestamp>, // Made optional to match Graphiti
  pub invalid_at: Option<Timestamp>,
  pub expired_at: Option<Timestamp>,
  pub fact_embedding: Option<Vec<f32>>,
  pub attributes: std::collections::HashMap<String, serde_json::Value>, // Added to match Graphiti
}

impl ToDatabase for Relates {
  type MetaContext = i32; // embedding dimensions
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    // Only include properties that are in the KuzuDB covering index
    // Properties used for: graph traversal, filtering (WHERE), ordering, or lookups
    let mut values = vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      ("source_uuid", kuzu::Value::UUID(self.source_concept_uuid)),
      ("target_uuid", kuzu::Value::UUID(self.target_concept_uuid)),
      ("group_id", kuzu::Value::String(self.group_id.clone())),
      (
        "created_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(self.created_at.as_nanosecond()).map_err(
            |e| slipstream_store::Error::Generic(format!("Invalid created_at timestamp: {}", e)),
          )?,
        ),
      ),
    ];

    // Add optional valid_at (used in temporal filtering)
    if let Some(ts) = &self.valid_at {
      values.push((
        "valid_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(ts.as_nanosecond()).map_err(|e| {
            slipstream_store::Error::Generic(format!("Invalid valid_at timestamp: {}", e))
          })?,
        ),
      ));
    } else {
      values.push(("valid_at", kuzu::Value::Null(kuzu::LogicalType::Timestamp)));
    }

    // Add optional timestamps (used in temporal filtering)
    if let Some(ts) = &self.invalid_at {
      values.push((
        "invalid_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(ts.as_nanosecond()).map_err(|e| {
            slipstream_store::Error::Generic(format!("Invalid invalid_at timestamp: {}", e))
          })?,
        ),
      ));
    } else {
      values.push((
        "invalid_at",
        kuzu::Value::Null(kuzu::LogicalType::Timestamp),
      ));
    }

    if let Some(ts) = &self.expired_at {
      values.push((
        "expired_at",
        kuzu::Value::Timestamp(
          OffsetDateTime::from_unix_timestamp_nanos(ts.as_nanosecond()).map_err(|e| {
            slipstream_store::Error::Generic(format!("Invalid expired_at timestamp: {}", e))
          })?,
        ),
      ));
    } else {
      values.push((
        "expired_at",
        kuzu::Value::Null(kuzu::LogicalType::Timestamp),
      ));
    }

    Ok(values)
  }

  fn as_meta_value(&self, ctx: Self::MetaContext) -> Result<RecordBatch> {
    use arrow_array::ListArray;

    let embedding_dimensions = ctx;

    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let source_array = StringArray::from(vec![self.source_concept_uuid.to_string()]);
    let target_array = StringArray::from(vec![self.target_concept_uuid.to_string()]);
    let fact_array = StringArray::from(vec![self.fact.clone()]);
    let name_array = StringArray::from(vec![self.name.clone()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);

    // Serialize attributes to JSON string
    let attributes_json = serde_json::to_string(&self.attributes)?;
    let attributes_array = StringArray::from(vec![attributes_json]);

    // Convert timestamps
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    // Handle optional valid_at
    let valid_at_array = if let Some(ts) = self.valid_at {
      TimestampMicrosecondArray::from(vec![Some(ts.as_microsecond())])
    } else {
      TimestampMicrosecondArray::from(vec![None])
    };

    // Build interactions list array
    let mut list_builder = StringBuilder::new();
    for interaction in &self.interactions {
      list_builder.append_value(interaction.to_string());
    }
    let values = list_builder.finish();
    let offsets = vec![0i32, self.interactions.len() as i32];
    let interactions_array = ListArray::try_new(
      Arc::new(Field::new("item", DataType::Utf8, false)),
      OffsetBuffer::new(offsets.into()),
      Arc::new(values) as ArrayRef,
      None,
    )?;

    // Handle optional timestamps
    let invalid_at_array = if let Some(ts) = self.invalid_at {
      TimestampMicrosecondArray::from(vec![Some(ts.as_microsecond())])
    } else {
      TimestampMicrosecondArray::from(vec![None])
    };

    let expired_at_array = if let Some(ts) = self.expired_at {
      TimestampMicrosecondArray::from(vec![Some(ts.as_microsecond())])
    } else {
      TimestampMicrosecondArray::from(vec![None])
    };

    // Build embedding array
    let mut embedding_builder = arrow_array::builder::FixedSizeListBuilder::new(
      arrow_array::builder::Float32Builder::new(),
      embedding_dimensions,
    );

    if let Some(embedding) = &self.fact_embedding {
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
        Arc::new(source_array),
        Arc::new(target_array),
        Arc::new(fact_array),
        Arc::new(name_array),
        Arc::new(group_id_array),
        Arc::new(attributes_array),
        Arc::new(interactions_array),
        Arc::new(created_at_array),
        Arc::new(valid_at_array),
        Arc::new(invalid_at_array),
        Arc::new(expired_at_array),
        Arc::new(embedding_builder.finish()),
      ],
    )?)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "relates_edges"
  }

  fn graph_table_name() -> &'static str {
    "RELATES_TO"
  }

  fn meta_schema(ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    let embedding_dimensions = ctx;
    Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("source_concept_uuid", DataType::Utf8, false),
      Field::new("target_concept_uuid", DataType::Utf8, false),
      Field::new("fact", DataType::Utf8, false),
      Field::new("name", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new("attributes", DataType::Utf8, false),
      Field::new(
        "interactions",
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false))),
        false,
      ),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
      Field::new(
        "valid_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        true,
      ), // Made optional
      Field::new(
        "invalid_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        true,
      ),
      Field::new(
        "expired_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        true,
      ),
      Field::new(
        "fact_embedding",
        DataType::FixedSizeList(
          Arc::new(Field::new("item", DataType::Float32, true)),
          embedding_dimensions,
        ),
        false,
      ),
    ]))
  }
}

impl FromRecordBatchRow for Relates {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let source_array = batch
      .column_by_name("source_concept_uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing source_concept_uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid source_concept_uuid type".to_string()))?;

    let target_array = batch
      .column_by_name("target_concept_uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing target_concept_uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid target_concept_uuid type".to_string()))?;

    let fact_array = batch
      .column_by_name("fact")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing fact column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid fact type".to_string()))?;

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

    let attributes_array = batch
      .column_by_name("attributes")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing attributes column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid attributes type".to_string()))?;

    let interactions_array = batch
      .column_by_name("interactions")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing interactions column".to_string()))?
      .as_any()
      .downcast_ref::<arrow_array::ListArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid interactions type".to_string()))?;

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

    let invalid_at_array = batch
      .column_by_name("invalid_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing invalid_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid invalid_at type".to_string()))?;

    let expired_at_array = batch
      .column_by_name("expired_at")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing expired_at column".to_string()))?
      .as_any()
      .downcast_ref::<TimestampMicrosecondArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid expired_at type".to_string()))?;

    // Parse interactions from list array
    let interactions_list = interactions_array.value(row);
    let interactions_strings = interactions_list
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid interactions values type".to_string()))?;

    let mut interactions = Vec::new();
    for i in 0..interactions_strings.len() {
      interactions.push(Uuid::parse_str(interactions_strings.value(i))?);
    }

    // Parse attributes from JSON
    let attributes_json = attributes_array.value(row);
    let attributes: std::collections::HashMap<String, serde_json::Value> =
      serde_json::from_str(attributes_json)?;

    // Handle optional timestamps
    let valid_at = if valid_at_array.is_null(row) {
      None
    } else {
      Some(Timestamp::from_microsecond(valid_at_array.value(row))?)
    };

    let invalid_at = if invalid_at_array.is_null(row) {
      None
    } else {
      Some(Timestamp::from_microsecond(invalid_at_array.value(row))?)
    };

    let expired_at = if expired_at_array.is_null(row) {
      None
    } else {
      Some(Timestamp::from_microsecond(expired_at_array.value(row))?)
    };

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      source_concept_uuid: Uuid::parse_str(source_array.value(row))?,
      target_concept_uuid: Uuid::parse_str(target_array.value(row))?,
      fact: fact_array.value(row).to_string(),
      name: name_array.value(row).to_string(),
      group_id: group_id_array.value(row).to_string(),
      attributes,
      interactions,
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
      valid_at,
      invalid_at,
      expired_at,
      fact_embedding: None, // Embeddings are not returned in queries
    })
  }
}

impl Default for Relates {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      source_concept_uuid: Uuid::now_v7(),
      target_concept_uuid: Uuid::now_v7(),
      fact: String::new(),
      name: String::new(),
      group_id: String::new(),
      interactions: Vec::new(),
      created_at: Timestamp::now(),
      valid_at: None,
      invalid_at: None,
      expired_at: None,
      fact_embedding: None,
      attributes: std::collections::HashMap::new(),
    }
  }
}

impl Display for Relates {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Relates(uuid={}, fact='{}')", self.uuid, self.fact)
  }
}

/// Includes edge (formerly HAS_MEMBER) - links Themes to Concepts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Includes {
  pub uuid: Uuid,
  pub source_theme_uuid: Uuid, // formerly source_node_uuid for Community
  pub target_concept_uuid: Uuid, // formerly target_node_uuid for Entity
  pub group_id: String,
  pub created_at: Timestamp,
}

impl ToDatabase for Includes {
  type MetaContext = ();
  type GraphContext = ();

  fn as_graph_value(&self, _ctx: Self::GraphContext) -> Result<Vec<(&'static str, kuzu::Value)>> {
    use time::OffsetDateTime;

    Ok(vec![
      ("uuid", kuzu::Value::UUID(self.uuid)),
      ("source_uuid", kuzu::Value::UUID(self.source_theme_uuid)),
      ("target_uuid", kuzu::Value::UUID(self.target_concept_uuid)),
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

  fn as_meta_value(&self, _ctx: Self::MetaContext) -> Result<RecordBatch> {
    let uuid_array = StringArray::from(vec![self.uuid.to_string()]);
    let source_array = StringArray::from(vec![self.source_theme_uuid.to_string()]);
    let target_array = StringArray::from(vec![self.target_concept_uuid.to_string()]);
    let group_id_array = StringArray::from(vec![self.group_id.clone()]);
    let created_at_array = TimestampMicrosecondArray::from(vec![self.created_at.as_microsecond()]);

    let schema = Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("source_theme_uuid", DataType::Utf8, false),
      Field::new("target_concept_uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
    ]));

    Ok(RecordBatch::try_new(
      schema,
      vec![
        Arc::new(uuid_array),
        Arc::new(source_array),
        Arc::new(target_array),
        Arc::new(group_id_array),
        Arc::new(created_at_array),
      ],
    )?)
  }

  fn primary_key_columns() -> &'static [&'static str] {
    &["uuid"]
  }

  fn meta_table_name() -> &'static str {
    "includes_edges"
  }

  fn graph_table_name() -> &'static str {
    "INCLUDES"
  }

  fn meta_schema(_ctx: Self::MetaContext) -> std::sync::Arc<arrow::datatypes::Schema> {
    Arc::new(Schema::new(vec![
      Field::new("uuid", DataType::Utf8, false),
      Field::new("source_theme_uuid", DataType::Utf8, false),
      Field::new("target_concept_uuid", DataType::Utf8, false),
      Field::new("group_id", DataType::Utf8, false),
      Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
      ),
    ]))
  }
}

impl FromRecordBatchRow for Includes {
  fn from_record_batch_row(batch: &RecordBatch, row: usize) -> Result<Self> {
    let uuid_array = batch
      .column_by_name("uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid uuid type".to_string()))?;

    let source_array = batch
      .column_by_name("source_theme_uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing source_theme_uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid source_theme_uuid type".to_string()))?;

    let target_array = batch
      .column_by_name("target_concept_uuid")
      .ok_or_else(|| Error::InvalidMetaDbData("Missing target_concept_uuid column".to_string()))?
      .as_any()
      .downcast_ref::<StringArray>()
      .ok_or_else(|| Error::InvalidMetaDbData("Invalid target_concept_uuid type".to_string()))?;

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

    Ok(Self {
      uuid: Uuid::parse_str(uuid_array.value(row))?,
      source_theme_uuid: Uuid::parse_str(source_array.value(row))?,
      target_concept_uuid: Uuid::parse_str(target_array.value(row))?,
      group_id: group_id_array.value(row).to_string(),
      created_at: Timestamp::from_microsecond(created_at_array.value(row))?,
    })
  }
}

impl Default for Includes {
  fn default() -> Self {
    Self {
      uuid: Uuid::now_v7(),
      source_theme_uuid: Uuid::now_v7(),
      target_concept_uuid: Uuid::now_v7(),
      group_id: String::new(),
      created_at: Timestamp::now(),
    }
  }
}

impl Display for Includes {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "Includes(uuid={}, {} -> {})",
      self.uuid, self.source_theme_uuid, self.target_concept_uuid
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jiff::Timestamp;
  use std::collections::HashMap;
  use uuid::Uuid;

  #[test]
  fn test_mentions_to_database() {
    let mentions = Mentions {
      uuid: Uuid::new_v4(),
      source_interaction_uuid: Uuid::new_v4(),
      target_concept_uuid: Uuid::new_v4(),
      group_id: "test-group".to_string(),
      created_at: Timestamp::now(),
    };

    // Test graph conversion
    let graph_values = mentions.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 5);
    assert_eq!(graph_values[0].0, "uuid");

    // Test meta conversion
    let batch = mentions.as_meta_value(()).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 5);
  }

  #[test]
  fn test_relates_to_database() {
    let mut attributes = HashMap::new();
    attributes.insert("confidence".to_string(), serde_json::json!(0.95));

    let relates = Relates {
      uuid: Uuid::new_v4(),
      source_concept_uuid: Uuid::new_v4(),
      target_concept_uuid: Uuid::new_v4(),
      fact: "works at".to_string(),
      name: "employment".to_string(),
      group_id: "test-group".to_string(),
      attributes,
      interactions: vec![Uuid::new_v4(), Uuid::new_v4()],
      created_at: Timestamp::now(),
      valid_at: Some(Timestamp::now()),
      invalid_at: None,
      expired_at: Some(Timestamp::now()),
      fact_embedding: Some(vec![0.1; 384]),
    };

    // Test graph conversion
    let graph_values = relates.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 8); // Only covering index properties

    // Test meta conversion with embedding dimensions
    let batch = relates.as_meta_value(384).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 13);
  }

  #[test]
  fn test_includes_to_database() {
    let includes = Includes {
      uuid: Uuid::new_v4(),
      source_theme_uuid: Uuid::new_v4(),
      target_concept_uuid: Uuid::new_v4(),
      group_id: "test-group".to_string(),
      created_at: Timestamp::now(),
    };

    // Test graph conversion
    let graph_values = includes.as_graph_value(()).unwrap();
    assert_eq!(graph_values.len(), 5);

    // Test meta conversion
    let batch = includes.as_meta_value(()).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 5);
  }

  #[test]
  fn test_primary_keys() {
    assert_eq!(Mentions::primary_key_columns(), &["uuid"]);
    assert_eq!(Relates::primary_key_columns(), &["uuid"]);
    assert_eq!(Includes::primary_key_columns(), &["uuid"]);
  }

  #[test]
  fn test_relates_with_optional_fields() {
    let relates = Relates {
      uuid: Uuid::new_v4(),
      source_concept_uuid: Uuid::new_v4(),
      target_concept_uuid: Uuid::new_v4(),
      fact: "related to".to_string(),
      name: "generic".to_string(),
      group_id: "test-group".to_string(),
      attributes: HashMap::new(),
      interactions: vec![],
      created_at: Timestamp::now(),
      valid_at: None, // Now optional
      invalid_at: None,
      expired_at: None,
      fact_embedding: None,
    };

    // Should handle None values properly
    let batch = relates.as_meta_value(384).unwrap();
    assert_eq!(batch.num_rows(), 1);
  }
}
