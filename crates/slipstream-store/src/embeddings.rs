use crate::config::Config;
use arrow::array::{ArrayData, Float32Builder};
use arrow::datatypes::DataType;
use arrow_array::{Array, ArrayRef, FixedSizeListArray, Float32Array};
use async_openai::{
  Client,
  config::OpenAIConfig,
  types::{CreateEmbeddingRequest, Embedding, EmbeddingInput, EncodingFormat},
};
use std::{borrow::Cow, sync::Arc};
use tokio::{runtime::Handle, task};

/// Simple embedding function for LanceDB
/// Uses OpenAI-compatible API (works with Ollama)
#[derive(Debug)]
pub struct EmbeddingFunction {
  provider: String,
  model: String,
  dimensions: i32,
  api_base: String,
  api_key: String,
}

impl EmbeddingFunction {
  /// Create from app config - only way to create this
  pub fn from_config(config: &Config) -> Self {
    Self {
      provider: config.embedding.provider.clone(),
      model: config.embedding.model.clone(),
      dimensions: config.embedding.dimensions,
      api_base: config.embedding.api_base.clone(),
      api_key: config.embedding.api_key.clone(),
    }
  }
}

impl lancedb::embeddings::EmbeddingFunction for EmbeddingFunction {
  fn name(&self) -> &str {
    &self.provider
  }

  fn source_type(&self) -> lancedb::Result<Cow<DataType>> {
    Ok(Cow::Owned(DataType::Utf8))
  }

  fn dest_type(&self) -> lancedb::Result<Cow<DataType>> {
    Ok(Cow::Owned(DataType::new_fixed_size_list(
      DataType::Float32,
      self.dimensions,
      false,
    )))
  }

  fn compute_source_embeddings(&self, source: ArrayRef) -> lancedb::Result<ArrayRef> {
    let len = source.len();
    let inner = self.compute_inner(source)?;

    let fsl = DataType::new_fixed_size_list(DataType::Float32, self.dimensions, false);

    let array_data = ArrayData::builder(fsl)
      .len(len)
      .add_child_data(inner.into_data())
      .build()?;

    Ok(Arc::new(FixedSizeListArray::from(array_data)))
  }

  fn compute_query_embeddings(&self, input: ArrayRef) -> lancedb::Result<ArrayRef> {
    let arr = self.compute_inner(input)?;
    Ok(Arc::new(arr))
  }
}

impl EmbeddingFunction {
  fn compute_inner(&self, source: Arc<dyn Array>) -> lancedb::Result<Float32Array> {
    // Only supports non-nullable string arrays
    if source.is_nullable() {
      return Err(lancedb::Error::InvalidInput {
        message: "Expected non-nullable data type".to_string(),
      });
    }

    if !matches!(source.data_type(), DataType::Utf8 | DataType::LargeUtf8) {
      return Err(lancedb::Error::InvalidInput {
        message: "Expected Utf8 data type".to_string(),
      });
    };

    let input = match source.data_type() {
      DataType::Utf8 => {
        let array = source
          .as_any()
          .downcast_ref::<arrow_array::StringArray>()
          .unwrap()
          .iter()
          .map(|s| s.unwrap().to_string())
          .collect::<Vec<String>>();
        EmbeddingInput::StringArray(array)
      }
      DataType::LargeUtf8 => {
        let array = source
          .as_any()
          .downcast_ref::<arrow_array::LargeStringArray>()
          .unwrap()
          .iter()
          .map(|s| s.unwrap().to_string())
          .collect::<Vec<String>>();
        EmbeddingInput::StringArray(array)
      }
      _ => unreachable!("Already checked data type"),
    };

    let model = self.model.clone();
    let api_base = self.api_base.clone();
    let api_key = self.api_key.clone();

    // Block in place to handle async operation
    task::block_in_place(move || {
      Handle::current().block_on(async {
        let creds = OpenAIConfig::new()
          .with_api_key(api_key)
          .with_api_base(api_base);

        let client = Client::with_config(creds);
        let embed = client.embeddings();
        let req = CreateEmbeddingRequest {
          model,
          input,
          encoding_format: Some(EncodingFormat::Float),
          user: None,
          dimensions: None,
        };

        let mut builder = Float32Builder::new();

        let res = embed
          .create(req)
          .await
          .map_err(|e| lancedb::Error::Runtime {
            message: format!("Embedding request failed: {e}"),
          })?;

        for Embedding { embedding, .. } in res.data.iter() {
          builder.append_slice(embedding);
        }

        Ok(builder.finish())
      })
    })
  }
}

/// Create embedding function from config - only way to create embedding functions
pub fn create_embedding_function_from_config(
  config: &Config,
) -> Arc<dyn lancedb::embeddings::EmbeddingFunction> {
  Arc::new(EmbeddingFunction::from_config(config))
}
