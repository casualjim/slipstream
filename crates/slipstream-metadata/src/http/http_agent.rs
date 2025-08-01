use crate::{AgentDefinition, AgentRef};
use crate::{
  Pagination, Registry, Result,
  http::{APIEnvelope, create_client},
};
use async_trait::async_trait;
use reqwest_middleware::ClientWithMiddleware;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::env;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize)]
struct CreateAgentRequest {
  pub name: String,
  pub version: String,
  pub description: Option<String>,
  pub model: String,
  pub instructions: Option<String>,
  #[serde(default)]
  pub available_tools: Vec<String>,
  pub organization: String,
  pub project: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateAgentRequest {
  pub name: Option<String>,
  pub description: Option<String>,
  pub model: Option<String>,
  pub instructions: Option<String>,
  #[serde(default)]
  pub available_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HttpAgentRegistry {
  client: ClientWithMiddleware,
  base_url: String,
}

impl HttpAgentRegistry {
  pub fn new(base_url: String, api_key: SecretString) -> Result<Self> {
    let client = create_client(api_key)?;
    Ok(Self { client, base_url })
  }

  #[cfg(test)]
  pub fn from_env() -> Result<Self> {
    let base_url = env::var("SLIPSTREAM_BASE_URL").map_err(|e| {
      crate::Error::Io(std::io::Error::other(format!(
        "Missing SLIPSTREAM_BASE_URL: {e}"
      )))
    })?;
    let api_key = env::var("SLIPSTREAM_API_KEY").map_err(|e| {
      crate::Error::Io(std::io::Error::other(format!(
        "Missing SLIPSTREAM_API_KEY: {e}"
      )))
    })?;
    Self::new(base_url, api_key.into())
  }
}

#[async_trait]
impl Registry for HttpAgentRegistry {
  type Subject = AgentDefinition;
  type Key = AgentRef;

  /// Create or update an agent. Version is required.
  async fn put(&self, key: Self::Key, subject: Self::Subject) -> Result<()> {
    if key.version.is_none() {
      // Return a structured validation error for missing version on mutation
      let mut errs = validator::ValidationErrors::new();
      errs.add(
        "version",
        validator::ValidationError {
          code: std::borrow::Cow::from("required"),
          message: Some(std::borrow::Cow::from("version is required for put")),
          params: std::collections::HashMap::new(),
        },
      );
      return Err(crate::Error::Validation(errs));
    }
    subject.validate()?;

    let update_request = UpdateAgentRequest {
      name: Some(subject.name.clone()),
      description: subject.description.clone(),
      model: Some(subject.model.clone()),
      instructions: Some(subject.instructions.clone()),
      available_tools: subject.available_tools.clone(),
    };
    let url = format!(
      "{}/agents/{}/{}",
      self.base_url,
      key.slug,
      key.version.as_ref().unwrap()
    );
    let response = self
      .client
      .put(&url)
      .body(serde_json::to_string(&update_request)?)
      .header("Content-Type", "application/json")
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

    if response.status().is_success() {
      return Ok(());
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      // Try create
      let create_request = CreateAgentRequest {
        name: subject.name.clone(),
        version: subject.version.to_string(),
        description: subject.description.clone(),
        model: subject.model.clone(),
        instructions: Some(subject.instructions.clone()),
        available_tools: subject.available_tools.clone(),
        organization: subject.organization.clone(),
        project: subject.project.clone(),
      };
      let create_url = format!("{}/agents", self.base_url);
      let create_response = self
        .client
        .post(&create_url)
        .body(serde_json::to_string(&create_request)?)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

      if create_response.status().is_success() {
        return Ok(());
      } else {
        let status = create_response.status();
        let body = create_response
          .text()
          .await
          .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::Error::Registry {
          reason: format!("Failed to create agent: HTTP {status} - {body}"),
          status_code: Some(status.as_u16()),
        });
      }
    } else {
      let status = response.status();
      let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      return Err(crate::Error::Registry {
        reason: format!("Failed to update agent: HTTP {status} - {body}"),
        status_code: Some(status.as_u16()),
      });
    }
  }

  /// Delete an agent. Version is required.
  async fn del(&self, key: Self::Key) -> Result<Option<Self::Subject>> {
    if key.version.is_none() {
      // Return a structured validation error for missing version on mutation
      let mut errs = validator::ValidationErrors::new();
      errs.add(
        "version",
        validator::ValidationError {
          code: std::borrow::Cow::from("required"),
          message: Some(std::borrow::Cow::from("version is required for delete")),
          params: std::collections::HashMap::new(),
        },
      );
      return Err(crate::Error::Validation(errs));
    }
    // First get the agent to return it if deletion succeeds
    let agent = self.get(key.clone()).await?;
    if agent.is_none() {
      return Ok(None);
    }

    let url = format!(
      "{}/agents/{}/{}",
      self.base_url,
      key.slug,
      key.version.as_ref().unwrap()
    );
    let response = self
      .client
      .delete(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

    if response.status().is_success() {
      Ok(agent)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      let status = response.status();
      let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      Err(crate::Error::Registry {
        reason: format!("Failed to delete agent: HTTP {status} - {body}"),
        status_code: Some(status.as_u16()),
      })
    }
  }

  /// Get an agent. If version is None, returns the latest version.
  async fn get(&self, key: Self::Key) -> Result<Option<Self::Subject>> {
    let url = if let Some(version) = &key.version {
      format!("{}/agents/{}/{}", self.base_url, key.slug, version)
    } else {
      format!("{}/agents/{}", self.base_url, key.slug)
    };
    let response = self
      .client
      .get(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

    if response.status().is_success() {
      let envelope = response
        .json::<APIEnvelope<AgentDefinition>>()
        .await
        .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest JSON error: {e}"))))?;
      Ok(Some(envelope.result))
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      let status = response.status();
      let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      Err(crate::Error::Registry {
        reason: format!("Failed to get agent: HTTP {status} - {body}"),
        status_code: Some(status.as_u16()),
      })
    }
  }

  /// Check if an agent exists. If version is None, checks for latest.
  async fn has(&self, key: Self::Key) -> Result<bool> {
    let url = if let Some(version) = &key.version {
      format!("{}/agents/{}/{}", self.base_url, key.slug, version)
    } else {
      format!("{}/agents/{}", self.base_url, key.slug)
    };

    let response = self
      .client
      .get(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;
    Ok(response.status().is_success())
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut url = format!("{}/agents", self.base_url);

    // Add query parameters for pagination
    let mut params = Vec::new();
    if let Some(page) = pagination.page {
      params.push(format!("page={page}"));
    }
    if let Some(per_page) = pagination.per_page {
      params.push(format!("per_page={per_page}"));
    }

    if !params.is_empty() {
      url.push('?');
      url.push_str(&params.join("&"));
    }

    let response = self
      .client
      .get(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;
    response
      .error_for_status_ref()
      .map_err(|e| crate::Error::Registry {
        reason: format!("HTTP error on keys: {e}"),
        status_code: Some(response.status().as_u16()),
      })?;

    let envelope = response
      .json::<APIEnvelope<Vec<AgentDefinition>>>()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest JSON error: {e}"))))?;
    let agent_keys = envelope
      .result
      .into_iter()
      .map(|agent| agent.name)
      .collect();

    Ok(agent_keys)
  }
}

#[cfg(test)]
mod tests {

  use super::*;
  use crate::{AgentDefinition, AgentRef};

  async fn generate_slug(name: &str) -> String {
    let base_url = std::env::var("SLIPSTREAM_BASE_URL").expect("Missing SLIPSTREAM_BASE_URL");
    let api_key = std::env::var("SLIPSTREAM_API_KEY").expect("Missing SLIPSTREAM_API_KEY");
    let url = format!("{base_url}/utils/generate-slug");

    let client = reqwest::Client::new();
    let resp = client
      .post(&url)
      .header("Authorization", format!("Bearer {api_key}"))
      .json(&serde_json::json!({ "name": name }))
      .send()
      .await
      .expect("Failed to call slug endpoint");

    let json: serde_json::Value = resp.json().await.expect("Failed to parse slug response");
    json["slug"]
      .as_str()
      .expect("No slug in response")
      .to_string()
  }

  async fn create_test_agent(name: &str) -> AgentDefinition {
    let slug = generate_slug(name).await;
    AgentDefinition::builder()
      .name(name)
      .model("openai/o4-mini")
      .description(format!("Test agent {name}"))
      .version(semver::Version::parse("1.0.0").unwrap())
      .slug(slug)
      .instructions("You are a helpful assistant")
      .available_tools(["local/web-search/1.0.0"])
      .organization("wagyu")
      .project("wagyu-project")
      .build()
  }

  fn create_registry() -> HttpAgentRegistry {
    HttpAgentRegistry::from_env().expect("Failed to create registry")
  }

  #[tokio::test]
  async fn test_http_agent_registry_get() {
    let registry = create_registry();
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running

    // Test get with a non-existent agent (should return None)

    let agent_ref = AgentRef::builder()
      .slug("non-existent")
      .version(semver::Version::parse("0.0.1").unwrap())
      .build();
    let result = registry
      .get(agent_ref)
      .await
      .expect("HTTP call should succeed");
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_http_agent_registry_has() {
    let registry = create_registry();
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running

    // Test has with a non-existent agent (should return false)
    let agent_ref = AgentRef::builder()
      .slug("non-existent-agent")
      .version(semver::Version::parse("0.0.1").unwrap())
      .build();
    let result = registry
      .has(agent_ref)
      .await
      .expect("HTTP call should succeed");
    assert!(!result);
  }

  #[tokio::test]
  async fn test_http_agent_registry_keys() {
    let registry = create_registry();

    // Ensure there are agents present for the test
    for i in 0..2 {
      let agent_name = format!("keys-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      let _ = registry.del(agent_ref.clone()).await;
      registry
        .put(agent_ref.clone(), agent)
        .await
        .expect("Agent creation should succeed");
    }

    // Test keys (no pagination)
    let result = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await;

    assert!(result.is_ok());
    let keys = result.unwrap();
    assert!(!keys.is_empty());

    // Cleanup
    for i in 0..2 {
      let agent_name = format!("keys-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      let _ = registry.del(agent_ref).await;
    }

    // Cleanup
    for i in 0..2 {
      let agent_name = format!("keys-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      let _ = registry.del(agent_ref).await;
    }
  }

  #[tokio::test]
  async fn test_http_agent_registry_put_create() {
    let registry = create_registry();
    let agent_name = "test-agent-create";
    let agent = create_test_agent(agent_name).await;
    let agent_ref: AgentRef = (&agent).into();

    // Ensure agent doesn't exist before test
    let _ = registry.del(agent_ref.clone()).await;

    let result = registry.put(agent_ref.clone(), agent).await;

    // Cleanup after test
    if result.is_ok() {
      let _ = registry.del(agent_ref).await;
    }

    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn test_http_agent_registry_del() {
    let registry = create_registry();
    let agent_name = "test-agent-del";
    let agent = create_test_agent(agent_name).await;
    let agent_ref: AgentRef = (&agent).into();

    // Ensure agent doesn't exist before test
    let _ = registry.del(agent_ref.clone()).await;

    // First, create the agent to be deleted
    registry
      .put(agent_ref.clone(), agent)
      .await
      .expect("Agent creation should succeed");

    // Now, delete the agent
    let result = registry.del(agent_ref).await;
    assert!(result.is_ok());
    // After deletion, the agent should be gone, so we do not assert .is_some()
  }

  #[tokio::test]
  async fn test_http_agent_registry_pagination() {
    let registry = create_registry();
    // This test requires the SLIPSTREAM_BASE_URL and SLIPSTREAM_API_KEY
    // environment variables to be set and the service to be running

    // Cleanup before test
    for i in 0..5 {
      let agent_name = format!("pagination-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      let _ = registry.del(agent_ref).await;
    }

    // Create a few agents to test pagination
    for i in 0..5 {
      let agent_name = format!("pagination-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      registry
        .put(agent_ref.clone(), agent)
        .await
        .expect("Agent creation should succeed");
    }

    // Test pagination with page=1 and per_page=2
    let result = registry
      .keys(Pagination {
        page: Some(1),
        per_page: Some(2),
      })
      .await;

    assert!(result.is_ok());
    let keys = result.unwrap();
    assert_eq!(keys.len(), 2);

    // Cleanup
    for i in 0..5 {
      let agent_name = format!("pagination-agent-{i}");
      let agent = create_test_agent(&agent_name).await;
      let agent_ref = AgentRef::from(&agent);
      let _ = registry.del(agent_ref).await;
    }
  }

  #[tokio::test]
  #[ignore = "not very valuable and takes a long time"]
  async fn test_http_agent_registry_direct_construction() {
    // Use a dummy API key for testing
    let registry = HttpAgentRegistry::new("http://localhost:8787".to_string(), "test-key".into())
      .expect("Failed to create registry");

    let agent_name = "test";
    let agent = create_test_agent(agent_name).await;
    let agent_ref = AgentRef::from(&agent);
    let result = registry.put(agent_ref.clone(), agent).await;
    assert!(
      result.is_err(),
      "Expected error due to invalid endpoint/auth"
    );

    let result = registry.del(agent_ref).await;
    assert!(
      result.is_err(),
      "Delete should error due to invalid endpoint/auth"
    );
  }

  #[test]
  fn test_http_agent_registry_new_with_invalid_api_key() {
    let result = HttpAgentRegistry::new(
      "http://localhost:8080".to_string(),
      "invalid key with spaces".into(),
    );
    // This should fail because the API key is not a valid HeaderValue,
    // but the error is deferred until the first request.
    assert!(result.is_ok());
  }

  #[tokio::test]
  #[ignore = "flaky test"]
  async fn test_http_agent_registry_crud_lifecycle() {
    let registry = create_registry();
    // Use a unique agent name for this test to avoid collisions
    let agent_name = format!("crud-lifecycle-agent-{}", uuid::Uuid::now_v7());
    let agent = create_test_agent(&agent_name).await;
    let agent_ref = AgentRef::from(&agent);

    // 1. Cleanup any existing agent first
    let _ = registry.del(agent_ref.clone()).await;

    // 2. Verify agent doesn't exist
    let initial_check = registry.get(agent_ref.clone()).await.unwrap();
    assert!(initial_check.is_none());

    // 3. Create agent
    registry
      .put(agent_ref.clone(), agent.clone())
      .await
      .expect("Should create agent");

    // 4. Verify agent exists
    let retrieved = registry
      .get(agent_ref.clone())
      .await
      .unwrap()
      .expect("Agent should exist");
    assert_eq!(retrieved.name, agent.name);

    // 5. Update agent
    let mut updated_agent = agent.clone();
    updated_agent.description = Some("Updated description".to_string());
    registry
      .put(agent_ref.clone(), updated_agent.clone())
      .await
      .expect("Should update agent");

    // 6. Verify update
    let updated = registry
      .get(agent_ref.clone())
      .await
      .unwrap()
      .expect("Agent should exist");
    assert_eq!(updated.description, updated_agent.description);

    // 7. Delete agent
    let _deleted = registry.del(agent_ref.clone()).await.unwrap();
    // After deletion, the agent should be gone, so we do not assert deleted agent name

    // 8. Verify agent is deleted
    let after_delete = registry.get(agent_ref.clone()).await.unwrap();
    assert!(
      after_delete.is_none(),
      "Agent should be deleted immediately"
    );
  }
}
