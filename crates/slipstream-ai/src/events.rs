use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::messages::{Checkpoint, Message, ModelMessage, Response};

/// Tagged union of all possible stream events during message processing.
/// Serialized with a discriminant "type" field for JSON compatibility.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum StreamEvent {
  #[serde(rename = "delim")]
  Delim(Delim),
  #[serde(rename = "chunk")]
  Chunk(Chunk),
  #[serde(rename = "response")]
  Response(StreamResponse),
  #[serde(rename = "error")]
  Error(StreamError),
}

/// A delimiter event marking boundaries in a stream, typically "start" or "end".
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Delim {
  pub run_id: Uuid,
  pub turn_id: Uuid,
  pub delim: String,
}

/// An incremental piece of a response being streamed back to the client.
/// Contains a response payload along with optional metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Chunk {
  pub run_id: Uuid,
  pub turn_id: Uuid,
  pub chunk: Response,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timestamp: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub meta: Option<Value>,
}

/// A complete response event containing both the final response and aggregator state.
/// The checkpoint captures the full conversation state at completion time.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct StreamResponse {
  pub run_id: Uuid,
  pub turn_id: Uuid,
  pub checkpoint: Checkpoint,
  pub response: Response,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timestamp: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub meta: Option<Value>,
}

/// An error event indicating something went wrong during processing.
/// Implements std::error::Error for integration with error handling.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct StreamError {
  pub run_id: Uuid,
  pub turn_id: Uuid,
  #[serde(rename = "error")]
  pub err: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timestamp: Option<Timestamp>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub meta: Option<Value>,
}

/// Converts a Chunk stream event to a Message with ModelMessage payload.
pub(crate) fn chunk_to_message(dst: &mut Message<ModelMessage>, src: &Chunk) {
  dst.meta = src.meta.clone();
  dst.run_id = Some(src.run_id);
  dst.timestamp = src.timestamp;
  dst.turn_id = Some(src.turn_id);
  dst.payload = ModelMessage::Response(src.chunk.clone());
}

/// Converts a StreamResponse event to a Message with ModelMessage payload.
pub(crate) fn response_to_message(dst: &mut Message<ModelMessage>, src: &StreamResponse) {
  dst.meta = src.meta.clone();
  dst.run_id = Some(src.run_id);
  dst.timestamp = src.timestamp;
  dst.turn_id = Some(src.turn_id);
  dst.payload = ModelMessage::Response(src.response.clone());
}

/// Helper constructors for creating stream events.
impl Delim {
  pub fn new(run_id: Uuid, turn_id: Uuid, delim: String) -> Self {
    Self {
      run_id,
      turn_id,
      delim,
    }
  }
}

impl Chunk {
  pub fn new(run_id: Uuid, turn_id: Uuid, chunk: Response) -> Self {
    Self {
      run_id,
      turn_id,
      chunk,
      timestamp: Some(Timestamp::now()),
      meta: None,
    }
  }

  pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
    self.timestamp = Some(timestamp);
    self
  }

  pub fn with_meta(mut self, meta: Value) -> Self {
    self.meta = Some(meta);
    self
  }
}

impl StreamResponse {
  pub fn new(run_id: Uuid, turn_id: Uuid, checkpoint: Checkpoint, response: Response) -> Self {
    Self {
      run_id,
      turn_id,
      checkpoint,
      response,
      timestamp: Some(Timestamp::now()),
      meta: None,
    }
  }

  pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
    self.timestamp = Some(timestamp);
    self
  }

  pub fn with_meta(mut self, meta: Value) -> Self {
    self.meta = Some(meta);
    self
  }
}

impl StreamError {
  pub fn new(run_id: Uuid, turn_id: Uuid, err: impl Into<String>) -> Self {
    Self {
      run_id,
      turn_id,
      err: err.into(),
      timestamp: Some(Timestamp::now()),
      meta: None,
    }
  }

  pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
    self.timestamp = Some(timestamp);
    self
  }

  pub fn with_meta(mut self, meta: Value) -> Self {
    self.meta = Some(meta);
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::messages::{Aggregator, AssistantContentOrParts, AssistantMessage, MessageBuilder};

  #[test]
  fn test_delim_creation() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let delim = Delim::new(run_id, turn_id, "start".to_string());

    assert_eq!(delim.run_id, run_id);
    assert_eq!(delim.turn_id, turn_id);
    assert_eq!(delim.delim, "start");
  }

  #[test]
  fn test_chunk_creation() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let response = Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content("Hello".to_string())),
      refusal: None,
    });

    let chunk = Chunk::new(run_id, turn_id, response.clone());

    assert_eq!(chunk.run_id, run_id);
    assert_eq!(chunk.turn_id, turn_id);
    assert_eq!(chunk.chunk, response);
    assert!(chunk.timestamp.is_some());
    assert!(chunk.meta.is_none());
  }

  #[test]
  fn test_stream_response_creation() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();
    let response = Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content("Hello".to_string())),
      refusal: None,
    });

    let stream_response =
      StreamResponse::new(run_id, turn_id, checkpoint.clone(), response.clone());

    assert_eq!(stream_response.run_id, run_id);
    assert_eq!(stream_response.turn_id, turn_id);
    assert_eq!(stream_response.checkpoint.id(), checkpoint.id());
    assert_eq!(stream_response.response, response);
    assert!(stream_response.timestamp.is_some());
    assert!(stream_response.meta.is_none());
  }

  #[test]
  fn test_stream_error_creation() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let error_msg = "Something went wrong";

    let stream_error = StreamError::new(run_id, turn_id, error_msg);

    assert_eq!(stream_error.run_id, run_id);
    assert_eq!(stream_error.turn_id, turn_id);
    assert_eq!(stream_error.err, error_msg);
    assert!(stream_error.timestamp.is_some());
    assert!(stream_error.meta.is_none());
  }

  #[test]
  fn test_chunk_to_message() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let response = Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content("Hello".to_string())),
      refusal: None,
    });

    let chunk = Chunk::new(run_id, turn_id, response.clone());
    let mut message = MessageBuilder::new().instructions("dummy");

    chunk_to_message(&mut message, &chunk);

    assert_eq!(message.run_id, Some(run_id));
    assert_eq!(message.turn_id, Some(turn_id));
    assert_eq!(message.timestamp, chunk.timestamp);
    match message.payload {
      ModelMessage::Response(r) => assert_eq!(r, response),
      _ => panic!("Expected Response payload"),
    }
  }

  #[test]
  fn test_response_to_message() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();
    let response = Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content("Hello".to_string())),
      refusal: None,
    });

    let stream_response =
      StreamResponse::new(run_id, turn_id, checkpoint.clone(), response.clone());
    let mut message = MessageBuilder::new().instructions("dummy");

    response_to_message(&mut message, &stream_response);

    assert_eq!(message.run_id, Some(run_id));
    assert_eq!(message.turn_id, Some(turn_id));
    assert_eq!(message.timestamp, stream_response.timestamp);
    match message.payload {
      ModelMessage::Response(r) => assert_eq!(r, response),
      _ => panic!("Expected Response payload"),
    }
  }

  #[test]
  fn test_serialization() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();

    // Test Delim serialization
    let delim = StreamEvent::Delim(Delim::new(run_id, turn_id, "start".to_string()));
    let serialized = serde_json::to_string(&delim).unwrap();
    let deserialized: StreamEvent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(delim, deserialized);

    // Test Chunk serialization
    let response = Response::Assistant(AssistantMessage {
      content: Some(AssistantContentOrParts::Content("Hello".to_string())),
      refusal: None,
    });
    let chunk = StreamEvent::Chunk(Chunk::new(run_id, turn_id, response));
    let serialized = serde_json::to_string(&chunk).unwrap();
    let deserialized: StreamEvent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(chunk, deserialized);

    // Test StreamError serialization
    let error = StreamEvent::Error(StreamError::new(run_id, turn_id, "test error"));
    let serialized = serde_json::to_string(&error).unwrap();
    let deserialized: StreamEvent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(error, deserialized);
  }
}
