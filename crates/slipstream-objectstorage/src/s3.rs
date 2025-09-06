use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::primitives::ByteStream as S3ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

use secrecy::ExposeSecret;
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::debug;

use super::{StorageClient, StorageError, StorageObject};

pub struct S3Storage {
  client: s3::Client,
}

const MIN_PART_SIZE: u64 = 5 * 1024 * 1024; // 5MB

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

  async fn put(
    &self,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    mut body: Box<dyn AsyncRead + Send + Unpin + 'static>,
  ) -> Result<(), StorageError> {
    let multipart_upload = self
      .client
      .create_multipart_upload()
      .bucket(bucket)
      .key(key)
      .set_content_type(content_type.map(|s| s.to_string()))
      .send()
      .await
      .map_err(|e| StorageError::Unavailable(e.to_string()))?;

    let upload_id = multipart_upload.upload_id().unwrap();
    let mut completed_parts = Vec::new();
    let mut part_number = 1;

    loop {
      let mut buffer = Vec::with_capacity(MIN_PART_SIZE as usize);

      while buffer.len() < MIN_PART_SIZE as usize {
        let mut temp_buf = [0u8; 8192];
        match body.read(&mut temp_buf).await {
          Ok(0) => break, // End of stream
          Ok(n) => buffer.extend_from_slice(&temp_buf[..n]),
          Err(e) => return Err(StorageError::Unavailable(e.to_string())),
        }
      }

      if buffer.is_empty() {
        break; // No more data to upload
      }

      let upload_part_res = self
        .client
        .upload_part()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(S3ByteStream::from(buffer))
        .send()
        .await
        .map_err(|e| StorageError::Unavailable(e.to_string()))?;

      completed_parts.push(
        CompletedPart::builder()
          .part_number(part_number)
          .e_tag(upload_part_res.e_tag.unwrap_or_default())
          .build(),
      );

      part_number += 1;
    }

    let completed_multipart_upload = CompletedMultipartUpload::builder()
      .set_parts(Some(completed_parts))
      .build();

    self
      .client
      .complete_multipart_upload()
      .bucket(bucket)
      .key(key)
      .upload_id(upload_id)
      .multipart_upload(completed_multipart_upload)
      .send()
      .await
      .map_err(|e| StorageError::Unavailable(e.to_string()))?;

    Ok(())
  }

  async fn get(
    &self,
    bucket: &str,
    key: &str,
  ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin + 'static>, StorageError> {
    let obj = self
      .client
      .get_object()
      .bucket(bucket)
      .key(key)
      .send()
      .await
      .map_err(|err| match err {
        SdkError::ServiceError(e) if e.err().is_no_such_key() => StorageError::NotFound,
        e => StorageError::Unavailable(e.to_string()),
      })?;

    // Return the S3 body directly as Box<dyn AsyncRead>
    Ok(Box::new(obj.body.into_async_read()))
  }

  async fn list(&self, bucket: &str, prefix: &str) -> Result<Vec<StorageObject>, StorageError> {
    let res = self
      .client
      .list_objects_v2()
      .bucket(bucket)
      .prefix(prefix)
      .send()
      .await
      .map_err(|e| StorageError::Unavailable(e.to_string()))?;

    let objects = res
      .contents()
      .iter()
      .filter_map(|obj| {
        let last_modified = obj
          .last_modified
          .and_then(|dt| jiff::Timestamp::from_second(dt.secs()).ok());

        Some(StorageObject {
          key: obj.key()?.to_string(),
          size: obj.size().unwrap_or_default() as u64,
          last_modified: last_modified?,
        })
      })
      .collect();

    Ok(objects)
  }

  async fn delete(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
    self
      .client
      .delete_object()
      .bucket(bucket)
      .key(key)
      .send()
      .await
      .map_err(|e| StorageError::Unavailable(e.to_string()))?;

    Ok(())
  }
}
