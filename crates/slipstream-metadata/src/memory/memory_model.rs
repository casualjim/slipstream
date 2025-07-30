use crate::{Modality, ModelCapability, ModelDefinition, Pagination, Provider, Registry, Result};
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
    let store = dashmap::DashMap::new();

    let gpt41: ModelDefinition = ModelDefinition::builder()
      .id("openai/gpt-4.1")
      .name("GPT-4.1")
      .description("OpenAI's GPT-4.1 model, optimized for general tasks.")
      .provider(Provider::OpenAI)
      .capabilities([
        ModelCapability::Chat,
        ModelCapability::Completion,
        ModelCapability::FunctionCalling,
        ModelCapability::StructuredOutput,
        ModelCapability::CodeExecution,
        ModelCapability::Tuning,
        ModelCapability::Search,
      ])
      .input_modalities([Modality::Text, Modality::Image])
      .output_modalities([Modality::Text])
      .context_size(1048576)
      .max_tokens(32768)
      .build();

    let o4mini: ModelDefinition = ModelDefinition::builder()
        .id("openai/o4-mini")
        .name("o4-mini")
        .description("OpenAI's o4-mini model, optimized for fast, effective reasoning with exceptionally efficient performance in coding and visual tasks.")
        .provider(Provider::OpenAI)
        .capabilities([
          ModelCapability::Chat,
          ModelCapability::Completion,
          ModelCapability::FunctionCalling,
          ModelCapability::StructuredOutput,
          ModelCapability::CodeExecution,
          ModelCapability::Tuning,
          ModelCapability::Search,
        ])
        .input_modalities([Modality::Text, Modality::Image])
        .output_modalities([Modality::Text])
        .context_size(200000)
        .max_tokens(100000)
        .build();

    let gpt41mini: ModelDefinition = ModelDefinition::builder()
      .id("openai/gpt-4.1-mini")
      .name("GPT-4.1 Mini")
      .description("OpenAI's GPT-4.1 Mini model, optimized for lightweight, fast general tasks.")
      .provider(Provider::OpenAI)
      .capabilities([
        ModelCapability::Chat,
        ModelCapability::Completion,
        ModelCapability::FunctionCalling,
        ModelCapability::StructuredOutput,
        ModelCapability::CodeExecution,
        ModelCapability::Tuning,
        ModelCapability::Search,
      ])
      .input_modalities([Modality::Text, Modality::Image])
      .output_modalities([Modality::Text])
      .context_size(1_000_000)
      .max_tokens(32_768)
      .build();

    let gpt41nano: ModelDefinition = ModelDefinition::builder()
        .id("openai/gpt-4.1-nano")
        .name("GPT-4.1 Nano")
        .description("OpenAI's GPT-4.1 Nano model, optimized for ultra-fast, efficient reasoning and coding tasks.")
        .provider(Provider::OpenAI)
        .capabilities([
          ModelCapability::Chat,
          ModelCapability::Completion,
          ModelCapability::FunctionCalling,
          ModelCapability::StructuredOutput,
          ModelCapability::CodeExecution,
          ModelCapability::Tuning,
          ModelCapability::Search,
        ])
        .input_modalities([Modality::Text, Modality::Image])
        .output_modalities([Modality::Text])
        .context_size(1_000_000)
        .max_tokens(32_768)
        .build();

    store.insert(gpt41mini.id.as_bytes().to_vec(), gpt41mini);
    store.insert(gpt41nano.id.as_bytes().to_vec(), gpt41nano);
    store.insert(gpt41.id.as_bytes().to_vec(), gpt41);
    store.insert(o4mini.id.as_bytes().to_vec(), o4mini);

    Self { store }
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

    // 1-based page, default to 1 if not provided
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination.per_page.unwrap_or(keys.len());
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
  use crate::{ApiDialect, Modality, ModelCapability, ModelDefinition, Provider};
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
        page: None,
        per_page: None,
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
