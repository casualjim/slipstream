use crate::{AgentDefinition, AgentRef, Pagination, Registry, Result};
use async_trait::async_trait;
use futures_util::StreamExt;

use crate::nats::setup::{create_kv_bucket, NatsKv};

#[derive(Debug, Clone)]
pub struct NatsAgentRegistry {
  inner: NatsKv,
}

impl NatsAgentRegistry {
  pub async fn new(bucket_prefix: &str) -> Result<Self> {
    let bucket_name = format!("{}-agents", bucket_prefix);
    let inner = create_kv_bucket(&bucket_name).await?;
    Ok(Self { inner })
  }
}

#[async_trait]
impl Registry for NatsAgentRegistry {
  type Subject = AgentDefinition;
  type Key = AgentRef;

  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    let key = name.to_string();
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
    let key = name.to_string();
    let entry = self
      .inner
      .kv
      .entry(&key)
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV entry error: {e}"),
        status_code: None,
      })?;
    let result = if let Some(entry) = entry {
      let agent: Self::Subject = serde_json::from_slice(&entry.value)?;
      self
        .inner
        .kv
        .delete(key)
        .await
        .map_err(|e| crate::Error::Registry {
          reason: format!("NATS KV delete error: {e}"),
          status_code: None,
        })?;
      Some(agent)
    } else {
      None
    };
    Ok(result)
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let key = name.to_string();
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
      if entry.operation == async_nats::jetstream::kv::Operation::Delete {
        Ok(None)
      } else {
        let agent: Self::Subject = serde_json::from_slice(&entry.value)?;
        Ok(Some(agent))
      }
    } else {
      Ok(None)
    }
  }

  async fn has(&self, name: Self::Key) -> Result<bool> {
    let key = name.to_string();
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
      Ok(entry.operation != async_nats::jetstream::kv::Operation::Delete)
    } else {
      Ok(false)
    }
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let raw_keys: Vec<Result<String, _>> = self.inner.kv.keys().await
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
  use crate::AgentDefinition;
  use std::env;

  fn test_prefix() -> String {
    env::var("NATS_TEST_PREFIX").unwrap_or_else(|_| "test-nats-agent".to_string())
  }

  fn create_test_agent(name: &str) -> AgentDefinition {
    AgentDefinition::builder()
      .name(name)
      .model("gpt-4")
      .version("1.0.0")
      .slug(name)
      .instructions("You are a helpful assistant")
      .organization("test-org")
      .project("test-project")
      .build()
  }

  #[tokio::test]
  async fn test_nats_agent_registry_crud() {
    let prefix = test_prefix();
    let registry = NatsAgentRegistry::new(&prefix).await.unwrap();
    let agent = create_test_agent("nats-agent-1");
    let key = AgentRef::from(&agent);

    // Clean up before test
    let _ = registry.del(key.clone()).await;

    // Put
    registry.put(key.clone(), agent.clone()).await.unwrap();
    // Get
    let got = registry.get(key.clone()).await.unwrap().unwrap();
    assert_eq!(got.name, agent.name);
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
    assert!(keys.contains(&key.to_string()));
    // Del
    let deleted = registry.del(key.clone()).await.unwrap().unwrap();
    assert_eq!(deleted.name, agent.name);
    // After delete
    assert!(!registry.has(key.clone()).await.unwrap());
    assert!(registry.get(key.clone()).await.unwrap().is_none());
  }
}
