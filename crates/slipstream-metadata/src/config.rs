use std::sync::Arc;

use secrecy::SecretString;

use crate::{
  AgentDefinition, AgentRef, ModelDefinition, Registry, Result, ToolDefinition, ToolRef,
  http::AgentRegistry as HttpAgentRegistry, http::ModelRegistry as HttpModelRegistry,
  http::ToolRegistry as HttpToolRegistry, memory::AgentRegistry as MemoryAgentRegistry,
  memory::ModelRegistry as MemoryModelRegistry, memory::ToolRegistry as MemoryToolRegistry,
  nats::NatsAgentRegistry, nats::NatsModelRegistry, nats::NatsToolRegistry,
};

pub type AgentRegistry = Arc<dyn Registry<Key = AgentRef, Subject = AgentDefinition>>;
pub type ToolRegistry = Arc<dyn Registry<Key = ToolRef, Subject = ToolDefinition>>;
pub type ModelRegistry = Arc<dyn Registry<Key = String, Subject = ModelDefinition>>;

pub enum Config {
  InMemory,
  ApiService {
    base_url: String,
    api_key: Option<SecretString>,
  },
  Nats {
    prefix: String,
  },
}

impl Config {
  pub fn from_env() -> Result<Self> {
    let base_url = std::env::var("SLIPSTREAM_BASE_URL").map_err(|e| {
      crate::Error::Io(std::io::Error::other(format!(
        "Missing SLIPSTREAM_BASE_URL: {e}"
      )))
    })?;
    let api_key = std::env::var("SLIPSTREAM_API_KEY")
      .ok()
      .map(SecretString::from);

    Ok(Self::ApiService { base_url, api_key })
  }

  pub fn memory() -> Self {
    Self::InMemory
  }

  pub fn nats(prefix: impl Into<String>) -> Self {
    Self::Nats {
      prefix: prefix.into(),
    }
  }
}

impl Config {
  pub(crate) async fn agents(&self) -> Result<AgentRegistry> {
    match self {
      Config::InMemory => Ok(Arc::new(MemoryAgentRegistry::new())),
      Config::ApiService { base_url, api_key } => Ok(Arc::new(HttpAgentRegistry::new(
        base_url.clone(),
        api_key.clone().unwrap_or_default(),
      )?)),
      Config::Nats { prefix } => {
        let registry = NatsAgentRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }

  pub(crate) async fn tools(&self) -> Result<ToolRegistry> {
    match self {
      Config::InMemory => Ok(Arc::new(MemoryToolRegistry::new())),
      Config::ApiService { base_url, api_key } => Ok(Arc::new(HttpToolRegistry::new(
        base_url.clone(),
        api_key.clone().unwrap_or_default(),
      )?)),
      Config::Nats { prefix } => {
        let registry = NatsToolRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }

  pub(crate) async fn models(&self) -> Result<ModelRegistry> {
    match self {
      Config::InMemory => Ok(Arc::new(MemoryModelRegistry::new())),
      Config::ApiService { base_url, api_key } => Ok(Arc::new(HttpModelRegistry::new(
        base_url.clone(),
        api_key.clone().unwrap_or_default(),
      )?)),
      Config::Nats { prefix } => {
        let registry = NatsModelRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }
}
