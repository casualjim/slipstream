use std::sync::Arc;

use crate::{
  AgentDefinition, AgentRef, ModelDefinition, Registry, Result, ToolDefinition, ToolRef,
  memory::AgentRegistry as MemoryAgentRegistry, memory::ModelRegistry as MemoryModelRegistry,
  memory::ToolRegistry as MemoryToolRegistry, nats::NatsAgentRegistry, nats::NatsModelRegistry,
  nats::NatsToolRegistry,
};

pub type AgentRegistry = Arc<dyn Registry<Key = AgentRef, Subject = AgentDefinition>>;
pub type ToolRegistry = Arc<dyn Registry<Key = ToolRef, Subject = ToolDefinition>>;
pub type ModelRegistry = Arc<dyn Registry<Key = String, Subject = ModelDefinition>>;

pub enum Config {
  InMemory,

  Nats { prefix: String },
}

impl Config {
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
      Config::Nats { prefix } => {
        let registry = NatsAgentRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }

  pub(crate) async fn tools(&self) -> Result<ToolRegistry> {
    match self {
      Config::InMemory => Ok(Arc::new(MemoryToolRegistry::new())),
      Config::Nats { prefix } => {
        let registry = NatsToolRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }

  pub(crate) async fn models(&self) -> Result<ModelRegistry> {
    match self {
      Config::InMemory => Ok(Arc::new(MemoryModelRegistry::new())),
      Config::Nats { prefix } => {
        let registry = NatsModelRegistry::new(prefix).await;
        Ok(Arc::new(registry?))
      }
    }
  }
}
