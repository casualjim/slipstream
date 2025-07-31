use std::sync::Arc;

use secrecy::SecretString;

use tokio;
use crate::{
  AgentDefinition, AgentRef, ModelDefinition, Registry, Result, ToolDefinition, ToolRef,
  http::AgentRegistry as HttpAgentRegistry, http::ModelRegistry as HttpModelRegistry,
  http::ToolRegistry as HttpToolRegistry, memory::AgentRegistry as MemoryAgentRegistry,
  memory::ModelRegistry as MemoryModelRegistry, memory::ToolRegistry as MemoryToolRegistry,
  nats::NatsAgentRegistry, nats::NatsModelRegistry, nats::NatsToolRegistry,
};

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
      crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Missing SLIPSTREAM_BASE_URL: {e}"),
      ))
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
    Self::Nats { prefix: prefix.into() }
  }
}

impl Config {
  pub(crate) fn agents(&self) -> Arc<dyn Registry<Key = AgentRef, Subject = AgentDefinition>> {
    match self {
      Config::InMemory => Arc::new(MemoryAgentRegistry::new()),
      Config::ApiService { base_url, api_key } => Arc::new(
        HttpAgentRegistry::new(base_url.clone(), api_key.clone().unwrap_or_default()).unwrap(),
      ),
      Config::Nats { prefix } => {
        // NOTE: This is async, so you may want to spawn or block_on in real usage
        let registry = tokio::runtime::Runtime::new()
          .unwrap()
          .block_on(NatsAgentRegistry::new(prefix));
        Arc::new(registry.unwrap())
      }
    }
  }

  pub(crate) fn tools(&self) -> Arc<dyn Registry<Key = ToolRef, Subject = ToolDefinition>> {
    match self {
      Config::InMemory => Arc::new(MemoryToolRegistry::new()),
      Config::ApiService { base_url, api_key } => Arc::new(
        HttpToolRegistry::new(base_url.clone(), api_key.clone().unwrap_or_default()).unwrap(),
      ),
      Config::Nats { prefix } => {
        let registry = tokio::runtime::Runtime::new()
          .unwrap()
          .block_on(NatsToolRegistry::new(prefix));
        Arc::new(registry.unwrap())
      }
    }
  }

  pub(crate) fn models(&self) -> Arc<dyn Registry<Key = String, Subject = ModelDefinition>> {
    match self {
      Config::InMemory => Arc::new(MemoryModelRegistry::new()),
      Config::ApiService { base_url, api_key } => Arc::new(
        HttpModelRegistry::new(base_url.clone(), api_key.clone().unwrap_or_default()).unwrap(),
      ),
      Config::Nats { prefix } => {
        let registry = tokio::runtime::Runtime::new()
          .unwrap()
          .block_on(NatsModelRegistry::new(prefix));
        Arc::new(registry.unwrap())
      }
    }
  }
}
