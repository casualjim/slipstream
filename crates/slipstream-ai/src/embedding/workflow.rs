use std::{fmt::Display, str::FromStr};

use crate::embedding::{EmbedOutput, EmbeddingService};

/// Tokenizer type for chunk size calculation
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Tokenizer {
  /// Simple character-based tokenization
  #[serde(rename = "characters")]
  Characters,
  /// OpenAI tiktoken tokenizer with encoding name (e.g., "cl100k_base", "p50k_base")
  #[serde(rename = "tiktoken")]
  Tiktoken(#[serde(rename = "encoding")] String),
  /// HuggingFace tokenizer with specified model
  #[serde(rename = "huggingface")]
  HuggingFace(#[serde(rename = "model_id")] String),
}

impl Default for Tokenizer {
  fn default() -> Self {
    Self::Characters
  }
}

impl std::fmt::Debug for Tokenizer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Tokenizer::Characters => write!(f, "Characters"),
      Tokenizer::Tiktoken(name) => write!(f, "Tiktoken({})", name),
      Tokenizer::HuggingFace(model) => write!(f, "HuggingFace({})", model),
    }
  }
}

impl FromStr for Tokenizer {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "characters" => Ok(Tokenizer::Characters),
      _ if s.starts_with("tiktoken:") => {
        let encoding = s["tiktoken:".len()..].to_string();
        Ok(Tokenizer::Tiktoken(encoding))
      }
      _ if s.starts_with("hf:") => {
        let model_id = s["hf:".len()..].to_string();
        Ok(Tokenizer::HuggingFace(model_id))
      }
      _ => Err(format!("Unknown tokenizer type: {}", s)),
    }
  }
}

impl Display for Tokenizer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Tokenizer::Characters => write!(f, "characters"),
      Tokenizer::Tiktoken(encoding) => write!(f, "tiktoken:{}", encoding),
      Tokenizer::HuggingFace(model_id) => write!(f, "hf:{}", model_id),
    }
  }
}
