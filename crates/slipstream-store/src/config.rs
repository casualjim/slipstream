use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
  pub data_dir: PathBuf,
  pub embedding: EmbeddingConfig,
}

impl Config {
  pub fn new_test<P: Into<PathBuf>>(data_dir: P) -> Self {
    Self {
      data_dir: data_dir.into(),
      embedding: EmbeddingConfig::default(),
    }
  }

  pub fn graph_data_dir(&self) -> PathBuf {
    self.data_dir.join("graphs")
  }

  pub fn meta_data_dir(&self) -> PathBuf {
    self.data_dir.join("meta")
  }
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
  /// Embedding provider (e.g., "ollama", "openai")
  pub provider: String,

  /// Embedding API base URL
  pub api_base: String,

  /// Embedding model to use
  pub model: String,

  /// Embedding API key (optional for Ollama, required for OpenAI)
  pub api_key: String,

  /// Embedding dimensions
  pub dimensions: i32,
}

impl Default for EmbeddingConfig {
  fn default() -> Self {
    Self {
      provider: "ollama".to_string(),
      api_base: "http://localhost:11434/v1".to_string(),
      model: "snowflake-arctic-embed:xs".to_string(),
      api_key: "doesntmatter".to_string(),
      dimensions: 384, // Match the actual dimensions of snowflake-arctic-embed:xs
    }
  }
}
