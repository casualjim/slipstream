use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
  pub name: String,
  pub model: ModelDefinition,
  pub description: Option<String>,
  pub version: String,
  pub slug: String,
  pub available_tools: Vec<ToolDefinition>,
  pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolRef {
  pub slug: String,
  pub version: Option<String>,
  pub provider: ToolProvider,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvider {
  Client,
  Local,
  MCP,
  Restate,
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
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
  Text,
  Image,
  Video,
  Audio,
  Pdf,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiDialect {
  OpenAI,
  Anthropic,
  Deepseek,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDefinition {
  pub id: String,                  // uuid7, readonly
  pub name: String,                // required
  pub provider: Provider,          // required
  pub description: Option<String>, // optional
  #[serde(rename = "contextSize")]
  pub context_size: u32, // required
  #[serde(rename = "maxTokens")]
  pub max_tokens: Option<u32>, // optional
  pub temperature: Option<f32>,    // optional
  #[serde(rename = "topP")]
  pub top_p: Option<f32>, // optional
  #[serde(rename = "frequencyPenalty")]
  pub frequency_penalty: Option<f32>, // optional
  #[serde(rename = "presencePenalty")]
  pub presence_penalty: Option<f32>, // optional
  pub capabilities: Vec<ModelCapability>, // required to have at least 1
  #[serde(rename = "inputModalities")]
  pub input_modalities: Vec<Modality>, // required to have at least 1
  #[serde(rename = "outputModalities")]
  pub output_modalities: Vec<Modality>, // required to have at least 1
  pub dialect: Option<ApiDialect>, // optional
}
