use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Message {
  System(SystemPrompt),
  User(UserMessage),
  Assistant(AssistantMessage),
  ToolCalls(ToolCallsMessage),
  ToolResponse(ToolResponseMessage),
}

impl Message {
  pub fn sender(&self) -> &str {
    match self {
      Message::System(system_prompt) => &system_prompt.sender,
      Message::User(user_message) => &user_message.sender,
      Message::Assistant(assistant_message) => &assistant_message.sender,
      Message::ToolCalls(tool_calls_message) => &tool_calls_message.sender,
      Message::ToolResponse(tool_response_message) => &tool_response_message.sender,
    }
  }

  pub fn metadata(&self) -> Option<&serde_json::Value> {
    match self {
      Message::System(system_prompt) => system_prompt.metadata.as_ref(),
      Message::User(user_message) => user_message.metadata.as_ref(),
      Message::Assistant(assistant_message) => assistant_message.metadata.as_ref(),
      Message::ToolCalls(tool_calls_message) => tool_calls_message.metadata.as_ref(),
      Message::ToolResponse(_) => None,
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct SystemPrompt {
  pub prompt: String,
  pub sender: String,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct UserMessage {
  pub content: String,
  pub sender: String,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct AssistantMessage {
  pub content: String,
  pub sender: String,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run_id: Option<String>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub finish_reason: Option<String>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub refusal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct ToolCallData {
  pub id: String,
  pub name: String,
  pub args: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct ToolCallsMessage {
  pub tool_calls: Vec<ToolCallData>,
  pub sender: String,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run_id: Option<String>,
  #[builder(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, TypedBuilder)]
pub struct ToolResponseMessage {
  pub id: String,
  pub name: String,
  pub result: String,
  pub sender: String,
}
