use super::Registry;
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
  use super::*;
  use crate::definitions::{
    AgentDefinition, Modality, ModelCapability, ModelDefinition, Provider, ToolDefinition,
    ToolProvider,
  };
  use std::sync::Arc;

  fn assert_agent_eq(a: &AgentDefinition, b: &AgentDefinition) {
    assert_eq!(a.name, b.name);
    assert_eq!(a.description, b.description);
    assert_eq!(a.version, b.version);
    assert_eq!(a.slug, b.slug);
    assert_eq!(a.instructions, b.instructions);

    // Compare ModelDefinition fields
    assert_eq!(a.model.id, b.model.id);
    assert_eq!(a.model.name, b.model.name);
    assert_eq!(a.model.provider, b.model.provider);
    assert_eq!(a.model.description, b.model.description);
    assert_eq!(a.model.context_size, b.model.context_size);
    assert_eq!(a.model.max_tokens, b.model.max_tokens);
    assert_eq!(a.model.temperature, b.model.temperature);
    assert_eq!(a.model.top_p, b.model.top_p);
    assert_eq!(a.model.frequency_penalty, b.model.frequency_penalty);
    assert_eq!(a.model.presence_penalty, b.model.presence_penalty);
    assert_eq!(a.model.capabilities, b.model.capabilities);
    assert_eq!(a.model.input_modalities, b.model.input_modalities);
    assert_eq!(a.model.output_modalities, b.model.output_modalities);
    assert_eq!(a.model.dialect, b.model.dialect);

    // Compare ToolDefinition fields
    assert_eq!(a.available_tools.len(), b.available_tools.len());
    for (tool_a, tool_b) in a.available_tools.iter().zip(b.available_tools.iter()) {
      assert_eq!(tool_a.slug, tool_b.slug);
      assert_eq!(tool_a.name, tool_b.name);
      assert_eq!(tool_a.description, tool_b.description);
      assert_eq!(tool_a.version, tool_b.version);
      assert_eq!(tool_a.provider, tool_b.provider);
      // Note: We skip comparing arguments (schemars::Schema) as it doesn't implement PartialEq
    }
  }

  fn create_test_agent(name: &str) -> AgentDefinition {
    AgentDefinition {
      name: name.to_string(),
      model: ModelDefinition {
        id: "gpt-4".to_string(),
        name: "GPT-4".to_string(),
        provider: Provider::OpenAI,
        description: Some("OpenAI GPT-4 model".to_string()),
        context_size: 8192,
        max_tokens: Some(4096),
        temperature: Some(0.7),
        top_p: Some(1.0),
        frequency_penalty: Some(0.0),
        presence_penalty: Some(0.0),
        capabilities: vec![ModelCapability::Chat, ModelCapability::FunctionCalling],
        input_modalities: vec![Modality::Text],
        output_modalities: vec![Modality::Text],
        dialect: None,
      },
      description: Some(format!("Test agent {name}")),
      version: "1.0.0".to_string(),
      slug: format!("agent-{name}"),
      available_tools: vec![ToolDefinition {
        slug: "tool1".to_string(),
        name: "Test Tool".to_string(),
        description: Some("A test tool".to_string()),
        version: "1.0.0".to_string(),
        arguments: None,
        provider: ToolProvider::Local,
      }],
      instructions: "You are a helpful assistant".to_string(),
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
        from: None,
        limit: None,
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
    agent1_v2.model.id = "gpt-4-turbo".to_string();
    agent1_v2.model.name = "GPT-4 Turbo".to_string();

    // Put initial version
    registry.put("agent1".to_string(), agent1_v1).await.unwrap();

    // Update with new version
    registry
      .put("agent1".to_string(), agent1_v2.clone())
      .await
      .unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get("agent1".to_string()).await.unwrap().unwrap();
    assert_eq!(retrieved.model.id, "gpt-4-turbo");
    assert_eq!(retrieved.model.name, "GPT-4 Turbo");
  }
}
