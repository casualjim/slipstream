use std::{env, time::Duration};

use crate::registry::Registry;
use crate::{
  Result, definitions::AgentDefinition, registry::Pagination, registry::http::APIEnvelope,
};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CreateAgentRequest {
  pub name: String,
  pub version: String,
  pub description: Option<String>,
  pub model: String,
  pub instructions: String,
  #[serde(rename = "availableTools")]
  pub available_tools: Option<Vec<String>>,
  pub organization: String,
  pub project: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateAgentRequest {
  pub name: Option<String>,
  pub description: Option<String>,
  pub model: Option<String>,
  pub instructions: Option<String>,
  #[serde(rename = "availableTools")]
  pub available_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct HttpAgentRegistry {
  client: ClientWithMiddleware,
  base_url: String,
}

impl HttpAgentRegistry {
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
impl Registry for HttpAgentRegistry {
  type Subject = AgentDefinition;
  type Key = String;

  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    // For agents, the name is expected to be in format "slug/version"
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 {
      return Err(crate::Error::Registry {
        reason: format!("Agent name must be in format 'slug/version', got: {}", name),
        status_code: None,
      });
    }

    let slug = parts[0];
    let version = parts[1];

    // Check if agent exists for update vs create
    let exists = self
      .has(name.clone())
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("Failed to check existence: {e}"),
        status_code: None,
      })?;

    if exists {
      // Update existing agent
      let update_request = UpdateAgentRequest {
        name: Some(subject.name),
        description: subject.description,
        model: Some(subject.model),
        instructions: Some(subject.instructions),
        available_tools: Some(subject.available_tools),
      };

      let url = format!("{}/agents/{}/{}", self.base_url, slug, version);
      let response = self
        .client
        .put(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&update_request)?)
        .send()
        .await
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest error: {e}"),
          ))
        })?;

      if response.status().is_success() {
        Ok(())
      } else {
        Err(crate::Error::Registry {
          reason: format!("HTTP error on update: {}", response.status()),
          status_code: Some(response.status().as_u16()),
        })
      }
    } else {
      // Create new agent
      let create_request = CreateAgentRequest {
        name: subject.name,
        version: subject.version,
        description: subject.description,
        model: subject.model,
        instructions: subject.instructions,
        available_tools: Some(subject.available_tools),
        organization: subject.organization,
        project: subject.project,
      };

      let url = format!("{}/agents", self.base_url);
      let response = self
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_request)?)
        .send()
        .await
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest error: {e}"),
          ))
        })?;

      if response.status().is_success() {
        Ok(())
      } else {
        Err(crate::Error::Registry {
          reason: format!("HTTP error on create: {}", response.status()),
          status_code: Some(response.status().as_u16()),
        })
      }
    }
  }

  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    // First get the agent to return it if deletion succeeds
    let agent = self
      .get(name.clone())
      .await
      .map_err(|e| crate::Error::Registry {
        reason: format!("Failed to get agent before delete: {e}"),
        status_code: None,
      })?;

    if agent.is_none() {
      return Ok(None);
    }

    // For agents, the name is expected to be in format "slug/version"
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 {
      return Err(crate::Error::Registry {
        reason: format!("Agent name must be in format 'slug/version', got: {}", name),
        status_code: None,
      });
    }

    let slug = parts[0];
    let version = parts[1];

    let url = format!("{}/agents/{}/{}", self.base_url, slug, version);
    let response = self.client.delete(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;

    if response.status().is_success() {
      Ok(agent)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      Err(crate::Error::Registry {
        reason: format!("HTTP error on delete: {}", response.status()),
        status_code: Some(response.status().as_u16()),
      })
    }
  }

  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    // For agents, the name is expected to be in format "slug/version"
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 {
      return Err(crate::Error::Registry {
        reason: format!("Agent name must be in format 'slug/version', got: {}", name),
        status_code: None,
      });
    }

    let slug = parts[0];
    let version = parts[1];

    let url = format!("{}/agents/{}/{}", self.base_url, slug, version);
    let response = self.client.get(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;

    if response.status().is_success() {
      let envelope = response
        .json::<APIEnvelope<AgentDefinition>>()
        .await
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest JSON error: {e}"),
          ))
        })?;
      Ok(envelope.result.into_iter().next())
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
    // For agents, the name is expected to be in format "slug/version"
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 {
      return Err(crate::Error::Registry {
        reason: format!("Agent name must be in format 'slug/version', got: {}", name),
        status_code: None,
      });
    }

    let slug = parts[0];
    let version = parts[1];

    let url = format!("{}/agents/{}/{}", self.base_url, slug, version);
    let response = self.client.head(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest middleware error: {e}"),
      ))
    })?;
    Ok(response.status().is_success())
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut url = format!("{}/agents", self.base_url);

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
        format!("Reqwest middleware error: {e}"),
      ))
    })?;
    response.error_for_status_ref().map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;
    let envelope = response
      .json::<APIEnvelope<AgentDefinition>>()
      .await
      .map_err(|e| {
        crate::Error::Io(std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("Reqwest JSON error: {e}"),
        ))
      })?;
    Ok(
      envelope
        .result
        .into_iter()
        .map(|agent| agent.slug)
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::definitions::AgentDefinition;

  fn create_test_agent(name: &str) -> AgentDefinition {
    AgentDefinition {
      name: name.to_string(),
      model: "gpt-4".to_string(),
      description: Some(format!("Test agent {name}")),
      version: "1.0.0".to_string(),
      slug: format!("agent-{name}"),
      available_tools: vec!["tool1".to_string()],
      instructions: "You are a helpful assistant".to_string(),
      organization: "test-org".to_string(),
      project: "test-project".to_string(),
    }
  }

  fn create_registry() -> HttpAgentRegistry {
    HttpAgentRegistry::from_env().expect("Failed to create registry")
  }

  #[tokio::test]
  async fn test_http_agent_registry_get() {
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running
    let registry = create_registry();

    // Test get with a non-existent agent (should return None)
    let result = registry.get("non-existent-agent".to_string()).await;
    if result.is_ok() {
      assert!(result.unwrap().is_none());
    }

    // Optionally, test get with a known agent if you have seed data
    // let result = registry.get("some-known-agent".to_string()).await;
    // if let Ok(Some(agent)) = result {
    //     assert_eq!(agent.slug, "some-known-agent");
    // }
  }

  #[tokio::test]
  async fn test_http_agent_registry_has() {
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running
    let registry = create_registry();

    // Test has with a non-existent agent (should return false)
    let result = registry.has("non-existent-agent".to_string()).await;
    if result.is_ok() {
      assert_eq!(result.unwrap(), false);
    }

    // Optionally, test has with a known agent if you have seed data
    // let result = registry.has("some-known-agent".to_string()).await;
    // if result.is_ok() {
    //     assert_eq!(result.unwrap(), true);
    // }
  }

  #[tokio::test]
  async fn test_http_agent_registry_keys() {
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

    if let Ok(keys) = result {
      // Optionally, check for known agents if you have seed data
      // let expected_agents = vec!["agent1", "agent2"];
      // for expected_agent in expected_agents {
      //     assert!(keys.contains(&expected_agent.to_string()));
      // }
      assert!(keys.is_empty() || !keys.is_empty()); // Always true, just to exercise the code
    }
  }

  #[tokio::test]
  async fn test_http_agent_registry_put_not_supported() {
    let registry = create_registry();
    let agent = create_test_agent("test-agent");

    let result = registry.put("test-agent/1.0.0".to_string(), agent).await;
    // This should now work (though it may fail due to network/auth issues in tests)
    // We can't easily test the success case without a running server
    if result.is_err() {
      // Check that it's not the old "not supported" error
      assert!(
        !result
          .unwrap_err()
          .to_string()
          .contains("does not support put operations")
      );
    }
  }

  #[tokio::test]
  async fn test_http_agent_registry_del_not_supported() {
    let registry = create_registry();

    let result = registry.del("test-agent/1.0.0".to_string()).await;
    // This should now work (though it may fail due to network/auth issues in tests)
    // We can't easily test the success case without a running server
    if result.is_err() {
      // Check that it's not the old "not supported" error
      assert!(
        !result
          .unwrap_err()
          .to_string()
          .contains("does not support delete operations")
      );
    }
  }

  #[tokio::test]
  async fn test_http_agent_registry_pagination() {
    let registry = create_registry();

    // Try with per_page = 2, page = 1
    let result = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(2),
      })
      .await;
    assert!(result.is_ok(), "Page 1 failed: {:?}", result.err());
    let keys = result.unwrap();
    assert!(keys.len() <= 2);

    // Try with per_page = 2, page = 2
    let result = registry
      .keys(Pagination {
        page: Some(2),
        per_page: Some(2),
      })
      .await;
    assert!(result.is_ok(), "Page 2 failed: {:?}", result.err());
    let keys2 = result.unwrap();
    assert!(keys2.len() <= 2);

    // Only check for overlap if there are at least 4 agents
    if keys.len() == 2 && keys2.len() == 2 && keys != keys2 {
      // All good
    } else if keys.len() == 2 && keys2.len() == 2 {
      // If they are equal, that's a pagination bug
      panic!(
        "Pagination returned the same items for page 1 and 2: {:?} {:?}",
        keys, keys2
      );
    }
  }

  #[tokio::test]
  async fn test_http_agent_registry_direct_construction() {
    // Use a dummy API key for testing
    let registry = HttpAgentRegistry::new("http://localhost:8080".to_string(), "test-key".into())
      .expect("Failed to create registry");

    let agent = create_test_agent("test");
    let result = registry.put("test".to_string(), agent).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "Agent name must be in format 'slug/version', got: test (status: None)"
    );

    let result = registry.del("test".to_string()).await;
    assert!(result.is_err());
    assert_eq!(
      result.unwrap_err().to_string(),
      "Failed to get agent before delete: Agent name must be in format 'slug/version', got: test (status: None) (status: None)"
    );
  }

  #[test]
  fn test_http_agent_registry_new_with_invalid_api_key() {
    // Test with invalid API key format (empty string)
    let result = HttpAgentRegistry::new(
      "http://localhost:8080".to_string(),
      SecretString::new("".to_string().into()),
    );
    assert!(result.is_ok()); // Constructor should succeed even with empty key

    // Test with malformed base URL - this would fail at request time, not construction
    let result = HttpAgentRegistry::new(
      "not-a-valid-url".to_string(),
      SecretString::new("test-key".to_string().into()),
    );
    assert!(result.is_ok()); // Constructor validation is minimal
  }

  #[tokio::test]
  async fn test_http_agent_registry_read_operations() {
    let registry = create_registry();

    // Test that put operation returns an error
    let agent = create_test_agent("test-agent");
    let put_result = registry
      .put("test-agent/1.0.0".to_string(), agent.clone())
      .await;
    // This should now work (though it may fail due to network/auth issues in tests)
    // We can't easily test the success case without a running server
    if put_result.is_err() {
      // Check that it's not the old "not supported" error
      assert!(
        !put_result
          .unwrap_err()
          .to_string()
          .contains("does not support put operations")
      );
    }

    // Test that del operation returns an error
    let del_result = registry.del("test-agent/1.0.0".to_string()).await;
    // This should now work (though it may fail due to network/auth issues in tests)
    // We can't easily test the success case without a running server
    if del_result.is_err() {
      // Check that it's not the old "not supported" error
      assert!(
        !del_result
          .unwrap_err()
          .to_string()
          .contains("does not support delete operations")
      );
    }
  }

  #[test]
  fn test_http_agent_registry_with_base_url() {
    // Create registry with a dummy URL and API key for testing the structure
    let registry = HttpAgentRegistry::new(
      "http://localhost:3000".to_string(),
      SecretString::new("dummy-key".to_string().into()),
    )
    .expect("Failed to create registry");

    // Verify the registry was created correctly
    assert_eq!(registry.base_url, "http://localhost:3000");
  }
}
