mod content;
mod session;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use content::*;
pub use session::*;

pub trait Erasable {
  type Erased;
  fn erase(self) -> Self::Erased;
}

// --- Core Message Envelope ---

/// The generic message envelope containing metadata and a payload.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Message<T> {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run_id: Option<Uuid>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub turn_id: Option<Uuid>,
  #[serde(flatten)]
  pub payload: T,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sender: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timestamp: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub meta: Option<Value>,
}

impl<T: Erasable<Erased = ModelMessage>> Erasable for Message<T> {
  type Erased = Message<ModelMessage>;

  fn erase(self) -> Self::Erased {
    Message {
      run_id: self.run_id,
      turn_id: self.turn_id,
      payload: self.payload.erase(),
      sender: self.sender,
      timestamp: self.timestamp,
      meta: self.meta,
    }
  }
}

// --- Message Type Enums ---

/// Represents a request that can be sent to the model.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum Request {
  #[serde(rename = "user")]
  User(UserMessage),
  #[serde(rename = "tool_response")]
  ToolResponse(ToolResponse),
  #[serde(rename = "retry")]
  Retry(Retry),
}

impl Erasable for Request {
  type Erased = ModelMessage;

  fn erase(self) -> Self::Erased {
    ModelMessage::Request(self)
  }
}

/// Represents a response from the model.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum Response {
  #[serde(rename = "assistant")]
  Assistant(AssistantMessage),
  #[serde(rename = "tool_call")]
  ToolCall(ToolCallMessage),
}

impl Erasable for Response {
  type Erased = ModelMessage;

  fn erase(self) -> Self::Erased {
    ModelMessage::Response(self)
  }
}

/// A general message that can be either a request, a response, or other types.
/// This is useful for storing a sequence of messages in history.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum ModelMessage {
  Request(Request),
  Response(Response),
  Instructions(InstructionsMessage),
}

impl<'de> Deserialize<'de> for ModelMessage {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    use serde::de::Error;
    use serde_json::Value;
    
    let value = Value::deserialize(deserializer)?;
    
    // Check if it's an object with a type field
    if let Value::Object(ref map) = value {
      match map.get("type") {
        Some(Value::String(type_str)) => {
          let type_str = type_str.clone();
          // Try to determine which category this type belongs to
          match type_str.as_str() {
            "user" | "tool_response" | "retry" => {
              Request::deserialize(value)
                .map(ModelMessage::Request)
                .map_err(|e| D::Error::custom(format!("Invalid {} message: {}", type_str, e)))
            }
            "assistant" | "tool_call" => {
              Response::deserialize(value)
                .map(ModelMessage::Response)
                .map_err(|e| D::Error::custom(format!("Invalid {} message: {}", type_str, e)))
            }
            "instructions" | "system" | "developer" => {
              InstructionsMessage::deserialize(value)
                .map(ModelMessage::Instructions)
                .map_err(|e| D::Error::custom(format!("Invalid {} message: {}", type_str, e)))
            }
            unknown => Err(D::Error::custom(format!(
              "Unknown message type '{}'. Valid types are: user, assistant, tool_call, tool_response, retry, instructions, system, developer",
              unknown
            ))),
          }
        }
        Some(_) => Err(D::Error::custom(
          "Message 'type' field must be a string. Valid types are: user, assistant, tool_call, tool_response, retry, instructions, system, developer"
        )),
        None => Err(D::Error::custom(
          "Missing required 'type' field in message. Valid types are: user, assistant, tool_call, tool_response, retry, instructions, system, developer"
        ))
      }
    } else {
      Err(D::Error::custom(
        "Message must be a JSON object with a 'type' field. Valid types are: user, assistant, tool_call, tool_response, retry, instructions, system, developer"
      ))
    }
  }
}

#[derive(
  Debug,
  Clone,
  PartialEq,
  Eq,
  Hash,
  Copy,
  Serialize,
  Deserialize,
  Default
)]
#[serde(rename_all = "lowercase")]
pub enum InstructionsType {
  System,
  #[default]
  Instructions,
  Developer,
}

// --- Message Variant Structs ---

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct InstructionsMessage {
  #[serde(rename = "type", default)]
  pub type_field: InstructionsType,
  pub content: String,
}

impl InstructionsMessage {
  pub fn new(content: String) -> Self {
    Self {
      type_field: InstructionsType::Instructions,
      content,
    }
  }

  pub fn system(content: String) -> Self {
    Self {
      type_field: InstructionsType::System,
      content,
    }
  }

  pub fn developer(content: String) -> Self {
    Self {
      type_field: InstructionsType::Developer,
      content,
    }
  }

  pub fn is_system(&self) -> bool {
    self.type_field == InstructionsType::System
  }

  pub fn is_developer(&self) -> bool {
    self.type_field == InstructionsType::Developer
  }
}

impl Erasable for InstructionsMessage {
  type Erased = ModelMessage;

  fn erase(self) -> Self::Erased {
    ModelMessage::Instructions(self)
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UserMessage {
  pub content: ContentOrParts,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AssistantMessage {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub content: Option<AssistantContentOrParts>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub refusal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ToolCallMessage {
  pub tool_calls: Vec<ToolCallData>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ToolCallData {
  pub id: String,
  pub name: String,
  pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ToolResponse {
  pub tool_name: String,
  pub tool_call_id: String,
  pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Retry {
  pub error: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_call_id: Option<String>,
}

// --- Message Builder ---

#[derive(Default, Clone)]
pub struct MessageBuilder {
  run_id: Option<Uuid>,
  turn_id: Option<Uuid>,
  sender: Option<String>,
  timestamp: Option<Timestamp>,
  meta: Option<Value>,
}

impl MessageBuilder {
  pub fn new() -> Self {
    Self {
      timestamp: Some(jiff::Timestamp::now()),
      ..Default::default()
    }
  }

  pub fn with_sender(mut self, sender: impl Into<String>) -> Self {
    self.sender = Some(sender.into());
    self
  }

  pub fn with_run_id(mut self, run_id: Uuid) -> Self {
    self.run_id = Some(run_id);
    self
  }

  pub fn with_turn_id(mut self, turn_id: Uuid) -> Self {
    self.turn_id = Some(turn_id);
    self
  }

  pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
    self.timestamp = Some(timestamp);
    self
  }

  pub fn with_meta(mut self, meta: Value) -> Self {
    self.meta = Some(meta);
    self
  }

  fn build<T>(self, payload: T) -> Message<T> {
    Message {
      run_id: self.run_id,
      turn_id: self.turn_id,
      sender: self.sender,
      timestamp: self.timestamp,
      meta: self.meta,
      payload,
    }
  }

  pub fn instructions(self, content: impl Into<String>) -> Message<ModelMessage> {
    self.build(ModelMessage::Instructions(InstructionsMessage {
      type_field: InstructionsType::Instructions,
      content: content.into(),
    }))
  }

  pub fn user_prompt(self, content: impl Into<String>) -> Message<Request> {
    self.build(Request::User(UserMessage {
      content: ContentOrParts::Content(content.into()),
    }))
  }

  pub fn user_prompt_multipart(self, parts: Vec<ContentPart>) -> Message<Request> {
    self.build(Request::User(UserMessage {
      content: ContentOrParts::Parts(parts),
    }))
  }

  pub fn assistant_message(self, content: impl Into<String>) -> Message<Response> {
    self.build(Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content(content.into())),
      refusal: None,
    }))
  }

  pub fn assistant_message_multipart(self, parts: Vec<AssistantContentPart>) -> Message<Response> {
    self.build(Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Parts(parts)),
      refusal: None,
    }))
  }

  pub fn assistant_refusal(self, refusal: impl Into<String>) -> Message<Response> {
    self.build(Response::Assistant(AssistantMessage {
      content: None,
      refusal: Some(refusal.into()),
    }))
  }

  pub fn tool_call(self, tool_calls: Vec<ToolCallData>) -> Message<Response> {
    self.build(Response::ToolCall(ToolCallMessage { tool_calls }))
  }

  pub fn tool_response(
    self,
    tool_call_id: &str,
    tool_name: &str,
    content: &str,
  ) -> Message<Request> {
    self.build(Request::ToolResponse(ToolResponse {
      tool_call_id: tool_call_id.to_string(),
      tool_name: tool_name.to_string(),
      content: content.to_string(),
    }))
  }

  pub fn tool_error(self, tool_call_id: &str, tool_name: &str, error: &str) -> Message<Request> {
    self.build(Request::Retry(Retry {
      tool_call_id: Some(tool_call_id.to_string()),
      tool_name: Some(tool_name.to_string()),
      error: error.to_string(),
    }))
  }
}

// --- Tests ---

#[cfg(test)]
mod tests {
  use std::str::FromStr as _;

  use super::*;
  use serde_json::json;

  #[test]
  fn test_new_builder() {
    let builder = MessageBuilder::new();
    assert!(builder.timestamp.is_some());
  }

  #[test]
  fn test_message_builder() {
    let now = jiff::Timestamp::now();
    let builder = MessageBuilder::default();
    let meta = json!({"key": "value"});

    // WithSender
    assert_eq!(
      builder.clone().with_sender("test-sender").sender,
      Some("test-sender".to_string())
    );

    // WithTimestamp
    assert_eq!(builder.clone().with_timestamp(now).timestamp, Some(now));

    // WithMetadata
    assert_eq!(
      builder.clone().with_meta(meta.clone()).meta,
      Some(meta.clone())
    );

    // Instructions
    let msg = builder
      .clone()
      .with_sender("test")
      .with_timestamp(now)
      .with_meta(meta.clone())
      .instructions("test content");
    assert_eq!(msg.sender.as_deref(), Some("test"));
    assert_eq!(msg.timestamp, Some(now));
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      ModelMessage::Instructions(i) => assert_eq!(i.content, "test content"),
      _ => panic!("Wrong message type"),
    }

    // UserPrompt
    let msg = builder
      .clone()
      .with_sender("test")
      .with_timestamp(now)
      .with_meta(meta.clone())
      .user_prompt("test content");
    assert_eq!(msg.sender.as_deref(), Some("test"));
    assert_eq!(msg.timestamp, Some(now));
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Request::User(u) => match u.content {
        ContentOrParts::Content(c) => assert_eq!(c, "test content"),
        _ => panic!("Wrong content type"),
      },
      _ => panic!("Wrong message type"),
    }

    // UserPromptMultipart
    let parts = vec![
      ContentPart::Text(TextContentPart {
        text: "part1".to_string(),
      }),
      ContentPart::Image(ImageContentPart {
        url: "image.jpg".to_string(),
        detail: None,
      }),
    ];
    let msg = builder
      .clone()
      .with_sender("test")
      .with_meta(meta.clone())
      .user_prompt_multipart(parts.clone());
    assert_eq!(msg.sender.as_deref(), Some("test"));
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Request::User(u) => match u.content {
        ContentOrParts::Parts(p) => assert_eq!(p, parts),
        _ => panic!("Wrong content type"),
      },
      _ => panic!("Wrong message type"),
    }

    // AssistantMessage
    let msg = builder
      .clone()
      .with_meta(meta.clone())
      .assistant_message("test content");
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Response::Assistant(a) => {
        assert!(a.refusal.is_none());
        match a.content {
          Some(AssistantContentOrParts::Content(c)) => assert_eq!(c, "test content"),
          _ => panic!("Wrong content type"),
        }
      }
      _ => panic!("Wrong message type"),
    }

    // AssistantRefusal
    let msg = builder
      .clone()
      .with_meta(meta.clone())
      .assistant_refusal("not allowed");
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Response::Assistant(a) => {
        assert!(a.content.is_none());
        assert_eq!(a.refusal.as_deref(), Some("not allowed"));
      }
      _ => panic!("Wrong message type"),
    }

    // AssistantMessageMultipart
    let assistant_parts = vec![
      AssistantContentPart::Text(TextContentPart {
        text: "part1".to_string(),
      }),
      AssistantContentPart::Refusal(RefusalContentPart {
        refusal: "not allowed".to_string(),
      }),
    ];
    let msg = builder
      .clone()
      .with_meta(meta.clone())
      .assistant_message_multipart(assistant_parts.clone());
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Response::Assistant(a) => match a.content {
        Some(AssistantContentOrParts::Parts(p)) => assert_eq!(p, assistant_parts),
        _ => panic!("Wrong content type"),
      },
      _ => panic!("Wrong message type"),
    }
  }

  #[test]
  fn test_tool_operations() {
    let now = jiff::Timestamp::now();
    let meta = json!({"key": "value"});
    let builder = MessageBuilder::default();

    // ToolCall
    let tool_data = ToolCallData {
      id: "call-id".to_string(),
      name: "test-tool".to_string(),
      arguments: r#"{"key":"value"}"#.to_string(),
    };
    let msg = builder
      .clone()
      .with_sender("test")
      .with_timestamp(now)
      .with_meta(meta.clone())
      .tool_call(vec![tool_data.clone()]);
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Response::ToolCall(tc) => {
        assert_eq!(tc.tool_calls[0].id, "call-id");
        assert_eq!(tc.tool_calls[0], tool_data);
      }
      _ => panic!("Wrong message type"),
    }

    // ToolResponse
    let msg = builder
      .clone()
      .with_sender("test")
      .with_timestamp(now)
      .with_meta(meta.clone())
      .tool_response("call-id", "test-tool", "result");
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Request::ToolResponse(tr) => {
        assert_eq!(tr.tool_call_id, "call-id");
        assert_eq!(tr.tool_name, "test-tool");
        assert_eq!(tr.content, "result");
      }
      _ => panic!("Wrong message type"),
    }

    // ToolError
    let msg = builder
      .clone()
      .with_sender("test")
      .with_timestamp(now)
      .with_meta(meta.clone())
      .tool_error("call-id", "test-tool", "test error");
    assert_eq!(msg.meta.as_ref(), Some(&meta));
    match msg.payload {
      Request::Retry(r) => {
        assert_eq!(r.tool_call_id.as_deref(), Some("call-id"));
        assert_eq!(r.tool_name.as_deref(), Some("test-tool"));
        assert_eq!(r.error, "test error");
      }
      _ => panic!("Wrong message type"),
    }
  }

  fn assert_json_eq(actual: &str, expected: &str) {
    let actual_val: Value = serde_json::from_str(actual)
      .unwrap_or_else(|e| panic!("actual is not valid JSON: {e}\n{actual}"));
    let expected_val: Value = serde_json::from_str(expected)
      .unwrap_or_else(|e| panic!("expected is not valid JSON: {e}\n{expected}"));
    assert_eq!(actual_val, expected_val);
  }

  #[test]
  fn test_message_json_marshaling() {
    let now_str = "2025-07-21T10:00:00Z";
    let now = jiff::Timestamp::from_str(now_str).unwrap();
    let run_id = Uuid::nil();
    let turn_id = Uuid::nil();

    // Instructions Message
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("system")
      .with_timestamp(now)
      .with_meta(json!({"key":"value"}))
      .instructions("test instructions");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"instructions","content":"test instructions","sender":"system","timestamp":"{}","meta":{{"key":"value"}}}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<ModelMessage> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // User Message with Text
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("user")
      .with_timestamp(now)
      .user_prompt("hello");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"user","content":"hello","sender":"user","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Request> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // User Message with Parts
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("user")
      .with_timestamp(now)
      .user_prompt_multipart(vec![
        ContentPart::Text(TextContentPart {
          text: "hello".into(),
        }),
        ContentPart::Image(ImageContentPart {
          url: "http://example.com/image.jpg".into(),
          detail: None,
        }),
      ]);
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"user","content":[{{"type":"text","text":"hello"}},{{"type":"image","image_url":"http://example.com/image.jpg"}}],"sender":"user","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Request> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Assistant Message with Text
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("assistant")
      .with_timestamp(now)
      .assistant_message("hello");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"assistant","content":"hello","sender":"assistant","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Response> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Assistant Message with Parts
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("assistant")
      .with_timestamp(now)
      .assistant_message_multipart(vec![
        AssistantContentPart::Text(TextContentPart {
          text: "hello".into(),
        }),
        AssistantContentPart::Refusal(RefusalContentPart {
          refusal: "cannot do that".into(),
        }),
      ]);
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"assistant","content":[{{"type":"text","text":"hello"}},{{"type":"refusal","refusal":"cannot do that"}}],"sender":"assistant","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Response> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Assistant Refusal Message
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("assistant")
      .with_timestamp(now)
      .assistant_refusal("cannot do that");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"assistant","refusal":"cannot do that","sender":"assistant","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Response> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Tool Call Message
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("assistant")
      .with_timestamp(now)
      .tool_call(vec![ToolCallData {
        id: "123".into(),
        name: "test_tool".into(),
        arguments: r#"{"arg":"value"}"#.into(),
      }]);
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"tool_call","tool_calls":[{{"id":"123","name":"test_tool","arguments":"{{\"arg\":\"value\"}}"}}],"sender":"assistant","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Response> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Tool Response Message
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("tool")
      .with_timestamp(now)
      .tool_response("123", "test_tool", "tool result");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"tool_response","tool_name":"test_tool","tool_call_id":"123","content":"tool result","sender":"tool","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Request> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);

    // Retry Message
    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("tool")
      .with_timestamp(now)
      .tool_error("123", "test_tool", "test error");
    let data = serde_json::to_string(&msg).unwrap();
    let expected = format!(
      r#"{{"run_id":"{}","turn_id":"{}","type":"retry","error":"test error","tool_name":"test_tool","tool_call_id":"123","sender":"tool","timestamp":"{}"}}"#,
      run_id, turn_id, now_str
    );
    assert_json_eq(&data, &expected);
    let decoded: Message<Request> = serde_json::from_str(&data).unwrap();
    assert_eq!(msg, decoded);
  }

  #[test]
  fn test_message_json_unmarshaling_errors() {
    let test_cases = [
      ("invalid json", r#"{invalid"#, "key must be a string"),
      (
        "missing type field",
        r#"{"content":"test"}"#,
        "Missing required 'type' field in message",
      ),
      (
        "invalid type field",
        r#"{"type":"unknown","content":"test"}"#,
        "unknown variant `unknown`",
      ),
      (
        "missing required content field for instructions",
        r#"{"type":"instructions"}"#,
        "unknown variant `instructions`",
      ),
      (
        "missing required content field for user message",
        r#"{"type":"user"}"#,
        "missing field `content`",
      ),
      (
        "both content and refusal in assistant message",
        r#"{"type":"assistant","content":"hello","refusal":"cannot"}"#,
        "unknown variant `assistant`",
      ),
      (
        "missing tool_calls in tool call",
        r#"{"type":"tool_call"}"#,
        "missing field `tool_calls`",
      ),
      (
        "invalid tool_calls type in tool call",
        r#"{"type":"tool_call","tool_calls":"not_array"}"#,
        "invalid type: string",
      ),
      (
        "missing tool_name in tool response",
        r#"{"type":"tool_response","tool_call_id":"123","content":"result"}"#,
        "missing field `tool_name`",
      ),
      (
        "missing tool_call_id in tool response",
        r#"{"type":"tool_response","tool_name":"test","content":"result"}"#,
        "missing field `tool_call_id`",
      ),
      (
        "missing content in tool response",
        r#"{"type":"tool_response","tool_name":"test","tool_call_id":"123"}"#,
        "missing field `content`",
      ),
      (
        "missing error in retry",
        r#"{"type":"retry","tool_name":"test","tool_call_id":"123"}"#,
        "missing field `error`",
      ),
    ];

    for (name, json, expected_error) in test_cases {
      let err_request = serde_json::from_str::<Message<Request>>(json).err();
      let err_response = serde_json::from_str::<Message<Response>>(json).err();
      let err_model_message = serde_json::from_str::<Message<ModelMessage>>(json).err();

      let err_string = format!("{:?}{:?}{:?}", err_request, err_response, err_model_message);

      assert!(
        err_string.contains(expected_error),
        "Test '{}' failed: expected error containing '{}', but got '{}'",
        name,
        expected_error,
        err_string
      );
    }
  }
}
