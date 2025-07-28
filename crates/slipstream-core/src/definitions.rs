use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
  pub name: String,
  pub model: String,
  pub description: Option<String>,
  pub version: String,
  pub slug: String,
  pub available_tools: Vec<String>,
  pub instructions: String,
  pub organization: String,
  pub project: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolRef {
  pub slug: String,
  pub version: Option<String>,
  pub provider: ToolProvider,
}

impl ToString for ToolRef {
  fn to_string(&self) -> String {
    if let Some(version) = &self.version {
      format!("{}/{}/{version}", self.provider, self.slug)
    } else {
      format!("{}/{}", self.provider, self.slug)
    }
  }
}

impl ToolRef {
  fn from_composed<S: Into<String>>(composed: S) -> Self {
    let composed: String = composed.into();
    let parts: Vec<&str> = composed.split('/').collect();
    let provider = match parts.get(0).map(|v| v.to_lowercase()).as_deref() {
      Some("client") => ToolProvider::Client,
      Some("local") => ToolProvider::Local,
      Some("mcp") => ToolProvider::MCP,
      Some("restate") => ToolProvider::Restate,
      _ => ToolProvider::Unknown,
    };

    let slug = parts.get(1).unwrap_or(&"").to_string();
    let version = parts.get(2).map(|v| v.to_string());

    Self {
      slug,
      version,
      provider,
    }
  }

  fn to_composed(&self) -> String {
    let provider_str = match self.provider {
      ToolProvider::Client => "client",
      ToolProvider::Local => "local",
      ToolProvider::MCP => "mcp",
      ToolProvider::Restate => "restate",
      ToolProvider::Unknown => "unknown",
    };

    match &self.version {
      Some(version) => format!("{}/{}/{}", provider_str, self.slug, version),
      None => format!("{}/{}", provider_str, self.slug),
    }
  }
}

impl Serialize for ToolRef {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    serializer.serialize_str(&self.to_composed())
  }
}

impl<'de> Deserialize<'de> for ToolRef {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let composed = String::deserialize(deserializer)?;
    Ok(Self::from_composed(composed))
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
      ToolProvider::Client => "client",
      ToolProvider::Local => "local",
      ToolProvider::MCP => "mcp",
      ToolProvider::Restate => "restate",
      ToolProvider::Unknown => "unknown",
    }
  }
}

impl FromStr for ToolProvider {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "client" => Ok(ToolProvider::Client),
      "local" => Ok(ToolProvider::Local),
      "mcp" => Ok(ToolProvider::MCP),
      "restate" => Ok(ToolProvider::Restate),
      _ => Err(format!("Invalid tool provider: {}", s)),
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
      ToolProvider::Client => write!(f, "client"),
      ToolProvider::Local => write!(f, "local"),
      ToolProvider::MCP => write!(f, "mcp"),
      ToolProvider::Restate => write!(f, "restate"),
      ToolProvider::Unknown => write!(f, "unknown"),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub slug: String,
  pub name: String,
  pub description: Option<String>,
  pub version: String,
  pub arguments: Option<schemars::Schema>,
  pub provider: ToolProvider,
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
  #[serde(other, skip_serializing)]
  Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
  pub id: String,                         // uuid7, readonly
  pub name: String,                       // required
  pub provider: Provider,                 // required
  pub description: Option<String>,        // optional
  pub context_size: u32,                  // required
  pub max_tokens: Option<u32>,            // optional
  pub temperature: Option<f32>,           // optional
  pub top_p: Option<f32>,                 // optional
  pub frequency_penalty: Option<f32>,     // optional
  pub presence_penalty: Option<f32>,      // optional
  pub capabilities: Vec<ModelCapability>, // required to have at least 1
  pub input_modalities: Vec<Modality>,    // required to have at least 1
  pub output_modalities: Vec<Modality>,   // required to have at least 1
  pub dialect: Option<ApiDialect>,        // optional
}
