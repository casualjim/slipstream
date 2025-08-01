use std::str::FromStr;

use async_stream::try_stream;
use futures::StreamExt as _;
use slipstream_metadata::{
  AgentDefinition, AgentRef, ModelDefinition, Provider, Store, ToolDefinition, ToolRef,
};
use uuid::Uuid;

use crate::{
  Error, ExecutionContext, Executor, Prompt, ProviderConfig, Result, ResultStream, StreamError,
  StreamEvent, executor::ExecutorConfig,
};

pub struct Engine {
  meta: Store,
  providers: dashmap::DashMap<Provider, ProviderConfig>,
  executor: ExecutorConfig,
}

impl Default for Engine {
  fn default() -> Self {
    Self {
      meta: Store::default(),
      providers: dashmap::DashMap::new(),
      executor: ExecutorConfig::default(),
    }
  }
}

impl Engine {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn register_agent(&self, agent: AgentDefinition) -> Result<()> {
    Ok(self.meta.agents().put((&agent).into(), agent).await?)
  }

  pub async fn register_tool(&self, tool: ToolDefinition) -> Result<()> {
    Ok(
      self
        .meta
        .tools()
        .put(
          tool
            .slug
            .parse::<ToolRef>()
            .map_err(|e| Error::AgentTool(e))?,
          tool,
        )
        .await?,
    )
  }

  pub async fn register_model(&self, model: ModelDefinition) -> Result<()> {
    Ok(self.meta.models().put(model.name.clone(), model).await?)
  }

  pub async fn register_provider(&self, provider: Provider, config: ProviderConfig) -> Result<()> {
    self.providers.insert(provider, config);
    Ok(())
  }
}

impl Engine {
  async fn validate_agent(&self, agent_ref: &AgentRef) -> Result<AgentDefinition> {
    self
      .meta
      .agents()
      .get(agent_ref.clone())
      .await?
      .ok_or_else(|| Error::UnknownAgent(agent_ref.to_string()))
  }

  async fn validate_model(&self, agent: &AgentDefinition) -> Result<ModelDefinition> {
    let model_name = agent.model.as_str();
    self
      .meta
      .models()
      .get(model_name.to_string())
      .await?
      .ok_or_else(|| Error::UnknownModel(model_name.to_string()))
  }

  async fn validate_tool(&self, tool_ref: &ToolRef) -> Result<ToolDefinition> {
    self
      .meta
      .tools()
      .get(tool_ref.clone())
      .await?
      .ok_or_else(|| Error::AgentTool(tool_ref.to_string()))
  }

  async fn validate_agent_tools(&self, agent: &AgentDefinition) -> Result<()> {
    for tool_ref in &agent.available_tools {
      let tool_ref: ToolRef =
        ToolRef::from_str(&tool_ref).map_err(|e| Error::AgentTool(e.to_string()))?;

      self.validate_tool(&tool_ref).await?;
    }
    Ok(())
  }

  pub async fn execute(
    &self,
    agent: impl AsRef<str>,
    prompt: Prompt,
  ) -> Result<ResultStream<StreamEvent>> {
    let mut context = ExecutionContext::new();

    let agent_ref: AgentRef = agent.as_ref().parse()?;
    let agent_definition = self.validate_agent(&agent_ref).await?;

    let model_definition = self.validate_model(&agent_definition).await?;

    let provider_name = model_definition.provider;
    let _provider_config = self
      .providers
      .get(&provider_name)
      .ok_or_else(|| Error::UnknownProvider(provider_name.to_string()))?
      .clone();

    self.validate_agent_tools(&agent_definition).await?;

    // TODO: convert all agent definitions to Agent instances and register on the context
    // TODO: convert all tools to AgentTool instances and register on the context
    // TODO: convert all providers to Completer instances and register on the context

    // Carry a single run_id through the entire run
    let run_id = Uuid::now_v7();
    let turn_id = context.session.id();

    let executor = match self.executor {
      ExecutorConfig::Local => crate::Local,
      ExecutorConfig::Restate { .. } => {
        // Return a one-shot error stream for unsupported executors
        let stream = try_stream! {
          yield StreamEvent::Error(StreamError::new(
            run_id,
            turn_id,
            "Restate executor is not supported yet".to_string(),
          ));
        };
        return Ok(stream.boxed());
      }
    };

    let params = prompt.build(run_id, &mut context.session, agent.as_ref().parse()?);
    Ok(executor.execute(context, params).await)
  }
}
