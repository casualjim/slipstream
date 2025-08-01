use crate::{Pagination, Registry, Result, ToolDefinition, ToolRef};
use async_trait::async_trait;
use futures::stream::StreamExt;

use crate::nats::setup::{NatsKv, create_kv_bucket};

#[derive(Debug, Clone)]
pub struct NatsToolRegistry {
  inner: NatsKv,
}

impl NatsToolRegistry {
  pub async fn new(bucket_prefix: &str) -> Result<Self> {
    let bucket_name = format!("{bucket_prefix}-tools");
    let inner = create_kv_bucket(&bucket_name).await?;
    Ok(Self { inner })
  }
}

// Helper: list versioned keys "provider/slug/<version>" (exactly two '/')
async fn list_versioned_keys(inner: &NatsKv, provider: &str, slug: &str) -> Result<Vec<String>> {
  let prefix = format!("{provider}/{slug}/");
  Ok(
    inner
      .kv
      .keys()
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("NATS KV keys error: {e}"),
        status_code: None,
      })?
      .filter_map(|r| {
        let prefix = prefix.clone();
        async move { r.ok().filter(|v| v.starts_with(&prefix)) }
      })
      .collect::<Vec<String>>()
      .await,
  )
}

// Helper: resolve latest semver versioned key
async fn resolve_latest_semver_key(
  inner: &NatsKv,
  provider: &str,
  slug: &str,
) -> Result<Option<String>> {
  let mut keys = list_versioned_keys(inner, provider, slug).await?;
  if keys.is_empty() {
    return Ok(None);
  }
  keys.sort_by(|a, b| {
    let va = a.split('/').nth(2).unwrap_or("");
    let vb = b.split('/').nth(2).unwrap_or("");
    match (semver::Version::parse(va), semver::Version::parse(vb)) {
      (Ok(sa), Ok(sb)) => sa.cmp(&sb),
      _ => va.cmp(vb),
    }
  });
  Ok(keys.last().cloned())
}

#[async_trait]
impl Registry for NatsToolRegistry {
  type Subject = ToolDefinition;
  type Key = ToolRef;

  /// Require version; write versioned entry only (no latest pointer).
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for put".to_string(),
        status_code: None,
      });
    }
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    let version_key = format!("{}/{}", base, name.version.as_ref().unwrap());
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

  /// Require version; delete only the versioned entry (no latest pointer maintenance).
  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for delete".to_string(),
        status_code: None,
      });
    }
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    let version_key = format!("{}/{}", base, name.version.as_ref().unwrap());

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

  /// Versionless GET resolves highest semver among provider/slug/*; versioned GET reads provider/slug/version.
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    let key = if let Some(v) = &name.version {
      format!("{base}/{v}")
    } else {
      match resolve_latest_semver_key(
        &self.inner,
        &AsRef::<str>::as_ref(&name.provider).to_lowercase(),
        &name.slug,
      )
      .await?
      {
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
        let tool: Self::Subject = serde_json::from_slice(&entry.value)?;
        Ok(Some(tool))
      }
    } else {
      Ok(None)
    }
  }

  /// Versionless HAS checks if any version exists under provider/slug/*.
  async fn has(&self, name: Self::Key) -> Result<bool> {
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    if let Some(v) = &name.version {
      let key = format!("{base}/{v}");
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
      let keys = list_versioned_keys(
        &self.inner,
        &AsRef::<str>::as_ref(&name.provider).to_lowercase(),
        &name.slug,
      )
      .await?;
      Ok(!keys.is_empty())
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
    // Return only versioned keys "provider/slug/version" (exactly two '/')

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
  use crate::ToolDefinition;
  use crate::ToolRef;
  use std::env;

  fn test_prefix() -> String {
    env::var("NATS_TEST_PREFIX").unwrap_or_else(|_| "test-nats-tool".to_string())
  }

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition::builder()
      .slug(name)
      .name(name)
      .version(semver::Version::parse("1.0.0").unwrap())
      .provider(crate::definitions::ToolProvider::Local)
      .description("A test tool")
      .build()
  }

  #[tokio::test]
  async fn test_nats_tool_registry_versionless_get_has_and_recompute_latest() {
    let prefix = test_prefix();
    let registry = NatsToolRegistry::new(&prefix).await.unwrap();

    let base = "nats-ver-tool";
    let mut t1 = create_test_tool(base);
    t1.slug = base.to_string();
    t1.version = semver::Version::parse("1.0.0").unwrap();
    let mut t2 = t1.clone();
    t2.version = semver::Version::parse("2.0.0").unwrap();

    let r1 = ToolRef::builder()
      .slug(base)
      .provider(crate::definitions::ToolProvider::Local)
      .version(semver::Version::parse("1.0.0").unwrap())
      .build();
    let r2 = ToolRef::builder()
      .slug(base)
      .provider(crate::definitions::ToolProvider::Local)
      .version(semver::Version::parse("2.0.0").unwrap())
      .build();

    let _ = registry.del(r1.clone()).await;
    let _ = registry.del(r2.clone()).await;

    registry.put(r1.clone(), t1.clone()).await.unwrap();
    registry.put(r2.clone(), t2.clone()).await.unwrap();

    let latest = registry
      .get(
        ToolRef::builder()
          .slug(base)
          .provider(crate::definitions::ToolProvider::Local)
          .build(),
      )
      .await
      .unwrap();
    assert_eq!(
      latest.unwrap().version,
      semver::Version::parse("2.0.0").unwrap()
    );

    let _ = registry.del(r2.clone()).await.unwrap();
    let latest_after = registry
      .get(
        ToolRef::builder()
          .slug(base)
          .provider(crate::definitions::ToolProvider::Local)
          .build(),
      )
      .await
      .unwrap();
    assert_eq!(
      latest_after.unwrap().version,
      semver::Version::parse("1.0.0").unwrap()
    );
  }

  #[tokio::test]
  async fn test_nats_tool_registry_crud() {
    let prefix = test_prefix();
    let registry = NatsToolRegistry::new(&prefix).await.unwrap();
    let tool = create_test_tool("nats-tool-1");
    let key = ToolRef::builder()
      .slug(tool.slug.clone())
      .provider(tool.provider)
      .version(tool.version.clone())
      .build();

    // Clean up before test
    let _ = registry.del(key.clone()).await;

    // Put
    registry.put(key.clone(), tool.clone()).await.unwrap();
    // Get
    let got = registry.get(key.clone()).await.unwrap().unwrap();
    assert_eq!(got.name, tool.name);
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
    // Expect the versioned key only
    assert!(keys.contains(&key.to_string()));
    // Del
    let deleted = registry.del(key.clone()).await.unwrap().unwrap();
    assert_eq!(deleted.name, tool.name);
    // After delete
    assert!(!registry.has(key.clone()).await.unwrap());
    assert!(registry.get(key.clone()).await.unwrap().is_none());
  }
}
