use crate::{AgentDefinition, AgentRef, Pagination, Registry, Result};
use async_trait::async_trait;
use futures::{TryStreamExt, stream::StreamExt};

use crate::nats::setup::{NatsKv, create_kv_bucket};

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

// Helper: list all versioned keys for a slug ("slug/*")
async fn list_versioned_keys(inner: &NatsKv, slug: &str) -> Result<Vec<String>> {
  let raw_keys: Vec<String> = inner
    .kv
    .keys()
    .await
    .map_err(|e| crate::Error::Registry {
      reason: format!("NATS KV keys error: {e}"),
      status_code: None,
    })?
    .filter_map(|r| async move { r.ok() })
    .collect()
    .await;
  let prefix = format!("{}/", slug);
  let keys: Vec<String> = raw_keys
    .into_iter()
    .filter(|k| k.starts_with(&prefix))
    .collect();
  Ok(keys)
}

// Helper: resolve latest semver key "slug/<version>" among available versions; falls back to lexicographic if parse fails
async fn resolve_latest_semver_key(inner: &NatsKv, slug: &str) -> Result<Option<String>> {
  let mut keys = list_versioned_keys(inner, slug).await?;
  if keys.is_empty() {
    return Ok(None);
  }
  // Extract version string and sort by semver when possible
  keys.sort_by(|a, b| {
    let va = a.split('/').nth(1).unwrap_or("");
    let vb = b.split('/').nth(1).unwrap_or("");
    match (semver::Version::parse(va), semver::Version::parse(vb)) {
      (Ok(sa), Ok(sb)) => sa.cmp(&sb),
      _ => va.cmp(vb),
    }
  });
  Ok(keys.last().cloned())
}

#[async_trait]
impl Registry for NatsAgentRegistry {
  type Subject = AgentDefinition;
  type Key = AgentRef;

  /// Require version; write versioned entry only (no latest pointer).
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for put".to_string(),
        status_code: None,
      });
    }
    let version_key = format!("{}/{}", name.slug, name.version.as_ref().unwrap());
    let value = serde_json::to_vec(&subject)?;
    self
      .inner
      .kv
      .put(version_key, value.into())
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV put error: {e}"),
        status_code: None,
      })?;
    Ok(())
  }

  /// Require version; delete only the versioned key (no latest pointer maintenance).
  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for delete".to_string(),
        status_code: None,
      });
    }
    let version_key = format!("{}/{}", name.slug, name.version.as_ref().unwrap());

    let entry = self
      .inner
      .kv
      .entry(&version_key)
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV entry error: {e}"),
        status_code: None,
      })?;
    let result = if let Some(entry) = entry {
      let removed: Self::Subject = serde_json::from_slice(&entry.value)?;
      self
        .inner
        .kv
        .delete(version_key.clone())
        .await
        .map_err(|e| crate::Error::Registry {
          reason: format!("NATS KV delete error: {e}"),
          status_code: None,
        })?;
      Some(removed)
    } else {
      None
    };
    Ok(result)
  }

  /// Versionless GET resolves highest semver among slug/*; versioned GET reads slug/version.
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let key = if let Some(v) = &name.version {
      format!("{}/{}", name.slug, v)
    } else {
      match resolve_latest_semver_key(&self.inner, &name.slug).await? {
        Some(k) => k,
        None => return Ok(None),
      }
    };
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

  /// Versionless HAS checks for existence of any slug/* version; versioned HAS checks slug/version directly.
  async fn has(&self, name: Self::Key) -> Result<bool> {
    if let Some(v) = &name.version {
      let key = format!("{}/{}", name.slug, v);
      let entry = self
        .inner
        .kv
        .entry(&key)
        .await
        .map_err(|e| crate::Error::Registry {
          reason: format!("NATS KV entry error: {e}"),
          status_code: None,
        })?;
      Ok(matches!(entry, Some(en) if en.operation != async_nats::jetstream::kv::Operation::Delete))
    } else {
      // scan keys to see if any version exists
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
      let prefix = format!("{}/", name.slug);
      Ok(
        raw_keys
          .into_iter()
          .filter_map(Result::ok)
          .any(|k| k.starts_with(&prefix) && k.matches('/').count() == 1),
      )
    }
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut keys: Vec<String> = self
      .inner
      .kv
      .keys()
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV keys error: {e}"),
        status_code: None,
      })?
      .filter_map(|r| async move { r.ok() })
      .collect()
      .await;

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
  async fn test_nats_agent_registry_versionless_get_has_and_recompute_latest() {
    let prefix = test_prefix();
    let registry = NatsAgentRegistry::new(&prefix).await.unwrap();

    // two versions for same slug
    let mut v1 = create_test_agent("nats-ver-agent");
    v1.slug = "nats-ver-agent".to_string();
    v1.version = "1.0.0".to_string();
    let mut v2 = v1.clone();
    v2.version = "1.1.0".to_string();

    let r1 = AgentRef::from(&v1);
    let r2 = AgentRef::from(&v2);

    let _ = registry.del(r1.clone()).await;
    let _ = registry.del(r2.clone()).await;

    registry.put(r1.clone(), v1.clone()).await.unwrap();
    registry.put(r2.clone(), v2.clone()).await.unwrap();

    // versionless get
    let latest = registry
      .get(AgentRef::builder().slug("nats-ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest.unwrap().version, "1.1.0");

    // delete 1.1.0 and ensure fallback to 1.0.0
    let _ = registry.del(r2.clone()).await.unwrap();
    let latest_after = registry
      .get(AgentRef::builder().slug("nats-ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest_after.unwrap().version, "1.0.0");
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
    // Expect the versioned key
    assert!(keys.contains(&key.to_string()));
    // Del
    let deleted = registry.del(key.clone()).await.unwrap().unwrap();
    assert_eq!(deleted.name, agent.name);
    // After delete
    assert!(!registry.has(key.clone()).await.unwrap());
    assert!(registry.get(key.clone()).await.unwrap().is_none());
  }
}
