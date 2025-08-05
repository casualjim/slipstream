use std::{str::FromStr, sync::Arc};

use async_stream::try_stream;
use futures::StreamExt as _;
use slipstream_metadata::{
  AgentDefinition, AgentRef, ModelDefinition, Provider, Store, ToolDefinition, ToolRef,
};
use tokio::try_join;
use uuid::Uuid;

use crate::{
  Agent, Error, ExecutionContext, Executor, Prompt, ProviderConfig, Result, ResultStream,
  StreamError, StreamEvent, agent, completer::Completer, executor::ExecutorConfig,
};

pub struct Engine {
  meta: Store,
  providers: dashmap::DashMap<Provider, ProviderConfig>,
  local_tools: dashmap::DashMap<ToolRef, Arc<agent::AgentTool>>,
  mcp_tools: dashmap::DashMap<ToolRef, Arc<agent::McpTool>>,
  restate_tools: dashmap::DashMap<ToolRef, Arc<agent::AgentTool>>,

  executor: ExecutorConfig,
  execution_context: ExecutionContext,
}

impl Default for Engine {
  fn default() -> Self {
    Self {
      meta: Store::default(),
      providers: dashmap::DashMap::new(),
      executor: ExecutorConfig::default(),
      execution_context: ExecutionContext::new(),
      local_tools: dashmap::DashMap::new(),
      mcp_tools: dashmap::DashMap::new(),
      restate_tools: dashmap::DashMap::new(),
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
    // TODO: Validate the provider exists
    // TODO: Validate the dialect is supported

    Ok(self.meta.models().put(model.name.clone(), model).await?)
  }

  pub async fn register_provider(&self, provider: Provider, config: ProviderConfig) -> Result<()> {
    self.providers.insert(provider, config);
    Ok(())
  }

  pub async fn initialize(&self) -> Result<()> {
    for key in self.meta.agents().keys(Default::default()).await? {
      let agent = self.meta.agents().get(key.try_into()?).await?.unwrap();
      self
        .execution_context
        .register_agent((&agent).into(), self.build_agent_from_definition(agent));
    }

    for key in self.meta.tools().keys(Default::default()).await? {
      let tool_ref = ToolRef::from_str(&key).map_err(|e| Error::AgentTool(e.to_string()))?;
      let tool = self.meta.tools().get(tool_ref.clone()).await?.unwrap();
      self
        .execution_context
        .register_tool(tool_ref, self.build_tool_from_definition(tool));
    }

    for provider in self.providers.iter() {
      self.execution_context.register_provider(
        provider.key().clone(),
        self.build_completer_from_provider(provider.value()),
      );
    }
    // This can be extended to include more complex initialization logic
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

  async fn validate_provider_config(
    &self,
    model_definition: &ModelDefinition,
  ) -> Result<ProviderConfig> {
    let provider_name = model_definition.provider;
    self
      .providers
      .get(&provider_name)
      .ok_or_else(|| Error::UnknownProvider(provider_name.to_string()))
      .map(|entry| entry.clone())
  }

  fn build_agent_from_definition(&self, agent_def: AgentDefinition) -> Arc<dyn Agent> {
    // Construct AgentRef from &agent_def
    let agent_ref: AgentRef = (&agent_def).into();

    // Build a minimal CompleterConfig with the model from the agent definition
    // Per instructions, provider can be left at default/empty; pass empty string.
    let completer_config = crate::CompleterConfig::builder()
      .model(agent_def.model.clone())
      .provider(String::new())
      .build();

    // Build the DefaultAgent via its builder
    let agent = crate::agent::DefaultAgent::builder()
      .name(agent_ref)
      .instructions(agent_def.instructions.clone())
      .model(completer_config)
      .build();

    Arc::new(agent) as Arc<dyn Agent>
  }

  fn build_tool_from_definition(&self, tool_def: ToolDefinition) -> Arc<agent::AgentTool> {
    // Determine schema and argument names using schemars-native inspection
    let (schema, arguments): (schemars::Schema, Vec<String>) =
      if let Some(s) = tool_def.arguments.clone() {
        let mut args: Vec<String> = Vec::new();
        if let Some(o) = s.get("properties").and_then(|p| p.as_object()) {
          // If the schema is an object, extract keys from "properties" and "patternProperties"
          args.extend(o.keys().cloned());

          (s, args)
        } else {
          // If not an object, we cannot infer argument names safely
          (s, Vec::new())
        }
      } else {
        (schemars::json_schema!(true), Vec::new())
      };

    Arc::new(agent::AgentTool {
      name: tool_def.name.clone(),
      description: tool_def.description.unwrap_or_default(),
      arguments,
      schema,
      handler: None,
    })
  }

  fn build_completer_from_provider(&self, provider: &ProviderConfig) -> Arc<dyn Completer> {
    // Assumption: unknown providers are impossible due to prior validation.
    // Directly construct an OpenAILikeCompleter and return as Arc<dyn Completer>.
    let completer = crate::completer::OpenAILikeCompleter::new(provider);
    Arc::new(completer) as Arc<dyn Completer>
  }

  pub async fn execute(
    &self,
    agent: impl AsRef<str>,
    prompt: Prompt,
  ) -> Result<ResultStream<StreamEvent>> {
    let mut context = ExecutionContext::new();

    let agent_ref: AgentRef = agent.as_ref().parse()?;
    let agent_definition = self.validate_agent(&agent_ref).await?;

    let (model_definition, _) = try_join!(
      self.validate_model(&agent_definition),
      self.validate_agent_tools(&agent_definition)
    )?;

    let _provider_config = self.validate_provider_config(&model_definition).await?;

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
