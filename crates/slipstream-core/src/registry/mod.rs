pub mod http;
pub mod memory;

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
  pub page: Option<usize>,
  pub per_page: Option<usize>,
}

#[async_trait]
pub trait Registry: Send + Sync {
  type Subject: Debug + Send + Sync + Serialize + for<'de> Deserialize<'de>;
  type Key: ToString + Send + Sync;

  /// Registers a tool with the registry.
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()>;

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>>;

  /// Retrieves a tool by name.
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>>;
  async fn has(&self, name: Self::Key) -> Result<bool>;

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>>;
}
