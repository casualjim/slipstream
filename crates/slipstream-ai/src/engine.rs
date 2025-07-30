use schemars::JsonSchema;
use serde::Deserialize;
use slipstream_core::messages::MessageBuilder;
use slipstream_metadata::{
  AgentDefinition, AgentRef, ModelDefinition, Provider, Store, ToolDefinition, ToolRef,
};
use uuid::Uuid;

use crate::{
  AgentRequest, Error, ExecutionContext, Executor, ProviderConfig, Result, StreamEvent,
  executor::{AgentResponse, ExecutorConfig},
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

  pub async fn execute<T: for<'de> Deserialize<'de> + JsonSchema>(
    &self,
    agent: impl Into<AgentRef>,
    prompt: impl Into<String>,
    sender: tokio::sync::broadcast::Sender<StreamEvent>,
  ) -> AgentResponse<T> {
    let prompt = prompt.into();

    let executor = match self.executor {
      ExecutorConfig::Local => crate::Local,
      ExecutorConfig::Restate { .. } => {
        return Err(Error::UnsupportedExecutor);
      }
    };

    let run_id = Uuid::now_v7();

    let mut context = ExecutionContext::new(sender);
    context.session.add_user_prompt(
      MessageBuilder::new()
        .with_run_id(run_id)
        .user_prompt(prompt),
    );

    let params = AgentRequest::builder()
      .run_id(Uuid::now_v7())
      .agent(agent.into())
      .build();

    executor.execute(context, params).await
  }
}
