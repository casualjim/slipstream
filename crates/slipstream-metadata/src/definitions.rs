use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize};
use typed_builder::TypedBuilder;
use validator::Validate;

use crate::{EnumKind, Error};
use semver::Version;

#[derive(
  Debug,
  Clone,
  PartialEq,
  Eq,
  Hash,
  Serialize,
  Deserialize,
  TypedBuilder,
  Default
)]
pub struct AgentRef {
  #[builder(setter(into))]
  pub slug: String,
  #[builder(default, setter(strip_option, into))]
  pub version: Option<Version>,
}

/// Display as "slug/version" if version is Some, else just "slug"
impl Display for AgentRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(version) = &self.version {
      write!(f, "{}/{}", self.slug, version)
    } else {
      write!(f, "{}", self.slug)
    }
  }
}

/// Accepts "slug" or "slug/version"
impl FromStr for AgentRef {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let parts: Vec<&str> = s.split('/').collect();
    match parts.len() {
      1 => Ok(AgentRef {
        slug: parts[0].to_string(),
        version: None,
      }),
      2 => {
        let v = Version::parse(parts[1]).map_err(Error::InvalidVersion)?;
        Ok(AgentRef {
          slug: parts[0].to_string(),
          version: Some(v),
        })
      }
      _ => Err(Error::InvalidRef {
        kind: "agent",
        reason: "expected format 'slug' or 'slug/version'",
      }),
    }
  }
}

impl From<AgentRef> for String {
  fn from(agent_ref: AgentRef) -> Self {
    agent_ref.to_string()
  }
}

impl TryFrom<String> for AgentRef {
  type Error = Error;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    Self::from_str(&value)
  }
}

fn deserialize_tools<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
  D: Deserializer<'de>,
{
  let opt: Option<Vec<String>> = Option::deserialize(deserializer)?;
  Ok(opt.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize, TypedBuilder, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent name cannot be empty"))]
  pub name: String,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent model cannot be empty"))]
  pub model: String,
  #[builder(default, setter(strip_option, into))]
  pub description: Option<String>,
  #[builder(setter(into))]
  pub version: Version,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent slug cannot be empty"))]
  pub slug: String,
  #[builder(default, setter(transform = |tools: impl IntoIterator<Item = impl Into<String>>| tools.into_iter().map(Into::into).collect()))]
  #[serde(default, deserialize_with = "deserialize_tools")]
  pub available_tools: Vec<String>,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent instructions cannot be empty"))]
  pub instructions: String,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent organization cannot be empty"))]
  pub organization: String,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Agent project cannot be empty"))]
  pub project: String,
}

impl From<AgentDefinition> for AgentRef {
  fn from(value: AgentDefinition) -> Self {
    Self {
      slug: value.slug.clone(),
      version: Some(value.version.clone()),
    }
  }
}

impl From<&AgentDefinition> for AgentRef {
  fn from(value: &AgentDefinition) -> Self {
    Self {
      slug: value.slug.clone(),
      version: Some(value.version.clone()),
    }
  }
}

#[derive(
  Debug,
  Clone,
  PartialEq,
  Eq,
  Hash,
  TypedBuilder,
  serde::Serialize,
  serde::Deserialize
)]
#[serde(try_from = "String", into = "String")]
pub struct ToolRef {
  #[builder(setter(into))]
  pub slug: String,
  #[builder(default, setter(strip_option, into))]
  pub version: Option<Version>,
  pub provider: ToolProvider,
}

// Custom validation for ToolRef to require version presence
impl validator::Validate for ToolRef {
  fn validate(&self) -> Result<(), validator::ValidationErrors> {
    let mut errors = validator::ValidationErrors::new();

    if self.slug.len() < 3 {
      let mut error = validator::ValidationError::new("length");
      error.message = Some("Tool slug cannot be empty".into());
      errors.add("slug", error);
    }

    // Version is now optional, so do not require it
    // If present, Version has already validated semver strictly via serde/builder parsing.
    if let Some(_v) = &self.version {
      // no-op
    }

    if errors.is_empty() {
      Ok(())
    } else {
      Err(errors)
    }
  }
}

impl std::fmt::Display for ToolRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(version) = &self.version {
      write!(
        f,
        "{}/{}/{}",
        AsRef::<str>::as_ref(&self.provider).to_lowercase(),
        self.slug,
        version
      )
    } else {
      write!(
        f,
        "{}/{}",
        AsRef::<str>::as_ref(&self.provider).to_lowercase(),
        self.slug
      )
    }
  }
}

impl FromStr for ToolRef {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let parts: Vec<&str> = s.split('/').collect();

    if parts.len() < 2 {
      return Err(format!(
        "Invalid tool reference format: '{s}'. Expected 'provider/slug' or 'provider/slug/version'"
      ));
    }

    let provider_str = parts[0].to_lowercase();
    let provider = ToolProvider::from_str(&provider_str)
      .map_err(|_| format!("Unknown tool provider: '{provider_str}'"))?;

    let slug = parts[1];
    if slug.is_empty() {
      return Err("Tool slug cannot be empty".to_string());
    }

    let version = if parts.len() > 2 {
      let v = parts[2];
      if v.is_empty() {
        return Err("Tool version cannot be empty when specified".to_string());
      }
      let parsed = Version::parse(v).map_err(|e| format!("Invalid semantic version: {e}"))?;
      Some(parsed)
    } else {
      None
    };

    if parts.len() > 3 {
      return Err(format!(
        "Invalid tool reference format: '{s}'. Too many components"
      ));
    }

    Ok(Self {
      slug: slug.to_string(),
      version,
      provider,
    })
  }
}

impl From<ToolRef> for String {
  fn from(tool_ref: ToolRef) -> Self {
    tool_ref.to_string()
  }
}

impl TryFrom<String> for ToolRef {
  type Error = String;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    Self::from_str(&value)
  }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvider {
  Client,
  Local,
  MCP,
  Restate,
  #[serde(other, skip_serializing)]
  Unknown,
}

impl AsRef<str> for ToolProvider {
  fn as_ref(&self) -> &str {
    match self {
      ToolProvider::Client => "Client",
      ToolProvider::Local => "Local",
      ToolProvider::MCP => "MCP",
      ToolProvider::Restate => "Restate",
      ToolProvider::Unknown => "Unknown",
    }
  }
}

impl FromStr for ToolProvider {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "client" => Ok(ToolProvider::Client),
      "local" => Ok(ToolProvider::Local),
      "mcp" => Ok(ToolProvider::MCP),
      "restate" => Ok(ToolProvider::Restate),
      _ => Err(Error::Unknown(EnumKind::ToolProvider, s.to_string())),
    }
  }
}

impl AsRef<[u8]> for ToolProvider {
  fn as_ref(&self) -> &[u8] {
    match self {
      ToolProvider::Client => b"client",
      ToolProvider::Local => b"local",
      ToolProvider::MCP => b"mcp",
      ToolProvider::Restate => b"restate",
      ToolProvider::Unknown => b"unknown",
    }
  }
}

impl std::fmt::Display for ToolProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ToolProvider::Client => write!(f, "Client"),
      ToolProvider::Local => write!(f, "Local"),
      ToolProvider::MCP => write!(f, "MCP"),
      ToolProvider::Restate => write!(f, "Restate"),
      ToolProvider::Unknown => write!(f, "Unknown"),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, TypedBuilder, Validate)]
pub struct ToolDefinition {
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Tool slug cannot be empty"))]
  pub slug: String,
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Tool name cannot be empty"))]
  pub name: String,
  #[builder(default, setter(strip_option, into))]
  pub description: Option<String>,
  #[builder(setter(into))]
  pub version: Version,
  #[builder(default)]
  pub arguments: Option<schemars::Schema>,
  pub provider: ToolProvider,
  #[serde(skip_serializing_if = "Option::is_none", alias = "createdAt")]
  #[builder(default, setter(strip_option, into))]
  pub created_at: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none", alias = "updatedAt")]
  #[builder(default, setter(strip_option, into))]
  pub updated_at: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
  Chat,
  Completion,
  Embeddings,
  Reranking,
  FunctionCalling,
  StructuredOutput,
  CodeExecution,
  Search,
  Thinking,
  ImageGeneration,
  VideoGeneration,
  Caching,
  Tuning,
  Batch,
  #[serde(other, skip_serializing)]
  Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
  Text,
  Image,
  Video,
  Audio,
  Pdf,
  #[serde(other, skip_serializing)]
  Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiDialect {
  OpenAI,
  Anthropic,
  Deepseek,
  #[serde(other, skip_serializing)]
  Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
  #[serde(rename = "OpenAI")]
  OpenAI,
  #[serde(rename = "Anthropic")]
  Anthropic,
  #[serde(rename = "DeepInfra")]
  DeepInfra,
  #[serde(rename = "Google")]
  Google,
  #[serde(rename = "OpenRouter")]
  OpenRouter,
}

impl AsRef<str> for Provider {
  fn as_ref(&self) -> &str {
    match self {
      Provider::OpenAI => "OpenAI",
      Provider::Anthropic => "Anthropic",
      Provider::DeepInfra => "DeepInfra",
      Provider::Google => "Google",
      Provider::OpenRouter => "OpenRouter",
    }
  }
}

impl std::str::FromStr for Provider {
  type Err = Error;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(match s.to_lowercase().as_str() {
      "openai" => Provider::OpenAI,
      "anthropic" => Provider::Anthropic,
      "deepinfra" => Provider::DeepInfra,
      "google" => Provider::Google,
      "openrouter" => Provider::OpenRouter,
      s => {
        return Err(Error::Unknown(EnumKind::ModelProvider, s.to_string()));
      }
    })
  }
}

impl AsRef<[u8]> for Provider {
  fn as_ref(&self) -> &[u8] {
    match self {
      Provider::OpenAI => b"openai",
      Provider::Anthropic => b"anthropic",
      Provider::DeepInfra => b"deepinfra",
      Provider::Google => b"google",
      Provider::OpenRouter => b"openrouter",
    }
  }
}

impl std::fmt::Display for Provider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Provider::OpenAI => write!(f, "OpenAI"),
      Provider::Anthropic => write!(f, "Anthropic"),
      Provider::DeepInfra => write!(f, "DeepInfra"),
      Provider::Google => write!(f, "Google"),
      Provider::OpenRouter => write!(f, "OpenRouter"),
    }
  }
}

#[derive(
  Debug,
  Clone,
  PartialEq,
  Serialize,
  Deserialize,
  TypedBuilder,
  Validate
)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Model ID cannot be empty"))]
  pub id: String, // uuid7, readonly
  #[builder(setter(into))]
  #[validate(length(min = 1, message = "Model name cannot be empty"))]
  pub name: String, // required
  pub provider: Provider, // required
  #[builder(default, setter(strip_option, into))]
  pub description: Option<String>, // optional
  #[validate(range(min = 1, message = "Context size must be greater than 0"))]
  pub context_size: u32, // required
  #[builder(default, setter(strip_option))]
  pub max_tokens: Option<u32>, // optional
  #[builder(default, setter(strip_option))]
  #[validate(range(
    min = 0.0,
    max = 2.0,
    message = "Temperature must be between 0.0 and 2.0"
  ))]
  pub temperature: Option<f32>, // optional
  #[builder(default, setter(strip_option))]
  #[validate(range(min = 0.0, max = 1.0, message = "Top-p must be between 0.0 and 1.0"))]
  pub top_p: Option<f32>, // optional
  #[builder(default, setter(strip_option))]
  #[validate(range(min = -2.0, max = 2.0, message = "Frequency penalty must be between -2.0 and 2.0"))]
  pub frequency_penalty: Option<f32>, // optional
  #[builder(default, setter(strip_option))]
  #[validate(range(min = -2.0, max = 2.0, message = "Presence penalty must be between -2.0 and 2.0"))]
  pub presence_penalty: Option<f32>, // optional
  #[builder(setter(transform = |caps: impl IntoIterator<Item = ModelCapability>| caps.into_iter().collect()))]
  #[validate(length(min = 1, message = "At least one capability is required"))]
  pub capabilities: Vec<ModelCapability>, // required to have at least 1
  #[builder(setter(transform = |mods: impl IntoIterator<Item = Modality>| mods.into_iter().collect()))]
  #[validate(length(min = 1, message = "At least one input modality is required"))]
  pub input_modalities: Vec<Modality>, // required to have at least 1
  #[builder(setter(transform = |mods: impl IntoIterator<Item = Modality>| mods.into_iter().collect()))]
  #[validate(length(min = 1, message = "At least one output modality is required"))]
  pub output_modalities: Vec<Modality>, // required to have at least 1
  #[builder(default, setter(strip_option, into))]
  pub dialect: Option<ApiDialect>, // optional
}

impl ModelDefinition {
  /// # Builder Examples
  ///
  /// ## Basic Usage
  /// ```rust
  /// use crate::*;
  ///
  /// let model = ModelDefinition::builder()
  ///     .id("gpt-4")
  ///     .name("GPT-4")
  ///     .provider(Provider::OpenAI)
  ///     .context_size(8192)
  ///     .capabilities([ModelCapability::Chat, ModelCapability::FunctionCalling])
  ///     .input_modalities([Modality::Text])
  ///     .output_modalities([Modality::Text])
  ///     .temperature(0.7)
  ///     .max_tokens(4096)
  ///     .description("Advanced language model")
  ///     .build();
  /// ```
  ///
  /// ## Using Convenience Constructors
  /// ```rust
  /// use crate::*;
  ///
  /// // Chat model with sensible defaults
  /// let chat_model = ModelDefinition::chat_model(
  ///     "gpt-4",
  ///     "GPT-4",
  ///     Provider::OpenAI,
  ///     8192
  /// );
  ///
  /// // Embedding model
  /// let embed_model = ModelDefinition::embedding_model(
  ///     "text-embedding-ada-002",
  ///     "Text Embedding Ada 002",
  ///     Provider::OpenAI,
  ///     8191
  /// );
  /// ```
  /// Create a new ModelDefinition with required fields and validation
  pub fn new(
    id: impl Into<String>,
    name: impl Into<String>,
    provider: Provider,
    context_size: u32,
    capabilities: impl Into<Vec<ModelCapability>>,
    input_modalities: impl Into<Vec<Modality>>,
    output_modalities: impl Into<Vec<Modality>>,
  ) -> Result<Self, String> {
    let capabilities = capabilities.into();
    let input_modalities = input_modalities.into();
    let output_modalities = output_modalities.into();

    if capabilities.is_empty() {
      return Err("At least one capability is required".to_string());
    }
    if input_modalities.is_empty() {
      return Err("At least one input modality is required".to_string());
    }
    if output_modalities.is_empty() {
      return Err("At least one output modality is required".to_string());
    }

    Ok(Self {
      id: id.into(),
      name: name.into(),
      provider,
      description: None,
      context_size,
      max_tokens: None,
      temperature: None,
      top_p: None,
      frequency_penalty: None,
      presence_penalty: None,
      capabilities,
      input_modalities,
      output_modalities,
      dialect: None,
    })
  }

  /// Create a chat model with common defaults
  pub fn chat_model(
    id: impl Into<String>,
    name: impl Into<String>,
    provider: Provider,
    context_size: u32,
  ) -> Self {
    Self {
      id: id.into(),
      name: name.into(),
      provider,
      description: None,
      context_size,
      max_tokens: None,
      temperature: Some(0.7),
      top_p: Some(1.0),
      frequency_penalty: None,
      presence_penalty: None,
      capabilities: vec![ModelCapability::Chat, ModelCapability::FunctionCalling],
      input_modalities: vec![Modality::Text],
      output_modalities: vec![Modality::Text],
      dialect: None,
    }
  }

  /// Create an embedding model
  pub fn embedding_model(
    id: impl Into<String>,
    name: impl Into<String>,
    provider: Provider,
    context_size: u32,
  ) -> Self {
    Self {
      id: id.into(),
      name: name.into(),
      provider,
      description: None,
      context_size,
      max_tokens: None,
      temperature: None,
      top_p: None,
      frequency_penalty: None,
      presence_penalty: None,
      capabilities: vec![ModelCapability::Embeddings],
      input_modalities: vec![Modality::Text],
      output_modalities: vec![Modality::Text],
      dialect: None,
    }
  }
}

impl ToolRef {
  /// # Builder Examples
  ///
  /// ## Using Builder
  /// ```rust
  /// use crate::*;
  ///
  /// let tool_ref = ToolRef::builder()
  ///     .slug("my-tool")
  ///     .provider(ToolProvider::MCP)
  ///     .version("1.5.0")
  ///     .build();
  /// ```
  ///
  /// ## Using Convenience Constructors
  /// ```rust
  /// use crate::*;
  ///
  /// // Without version
  /// let tool1 = ToolRef::new(ToolProvider::Local, "calculator");
  ///
  /// // With version
  /// let tool2 = ToolRef::with_version(ToolProvider::Client, "web-search", "2.0.0");
  /// ```
  /// Create a new ToolRef with provider and slug
  pub fn new(provider: ToolProvider, slug: impl Into<String>) -> Self {
    Self {
      slug: slug.into(),
      version: None,
      provider,
    }
  }

  /// Create a ToolRef with a specific version
  pub fn with_version(provider: ToolProvider, slug: impl Into<String>, version: Version) -> Self {
    Self {
      slug: slug.into(),
      version: Some(version),
      provider,
    }
  }
}

impl AgentDefinition {}

impl ToolDefinition {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_definition_builder() {
    let agent = AgentDefinition::builder()
      .name("test-agent")
      .model("gpt-4")
      .version(semver::Version::parse("1.0.0").unwrap())
      .slug("test-agent")
      .instructions("You are a helpful assistant")
      .organization("test-org")
      .project("test-project")
      .description("A test agent")
      .available_tools(vec!["tool1", "tool2"])
      .build();

    assert_eq!(agent.name, "test-agent");
    assert_eq!(agent.model, "gpt-4");
    assert_eq!(agent.description, Some("A test agent".to_string()));
    assert_eq!(agent.available_tools.len(), 2);
  }

  #[test]
  fn test_agent_definition_builder_minimal() {
    let agent = AgentDefinition::builder()
      .name("minimal-agent")
      .model("gpt-3.5-turbo")
      .version(semver::Version::parse("1.0.0").unwrap())
      .slug("minimal-agent")
      .instructions("Minimal instructions")
      .organization("org")
      .project("project")
      .build();

    assert_eq!(agent.name, "minimal-agent");
    assert!(agent.description.is_none());
    assert!(agent.available_tools.is_empty());
  }

  #[test]
  fn test_agent_ref_from_str() {
    let a = AgentRef::from_str("foo").unwrap();
    assert_eq!(a.slug, "foo");
    assert_eq!(a.version, None);

    let b = AgentRef::from_str("foo/1.2.3").unwrap();
    assert_eq!(b.slug, "foo");
    assert_eq!(b.version, Some(Version::parse("1.2.3").unwrap()));
  }

  #[test]
  fn test_agent_ref_display() {
    let a = AgentRef {
      slug: "foo".to_string(),
      version: None,
    };
    assert_eq!(a.to_string(), "foo");
    let b = AgentRef {
      slug: "foo".to_string(),
      version: Some(Version::parse("1.2.3").unwrap()),
    };
    assert_eq!(b.to_string(), "foo/1.2.3");
  }

  #[test]
  fn test_tool_ref_from_str_valid() {
    // Test basic provider/slug format
    let tool_ref = ToolRef::from_str("local/calculator").unwrap();
    assert_eq!(tool_ref.provider, ToolProvider::Local);
    assert_eq!(tool_ref.slug, "calculator");
    assert_eq!(tool_ref.version, None);

    // Test provider/slug/version format
    let tool_ref = ToolRef::from_str("mcp/file-manager/1.2.3").unwrap();
    assert_eq!(tool_ref.provider, ToolProvider::MCP);
    assert_eq!(tool_ref.slug, "file-manager");
    assert_eq!(tool_ref.version, Some(Version::parse("1.2.3").unwrap()));

    // Test all providers
    let providers = [
      ("client", ToolProvider::Client),
      ("local", ToolProvider::Local),
      ("mcp", ToolProvider::MCP),
      ("restate", ToolProvider::Restate),
    ];

    for (provider_str, expected_provider) in providers {
      let tool_ref = ToolRef::from_str(&format!("{provider_str}/test-tool")).unwrap();
      assert_eq!(tool_ref.provider, expected_provider);
      assert_eq!(tool_ref.slug, "test-tool");
    }
  }

  #[test]
  fn test_tool_ref_from_str_errors() {
    // Test missing slug
    assert!(ToolRef::from_str("local").is_err());
    assert!(ToolRef::from_str("local/").is_err());

    // Test empty input
    assert!(ToolRef::from_str("").is_err());

    // Test invalid provider
    assert!(ToolRef::from_str("invalid/tool").is_err());

    // Test empty version when specified
    assert!(ToolRef::from_str("local/tool/").is_err());

    // Test too many components
    assert!(ToolRef::from_str("local/tool/1.0.0/extra").is_err());
  }

  #[test]
  fn test_tool_ref_display() {
    // Test without version
    let tool_ref = ToolRef {
      slug: "calculator".to_string(),
      version: None,
      provider: ToolProvider::Local,
    };
    assert_eq!(tool_ref.to_string(), "local/calculator");

    // Test with version
    let tool_ref = ToolRef {
      slug: "file-manager".to_string(),
      version: Some(Version::parse("1.2.3").unwrap()),
      provider: ToolProvider::MCP,
    };
    assert_eq!(tool_ref.to_string(), "mcp/file-manager/1.2.3");
  }

  #[test]
  fn test_tool_ref_roundtrip() {
    let test_cases = [
      "local/calculator",
      "mcp/file-manager/1.2.3",
      "client/browser-tool",
      "restate/workflow/2.0.0",
    ];

    for case in test_cases {
      let parsed = ToolRef::from_str(case).unwrap();
      let serialized = parsed.to_string();
      assert_eq!(case, serialized);
    }
  }

  #[test]
  fn test_tool_ref_serde() {
    use serde_json;

    // Test serialization
    let tool_ref = ToolRef {
      slug: "test-tool".to_string(),
      version: Some(Version::parse("1.0.0").unwrap()),
      provider: ToolProvider::Local,
    };
    let json = serde_json::to_string(&tool_ref).unwrap();
    assert_eq!(json, "\"local/test-tool/1.0.0\"");

    // Test deserialization
    let deserialized: ToolRef = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.provider, ToolProvider::Local);
    assert_eq!(deserialized.slug, "test-tool");
    assert_eq!(deserialized.version, Some(Version::parse("1.0.0").unwrap()));

    // Test invalid JSON deserialization
    let invalid_json = "\"invalid\"";
    let result: Result<ToolRef, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());
  }

  #[test]
  fn test_tool_ref_builder() {
    let tool_ref = ToolRef::builder()
      .slug("test-tool")
      .provider(ToolProvider::Local)
      .version(Version::parse("2.0.0").unwrap())
      .build();

    assert_eq!(tool_ref.slug, "test-tool");
    assert_eq!(tool_ref.provider, ToolProvider::Local);
    assert_eq!(
      tool_ref.version,
      Some(semver::Version::parse("2.0.0").unwrap())
    );
  }

  #[test]
  fn test_tool_ref_convenience_constructors() {
    let tool_ref1 = ToolRef::new(ToolProvider::MCP, "my-tool");
    assert_eq!(tool_ref1.slug, "my-tool");
    assert_eq!(tool_ref1.provider, ToolProvider::MCP);
    assert!(tool_ref1.version.is_none());

    let tool_ref2 = ToolRef::with_version(
      ToolProvider::Client,
      "versioned-tool",
      Version::parse("1.5.0").unwrap(),
    );
    assert_eq!(tool_ref2.slug, "versioned-tool");
    assert_eq!(tool_ref2.provider, ToolProvider::Client);
    assert_eq!(tool_ref2.version, Some(Version::parse("1.5.0").unwrap()));
  }

  #[test]
  fn test_tool_definition_builder() {
    let tool = ToolDefinition::builder()
      .slug("test-tool")
      .name("Test Tool")
      .version(Version::parse("1.0.0").unwrap())
      .provider(ToolProvider::Local)
      .description("A test tool")
      .build();

    assert_eq!(tool.slug, "test-tool");
    assert_eq!(tool.name, "Test Tool");
    assert_eq!(tool.provider, ToolProvider::Local);
    assert_eq!(tool.description, Some("A test tool".to_string()));
    assert!(tool.arguments.is_none());
    assert!(tool.created_at.is_none());
    assert!(tool.updated_at.is_none());
  }

  #[test]
  fn test_model_definition_builder() {
    let model = ModelDefinition::builder()
      .id("model-123")
      .name("GPT-4")
      .provider(Provider::OpenAI)
      .context_size(8192)
      .capabilities([ModelCapability::Chat, ModelCapability::FunctionCalling])
      .input_modalities([Modality::Text])
      .output_modalities([Modality::Text])
      .temperature(0.7)
      .max_tokens(4096)
      .build();

    assert_eq!(model.id, "model-123");
    assert_eq!(model.name, "GPT-4");
    assert_eq!(model.provider, Provider::OpenAI);
    assert_eq!(model.context_size, 8192);
    assert_eq!(model.capabilities.len(), 2);
    assert_eq!(model.temperature, Some(0.7));
    assert_eq!(model.max_tokens, Some(4096));
  }

  #[test]
  fn test_model_definition_chat_convenience() {
    let model = ModelDefinition::chat_model("gpt-4", "GPT-4", Provider::OpenAI, 8192);

    assert_eq!(model.id, "gpt-4");
    assert_eq!(model.name, "GPT-4");
    assert_eq!(model.provider, Provider::OpenAI);
    assert_eq!(model.context_size, 8192);
    assert!(model.capabilities.contains(&ModelCapability::Chat));
    assert!(
      model
        .capabilities
        .contains(&ModelCapability::FunctionCalling)
    );
    assert_eq!(model.temperature, Some(0.7));
    assert_eq!(model.top_p, Some(1.0));
  }

  #[test]
  fn test_model_definition_embedding_convenience() {
    let model = ModelDefinition::embedding_model(
      "text-embedding-ada-002",
      "Text Embedding Ada 002",
      Provider::OpenAI,
      8191,
    );

    assert_eq!(model.id, "text-embedding-ada-002");
    assert_eq!(model.name, "Text Embedding Ada 002");
    assert_eq!(model.provider, Provider::OpenAI);
    assert_eq!(model.context_size, 8191);
    assert!(model.capabilities.contains(&ModelCapability::Embeddings));
    assert_eq!(model.capabilities.len(), 1);
    assert!(model.temperature.is_none());
  }

  #[test]
  fn test_model_definition_validation() {
    // Valid model should pass validation
    let valid_model = ModelDefinition::chat_model("test", "Test", Provider::OpenAI, 1000);
    assert!(Validate::validate(&valid_model).is_ok());

    // Model with invalid temperature should fail
    let invalid_temp_model = ModelDefinition::builder()
      .id("test")
      .name("Test")
      .provider(Provider::OpenAI)
      .context_size(1000)
      .capabilities([ModelCapability::Chat])
      .input_modalities([Modality::Text])
      .output_modalities([Modality::Text])
      .temperature(3.0)
      .build();
    assert!(Validate::validate(&invalid_temp_model).is_err());

    // Model with invalid top_p should fail
    let invalid_top_p_model = ModelDefinition::builder()
      .id("test")
      .name("Test")
      .provider(Provider::OpenAI)
      .context_size(1000)
      .capabilities([ModelCapability::Chat])
      .input_modalities([Modality::Text])
      .output_modalities([Modality::Text])
      .top_p(1.5)
      .build();
    assert!(Validate::validate(&invalid_top_p_model).is_err());

    // Model with no capabilities should fail validation at build time
    // This test is no longer needed since typed-builder prevents empty required fields
  }

  #[test]
  fn test_agent_definition_validation() {
    let valid_agent = AgentDefinition::builder()
      .name("test-agent")
      .model("gpt-4")
      .version(Version::parse("1.0.0").unwrap())
      .slug("test-agent")
      .instructions("You are helpful")
      .organization("org")
      .project("project")
      .build();

    assert!(Validate::validate(&valid_agent).is_ok());

    // Empty name should fail validation
    let invalid_agent = AgentDefinition::builder()
      .name("")
      .model("gpt-4")
      .version(Version::parse("1.0.0").unwrap())
      .slug("test-agent")
      .instructions("You are helpful")
      .organization("org")
      .project("project")
      .build();
    assert!(Validate::validate(&invalid_agent).is_err());

    // Empty instructions should fail validation
    let invalid_agent2 = AgentDefinition::builder()
      .name("test-agent")
      .model("gpt-4")
      .version(Version::parse("1.0.0").unwrap())
      .slug("test-agent")
      .instructions("")
      .organization("org")
      .project("project")
      .build();
    assert!(Validate::validate(&invalid_agent2).is_err());
  }

  #[test]
  fn test_tool_definition_validation() {
    let valid_tool = ToolDefinition::builder()
      .slug("test-tool")
      .name("Test Tool")
      .version(Version::parse("1.0.0").unwrap())
      .provider(ToolProvider::Local)
      .build();

    assert!(Validate::validate(&valid_tool).is_ok());

    // Empty slug should fail validation
    let invalid_tool = ToolDefinition::builder()
      .slug("")
      .name("Test Tool")
      .version(Version::parse("1.0.0").unwrap())
      .provider(ToolProvider::Local)
      .build();
    assert!(Validate::validate(&invalid_tool).is_err());

    // Version emptiness no longer applicable: Version is strongly typed.
    // Keep a negative case on invalid provider/slug combo if needed in the future.
    // For now, ensure another invalid case is covered: empty name should fail.
    let invalid_tool2 = ToolDefinition::builder()
      .slug("test-tool")
      .name("")
      .version(Version::parse("1.0.0").unwrap())
      .provider(ToolProvider::Local)
      .build();
    assert!(Validate::validate(&invalid_tool2).is_err());
  }

  #[test]
  fn test_builder_into_setters() {
    // Test that Vec setters accept Into<Vec<T>>
    let model = ModelDefinition::builder()
      .id("test")
      .name("Test")
      .provider(Provider::OpenAI)
      .context_size(1000)
      .capabilities([ModelCapability::Chat, ModelCapability::Embeddings])
      .input_modalities([Modality::Text])
      .output_modalities([Modality::Text, Modality::Image])
      .build();

    assert_eq!(model.capabilities.len(), 2);
    assert_eq!(model.input_modalities.len(), 1);
    assert_eq!(model.output_modalities.len(), 2);

    // Test that available_tools accepts Into<Vec<String>>
    let agent = AgentDefinition::builder()
      .name("test")
      .model("gpt-4")
      .version(semver::Version::parse("1.0.0").unwrap())
      .slug("test")
      .instructions("test")
      .organization("org")
      .project("project")
      .available_tools(["tool1", "tool2"])
      .build();

    assert_eq!(agent.available_tools.len(), 2);
    assert_eq!(agent.available_tools[0], "tool1");
    assert_eq!(agent.available_tools[1], "tool2");
  }
}
