use super::Registry;
use crate::{Result, definitions::ToolDefinition, registry::Pagination};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MemoryToolRegistry {
  store: dashmap::DashMap<Vec<u8>, ToolDefinition>,
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
  type Key = String;

  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    self.store.insert(name.into_bytes(), subject);
    Ok(())
  }

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if let Some((_, removed)) = self.store.remove(&name.into_bytes()) {
      Ok(Some(removed))
    } else {
      Ok(None)
    }
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if let Some(subject) = self.store.get(name.as_bytes()) {
      Ok(Some(subject.clone()))
    } else {
      Ok(None)
    }
  }

  async fn has(&self, name: Self::Key) -> Result<bool> {
    Ok(self.store.contains_key(&name.into_bytes()))
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut keys: Vec<String> = self
      .store
      .iter()
      .map(|kv| String::from_utf8_lossy(kv.key()).to_string())
      .collect();

    // Sort for consistent pagination
    keys.sort();

    // Apply pagination
    let start_index = if let Some(from) = pagination.from {
      keys.binary_search(&from).unwrap_or_else(|x| x)
    } else {
      0
    };

    let end_index = if let Some(limit) = pagination.limit {
      std::cmp::min(start_index + limit, keys.len())
    } else {
      keys.len()
    };

    Ok(
      keys
        .into_iter()
        .skip(start_index)
        .take(end_index - start_index)
        .collect::<Vec<_>>(),
    )
  }
}

#[cfg(test)]
mod tests {
  use schemars::schema_for;

  use super::*;
  use crate::definitions::{ToolDefinition, ToolProvider};
  use std::sync::Arc;

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
      slug: format!("tool-{name}"),
      name: name.to_string(),
      description: Some(format!("Test tool {name}")),
      version: "1.0.0".to_string(),
      arguments: Some(schema_for!(bool)),
      provider: ToolProvider::Local,
    }
  }

  fn create_registry() -> MemoryToolRegistry {
    MemoryToolRegistry::new()
  }

  /// Test template function for any Registry implementation
  /// This can be reused by other registry implementations by passing their instance
  pub async fn test_registry_basic_operations(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = String>>,
  ) {
    let tool1 = create_test_tool("tool1");
    let tool2 = create_test_tool("tool2");

    // Test put and get
    registry
      .put("tool1".to_string(), tool1.clone())
      .await
      .unwrap();
    let retrieved = registry.get("tool1".to_string()).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved_tool = retrieved.unwrap();
    assert_eq!(retrieved_tool.name, tool1.name);
    assert_eq!(retrieved_tool.version, tool1.version);

    // Test has
    assert!(registry.has("tool1".to_string()).await.unwrap());
    assert!(!registry.has("nonexistent".to_string()).await.unwrap());

    // Test put another tool
    registry
      .put("tool2".to_string(), tool2.clone())
      .await
      .unwrap();

    // Test keys (no pagination)
    let keys = registry
      .keys(Pagination {
        from: None,
        limit: None,
      })
      .await
      .unwrap();
    let mut expected_keys = vec!["tool1".to_string(), "tool2".to_string()];
    expected_keys.sort();
    assert_eq!(keys, expected_keys);

    // Test delete
    let deleted = registry.del("tool1".to_string()).await.unwrap();
    assert!(deleted.is_some());
    let deleted_tool = deleted.unwrap();
    assert_eq!(deleted_tool.name, tool1.name);
    assert!(!registry.has("tool1".to_string()).await.unwrap());

    // Verify tool1 is gone
    let retrieved = registry.get("tool1".to_string()).await.unwrap();
    assert!(retrieved.is_none());

    // Delete non-existent
    let deleted = registry.del("nonexistent".to_string()).await.unwrap();
    assert!(deleted.is_none());
  }

  /// Test template for pagination functionality
  pub async fn test_registry_pagination(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = String>>,
  ) {
    // Add multiple tools
    for i in 1..=10 {
      let tool = create_test_tool(&format!("tool{i:02}"));
      registry.put(format!("tool{i:02}"), tool).await.unwrap();
    }

    // Test no pagination (all keys)
    let all_keys = registry
      .keys(Pagination {
        from: None,
        limit: None,
      })
      .await
      .unwrap();
    assert_eq!(all_keys.len(), 10);
    assert!(all_keys.windows(2).all(|w| w[0] <= w[1])); // Should be sorted

    // Test limit only
    let limited = registry
      .keys(Pagination {
        from: None,
        limit: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(limited.len(), 3);
    assert_eq!(limited, vec!["tool01", "tool02", "tool03"]);

    // Test from only
    let from_tool05 = registry
      .keys(Pagination {
        from: Some("tool05".to_string()),
        limit: None,
      })
      .await
      .unwrap();
    assert_eq!(from_tool05.len(), 6); // tool05 through tool10
    assert_eq!(from_tool05[0], "tool05");

    // Test from + limit
    let from_limit = registry
      .keys(Pagination {
        from: Some("tool03".to_string()),
        limit: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(from_limit, vec!["tool03", "tool04", "tool05"]);

    // Test from non-existent key (should start from next available)
    let from_nonexistent = registry
      .keys(Pagination {
        from: Some("tool03.5".to_string()),
        limit: Some(2),
      })
      .await
      .unwrap();
    assert_eq!(from_nonexistent, vec!["tool04", "tool05"]);

    // Test from key after all keys
    let from_end = registry
      .keys(Pagination {
        from: Some("tool99".to_string()),
        limit: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(from_end, Vec::<String>::new());

    // Test limit larger than available keys
    let large_limit = registry
      .keys(Pagination {
        from: Some("tool08".to_string()),
        limit: Some(10),
      })
      .await
      .unwrap();
    assert_eq!(large_limit, vec!["tool08", "tool09", "tool10"]);
  }

  /// Test template for concurrent access
  pub async fn test_registry_concurrent_access(
    registry: Arc<dyn Registry<Subject = ToolDefinition, Key = String>>,
  ) {
    use tokio::task::JoinSet;

    let mut tasks = JoinSet::new();

    // Spawn multiple tasks writing concurrently
    for i in 0..10 {
      let reg = registry.clone();
      tasks.spawn(async move {
        let tool = create_test_tool(&format!("concurrent_tool_{}", i));
        reg
          .put(format!("concurrent_tool_{}", i), tool)
          .await
          .unwrap();
      });
    }

    // Wait for all writes to complete
    while let Some(result) = tasks.join_next().await {
      result.unwrap();
    }

    // Verify all tools were written
    for i in 0..10 {
      assert!(
        registry
          .has(format!("concurrent_tool_{}", i))
          .await
          .unwrap()
      );
    }

    // Test concurrent reads
    let mut read_tasks = JoinSet::new();
    for i in 0..10 {
      let reg = registry.clone();
      read_tasks.spawn(async move {
        let tool = reg.get(format!("concurrent_tool_{}", i)).await.unwrap();
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
  async fn test_memory_registry_update_existing() {
    let registry = create_registry();
    let tool1_v1 = create_test_tool("tool1");
    let mut tool1_v2 = create_test_tool("tool1");
    tool1_v2.version = "2.0.0".to_string();

    // Put initial version
    registry.put("tool1".to_string(), tool1_v1).await.unwrap();

    // Update with new version
    registry
      .put("tool1".to_string(), tool1_v2.clone())
      .await
      .unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get("tool1".to_string()).await.unwrap().unwrap();
    assert_eq!(retrieved.version, "2.0.0");
  }

  #[tokio::test]
  async fn test_memory_registry_empty_pagination() {
    let registry = create_registry();

    // Test pagination on empty registry
    let keys = registry
      .keys(Pagination {
        from: None,
        limit: None,
      })
      .await
      .unwrap();
    assert_eq!(keys, Vec::<String>::new());

    let limited = registry
      .keys(Pagination {
        from: None,
        limit: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(limited, Vec::<String>::new());

    let from_key = registry
      .keys(Pagination {
        from: Some("any".to_string()),
        limit: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(from_key, Vec::<String>::new());
  }
}
