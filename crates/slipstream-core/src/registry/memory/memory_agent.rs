use crate::registry::Registry;
use crate::{Result, definitions::AgentDefinition, registry::Pagination};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MemoryAgentRegistry {
  store: dashmap::DashMap<Vec<u8>, AgentDefinition>,
}

impl Default for MemoryAgentRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl MemoryAgentRegistry {
  pub fn new() -> Self {
    Self {
      store: dashmap::DashMap::new(),
    }
  }
}

#[async_trait]
impl Registry for MemoryAgentRegistry {
  type Subject = AgentDefinition;
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
  use crate::definitions::AgentDefinition;
  use std::sync::Arc;

  fn assert_agent_eq(a: &AgentDefinition, b: &AgentDefinition) {
    assert_eq!(a.name, b.name);
    assert_eq!(a.description, b.description);
    assert_eq!(a.version, b.version);
    assert_eq!(a.slug, b.slug);
    assert_eq!(a.instructions, b.instructions);

    // Compare ModelDefinition fields
    assert_eq!(a.model, b.model);

    // Compare available_tools (now Vec<String>)
    assert_eq!(a.available_tools, b.available_tools);
  }

  fn create_test_agent(name: &str) -> AgentDefinition {
    AgentDefinition {
      name: name.to_string(),
      model: "gpt-4.1-nano".to_string(),
      description: Some(format!("Test agent {name}")),
      version: "1.0.0".to_string(),
      slug: format!("agent-{name}"),
      available_tools: vec!["local/tool1/1.0.0".to_string()],
      instructions: "You are a helpful assistant".to_string(),
      organization: "test-org".to_string(),
      project: "test-project".to_string(),
    }
  }

  fn create_registry() -> MemoryAgentRegistry {
    MemoryAgentRegistry::new()
  }

  /// Test template function for agent registry
  pub async fn test_agent_registry_basic_operations(
    registry: Arc<dyn Registry<Subject = AgentDefinition, Key = String>>,
  ) {
    let agent1 = create_test_agent("agent1");
    let agent2 = create_test_agent("agent2");

    // Test put and get
    registry
      .put("agent1".to_string(), agent1.clone())
      .await
      .unwrap();
    let retrieved = registry.get("agent1".to_string()).await.unwrap();
    assert!(retrieved.is_some());
    assert_agent_eq(&retrieved.unwrap(), &agent1);

    // Test has
    assert!(registry.has("agent1".to_string()).await.unwrap());
    assert!(!registry.has("nonexistent".to_string()).await.unwrap());

    // Test put another agent
    registry
      .put("agent2".to_string(), agent2.clone())
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
    let mut expected_keys = vec!["agent1".to_string(), "agent2".to_string()];
    expected_keys.sort();
    assert_eq!(keys, expected_keys);

    // Test delete
    let deleted = registry.del("agent1".to_string()).await.unwrap();
    assert!(deleted.is_some());
    assert_agent_eq(&deleted.unwrap(), &agent1);
    assert!(!registry.has("agent1".to_string()).await.unwrap());

    // Verify agent1 is gone
    let retrieved = registry.get("agent1".to_string()).await.unwrap();
    assert!(retrieved.is_none());

    // Delete non-existent
    let deleted = registry.del("nonexistent".to_string()).await.unwrap();
    assert!(deleted.is_none());
  }

  #[tokio::test]
  async fn test_memory_agent_registry_basic_operations() {
    let registry = Arc::new(create_registry());
    test_agent_registry_basic_operations(registry).await;
  }

  #[tokio::test]
  async fn test_memory_agent_registry_update_existing() {
    let registry = create_registry();
    let agent1_v1 = create_test_agent("agent1");
    let mut agent1_v2 = create_test_agent("agent1");
    agent1_v2.model = "gpt-4.1-mini".to_string();

    // Put initial version
    registry.put("agent1".to_string(), agent1_v1).await.unwrap();

    // Update with new version
    registry
      .put("agent1".to_string(), agent1_v2.clone())
      .await
      .unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get("agent1".to_string()).await.unwrap().unwrap();
    assert_eq!(retrieved.model, "gpt-4.1-mini");
  }
}
