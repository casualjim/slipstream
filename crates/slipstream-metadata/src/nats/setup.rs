use crate::Result;
use async_nats::jetstream;
use async_nats::jetstream::kv::Store as KvStore;

#[derive(Debug, Clone)]
pub struct NatsKv {
  pub kv: KvStore,
}

pub async fn create_kv_bucket(bucket_name: &str) -> Result<NatsKv> {
  let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
  let client = async_nats::connect(nats_url)
    .await
    .map_err(|e| crate::Error::Registry {
      reason: format!("Failed to connect to NATS: {e}"),
      status_code: None,
    })?;
  let jetstream = jetstream::new(client.clone());
  let kv = match jetstream.get_key_value(bucket_name).await {
    Ok(store) => store,
    Err(_) => jetstream
      .create_key_value(async_nats::jetstream::kv::Config {
        bucket: bucket_name.to_string(),
        ..Default::default()
      })
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("Failed to create KV bucket: {e}"),
        status_code: None,
      })?,
  };
  Ok(NatsKv { kv })
}
