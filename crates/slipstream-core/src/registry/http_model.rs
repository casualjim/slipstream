use super::Registry;
use crate::{Result, definitions::ModelDefinition, registry::Pagination};
use async_trait::async_trait;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct HttpModelRegistry {
    client: ClientWithMiddleware,
    base_url: String,
}

impl HttpModelRegistry {
    pub fn new(client: ClientWithMiddleware, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct KeysResponse {
    keys: Vec<String>,
}

#[async_trait]
impl Registry for HttpModelRegistry {
    type Subject = ModelDefinition;
    type Key = String;

    async fn put(&self, _name: Self::Key, _subject: Self::Subject) -> Result<()> {
        Err(eyre::eyre!("HttpModelRegistry is read-only. PUT operation not supported."))
    }

    async fn del(&self, _name: Self::Key) -> Result<Option<Self::Subject>> {
        Err(eyre::eyre!("HttpModelRegistry is read-only. DELETE operation not supported."))
    }

    async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
        let url = format!("{}/models/{}", self.base_url, name);
        let response = self.client.get(&url).send().await?;
        
        if response.status().is_success() {
            let model = response.json::<ModelDefinition>().await?;
            Ok(Some(model))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            Err(eyre::eyre!("HTTP error: {}", response.status()))
        }
    }

    async fn has(&self, name: Self::Key) -> Result<bool> {
        let url = format!("{}/models/{}/exists", self.base_url, name);
        let response = self.client.head(&url).send().await?;
        Ok(response.status().is_success())
    }

    async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
        let mut url = format!("{}/models", self.base_url);
        
        // Add query parameters for pagination
        let mut params = Vec::new();
        if let Some(from) = &pagination.from {
            params.push(format!("from={}", from));
        }
        if let Some(limit) = pagination.limit {
            params.push(format!("limit={}", limit));
        }
        
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        
        let response = self.client.get(&url).send().await?;
        response.error_for_status_ref()?;
        
        let keys_response = response.json::<KeysResponse>().await?;
        Ok(keys_response.keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{ApiDialect, Modality, ModelCapability, ModelDefinition, Provider};
    use reqwest_middleware::ClientBuilder;
    use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
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

    fn create_registry() -> Result<HttpModelRegistry> {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        
        HttpModelRegistry::from_env(client)
    }

    #[tokio::test]
    async fn test_http_model_registry_creation() {
        // This test will fail if SLIPSTREAM_AGENTS_BASE_URL is not set
        // which is the expected behavior according to requirements
        let result = create_registry();
        if std::env::var("SLIPSTREAM_AGENTS_BASE_URL").is_err() {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("SLIPSTREAM_AGENTS_BASE_URL"));
        }
    }

    #[tokio::test]
    async fn test_http_model_registry_with_base_url() {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        
        // Create registry with a dummy URL for testing the structure
        let registry = HttpModelRegistry::new(client, "http://localhost:3000".to_string());
        
        // Verify the registry was created correctly
        assert_eq!(registry.base_url, "http://localhost:3000");
    }
}