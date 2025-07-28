use super::Registry;
use crate::{Result, definitions::ModelDefinition, registry::Pagination};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MemoryModelRegistry {
  store: dashmap::DashMap<Vec<u8>, ModelDefinition>,
}

impl Default for MemoryModelRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl MemoryModelRegistry {
  pub fn new() -> Self {
    Self {
      store: dashmap::DashMap::new(),
    }
  }
}

#[async_trait]
impl Registry for MemoryModelRegistry {
  type Subject = ModelDefinition;
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
  use crate::definitions::{ApiDialect, Modality, ModelCapability, ModelDefinition, Provider};
  use std::sync::Arc;

  fn create_test_model(name: &str) -> ModelDefinition {
    ModelDefinition {
      id: format!("model-{name}"),
      name: name.to_string(),
      provider: Provider::OpenAI,
      description: Some(format!("Test model {name}")),
      context_size: 4096,
      max_tokens: Some(2048),
      temperature: Some(0.7),
      top_p: Some(1.0),
      frequency_penalty: Some(0.0),
      presence_penalty: Some(0.0),
      capabilities: vec![ModelCapability::Chat, ModelCapability::Completion],
      input_modalities: vec![Modality::Text],
      output_modalities: vec![Modality::Text],
      dialect: Some(ApiDialect::OpenAI),
    }
  }

  fn create_registry() -> MemoryModelRegistry {
    MemoryModelRegistry::new()
  }

  /// Test template function for model registry
  pub async fn test_model_registry_basic_operations(
    registry: Arc<dyn Registry<Subject = ModelDefinition, Key = String>>,
  ) {
    let model1 = create_test_model("model1");
    let model2 = create_test_model("model2");

    // Test put and get
    registry
      .put("model1".to_string(), model1.clone())
      .await
      .unwrap();
    let retrieved = registry.get("model1".to_string()).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved_model = retrieved.unwrap();
    assert_eq!(retrieved_model.name, model1.name);
    assert_eq!(retrieved_model.context_size, model1.context_size);

    // Test has
    assert!(registry.has("model1".to_string()).await.unwrap());
    assert!(!registry.has("nonexistent".to_string()).await.unwrap());

    // Test put another model
    registry
      .put("model2".to_string(), model2.clone())
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
    let mut expected_keys = vec!["model1".to_string(), "model2".to_string()];
    expected_keys.sort();
    assert_eq!(keys, expected_keys);

    // Test delete
    let deleted = registry.del("model1".to_string()).await.unwrap();
    assert!(deleted.is_some());
    let deleted_model = deleted.unwrap();
    assert_eq!(deleted_model.name, model1.name);
    assert!(!registry.has("model1".to_string()).await.unwrap());

    // Verify model1 is gone
    let retrieved = registry.get("model1".to_string()).await.unwrap();
    assert!(retrieved.is_none());

    // Delete non-existent
    let deleted = registry.del("nonexistent".to_string()).await.unwrap();
    assert!(deleted.is_none());
  }

  #[tokio::test]
  async fn test_memory_model_registry_basic_operations() {
    let registry = Arc::new(create_registry());
    test_model_registry_basic_operations(registry).await;
  }

  #[tokio::test]
  async fn test_memory_model_registry_update_existing() {
    let registry = create_registry();
    let model1_v1 = create_test_model("model1");
    let mut model1_v2 = create_test_model("model1");
    model1_v2.context_size = 8192;
    model1_v2.provider = Provider::Anthropic;

    // Put initial version
    registry.put("model1".to_string(), model1_v1).await.unwrap();

    // Update with new version
    registry
      .put("model1".to_string(), model1_v2.clone())
      .await
      .unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get("model1".to_string()).await.unwrap().unwrap();
    assert_eq!(retrieved.context_size, 8192);
    assert_eq!(retrieved.provider, Provider::Anthropic);
  }
}
