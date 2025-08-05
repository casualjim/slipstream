use crate::Registry;
use crate::{Pagination, Result};
use crate::{ToolDefinition, ToolRef};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MemoryToolRegistry {
  // Keyed by "provider/slug/version" for concrete versions
  store: dashmap::DashMap<String, ToolDefinition>,
}

impl Default for MemoryToolRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl MemoryToolRegistry {
  pub fn new() -> Self {
    Self {
      store: dashmap::DashMap::new(),
    }
  }
}

#[async_trait]
impl Registry for MemoryToolRegistry {
  type Subject = ToolDefinition;
  type Key = ToolRef;

  /// Create or update requires version to be present.
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for put".to_string(),
        status_code: None,
      });
    }
    // Persist the concrete version record under "provider/slug/version"
    // Build keys explicitly to avoid relying on Display with/without version
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    let versioned_key = format!("{}/{}", base, name.version.as_ref().unwrap());
    self.store.insert(versioned_key, subject.clone());
    Ok(())
  }

  /// Delete requires version; delete specific version and fix latest pointer if needed.
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
    let versioned_key = format!("{}/{}", base, name.version.as_ref().unwrap());

    let removed = if let Some((_, removed)) = self.store.remove(&versioned_key) {
      Some(removed)
    } else {
      None
    };
    Ok(removed)
  }

  /// Get supports versionless lookups: when version is None, compute latest by scanning "provider/slug/*"
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    if let Some(v) = &name.version {
      let key = format!("{base}/{v}");
      return Ok(self.store.get(&key).map(|v| v.clone()));
    }
    // versionless: compute latest by semver ordering across versioned entries
    let prefix = format!("{base}/");
    let mut candidates: Vec<ToolDefinition> = self
      .store
      .iter()
      .filter_map(|kv| {
        let k = kv.key();
        if k.starts_with(&prefix) {
          Some(kv.value().clone())
        } else {
          None
        }
      })
      .collect();
    if candidates.is_empty() {
      return Ok(None);
    }
    candidates.sort_by(|a, b| a.version.cmp(&b.version));
    Ok(candidates.pop())
  }

  /// Has supports versionless lookups: when version is None, check if any "provider/slug/*" exists
  async fn has(&self, name: Self::Key) -> Result<bool> {
    let base = format!(
      "{}/{}",
      AsRef::<str>::as_ref(&name.provider).to_lowercase(),
      name.slug
    );
    if let Some(v) = &name.version {
      let key = format!("{base}/{v}");
      return Ok(self.store.contains_key(&key));
    }
    let prefix = format!("{base}/");
    Ok(self.store.iter().any(|kv| kv.key().starts_with(&prefix)))
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    // Return all keys: latest pointers (provider/slug) and all versions (provider/slug/version)
    let mut keys: Vec<String> = self.store.iter().map(|kv| kv.key().clone()).collect();

    // Sort for consistent pagination
    keys.sort();

    // 1-based page, default to 1 if not provided
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination
      .per_page
      .unwrap_or_else(|| keys.len().max(1))
      .max(1);

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
  use crate::{ToolDefinition, ToolProvider};
  use schemars::schema_for;
  use std::sync::Arc;

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
      slug: format!("tool-{name}"),
      name: name.to_string(),
      description: Some(format!("Test tool {name}")),
      version: semver::Version::parse("1.0.0").unwrap(),
      arguments: Some(schema_for!(bool)),
      provider: ToolProvider::Local,
      created_at: None,
      updated_at: None,
      restate: None,
      mcp: None,
    }
  }

  fn create_registry() -> MemoryToolRegistry {
    MemoryToolRegistry::new()
  }

  /// Test template function for any Registry implementation (versioned paths)
  /// This can be reused by other registry implementations by passing their instance
  pub async fn test_registry_basic_operations(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = ToolRef>>,
  ) {
    let tool1 = create_test_tool("tool1");
    let tool2 = create_test_tool("tool2");

    // Test put and get
    let tool1_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool1".to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };
    registry
      .put(tool1_ref.clone(), tool1.clone())
      .await
      .unwrap();
    let retrieved = registry.get(tool1_ref.clone()).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved_tool = retrieved.unwrap();
    assert_eq!(retrieved_tool.name, tool1.name);
    assert_eq!(retrieved_tool.version, tool1.version);

    // Test has
    assert!(registry.has(tool1_ref.clone()).await.unwrap());
    let nonexistent_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "nonexistent".to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };
    assert!(!registry.has(nonexistent_ref).await.unwrap());

    // Test put another tool
    let tool2_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool2".to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };
    registry
      .put(tool2_ref.clone(), tool2.clone())
      .await
      .unwrap();

    // Test keys (no pagination): expect only versioned keys (no latest pointers)
    let keys = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await
      .unwrap();
    let mut expected_keys = vec![tool1_ref.to_string(), tool2_ref.to_string()];
    expected_keys.sort();
    assert_eq!(keys, expected_keys);

    // Test delete
    let deleted = registry.del(tool1_ref.clone()).await.unwrap();
    assert!(deleted.is_some());
    let deleted_tool = deleted.unwrap();
    assert_eq!(deleted_tool.name, tool1.name);
    assert!(!registry.has(tool1_ref.clone()).await.unwrap());

    // Verify tool1 is gone
    let retrieved = registry.get(tool1_ref).await.unwrap();
    assert!(retrieved.is_none());

    // Delete non-existent
    let nonexistent_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "nonexistent".to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };
    let deleted = registry.del(nonexistent_ref).await.unwrap();
    assert!(deleted.is_none());
  }

  /// Test template for pagination functionality
  pub async fn test_registry_pagination(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = ToolRef>>,
  ) {
    // Add multiple tools
    for i in 1..=10 {
      let tool = create_test_tool(&format!("tool{i:02}"));
      let tool_ref = ToolRef {
        provider: ToolProvider::Local,
        slug: format!("tool{i:02}"),
        version: Some(semver::Version::parse("1.0.0").unwrap()),
      };
      registry.put(tool_ref, tool).await.unwrap();
    }

    // Test no pagination (all keys)
    let all_keys = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await
      .unwrap();
    // keys() returns only versioned keys, with 10 tools and 1 version each => 10
    assert_eq!(all_keys.len(), 10);
    assert!(all_keys.windows(2).all(|w| w[0] <= w[1])); // Should be sorted

    // Test limit only
    let limited = registry
      .keys(Pagination {
        page: None,
        per_page: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(limited.len(), 3);
    // We can't assert exact toolXX at positions because latest and versioned keys interleave lexicographically.
    // Just assert that entries are sorted and belong to the expected prefix.
    assert!(limited.windows(2).all(|w| w[0] <= w[1]));
    for k in &limited {
      assert!(k.starts_with("local/tool0") || k.starts_with("local/tool1"));
    }

    // Test page 2 with per_page 5: still returns 5, but contents may mix latest and versioned
    let page2 = registry
      .keys(Pagination {
        page: Some(2),
        per_page: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(page2.len(), 5);
    assert!(page2.windows(2).all(|w| w[0] <= w[1]));

    // Test page 1 with per_page 3
    let page1_limit3 = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(page1_limit3.len(), 3);
    assert!(page1_limit3.windows(2).all(|w| w[0] <= w[1]));

    // Test page 2 with per_page 3
    let page2_limit3 = registry
      .keys(Pagination {
        page: Some(2),
        per_page: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(page2_limit3.len(), 3);
    assert!(page2_limit3.windows(2).all(|w| w[0] <= w[1]));

    // Test page out of range (should return empty)
    let page_out_of_range = registry
      .keys(Pagination {
        page: Some(99),
        per_page: Some(2),
      })
      .await
      .unwrap();
    assert_eq!(page_out_of_range, Vec::<String>::new());

    // Test last page with partial results for per_page=4 over 10 total keys (only versioned)
    let last_partial = registry
      .keys(Pagination {
        page: Some(3), // 3rd page of 4-per-page over 10 entries -> last 2 entries
        per_page: Some(4),
      })
      .await
      .unwrap();
    assert_eq!(last_partial.len(), 2);
    assert!(last_partial.windows(2).all(|w| w[0] <= w[1]));

    // Test page 1 with large per_page (should return all 10 keys now)
    let large_limit = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(20),
      })
      .await
      .unwrap();
    assert_eq!(large_limit.len(), 10);
    // Check that every toolXX appears at least in some key
    for i in 1..=10 {
      let expected = format!("tool{i:02}");
      assert!(large_limit.iter().any(|key| key.contains(&expected)));
    }
  }

  /// Test template for concurrent access
  pub async fn test_registry_concurrent_access(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = ToolRef>>,
  ) {
    use tokio::task::JoinSet;

    let mut tasks = JoinSet::new();

    // Spawn multiple tasks writing concurrently
    for i in 0..10 {
      let reg = registry.clone();
      tasks.spawn(async move {
        let tool = create_test_tool(&format!("concurrent_tool_{i}"));
        let tool_ref = ToolRef {
          provider: ToolProvider::Local,
          slug: format!("concurrent_tool_{i}"),
          version: Some(semver::Version::parse("1.0.0").unwrap()),
        };
        reg.put(tool_ref, tool).await.unwrap();
      });
    }

    // Wait for all writes to complete
    while let Some(result) = tasks.join_next().await {
      result.unwrap();
    }

    // Verify all tools were written
    for i in 0..10 {
      let tool_ref = ToolRef {
        provider: ToolProvider::Local,
        slug: format!("concurrent_tool_{i}"),
        version: Some(semver::Version::parse("1.0.0").unwrap()),
      };
      assert!(registry.has(tool_ref).await.unwrap());
    }

    // Test concurrent reads
    let mut read_tasks = JoinSet::new();
    for i in 0..10 {
      let reg = registry.clone();
      read_tasks.spawn(async move {
        let tool_ref = ToolRef {
          provider: ToolProvider::Local,
          slug: format!("concurrent_tool_{i}"),
          version: Some(semver::Version::parse("1.0.0").unwrap()),
        };
        let tool = reg.get(tool_ref).await.unwrap();
        assert!(tool.is_some());
        tool.unwrap()
      });
    }

    let mut results = Vec::new();
    while let Some(result) = read_tasks.join_next().await {
      results.push(result.unwrap());
    }

    assert_eq!(results.len(), 10);
  }

  #[tokio::test]
  async fn test_memory_registry_basic_operations() {
    let registry = Arc::new(create_registry());
    test_registry_basic_operations(registry).await;
  }

  #[tokio::test]
  async fn test_memory_registry_pagination() {
    let registry = Arc::new(create_registry());
    test_registry_pagination(registry).await;
  }

  #[tokio::test]
  async fn test_memory_registry_concurrent_access() {
    let registry = Arc::new(create_registry());
    test_registry_concurrent_access(registry).await;
  }

  #[tokio::test]
  async fn test_memory_registry_versionless_get_has() {
    let registry = create_registry();
    // create two versions for same tool
    let base = "ver-tool";
    let mut t1 = create_test_tool(base);
    t1.slug = base.to_string();
    t1.version = semver::Version::parse("1.0.0").unwrap();

    let mut t2 = t1.clone();
    t2.version = semver::Version::parse("1.2.0").unwrap();

    let tref1 = ToolRef {
      provider: ToolProvider::Local,
      slug: base.to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };
    let tref2 = ToolRef {
      provider: ToolProvider::Local,
      slug: base.to_string(),
      version: Some(semver::Version::parse("1.2.0").unwrap()),
    };

    registry.put(tref1.clone(), t1.clone()).await.unwrap();
    registry.put(tref2.clone(), t2.clone()).await.unwrap();

    // versionless GET
    let latest = registry
      .get(ToolRef {
        provider: ToolProvider::Local,
        slug: base.to_string(),
        version: None,
      })
      .await
      .unwrap();
    assert_eq!(
      latest.unwrap().version,
      semver::Version::parse("1.2.0").unwrap()
    );

    // delete 1.2.0 and ensure fallback
    let _ = registry.del(tref2.clone()).await.unwrap();
    let latest_after = registry
      .get(ToolRef {
        provider: ToolProvider::Local,
        slug: base.to_string(),
        version: None,
      })
      .await
      .unwrap();
    assert_eq!(
      latest_after.unwrap().version,
      semver::Version::parse("1.0.0").unwrap()
    );
  }

  #[tokio::test]
  async fn test_memory_registry_update_existing() {
    let registry = create_registry();
    let tool1_v1 = create_test_tool("tool1");
    let mut tool1_v2 = create_test_tool("tool1");
    tool1_v2.version = semver::Version::parse("2.0.0").unwrap();

    let tool_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool1".to_string(),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    };

    // Put initial version
    registry.put(tool_ref.clone(), tool1_v1).await.unwrap();

    // Update with new version
    registry
      .put(tool_ref.clone(), tool1_v2.clone())
      .await
      .unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get(tool_ref).await.unwrap().unwrap();
    assert_eq!(retrieved.version, semver::Version::parse("2.0.0").unwrap());
  }

  #[tokio::test]
  async fn test_memory_registry_empty_pagination() {
    let registry = create_registry();

    // Test pagination on empty registry
    let keys = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await
      .unwrap();
    assert_eq!(keys, Vec::<String>::new());

    let limited = registry
      .keys(Pagination {
        page: None,
        per_page: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(limited, Vec::<String>::new());

    let page1 = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(page1, Vec::<String>::new());
  }
}
