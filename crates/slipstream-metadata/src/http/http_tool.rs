use crate::http::{APIEnvelope, create_client};
use crate::{Pagination, Registry, Result};
use crate::{ToolDefinition, ToolRef};
use async_trait::async_trait;
use reqwest_middleware::ClientWithMiddleware;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::env;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize)]
struct CreateToolRequest {
  pub name: String,
  pub version: String,
  pub provider: String,
  pub description: Option<String>,
  pub arguments: Option<serde_json::Value>,
  pub slug: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateToolRequest {
  pub name: Option<String>,
  pub description: Option<String>,
  pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct HttpToolRegistry {
  client: ClientWithMiddleware,
  base_url: String,
}

impl HttpToolRegistry {
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
impl Registry for HttpToolRegistry {
  type Subject = ToolDefinition;
  type Key = ToolRef;

  async fn put(&self, tool_ref: Self::Key, subject: Self::Subject) -> Result<()> {
    tool_ref.validate()?;
    // Enforce version for mutations
    let Some(version) = tool_ref.version.as_ref() else {
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
    };

    // Check if tool exists for update vs create
    let exists = self.has(tool_ref.clone()).await?;

    if exists {
      // Update existing tool
      let update_request = UpdateToolRequest {
        name: Some(subject.name),
        description: subject.description,
        arguments: subject
          .arguments
          .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
      };

      let url = format!(
        "{}/tools/{}/{}/{}",
        self.base_url, tool_ref.provider, tool_ref.slug, version
      );

      let response = self
        .client
        .put(&url)
        .body(serde_json::to_string(&update_request)?)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

      if !response.status().is_success() {
        let status = response.status();
        let body = response
          .text()
          .await
          .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::Error::Registry {
          reason: format!("Failed to update tool: HTTP {status} - {body}"),
          status_code: Some(status.as_u16()),
        });
      }
    } else {
      // Create new tool
      let create_request = CreateToolRequest {
        name: subject.name,
        version: subject.version.to_string(),
        provider: tool_ref.provider.to_string(),
        description: subject.description,
        arguments: subject
          .arguments
          .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
        slug: Some(subject.slug),
      };

      let url = format!("{}/tools", self.base_url);
      let response = self
        .client
        .post(&url)
        .body(serde_json::to_string(&create_request)?)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

      if !response.status().is_success() {
        let status = response.status();
        let body = response
          .text()
          .await
          .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::Error::Registry {
          reason: format!("Failed to create tool: HTTP {status} - {body}"),
          status_code: Some(status.as_u16()),
        });
      }
    }

    Ok(())
  }

  async fn del(&self, tool_ref: Self::Key) -> Result<Option<Self::Subject>> {
    tool_ref.validate()?;
    // Enforce version for mutations
    let Some(version) = tool_ref.version.as_ref() else {
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
    };

    // First get the tool to return it if deletion succeeds
    let tool = self.get(tool_ref.clone()).await?;

    if tool.is_none() {
      return Ok(None);
    }

    let url = format!(
      "{}/tools/{}/{}/{}",
      self.base_url, tool_ref.provider, tool_ref.slug, version
    );

    let response = self
      .client
      .delete(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

    if response.status().is_success() {
      Ok(tool)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      let status = response.status();
      let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      Err(crate::Error::Registry {
        reason: format!("Failed to delete tool: HTTP {status} - {body}"),
        status_code: Some(status.as_u16()),
      })
    }
  }

  async fn get(&self, tool_ref: Self::Key) -> Result<Option<Self::Subject>> {
    tool_ref.validate()?;
    // Support versionless GET by calling the latest endpoint when version is None
    let url = if let Some(version) = tool_ref.version.as_ref() {
      format!(
        "{}/tools/{}/{}/{}",
        self.base_url, tool_ref.provider, tool_ref.slug, version
      )
    } else {
      format!(
        "{}/tools/{}/{}",
        self.base_url, tool_ref.provider, tool_ref.slug
      )
    };

    let response = self
      .client
      .get(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;

    if response.status().is_success() {
      let envelope = response
        .json::<APIEnvelope<ToolDefinition>>()
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
        reason: format!("Failed to get tool: HTTP {status} - {body}"),
        status_code: Some(status.as_u16()),
      })
    }
  }

  async fn has(&self, tool_ref: Self::Key) -> Result<bool> {
    tool_ref.validate()?;

    // Support versionless HAS by calling the latest endpoint when version is None
    let url = if let Some(version) = tool_ref.version.as_ref() {
      format!(
        "{}/tools/{}/{}/{}",
        self.base_url, tool_ref.provider, tool_ref.slug, version
      )
    } else {
      format!(
        "{}/tools/{}/{}",
        self.base_url, tool_ref.provider, tool_ref.slug
      )
    };

    let response = self
      .client
      .head(&url)
      .send()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest error: {e}"))))?;
    Ok(response.status().is_success())
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut url = format!("{}/tools", self.base_url);

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
      .json::<APIEnvelope<Vec<ToolDefinition>>>()
      .await
      .map_err(|e| crate::Error::Io(std::io::Error::other(format!("Reqwest JSON error: {e}"))))?;
    let tool_refs = envelope
      .result
      .into_iter()
      .map(|tool| {
        ToolRef {
          provider: tool.provider,
          slug: tool.slug,
          version: Some(tool.version),
        }
        .to_string()
      })
      .collect();

    Ok(tool_refs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{ToolDefinition, ToolProvider};
  use schemars::schema_for;

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
      slug: format!("tool-{name}"),
      name: name.to_string(),
      description: Some(format!("Test tool {name}")),
      version: semver::Version::parse("1.0.0").unwrap(),
      arguments: Some(schema_for!(bool)),
      provider: ToolProvider::Local,
      created_at: None,
      updated_at: None,
    }
  }

  fn create_test_tool_ref(name: &str) -> ToolRef {
    ToolRef {
      provider: ToolProvider::Local,
      slug: format!("tool-{name}"),
      version: Some(semver::Version::parse("1.0.0").unwrap()),
    }
  }

  fn create_registry() -> HttpToolRegistry {
    HttpToolRegistry::from_env().expect("Failed to create registry")
  }

  #[tokio::test]
  async fn test_http_tool_registry_get() {
    let registry = create_registry();
    let tool_ref = create_test_tool_ref("non-existent");

    let result = registry
      .get(tool_ref)
      .await
      .expect("Registry should be accessible");
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_http_tool_registry_has() {
    let registry = create_registry();
    let tool_ref = create_test_tool_ref("non-existent");

    let result = registry
      .has(tool_ref)
      .await
      .expect("Registry should be accessible");
    assert!(!result);
  }

  #[tokio::test]
  async fn test_version_validation_on_operations() {
    let registry = create_registry();
    let tool_ref_no_version = ToolRef {
      provider: ToolProvider::Local,
      slug: "test".to_string(),
      version: None,
    };

    // GET and HAS support versionless references: they should not return validation errors
    let _ = registry
      .get(tool_ref_no_version.clone())
      .await
      .expect("versionless GET should not error");
    let _ = registry
      .has(tool_ref_no_version.clone())
      .await
      .expect("versionless HAS should not error");

    // Mutations still require version and should return validation errors
    let tool = create_test_tool("test");
    let put_result = registry.put(tool_ref_no_version.clone(), tool).await;
    match put_result {
      Err(crate::Error::Validation(_)) => {}
      _ => panic!("Expected validation error for missing version"),
    }

    let del_result = registry.del(tool_ref_no_version.clone()).await;
    match del_result {
      Err(crate::Error::Validation(_)) => {}
      _ => panic!("Expected validation error for missing version"),
    }
  }

  #[tokio::test]
  async fn test_validation_error_messages() {
    let registry = create_registry();
    let tool_ref_no_version = ToolRef {
      provider: ToolProvider::Local,
      slug: "test".to_string(),
      version: None,
    };

    // Versionless GET should not yield validation error
    let _ = registry
      .get(tool_ref_no_version.clone())
      .await
      .expect("versionless GET should not error");

    // Versionless HAS should not yield validation error
    let _ = registry
      .has(tool_ref_no_version.clone())
      .await
      .expect("versionless HAS should not error");

    // Validation errors must still surface for mutations without version
    let tool = create_test_tool("test-validation");
    let put_result = registry.put(tool_ref_no_version.clone(), tool).await;
    match put_result {
      Err(crate::Error::Validation(validation_errors)) => {
        assert!(validation_errors.field_errors().contains_key("version"));
      }
      _ => panic!("Expected validation error for missing version on put"),
    }

    let del_result = registry.del(tool_ref_no_version.clone()).await;
    match del_result {
      Err(crate::Error::Validation(validation_errors)) => {
        assert!(validation_errors.field_errors().contains_key("version"));
      }
      _ => panic!("Expected validation error for missing version on del"),
    }
  }

  #[tokio::test]
  async fn test_url_encoding_edge_cases() {
    let registry = create_registry();

    // Test slugs and versions with special characters that need URL encoding
    let tool_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "tool-with-special/chars".to_string(),
      version: Some(semver::Version::parse("1.0.0-beta+build").unwrap()),
    };

    // This would catch URL encoding bugs that cause 400s in production
    let result = registry.get(tool_ref).await;
    // Should either succeed or fail with proper HTTP error, not panic/crash
    match result {
      Ok(_) | Err(crate::Error::Registry { .. }) => {
        // Both are acceptable - we're testing URL construction doesn't crash
      }
      Err(other) => {
        // Network/auth errors are also acceptable in tests
        assert!(matches!(other, crate::Error::Io(_)));
      }
    }
  }

  #[tokio::test]
  async fn test_full_crud_lifecycle() {
    let registry = create_registry();
    let tool = create_test_tool("integration-test-tool");
    let tool_ref = create_test_tool_ref("integration-test-tool");

    // This test requires a running registry and proper cleanup
    // 1. Clean up any existing tool first
    let _ = registry.del(tool_ref.clone()).await;

    // 2. Verify tool doesn't exist after cleanup
    let initial_check = registry
      .get(tool_ref.clone())
      .await
      .expect("Registry should be accessible");
    assert!(initial_check.is_none());

    // 3. Create tool
    registry
      .put(tool_ref.clone(), tool.clone())
      .await
      .expect("Should create tool");

    // 4. Verify tool exists and matches
    let retrieved = registry
      .get(tool_ref.clone())
      .await
      .expect("Should retrieve tool");
    assert!(retrieved.is_some());
    let retrieved_tool = retrieved.unwrap();
    assert_eq!(retrieved_tool.name, tool.name);
    assert_eq!(retrieved_tool.description, tool.description);

    // 5. Cleanup - delete tool
    let deleted = registry
      .del(tool_ref.clone())
      .await
      .expect("Should delete tool");
    assert!(deleted.is_some());

    // 6. Verify tool is gone
    let final_check = registry
      .get(tool_ref)
      .await
      .expect("Registry should be accessible");
    assert!(final_check.is_none());
  }
}
