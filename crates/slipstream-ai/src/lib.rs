mod completer;
mod embedder;
mod error;
pub mod messages;
mod reranker;

use async_trait::async_trait;
pub use error::{Error, Result};

#[async_trait]
pub trait Completer: Send + Sync {
  async fn complete(&self, prompt: &str) -> Result<Vec<messages::Message<messages::ModelMessage>>>;
}
