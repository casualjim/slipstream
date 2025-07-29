use std::{env, time::Duration};

use crate::registry::Registry;
use crate::registry::http::APIEnvelope;
use crate::{Result, definitions::ModelDefinition, registry::Pagination};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct HttpModelRegistry {
  client: ClientWithMiddleware,
  base_url: String,
}

impl HttpModelRegistry {
  pub fn new(base_url: String, api_key: SecretString) -> Result<Self> {
    let mut default_headers = HeaderMap::new();
    let api_key = HeaderValue::from_bytes(format!("Bearer {}", api_key.expose_secret()).as_bytes())
      .map_err(|e| {
        crate::Error::Io(std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("HeaderValue error: {e}"),
        ))
      })?;
    default_headers.insert(AUTHORIZATION, api_key);
    let client = ClientBuilder::new(
      reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest client build error: {e}"),
          ))
        })?,
    )
    .with(RetryTransientMiddleware::new_with_policy(
      ExponentialBackoff::builder()
        .base(2)
        .jitter(reqwest_retry::Jitter::Full)
        .retry_bounds(Duration::from_millis(500), Duration::from_secs(60))
        .build_with_total_retry_duration(Duration::from_secs(900)),
    ))
    .build();

    Ok(Self { client, base_url })
  }

  pub fn from_env() -> Result<Self> {
    let base_url = env::var("SLIPSTREAM_BASE_URL").map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Missing SLIPSTREAM_BASE_URL: {e}"),
      ))
    })?;
    let api_key = env::var("SLIPSTREAM_API_KEY").map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Missing SLIPSTREAM_API_KEY: {e}"),
      ))
    })?;
    Self::new(base_url, api_key.into())
  }
}

#[async_trait]
impl Registry for HttpModelRegistry {
  type Subject = ModelDefinition;
  type Key = String;

  async fn put(&self, _name: Self::Key, _subject: Self::Subject) -> Result<()> {
    Err(crate::Error::Registry {
      reason: "HttpModelRegistry does not support put operations".to_string(),
      status_code: None,
    })
  }

  async fn del(&self, _name: Self::Key) -> Result<Option<Self::Subject>> {
    Err(crate::Error::Registry {
      reason: "HttpModelRegistry does not support delete operations".to_string(),
      status_code: None,
    })
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let url = format!("{}/models/{}", self.base_url, name);
    let response = self.client.get(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;

    if response.status().is_success() {
      let envelope = response
        .json::<APIEnvelope<ModelDefinition>>()
        .await
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest JSON error: {e}"),
          ))
        })?;
      Ok(Some(envelope.result))
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      Err(crate::Error::Registry {
        reason: format!("HTTP error on get: {}", response.status()),
        status_code: Some(response.status().as_u16()),
      })
    }
  }

  async fn has(&self, name: Self::Key) -> Result<bool> {
    let url = format!("{}/models/{}", self.base_url, name);
    let response = self.client.head(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;
    Ok(response.status().is_success())
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut url = format!("{}/models", self.base_url);

    // Add query parameters for pagination
    let mut params = Vec::new();
    if let Some(page) = pagination.page {
      params.push(format!("page={}", page));
    }
    if let Some(per_page) = pagination.per_page {
      params.push(format!("per_page={}", per_page));
    }

    if !params.is_empty() {
      url.push('?');
      url.push_str(&params.join("&"));
    }

    let response = self.client.get(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;
    response
      .error_for_status_ref()
      .map_err(|e| crate::Error::Registry {
        reason: format!("HTTP error on keys: {e}"),
        status_code: Some(response.status().as_u16()),
      })?;

    let envelope = response
      .json::<APIEnvelope<Vec<ModelDefinition>>>()
      .await
      .map_err(|e| {
        crate::Error::Io(std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("Reqwest JSON error: {e}"),
        ))
      })?;

    Ok(envelope.result.into_iter().map(|model| model.id).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::definitions::{ApiDialect, Modality, ModelCapability, ModelDefinition, Provider};

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

  fn create_registry() -> HttpModelRegistry {
    HttpModelRegistry::from_env().expect("Failed to create registry")
  }

  #[tokio::test]
  async fn test_http_model_registry_get() {
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running
    let registry = create_registry();

    // Test get with a non-existent model (should return None)
    let result = registry
      .get("non-existent-model".to_string())
      .await
      .expect("HTTP call should succeed");
    assert_eq!(result, None);

    // Test get with a known model from seed data
    let result = registry
      .get("deepseek-ai/DeepSeek-R1-0528".to_string())
      .await
      .expect("HTTP call should succeed");
    let model = result.expect("Model should exist in seed data");
    assert_eq!(model.id, "deepseek-ai/DeepSeek-R1-0528");
    assert_eq!(model.name, "DeepSeek R1");
  }

  #[tokio::test]
  async fn test_http_model_registry_has() {
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running
    let registry = create_registry();

    // Test has with a non-existent model (should return false)
    let result = registry
      .has("non-existent-model".to_string())
      .await
      .expect("HTTP call should succeed");
    assert_eq!(result, false);

    // Test has with a known model from seed data (should return true)
    let result = registry
      .has("google/gemini-2.5-flash".to_string())
      .await
      .expect("HTTP call should succeed");
    assert_eq!(result, true);
  }

  #[tokio::test]
  async fn test_http_model_registry_keys() {
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running
    let registry = create_registry();

    // Test keys (no pagination)
    let result = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await;

    assert!(result.is_ok(), "Expected Ok result from keys()");
    let keys = result.unwrap();
    assert!(!keys.is_empty(), "Expected non-empty model keys list");
    let expected_models = vec![
      "deepseek-ai/DeepSeek-R1-0528",
      "google/gemini-2.5-flash",
      "openai/gpt-4.1",
      "anthropic/claude-sonnet-4-0",
    ];
    for expected_model in expected_models {
      assert!(
        keys.contains(&expected_model.to_string()),
        "Expected model '{}' not found in keys: {:?}",
        expected_model,
        keys
      );
    }
  }

  #[tokio::test]
  async fn test_http_model_registry_put_not_supported() {
    let registry = create_registry();
    let model = create_test_model("test-model");

    let result = registry.put("test-model".to_string(), model).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "HttpModelRegistry does not support put operations (status: None)"
    );
  }

  #[tokio::test]
  async fn test_http_model_registry_del_not_supported() {
    let registry = create_registry();

    let result = registry.del("test-model".to_string()).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "HttpModelRegistry does not support delete operations (status: None)"
    );
  }

  #[tokio::test]
  async fn test_http_model_registry_direct_construction() {
    // Use a dummy API key for testing
    let registry = HttpModelRegistry::new("http://localhost:8080".to_string(), "test-key".into())
      .expect("Failed to create registry");

    let model = create_test_model("test");
    let result = registry.put("test".to_string(), model).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "HttpModelRegistry does not support put operations (status: None)"
    );

    let result = registry.del("test".to_string()).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "HttpModelRegistry does not support delete operations (status: None)"
    );
  }

  #[tokio::test]
  async fn test_http_model_registry_read_operations() {
    let registry = create_registry();

    // Test that put operation returns an error
    let model = create_test_model("test-model");
    let put_result = registry.put("test-model".to_string(), model.clone()).await;
    assert!(put_result.is_err());
    assert!(
      put_result
        .unwrap_err()
        .to_string()
        .contains("does not support put operations")
    );

    // Test that del operation returns an error
    let del_result = registry.del("test-model".to_string()).await;
    assert!(del_result.is_err());
    assert!(
      del_result
        .unwrap_err()
        .to_string()
        .contains("does not support delete operations")
    );
  }
}
