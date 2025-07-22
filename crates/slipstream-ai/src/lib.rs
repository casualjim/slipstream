mod content;
mod completer;
mod embedder;
mod error;
mod messages;
mod reranker;

use async_trait::async_trait;
pub use error::{Error, Result};

use crate::messages::{MessageEnvelope, ModelMessage};

#[async_trait]
pub trait Completer: Send + Sync {
  async fn complete(&self, prompt: &str) -> Result<Vec<MessageEnvelope<ModelMessage>>>;
}
