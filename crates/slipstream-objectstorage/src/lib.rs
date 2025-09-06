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

#[async_trait::async_trait]
pub trait StorageClient {
  async fn exists(&self, bucket: &str, key: &str) -> Result<bool, StorageError>;
  async fn put(
    &self,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    body: Box<dyn tokio::io::AsyncRead + Send + Unpin + 'static>,
  ) -> Result<(), StorageError>;

  async fn get(
    &self,
    bucket: &str,
    key: &str,
  ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin + 'static>, StorageError>;

  async fn list(&self, bucket: &str, prefix: &str) -> Result<Vec<StorageObject>, StorageError>;

  async fn delete(&self, bucket: &str, key: &str) -> Result<(), StorageError>;
}

pub mod s3;

#[cfg(test)]
mod tests {
  use super::s3::S3Storage;
  use crate::StorageClient;
  use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
  use aws_sdk_s3 as s3;
  use aws_sdk_s3::primitives::ByteStream as S3ByteStream;

  use secrecy::SecretString;
  use std::sync::{Arc, OnceLock};
  use testcontainers_modules::minio::MinIO;
  use testcontainers_modules::testcontainers::ContainerAsync;
  use testcontainers_modules::testcontainers::runners::AsyncRunner as _;
  use tokio::io::AsyncReadExt;
  use tracing_subscriber::EnvFilter;
  use uuid::Uuid;

  static TRACING: OnceLock<()> = OnceLock::new();
  fn init_tracing() {
    TRACING.get_or_init(|| {
      // Prefer RUST_LOG if set; otherwise, enable debug for our crate and
      // keep common dependencies quieter by default.
      let default_filter = "info,s3=warn,aws_smithy_http=warn,aws_smithy_types=warn,aws_config=warn,hyper=warn,tower=warn,reqwest=warn";
      let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

      let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_test_writer()
        .without_time()
        .compact()
        .try_init();
    });
  }

  pub async fn start_minio() -> (ContainerAsync<MinIO>, String) {
    let minio = MinIO::default().start().await.expect("start minio");
    let host = minio.get_host().await.expect("host");
    let api_port = minio.get_host_port_ipv4(9000).await.expect("port");
    let endpoint = format!("http://{host}:{api_port}");
    (minio, endpoint)
  }

  pub async fn make_s3_client(endpoint: &str) -> s3::Client {
    let creds = Credentials::from_keys("minioadmin", "minioadmin", None);
    let sdk_cfg = aws_config::SdkConfig::builder()
      .endpoint_url(endpoint)
      .region(aws_sdk_s3::config::Region::new("us-east-1".to_string()))
      .credentials_provider(SharedCredentialsProvider::new(creds))
      .build();
    let conf = s3::config::Builder::from(&sdk_cfg)
      .force_path_style(true)
      .build();
    s3::Client::from_conf(conf)
  }

  pub async fn build_storage(endpoint: &str) -> S3Storage {
    S3Storage::new(
      endpoint,
      "us-east-1",
      &SecretString::from("minioadmin".to_string()),
      &SecretString::from("minioadmin".to_string()),
      true,
    )
    .await
    .expect("storage")
  }

  pub struct TestCtx {
    pub minio: ContainerAsync<MinIO>,
    pub client: s3::Client,
    pub storage: Arc<dyn StorageClient + Send + Sync>,
    pub bucket: String,
  }

  impl TestCtx {
    pub async fn new() -> Self {
      init_tracing();
      let (minio, endpoint) = start_minio().await;
      let client = make_s3_client(&endpoint).await;
      let storage: Arc<dyn StorageClient + Send + Sync> = Arc::new(build_storage(&endpoint).await);
      let bucket = format!("test-{}", Uuid::now_v7());
      client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

      Self {
        minio,
        client,
        storage,
        bucket,
      }
    }

    pub async fn stop(&self) {
      self.minio.stop().await.expect("stop minio");
    }
  }

  #[tokio::test]
  async fn exists_negative_for_missing_key() {
    let ctx = TestCtx::new().await;
    let exists = ctx
      .storage
      .exists(&ctx.bucket, "missing.txt")
      .await
      .expect("exists");
    assert!(!exists);
    ctx.stop().await;
  }

  #[tokio::test]
  async fn exists_positive_for_present_key() {
    let ctx = TestCtx::new().await;
    let key = "present.txt";
    ctx
      .client
      .put_object()
      .bucket(&ctx.bucket)
      .key(key)
      .body(S3ByteStream::from_static(b"hi"))
      .send()
      .await
      .expect("put_object");

    let exists = ctx.storage.exists(&ctx.bucket, key).await.expect("exists");
    assert!(exists);
    ctx.stop().await;
  }

  #[tokio::test]
  async fn put_body_writes_object() {
    let ctx = TestCtx::new().await;
    let key = "upload.txt";
    let body = Box::new(std::io::Cursor::new(b"world".to_vec()));

    ctx
      .storage
      .put(&ctx.bucket, key, Some("text/plain"), body)
      .await
      .expect("put_body");

    let mut read_body = ctx.storage.get(&ctx.bucket, key).await.expect("get");
    let mut buffer = Vec::new();
    read_body
      .read_to_end(&mut buffer)
      .await
      .expect("read_to_end");
    assert_eq!(&buffer[..], b"world");
    ctx.stop().await;
  }
}
