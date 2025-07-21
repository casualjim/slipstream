use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::messages::{Message, UserMessage};

/// Represents different types of events that can occur in a conversation turn.
/// Each event is associated with a unique identifier (UUIDv7) representing the turn ID.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Event {
  /// Event for completion with a unique identifier (turn ID) and user message.
  Completion(Uuid, UserMessage),
  /// Event for delimiter with a unique identifier (turn ID) and delimiter type.
  Delim(Uuid, Delim),
  /// Event for a chunk of completion with a unique identifier (turn ID) and completion chunk.
  Chunk(Uuid, CompletionChunk),
  /// Event for a response with a unique identifier (turn ID) and message.
  Response(Uuid, Message),
}

/// Represents the start and end delimiters in a conversation turn.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Delim {
  /// Start delimiter with a unique identifier (turn ID).
  Start(Uuid),
  /// End delimiter with a unique identifier (turn ID).
  End(Uuid),
}

/// Represents a chunk of completion data within a conversation turn.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CompletionChunk {
  /// The delta of the completion.
  pub delta: CompletionDelta,
  /// The sender of the completion chunk.
  pub sender: String,
  /// The run identifier, if any.
  pub run_id: Option<String>,
  /// The reason for finishing, if any.
  pub finish_reason: Option<String>,
}

/// Represents the delta of a completion within a conversation turn.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CompletionDelta {
  /// The content of the completion delta.
  pub content: Option<String>,
  /// The sender of the completion delta.
  pub sender: Option<String>,
  /// The refusal message, if any.
  pub refusal: Option<String>,
  /// The role of the completion delta.
  pub role: Option<CompletionDeltaRole>,
  /// The reason for finishing, if any.
  pub finish_reason: Option<String>,
  /// The identifier of the completion delta, if any.
  pub id: Option<String>,
  /// The tool calls associated with the completion delta.
  pub tool_calls: Option<Vec<CompletionDeltaToolCall>>,
}

/// Represents a tool call within a completion delta.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CompletionDeltaToolCall {
  /// The index of the tool call.
  pub index: i64,
  /// The identifier of the tool call.
  pub id: String,
  /// The type of the tool call.
  #[serde(rename = "type")]
  pub r#type: String,
  /// The function associated with the tool call.
  pub function: CompletionDeltaToolCallFunction,
}

/// Represents a function within a tool call.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CompletionDeltaToolCallFunction {
  /// The name of the function.
  pub name: String,
  /// The arguments of the function, if any.
  pub arguments: Option<String>,
}

/// Represents the role of a completion delta.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CompletionDeltaRole {
  /// System role.
  System,
  /// User role.
  User,
  /// Assistant role.
  Assistant,
  /// Tool role.
  Tool,
}
