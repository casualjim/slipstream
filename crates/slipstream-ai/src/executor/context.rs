use std::sync::Arc;

use dashmap::DashMap;
use slipstream_core::messages::Aggregator;
use slipstream_metadata::{AgentRef, Provider, ToolRef};
use tokio::sync::broadcast::Sender;

use crate::{Agent, StreamEvent, agent::AgentTool, completer::Completer};

pub struct ExecutionContext {
  providers: DashMap<Provider, Arc<dyn Completer>>,
  tools: DashMap<ToolRef, Arc<AgentTool>>,
  agents: DashMap<AgentRef, Arc<dyn Agent>>,
  pub session: Aggregator,
  pub sender: Sender<StreamEvent>,
}

impl ExecutionContext {
  pub fn new(sender: Sender<StreamEvent>) -> Self {
    Self {
      providers: DashMap::new(),
      tools: DashMap::new(),
      agents: DashMap::new(),
      session: Aggregator::new(),
      sender,
    }
  }

  pub fn register_provider(&self, provider_id: Provider, provider: Arc<dyn Completer>) {
    self.providers.insert(provider_id, provider);
  }

  pub fn register_tool(&self, tool_ref: ToolRef, tool: Arc<AgentTool>) {
    self.tools.insert(tool_ref, tool);
  }

  pub fn register_agent(&self, agent_ref: AgentRef, agent: Arc<dyn Agent>) {
    self.agents.insert(agent_ref, agent);
  }

  pub fn get_provider(&self, provider: Provider) -> Option<Arc<dyn Completer>> {
    self.providers.get(&provider).map(|v| v.clone())
  }

  pub fn get_tool(&self, tool_ref: &ToolRef) -> Option<Arc<AgentTool>> {
    self.tools.get(tool_ref).map(|v| v.clone())
  }

  pub fn get_agent(&self, agent_ref: &AgentRef) -> Option<Arc<dyn Agent>> {
    self.agents.get(agent_ref).map(|v| v.clone())
  }
}
