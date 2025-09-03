use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream as S3ByteStream;
use secrecy::ExposeSecret;
use tracing::debug;

use super::{BoxBody, StorageClient, StorageError};

pub struct S3Storage {
  client: s3::Client,
}

impl S3Storage {
  pub async fn new(
    endpoint: &str,
    region: &str,
    access_key_id: &secrecy::SecretString,
    secret_access_key: &secrecy::SecretString,
    force_path_style: bool,
  ) -> eyre::Result<Self> {
    let creds = Credentials::from_keys(
      access_key_id.expose_secret(),
      secret_access_key.expose_secret(),
      None,
    );
    let sdk_cfg = aws_config::SdkConfig::builder()
      .endpoint_url(endpoint)
      .region(Region::new(region.to_string()))
      .credentials_provider(SharedCredentialsProvider::new(creds))
      .build();

    let cfg = s3::config::Builder::from(&sdk_cfg)
      .force_path_style(force_path_style)
      .build();

    Ok(Self {
      client: s3::Client::from_conf(cfg),
    })
  }
}

#[async_trait::async_trait]
impl StorageClient for S3Storage {
  async fn exists(&self, bucket: &str, key: &str) -> Result<bool, StorageError> {
    match self
      .client
      .head_object()
      .bucket(bucket)
      .key(key)
      .send()
      .await
    {
      Ok(_) => Ok(true),
      Err(err) => {
        if let Some(svc) = err.as_service_error() {
          // AWS S3 typically signals missing objects with NotFound.
          if svc.is_not_found() {
            return Ok(false);
          }

          debug!(err = ?err, "S3 head_object failed");
        }
        Err(StorageError::Unavailable(err.to_string()))
      }
    }
  }

  async fn put_body(
    &self,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    body: BoxBody,
  ) -> Result<(), StorageError> {
    let bs = S3ByteStream::from_body_1_x(body);

    let mut req = self.client.put_object().bucket(bucket).key(key).body(bs);
    if let Some(ct) = content_type {
      req = req.content_type(ct);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(60), req.send()).await {
      Ok(Ok(_)) => Ok(()),
      Ok(Err(e)) => Err(StorageError::Unavailable(e.to_string())),
      Err(_) => Err(StorageError::Timeout),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::TestCtx;
  use aws_sdk_s3::primitives::ByteStream as S3ByteStream;
  use bytes::Bytes;
  use http_body_util::BodyExt as _;
  use http_body_util::Full;

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
    let body = Full::new(Bytes::from_static(b"world"))
      .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "impossible"))
      .boxed();

    ctx
      .storage
      .put_body(&ctx.bucket, key, Some("text/plain"), body)
      .await
      .expect("put_body");

    let out = ctx
      .client
      .get_object()
      .bucket(&ctx.bucket)
      .key(key)
      .send()
      .await
      .expect("get_object");
    let bytes = out.body.collect().await.expect("collect").into_bytes();
    assert_eq!(&bytes[..], b"world");
    ctx.stop().await;
  }
}
