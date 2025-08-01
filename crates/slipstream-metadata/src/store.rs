use crate::{
  AgentRegistry, Config, ModelRegistry, Result, ToolRegistry,
  memory::{
    AgentRegistry as MemoryAgentRegistry, ModelRegistry as MemoryModelRegistry,
    ToolRegistry as MemoryToolRegistry,
  },
};
use std::sync::Arc;

pub struct Store {
  ag: AgentRegistry,
  to: ToolRegistry,
  mo: ModelRegistry,
}

impl Default for Store {
  fn default() -> Self {
    Self {
      ag: Arc::new(MemoryAgentRegistry::new()),
      to: Arc::new(MemoryToolRegistry::new()),
      mo: Arc::new(MemoryModelRegistry::new()),
    }
  }
}

impl Store {
  pub async fn new(config: Config) -> Result<Self> {
    let ag = config.agents().await?;
    let to = config.tools().await?;
    let mo = config.models().await?;
    Ok(Self { ag, to, mo })
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
