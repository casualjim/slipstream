mod memory_agent;
mod memory_model;
mod memory_tool;

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub use memory_agent::MemoryAgentRegistry;
pub use memory_model::MemoryModelRegistry;
pub use memory_tool::MemoryToolRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
  pub from: Option<String>,
  pub limit: Option<usize>,
}

#[async_trait]
pub trait Registry: Send + Sync {
  type Subject: Debug + Send + Sync + Serialize + for<'de> Deserialize<'de>;
  type Key: AsRef<[u8]> + Send + Sync;

  /// Registers a tool with the registry.
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()>;

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>>;

  /// Retrieves a tool by name.
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>>;
  async fn has(&self, name: Self::Key) -> Result<bool>;

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>>;
}
