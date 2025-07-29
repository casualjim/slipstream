use crate::definitions::ToolRef;
use crate::registry::Registry;
use crate::{Result, definitions::ToolDefinition, registry::Pagination};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MemoryToolRegistry {
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

  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    self.store.insert(name.to_string(), subject);
    Ok(())
  }

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if let Some((_, removed)) = self.store.remove(&name.to_string()) {
      Ok(Some(removed))
    } else {
      Ok(None)
    }
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if let Some(subject) = self.store.get(&name.to_string()) {
      Ok(Some(subject.clone()))
    } else {
      Ok(None)
    }
  }

  async fn has(&self, name: Self::Key) -> Result<bool> {
    Ok(self.store.contains_key(&name.to_string()))
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut keys: Vec<String> = self.store.iter().map(|kv| kv.key().clone()).collect();

    // Sort for consistent pagination
    keys.sort();

    // 1-based page, default to 1 if not provided
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
  use crate::definitions::{ToolDefinition, ToolProvider};
  use schemars::schema_for;
  use std::sync::Arc;

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
      slug: format!("tool-{name}"),
      name: name.to_string(),
      description: Some(format!("Test tool {name}")),
      version: "1.0.0".to_string(),
      arguments: Some(schema_for!(bool)),
      provider: ToolProvider::Local,
      created_at: None,
      updated_at: None,
    }
  }

  fn create_registry() -> MemoryToolRegistry {
    MemoryToolRegistry::new()
  }

  /// Test template function for any Registry implementation
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
      version: Some("1.0.0".to_string()),
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
      version: Some("1.0.0".to_string()),
    };
    assert!(!registry.has(nonexistent_ref).await.unwrap());

    // Test put another tool
    let tool2_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool2".to_string(),
      version: Some("1.0.0".to_string()),
    };
    registry
      .put(tool2_ref.clone(), tool2.clone())
      .await
      .unwrap();

    // Test keys (no pagination)
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
      version: Some("1.0.0".to_string()),
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
        version: Some("1.0.0".to_string()),
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
    assert_eq!(limited.len(), 3);
    assert!(limited[0].contains("tool01"));
    assert!(limited[1].contains("tool02"));
    assert!(limited[2].contains("tool03"));

    // Test page 2 with per_page 5 (should give tool06 through tool10)
    let page2 = registry
      .keys(Pagination {
        page: Some(2),
        per_page: Some(5),
      })
      .await
      .unwrap();
    assert_eq!(page2.len(), 5); // tool06 through tool10
    assert!(page2[0].contains("tool06"));
    assert!(page2[4].contains("tool10"));

    // Test page 1 with per_page 3 (should give tool01, tool02, tool03)
    let page1_limit3 = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(page1_limit3.len(), 3);
    assert!(page1_limit3[0].contains("tool01"));
    assert!(page1_limit3[1].contains("tool02"));
    assert!(page1_limit3[2].contains("tool03"));

    // Test page 2 with per_page 3 (should give tool04, tool05, tool06)
    let page2_limit3 = registry
      .keys(Pagination {
        page: Some(2),
        per_page: Some(3),
      })
      .await
      .unwrap();
    assert_eq!(page2_limit3.len(), 3);
    assert!(page2_limit3[0].contains("tool04"));
    assert!(page2_limit3[1].contains("tool05"));
    assert!(page2_limit3[2].contains("tool06"));

    // Test page out of range (should return empty)
    let page_out_of_range = registry
      .keys(Pagination {
        page: Some(99),
        per_page: Some(2),
      })
      .await
      .unwrap();
    assert_eq!(page_out_of_range, Vec::<String>::new());

    // Test last page with partial results (page 3, per_page 4, should give tool09, tool10)
    let last_partial = registry
      .keys(Pagination {
        page: Some(3),
        per_page: Some(4),
      })
      .await
      .unwrap();
    assert_eq!(last_partial.len(), 2);
    assert!(last_partial[0].contains("tool09"));
    assert!(last_partial[1].contains("tool10"));

    // Test page 1 with large per_page (should return all)
    let large_limit = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(20),
      })
      .await
      .unwrap();
    assert_eq!(large_limit.len(), 10);
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
        let tool = create_test_tool(&format!("concurrent_tool_{}", i));
        let tool_ref = ToolRef {
          provider: ToolProvider::Local,
          slug: format!("concurrent_tool_{}", i),
          version: Some("1.0.0".to_string()),
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
        slug: format!("concurrent_tool_{}", i),
        version: Some("1.0.0".to_string()),
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
          slug: format!("concurrent_tool_{}", i),
          version: Some("1.0.0".to_string()),
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
  async fn test_memory_registry_update_existing() {
    let registry = create_registry();
    let tool1_v1 = create_test_tool("tool1");
    let mut tool1_v2 = create_test_tool("tool1");
    tool1_v2.version = "2.0.0".to_string();

    let tool_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool1".to_string(),
      version: Some("1.0.0".to_string()),
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
    assert_eq!(retrieved.version, "2.0.0");
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
