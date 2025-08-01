use crate::{AgentRequest, ReasoningEffort, Result, completer::StructuredOutput};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use slipstream_core::messages::{Aggregator, MessageBuilder};
use slipstream_metadata::AgentRef;
use uuid::Uuid;

#[derive(Debug)]
pub struct Prompt {
  streaming: bool,
  max_turns: usize,
  context_variables: serde_json::Value,
  reasoning_effort: ReasoningEffort,
  structured_output: Option<StructuredOutput>,
  content: String,
  sender: String,
  metadata: Option<serde_json::Value>,
}

impl Prompt {
  pub fn new(content: impl Into<String>) -> Result<Self> {
    Ok(Self {
      streaming: false,
      max_turns: 10,
      context_variables: serde_json::Value::Null,
      reasoning_effort: ReasoningEffort::default(),
      structured_output: None,
      content: content.into(),
      sender: "user".to_string(),
      metadata: None,
    })
  }

  pub fn streaming(mut self) -> Self {
    self.streaming = true;
    self
  }

  pub fn max_turns(mut self, turns: usize) -> Self {
    self.max_turns = usize::max(1, turns);
    self
  }

  pub fn context_variables(mut self, variables: serde_json::Value) -> Self {
    self.context_variables = variables;
    self
  }

  pub fn reasoning_effort(mut self, effort: crate::ReasoningEffort) -> Self {
    self.reasoning_effort = effort;
    self
  }

  pub fn sender(mut self, sender: impl Into<String>) -> Self {
    self.sender = sender.into();
    self
  }

  pub fn structured_output<T>(
    self,
    name: impl Into<String>,
    description: Option<impl Into<String>>,
  ) -> Prompt
  where
    T: JsonSchema + for<'de> Deserialize<'de>,
  {
    let name = name.into();
    let description = description
      .map(Into::into)
      .unwrap_or_else(|| format!("Structured output for {name}"));
    Prompt {
      structured_output: Some(StructuredOutput {
        name,
        description,
        parameters: schema_for!(T),
      }),
      ..self
    }
  }

  pub(crate) fn build(
    self,
    run_id: Uuid,
    session: &mut Aggregator,
    agent: AgentRef,
  ) -> AgentRequest {
    session.add_user_prompt(
      MessageBuilder::new()
        .with_run_id(run_id)
        .with_turn_id(session.id())
        .with_timestamp(jiff::Timestamp::now())
        .with_sender(self.sender)
        .with_meta(self.metadata.unwrap_or_default())
        .user_prompt(self.content),
    );
    crate::executor::AgentRequest {
      agent: agent,
      run_id,
      tool_choice: None,
      structured_output: self.structured_output,
      stream: self.streaming,
      max_turns: if self.max_turns > 0 {
        Some(self.max_turns)
      } else {
        None
      },
      context_variables: self.context_variables,
      reasoning_effort: self.reasoning_effort,
    }
  }
}
