#![allow(dead_code)]

use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
pub use aws_sdk_s3 as s3;
use secrecy::SecretString;
use std::sync::{Arc, OnceLock};
pub use testcontainers_modules::minio::MinIO;
pub use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner as _;
use tracing_subscriber::EnvFilter;
pub use uuid::Uuid;

use slipstream_objectstorage::StorageClient;
pub use slipstream_objectstorage::s3::S3Storage;

static TRACING: OnceLock<()> = OnceLock::new();
fn init_tracing() {
  TRACING.get_or_init(|| {
    // Prefer RUST_LOG if set; otherwise, enable debug for our crate and
    // keep common dependencies quieter by default.
    let default_filter = "\
s3=warn,aws_smithy_http=warn,aws_smithy_types=warn,aws_config=warn,\n hyper=warn,tower=warn,reqwest=warn,\n info";
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
