use crate::nats::setup::{NatsKv, create_kv_bucket};
use crate::{ModelDefinition, Pagination, Registry, Result};
use async_trait::async_trait;
use futures::stream::StreamExt;

#[derive(Debug, Clone)]
pub struct NatsModelRegistry {
  inner: NatsKv,
}

impl NatsModelRegistry {
  pub async fn new(bucket_prefix: &str) -> Result<Self> {
    let bucket_name = format!("{bucket_prefix}-models");
    let inner = create_kv_bucket(&bucket_name).await?;
    Ok(Self { inner })
  }
}

#[async_trait]
impl Registry for NatsModelRegistry {
  type Subject = ModelDefinition;
  type Key = String;

  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    let key = name;
    let value = serde_json::to_vec(&subject)?;
    self
      .inner
      .kv
      .put(key, value.into())
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV put error: {e}"),
        status_code: None,
      })?;
    Ok(())
  }

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let key = name;
    let entry = self
      .inner
      .kv
      .entry(&key)
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV entry error: {e}"),
        status_code: None,
      })?;
    if let Some(entry) = entry {
      let model: Self::Subject = serde_json::from_slice(&entry.value)?;
      self
        .inner
        .kv
        .delete(key)
        .await
        .map_err(|e| crate::Error::Registry {
          reason: format!("NATS KV delete error: {e}"),
          status_code: None,
        })?;
      Ok(Some(model))
    } else {
      Ok(None)
    }
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let key = name;
    let entry = self
      .inner
      .kv
      .entry(&key)
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV entry error: {e}"),
        status_code: None,
      })?;
    if let Some(entry) = entry {
      // If the entry is a tombstone (delete), treat as absent
      if entry.operation == async_nats::jetstream::kv::Operation::Delete {
        Ok(None)
      } else {
        let model: Self::Subject = serde_json::from_slice(&entry.value)?;
        Ok(Some(model))
      }
    } else {
      Ok(None)
    }
  }

  async fn has(&self, name: Self::Key) -> Result<bool> {
    let key = name;
    let entry = self
      .inner
      .kv
      .entry(&key)
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV entry error: {e}"),
        status_code: None,
      })?;
    if let Some(entry) = entry {
      // If the entry is a tombstone (delete), treat as absent
      Ok(entry.operation != async_nats::jetstream::kv::Operation::Delete)
    } else {
      Ok(false)
    }
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let raw_keys: Vec<Result<String, _>> = self
      .inner
      .kv
      .keys()
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV keys error: {e}"),
        status_code: None,
      })?
      .collect()
      .await;
    let mut keys: Vec<String> = raw_keys.into_iter().filter_map(Result::ok).collect();
    keys.sort();
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination.per_page.unwrap_or(keys.len()).max(1);
    let start_index = (page - 1) * per_page;
    let end_index = std::cmp::min(start_index + per_page, keys.len());
    let paged_keys = if start_index < keys.len() {
      keys[start_index..end_index].to_vec()
    } else {
      Vec::new()
    };
    Ok(paged_keys)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ModelDefinition;
  use crate::definitions::Provider;
  use crate::definitions::{Modality, ModelCapability};
  use std::env;

  fn test_prefix() -> String {
    env::var("NATS_TEST_PREFIX").unwrap_or_else(|_| "test-nats-model".to_string())
  }

  fn create_test_model(name: &str) -> ModelDefinition {
    ModelDefinition::builder()
      .id(name)
      .name(name)
      .provider(Provider::OpenAI)
      .context_size(1024)
      .capabilities([ModelCapability::Chat])
      .input_modalities([Modality::Text])
      .output_modalities([Modality::Text])
      .build()
  }

  #[tokio::test]
  async fn test_nats_model_registry_crud() {
    let prefix = test_prefix();
    let registry = NatsModelRegistry::new(&prefix).await.unwrap();
    let model = create_test_model("nats-model-1");
    let key = model.id.clone();

    // Clean up before test
    let _ = registry.del(key.clone()).await;

    // Put
    registry.put(key.clone(), model.clone()).await.unwrap();
    // Get
    let got = registry.get(key.clone()).await.unwrap().unwrap();
    assert_eq!(got.name, model.name);
    // Has
    assert!(registry.has(key.clone()).await.unwrap());
    // Keys
    let keys = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await
      .unwrap();
    assert!(keys.contains(&key));
    // Del
    let deleted = registry.del(key.clone()).await.unwrap().unwrap();
    assert_eq!(deleted.name, model.name);
    // After delete
    assert!(!registry.has(key.clone()).await.unwrap());
    assert!(registry.get(key.clone()).await.unwrap().is_none());
  }
}
