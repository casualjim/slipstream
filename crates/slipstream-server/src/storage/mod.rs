use bytes::Bytes;
use http_body_util::combinators::BoxBody as HbBoxBody;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
  #[error("not found")]
  NotFound,
  #[error("already exists")]
  AlreadyExists,
  #[error("unavailable: {0}")]
  Unavailable(String),
  #[error("timeout")]
  Timeout,
  #[error("internal: {0}")]
  Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StorageObject {
  pub key: String,
  pub size: u64,
  pub last_modified: jiff::Timestamp,
}

pub type BoxBody = HbBoxBody<Bytes, std::io::Error>;

#[async_trait::async_trait]
pub trait StorageClient {
  async fn exists(&self, bucket: &str, key: &str) -> Result<bool, StorageError>;
  async fn put_body(
    &self,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    body: BoxBody,
  ) -> Result<(), StorageError>;

  async fn get(&self, bucket: &str, key: &str) -> Result<BoxBody, StorageError>;

  async fn list(&self, bucket: &str, prefix: &str) -> Result<Vec<StorageObject>, StorageError>;

  async fn delete(&self, bucket: &str, key: &str) -> Result<(), StorageError>;
}

pub mod s3;
