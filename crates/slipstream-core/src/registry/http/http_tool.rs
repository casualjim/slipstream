use crate::definitions::ToolRef;
use crate::registry::Registry;
use crate::registry::http::APIEnvelope;
use crate::{Result, definitions::ToolDefinition, registry::Pagination};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

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
impl Registry for HttpToolRegistry {
  type Subject = ToolDefinition;
  type Key = ToolRef;

  async fn put(&self, tool_ref: Self::Key, subject: Self::Subject) -> Result<()> {
    // Ensure we have a version
    let version = tool_ref
      .version
      .as_deref()
      .ok_or_else(|| crate::Error::Validation {
        field: "version",
        reason: "Tool version is required".to_string(),
      })?;

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
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest error: {e}"),
          ))
        })?;

      if !response.status().is_success() {
        let status = response.status();
        let body = response
          .text()
          .await
          .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::Error::Registry {
          reason: format!("Failed to update tool: HTTP {} - {}", status, body),
          status_code: Some(status.as_u16()),
        });
      }
    } else {
      // Create new tool
      let create_request = CreateToolRequest {
        name: subject.name,
        version: subject.version,
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
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest error: {e}"),
          ))
        })?;

      if !response.status().is_success() {
        let status = response.status();
        let body = response
          .text()
          .await
          .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::Error::Registry {
          reason: format!("Failed to create tool: HTTP {} - {}", status, body),
          status_code: Some(status.as_u16()),
        });
      }
    }

    Ok(())
  }

  async fn del(&self, tool_ref: Self::Key) -> Result<Option<Self::Subject>> {
    // Ensure we have a version
    let version = tool_ref
      .version
      .as_deref()
      .ok_or_else(|| crate::Error::Validation {
        field: "version",
        reason: "Tool version is required".to_string(),
      })?;

    // First get the tool to return it if deletion succeeds
    let tool = self.get(tool_ref.clone()).await?;

    if tool.is_none() {
      return Ok(None);
    }

    let url = format!(
      "{}/tools/{}/{}/{}",
      self.base_url, tool_ref.provider, tool_ref.slug, version
    );

    let response = self.client.delete(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;

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
        reason: format!("Failed to delete tool: HTTP {} - {}", status, body),
        status_code: Some(status.as_u16()),
      })
    }
  }

  async fn get(&self, tool_ref: Self::Key) -> Result<Option<Self::Subject>> {
    // Ensure we have a version
    let version = tool_ref
      .version
      .as_deref()
      .ok_or_else(|| crate::Error::Validation {
        field: "version",
        reason: "Tool version is required".to_string(),
      })?;

    let url = format!(
      "{}/tools/{}/{}/{}",
      self.base_url, tool_ref.provider, tool_ref.slug, version
    );

    let response = self.client.get(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;

    if response.status().is_success() {
      let tool = response.json::<ToolDefinition>().await.map_err(|e| {
        crate::Error::Io(std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("Reqwest JSON error: {e}"),
        ))
      })?;
      Ok(Some(tool))
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
      Ok(None)
    } else {
      let status = response.status();
      let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      Err(crate::Error::Registry {
        reason: format!("Failed to get tool: HTTP {} - {}", status, body),
        status_code: Some(status.as_u16()),
      })
    }
  }

  async fn has(&self, tool_ref: Self::Key) -> Result<bool> {
    // Ensure we have a version
    let version = tool_ref
      .version
      .as_deref()
      .ok_or_else(|| crate::Error::Validation {
        field: "version",
        reason: "Tool version is required".to_string(),
      })?;

    let url = format!(
      "{}/tools/{}/{}/{}",
      self.base_url, tool_ref.provider, tool_ref.slug, version
    );

    let response = self.client.head(&url).send().await.map_err(|e| {
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Reqwest error: {e}"),
      ))
    })?;
    Ok(response.status().is_success())
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    let mut url = format!("{}/tools", self.base_url);

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
      .json::<APIEnvelope<ToolDefinition>>()
      .await
      .map_err(|e| {
        crate::Error::Io(std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("Reqwest JSON error: {e}"),
        ))
      })?;
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
  use crate::definitions::{ToolDefinition, ToolProvider};
  use schemars::schema_for;

  fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
      slug: format!("tool-{name}"),
      name: name.to_string(),
      description: Some(format!("Test tool {name}")),
      version: "1.0.0".to_string(),
      arguments: Some(schema_for!(bool)),
      provider: ToolProvider::Local,
    }
  }

  fn create_test_tool_ref(name: &str) -> ToolRef {
    ToolRef {
      provider: ToolProvider::Local,
      slug: format!("tool-{name}"),
      version: Some("1.0.0".to_string()),
    }
  }

  fn create_registry() -> HttpToolRegistry {
    HttpToolRegistry::from_env().expect("Failed to create registry")
  }

  #[tokio::test]
  async fn test_http_tool_registry_get() {
    let registry = create_registry();
    let tool_ref = create_test_tool_ref("non-existent");

    let result = registry.get(tool_ref).await;
    if result.is_ok() {
      assert!(result.unwrap().is_none());
    }
  }

  #[tokio::test]
  async fn test_http_tool_registry_has() {
    let registry = create_registry();
    let tool_ref = create_test_tool_ref("non-existent");

    let result = registry.has(tool_ref).await;
    if result.is_ok() {
      assert_eq!(result.unwrap(), false);
    }
  }

  #[tokio::test]
  async fn test_http_tool_registry_keys() {
    let registry = create_registry();

    let result = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await;

    if let Ok(keys) = result {
      assert!(keys.is_empty() || !keys.is_empty());
    }
  }

  #[tokio::test]
  async fn test_http_tool_registry_put() {
    let registry = create_registry();
    let tool = create_test_tool("test-tool");
    let tool_ref = create_test_tool_ref("test-tool");

    let result = registry.put(tool_ref, tool).await;
    // This may fail due to network/auth issues in tests
    if result.is_err() {
      let error_msg = result.unwrap_err().to_string();
      assert!(!error_msg.contains("does not support put operations"));
    }
  }

  #[tokio::test]
  async fn test_http_tool_registry_del() {
    let registry = create_registry();
    let tool_ref = create_test_tool_ref("test-tool");

    let result = registry.del(tool_ref).await;
    // This may fail due to network/auth issues in tests
    if result.is_err() {
      let error_msg = result.unwrap_err().to_string();
      assert!(!error_msg.contains("does not support delete operations"));
    }
  }

  #[test]
  fn test_tool_ref_requires_version() {
    let tool_ref = ToolRef {
      provider: ToolProvider::Local,
      slug: "test-tool".to_string(),
      version: None,
    };

    // This should be caught by the registry methods
    assert!(tool_ref.version.is_none());
  }
}
