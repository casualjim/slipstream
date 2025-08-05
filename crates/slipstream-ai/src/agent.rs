mod tool;
use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use either::Either;
use mockall::automock;
use schemars::{JsonSchema, SchemaGenerator, generate::SchemaSettings};
use slipstream_core::messages::{InstructionsMessage, ToolCallData};
use slipstream_metadata::AgentRef;
use typed_builder::TypedBuilder;

use crate::{CompleterConfig, Error, Result};
pub use tool::*;

#[async_trait]
#[automock]
pub trait Agent: Debug + Send + Sync + 'static {
  fn name(&self) -> &str;
  fn version(&self) -> &semver::Version;

  fn instructions(&self) -> InstructionsMessage;
  fn model(&self) -> &CompleterConfig;
  fn tools(&self) -> &[&'static AgentTool];
  fn response_schema(&self) -> Option<schemars::Schema>;

  fn call_tool_blocking(
    &self,
    input: &ToolCallData,
  ) -> Result<Either<serde_json::Value, Arc<dyn Agent>>> {
    let function_args = serde_json::from_str(&input.arguments)?;

    let function = self
      .tools()
      .iter()
      .find(|f| &f.name == &input.name)
      .ok_or_else(|| Error::FunctionNotFound(input.name.clone()))?;

    let handler = function
      .handler
      .as_ref()
      .ok_or_else(|| Error::FunctionHandlerNotFound(input.name.clone()))?;
    handler.handle_blocking(function_args)
  }

  async fn call_tool(
    &self,
    input: &ToolCallData,
  ) -> Result<Either<serde_json::Value, Arc<dyn Agent>>> {
    let function_args = serde_json::from_str(&input.arguments)?;

    let function = self
      .tools()
      .iter()
      .find(|f| &f.name == &input.name)
      .ok_or_else(|| Error::FunctionNotFound(input.name.clone()))?;

    let handler = function
      .handler
      .as_ref()
      .ok_or_else(|| Error::FunctionHandlerNotFound(input.name.clone()))?;
    handler.handle(function_args).await
  }
}

#[derive(Debug, Default, TypedBuilder)]
pub struct DefaultAgent {
  #[builder(setter(into))]
  name: AgentRef,
  instructions: String,
  #[builder(default)]
  model: CompleterConfig,
  #[builder(default)]
  functions: Vec<&'static AgentTool>,
  #[builder(default)]
  output_schema: Option<schemars::Schema>,
}

impl Agent for DefaultAgent {
  fn name(&self) -> &str {
    &self.name.slug
  }

  fn version(&self) -> &semver::Version {
    self
      .name
      .version
      .as_ref()
      .expect("agent version must be set")
  }

  fn instructions(&self) -> InstructionsMessage {
    InstructionsMessage::new(self.instructions.clone())
  }

  fn model(&self) -> &CompleterConfig {
    &self.model
  }

  fn tools(&self) -> &[&'static AgentTool] {
    self.functions.as_slice()
  }

  fn response_schema(&self) -> Option<schemars::Schema> {
    self.output_schema.clone()
  }
}

impl DefaultAgent {
  pub fn with_output_schema<T: JsonSchema>(self) -> Self {
    let generated =
      SchemaGenerator::new(SchemaSettings::draft2019_09().with(|s| s.inline_subschemas = true))
        .subschema_for::<T>();
    DefaultAgent {
      output_schema: Some(generated),
      ..self
    }
  }

  pub fn add_function(&mut self, function: &'static AgentTool) {
    self.functions.push(function);
  }
}

#[cfg(test)]
mod tests {
  use std::sync::OnceLock;

  use semver::Version;
  use uuid::Uuid;

  use super::*;

  fn hello(hame: String) -> String {
    format!("Hello, {}!", hame)
  }

  #[tokio::test]
  async fn test_hello() {
    let handler = wrap_value_fn(|args: serde_json::Value| async move {
      let name = args["name"].as_str().unwrap_or("world");
      let res = hello(name.to_string());
      Ok(res)
    });
    let result = handler
      .handle(serde_json::json!({"name": "world"}))
      .await
      .unwrap();
    assert_eq!(result.left(), Some(serde_json::json!("Hello, world!")));
  }

  fn hello_agent_agent_function() -> &'static AgentTool {
    static HELLO_AGENT_FUNCTION: OnceLock<AgentTool> = OnceLock::new();
    HELLO_AGENT_FUNCTION.get_or_init(|| AgentTool {
      name: "hello".to_string(),
      description: "Say hello to the world".to_string(),
      arguments: vec!["name".to_string()],
      schema: schemars::json_schema!(true),
      handler: Some(Box::new(wrap_value_fn(
        |args: serde_json::Value| async move {
          let name = args["name"].as_str().unwrap_or("world");
          let res = hello(name.to_string());
          Ok(res)
        },
      ))),
    })
  }

  #[tokio::test]
  async fn test_hello_agent() {
    let mut agent = DefaultAgent {
      name: "hello/0.1.0".parse().expect("Invalid agent name"),
      instructions: "Say hello to the world".to_string(),
      model: CompleterConfig::builder()
        .model("gpt-4.1-mini".to_string())
        .provider("openai".to_string())
        .build(),
      output_schema: None,
      functions: vec![],
    };
    agent.add_function(hello_agent_agent_function());

    let result = agent
      .call_tool(&super::ToolCallData {
        id: Uuid::now_v7().to_string(),
        name: "hello".to_string(),
        arguments: r#"{"name": "world"}"#.to_string(),
      })
      .await
      .unwrap();
    assert_eq!(result.left(), Some(serde_json::json!("Hello, world!")));
  }

  fn hello_agent_returning_agent_function() -> &'static AgentTool {
    static HELLO_AGENT_FUNCTION: OnceLock<AgentTool> = OnceLock::new();
    HELLO_AGENT_FUNCTION.get_or_init(|| AgentTool {
      name: "create_subagent".to_string(),
      description: "Creates a new agent".to_string(),
      arguments: vec!["name".to_string(), "version".to_string()],
      schema: schemars::json_schema!(true),
      handler: Some(Box::new(wrap_agent_fn(
        |args: serde_json::Value| async move {
          let name = args["name"].as_str().unwrap_or("subagent");
          let version = args["version"].as_str().unwrap_or("0.1.0");
          let agent = DefaultAgent::builder()
            .name(
              AgentRef::builder()
                .slug(name)
                .version(Version::parse(version).unwrap())
                .build(),
            )
            .instructions("I am a subagent".to_string())
            .build();
          Ok(Arc::new(agent) as Arc<dyn Agent>)
        },
      ))),
    })
  }

  #[tokio::test]
  async fn test_function_wrappers() {
    // Test value-returning function
    let wrapper = wrap_value_fn(|_args| async move { Ok("Hello World".to_string()) });
    let result = wrapper.handle(serde_json::json!({})).await.unwrap();
    assert!(result.is_left());
    assert_eq!(result.left().unwrap(), serde_json::json!("Hello World"));

    // Test agent-returning function
    let wrapper =
      wrap_agent_fn(|_args| async move { Ok(Arc::new(DefaultAgent::default()) as Arc<dyn Agent>) });
    let result = wrapper.handle(serde_json::json!({})).await.unwrap();
    assert!(result.is_right());
  }

  #[tokio::test]
  async fn test_agent_tool_handlers() {
    // Test regular value-returning tool
    let tool = hello_agent_agent_function();
    let result = tool
      .handler
      .as_ref()
      .unwrap()
      .handle(serde_json::json!({"name": "test"}))
      .await
      .unwrap();
    assert!(result.is_left());
    assert_eq!(result.left().unwrap(), serde_json::json!("Hello, test!"));

    // Test agent-returning tool
    let tool = hello_agent_returning_agent_function();
    let result = tool
      .handler
      .as_ref()
      .unwrap()
      .handle(serde_json::json!({"name": "test_agent"}))
      .await
      .unwrap();

    assert!(result.is_right());
    let agent = result.right().unwrap();
    assert_eq!(agent.name(), "test_agent");
    assert_eq!(agent.instructions().content, "I am a subagent");
  }
}
