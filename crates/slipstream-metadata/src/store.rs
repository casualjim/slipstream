use std::sync::Arc;

use crate::{
  AgentDefinition, AgentRef, Config, ModelDefinition, Registry, ToolDefinition, ToolRef,
};

pub type AgentRegistry = Arc<dyn Registry<Key = AgentRef, Subject = AgentDefinition>>;
pub type ToolRegistry = Arc<dyn Registry<Key = ToolRef, Subject = ToolDefinition>>;
pub type ModelRegistry = Arc<dyn Registry<Key = String, Subject = ModelDefinition>>;

pub struct Store {
  ag: AgentRegistry,
  to: ToolRegistry,
  mo: ModelRegistry,
}

impl Default for Store {
  fn default() -> Self {
    Self::new(Config::memory())
  }
}

impl Store {
  pub fn new(config: Config) -> Self {
    let ag = config.agents();
    let to = config.tools();
    let mo = config.models();
    Self { ag, to, mo }
  }

  pub fn agents(&self) -> AgentRegistry {
    self.ag.clone()
  }

  pub fn tools(&self) -> ToolRegistry {
    self.to.clone()
  }

  pub fn models(&self) -> ModelRegistry {
    self.mo.clone()
  }
}
