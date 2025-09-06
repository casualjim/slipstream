use std::sync::Arc;

use crate::config::AppConfig;
use slipstream_objectstorage::{StorageClient, s3::S3Storage};

#[derive(Clone)]
pub struct AppState {
  pub storage: Arc<dyn StorageClient + Send + Sync>,
  pub bucket: String,
  pub max_file_size_bytes: usize,
}

impl AppState {
  pub async fn new(cfg: &AppConfig) -> eyre::Result<Self> {
    // config already validated once in load_config
    let storage = S3Storage::new(
      &cfg.uploads.s3_endpoint,
      &cfg.uploads.s3_region,
      &cfg.uploads.s3_access_key_id,
      &cfg.uploads.s3_secret_access_key,
      cfg.uploads.s3_force_path_style,
    )
    .await?;

    Ok(Self {
      storage: Arc::new(storage),
      bucket: cfg.uploads.bucket.clone(),
      max_file_size_bytes: cfg.uploads.max_file_size_bytes,
    })
  }
}

impl AppState {}
