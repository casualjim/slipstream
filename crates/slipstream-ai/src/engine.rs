use std::sync::Arc;

use slipstream_core::{
  definitions::{AgentDefinition, ModelDefinition, ToolDefinition, ToolRef},
  registry::Registry,
  registry::http::{
    AgentRegistry as HttpAgentRegistry, ModelRegistry as HttpModelRegistry,
    ToolRegistry as HttpToolRegistry,
  },
  registry::memory::{
    AgentRegistry as MemoryAgentRegistry, ModelRegistry as MemoryModelRegistry,
    ToolRegistry as MemoryToolRegistry,
  },
};

use crate::{Error, Result};

pub struct Engine {
  agent_registry: Arc<dyn Registry<Key = String, Subject = AgentDefinition>>,
  model_registry: Arc<dyn Registry<Key = String, Subject = ModelDefinition>>,
  tool_registry: Arc<dyn Registry<Key = ToolRef, Subject = ToolDefinition>>,
}

pub enum RegistryKind {
  InMemory,
  ApiService,
}

impl Engine {
  pub fn new(registry_kind: RegistryKind) -> Result<Self> {
    match registry_kind {
      RegistryKind::InMemory => {
        let agent_registry = Arc::new(MemoryAgentRegistry::new());
        let model_registry = Arc::new(MemoryModelRegistry::new());
        let tool_registry = Arc::new(MemoryToolRegistry::new());
        Ok(Self {
          agent_registry,
          model_registry,
          tool_registry,
        })
      }
      RegistryKind::ApiService => {
        let agent_registry = Arc::new(HttpAgentRegistry::from_env().map_err(Error::from)?);
        let model_registry = Arc::new(HttpModelRegistry::from_env().map_err(Error::from)?);
        let tool_registry = Arc::new(HttpToolRegistry::from_env().map_err(Error::from)?);
        Ok(Self {
          agent_registry,
          model_registry,
          tool_registry,
        })
      }
    }
  }
}
