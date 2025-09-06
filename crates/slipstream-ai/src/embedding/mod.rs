/// Embedding module for Slipstream ReState.
pub mod openailike;
pub mod workflow;

use std::time::Duration;

use async_trait::async_trait;
use typed_builder::TypedBuilder;

/// Configuration for the embedding module.
#[derive(Debug, Clone, TypedBuilder)]
pub struct Config {
  #[builder(default)]
  api_key: Option<secrecy::SecretString>,
  base_url: String,
  #[builder(default = Duration::from_secs(10))]
  timeout: Duration,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmbedInput {
  Text(String),
  TextBatch(Vec<String>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedOutput {
  embeddings: Vec<Vec<f32>>,
}

#[async_trait]
pub trait EmbeddingService {
  async fn embed(&self, text: EmbedInput) -> eyre::Result<EmbedOutput>;
}
