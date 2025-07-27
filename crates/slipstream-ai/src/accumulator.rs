//! # Stream Accumulator
//!
//! Implements `StreamAccumulator` to process `StreamEvent` streams, specifically
//! designed to aggregate `Chunk` events into a final, complete `Response`. This
//! utility is essential for clients that consume streaming API responses and need
//! to reconstruct the full message payload.
//!
//! The accumulator maintains the state of the streaming response, combining
//! incremental updates for content, refusals, and tool calls. It also provides
//! helpers to detect when a specific part of the response (like a tool call) has
//! finished streaming, enabling more advanced client-side logic.
//!
//! This implementation is inspired by the Go accumulator from the `openai-go`
//! library and adapted to fit the `slipstream` project's data structures.

use uuid::Uuid;

use crate::events::Chunk;
use crate::messages::{AssistantContentOrParts, AssistantMessage, Response};

/// Represents a tool call that has been fully streamed.
///
/// This struct is returned by the accumulator when a tool call is complete,
/// providing the final, aggregated details of the function call.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedToolCall {
  pub id: String,
  pub index: usize,
  pub name: String,
  pub arguments: String,
}

/// Indicates which part of a streaming response was just completed by the last added chunk.
///
/// This enum allows consumers of the accumulator to react immediately when a
/// specific part of the message, such as the main content or a tool call, has
/// been fully received.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum JustFinished {
  /// Nothing was completed by the last chunk.
  #[default]
  None,
  /// The main content (`AssistantMessage.content`) just finished streaming.
  Content,
  /// The refusal message (`AssistantMessage.refusal`) just finished streaming.
  Refusal,
  /// A tool call just finished streaming. The enclosed `FinishedToolCall` contains
  /// the complete details.
  ToolCall(FinishedToolCall),
}

/// The internal state of a streaming response part (e.g., content, a specific tool call).
///
/// This is used internally by the accumulator to track the context of the current
/// stream of chunks.
#[derive(Debug, Clone, PartialEq)]
enum ResponsePartState {
  Content,
  Refusal,
  ToolCall { index: usize },
}

/// Accumulates `Chunk` events from a stream into a single, cohesive `Response`.
///
/// As `Chunk` events are received, the accumulator merges them into a complete
/// message. It handles the concatenation of text content, refusals, and tool
/// call arguments.
#[derive(Debug, Clone)]
pub struct StreamAccumulator {
  /// The up-to-date accumulation of the model's response.
  pub response: Response,
  /// The state of the part that was being streamed in the *previous* chunk.
  last_part_state: Option<ResponsePartState>,
  /// What part of the response was just finished by the *current* chunk.
  just_finished: JustFinished,
  /// The unique identifier for the run, captured from the first chunk.
  run_id: Option<Uuid>,
  /// The unique identifier for the turn, captured from the first chunk.
  turn_id: Option<Uuid>,
}

impl Default for StreamAccumulator {
  fn default() -> Self {
    Self {
      response: Response::Assistant(AssistantMessage {
        content: None,
        refusal: None,
      }),
      last_part_state: None,
      just_finished: JustFinished::None,
      run_id: None,
      turn_id: None,
    }
  }
}

impl StreamAccumulator {
  /// Creates a new, empty `StreamAccumulator`.
  pub fn new() -> Self {
    Self::default()
  }

  /// Incorporates a `Chunk` into the accumulation. Chunks must be added in order.
  ///
  /// This is the main method for processing a stream. It updates the internal
  /// `response` and determines if any part of the message has just completed.
  ///
  /// # Arguments
  ///
  /// * `chunk` - A reference to the `Chunk` event to process.
  ///
  /// # Returns
  ///
  /// Returns `true` if the chunk was successfully accumulated, and `false` if
  /// the chunk was ignored due to a mismatched `run_id` or `turn_id`.
  pub fn add_chunk(&mut self, chunk: &Chunk) -> bool {
    let is_first_chunk = self.run_id.is_none();

    if is_first_chunk {
      self.run_id = Some(chunk.run_id);
      self.turn_id = Some(chunk.turn_id);
    } else if self.run_id != Some(chunk.run_id) || self.turn_id != Some(chunk.turn_id) {
      // Ensure subsequent chunks belong to the same stream.
      return false;
    }

    self.update_finished_state(chunk);

    if is_first_chunk {
      self.response = chunk.chunk.clone();
    } else {
      self.accumulate_chunk(chunk);
    }

    true
  }

  /// Retrieves the `JustFinished` state and immediately resets it.
  ///
  /// This method is designed to be called after `add_chunk` to check if a
  /// part of the response has completed. Calling this method consumes the
  /// "finished" state, ensuring that it is reported only once.
  ///
  /// # Returns
  ///
  /// The `JustFinished` enum indicating what, if anything, was just completed.
  pub fn take_just_finished(&mut self) -> JustFinished {
    std::mem::take(&mut self.just_finished)
  }

  /// Retrieves the content if it was just finished.
  pub fn just_finished_content(&mut self) -> Option<String> {
    if matches!(self.just_finished, JustFinished::Content) {
      std::mem::take(&mut self.just_finished); // Consume the state
      if let Response::Assistant(AssistantMessage {
        content: Some(AssistantContentOrParts::Content(text)),
        ..
      }) = &self.response
      {
        return Some(text.clone());
      }
    }
    None
  }

  /// Retrieves the refusal if it was just finished.
  pub fn just_finished_refusal(&mut self) -> Option<String> {
    if matches!(self.just_finished, JustFinished::Refusal) {
      std::mem::take(&mut self.just_finished); // Consume the state
      if let Response::Assistant(AssistantMessage {
        refusal: Some(refusal),
        ..
      }) = &self.response
      {
        return Some(refusal.clone());
      }
    }
    None
  }

  /// Retrieves the tool call if one was just finished.
  pub fn just_finished_tool_call(&mut self) -> Option<FinishedToolCall> {
    if let JustFinished::ToolCall(_) = &self.just_finished {
      if let JustFinished::ToolCall(tool) = std::mem::take(&mut self.just_finished) {
        return Some(tool);
      }
    }
    None
  }

  /// Merges the data from a `Chunk` into the accumulator's `response`.
  fn accumulate_chunk(&mut self, chunk: &Chunk) {
    match (&mut self.response, &chunk.chunk) {
      // Both are assistant messages; merge them.
      (Response::Assistant(acc), Response::Assistant(chunk_ass)) => {
        // Append refusal
        if let Some(chunk_refusal) = &chunk_ass.refusal {
          let acc_refusal = acc.refusal.get_or_insert_with(String::new);
          acc_refusal.push_str(chunk_refusal);
        }

        // Append content
        if let Some(chunk_content) = &chunk_ass.content {
          let acc_content = acc
            .content
            .get_or_insert_with(|| AssistantContentOrParts::Content(String::new()));
          match (acc_content, chunk_content) {
            (
              AssistantContentOrParts::Content(acc_text),
              AssistantContentOrParts::Content(chunk_text),
            ) => acc_text.push_str(chunk_text),
            // TODO: Handle more complex merging scenarios if necessary,
            // e.g., merging `Part` variants. For now, we handle simple text.
            _ => {}
          }
        }
      }

      // Both are tool messages; merge them.
      (Response::ToolCall(acc), Response::ToolCall(chunk_tool)) => {
        for delta_tool in &chunk_tool.tool_calls {
          // Find a tool to update. We try to find by ID if the delta has one.
          // If not, we assume the delta is for the last tool call in the accumulator.
          let tool_to_update = if !delta_tool.id.is_empty() {
            acc.tool_calls.iter_mut().find(|t| t.id == delta_tool.id)
          } else {
            acc.tool_calls.last_mut()
          };

          if let Some(tool) = tool_to_update {
            // Append function name and arguments
            tool.name.push_str(&delta_tool.name);
            tool.arguments.push_str(&delta_tool.arguments);
          } else {
            // It's a new tool call, add it to the list.
            acc.tool_calls.push(delta_tool.clone());
          }
        }
      }
      // Mismatched types; for now, we do nothing.
      // A more robust implementation might return an error here.
      _ => {}
    }
  }

  /// Determines the current response part from the chunk and updates the
  /// `just_finished` state if the part has changed since the last chunk.
  fn update_finished_state(&mut self, chunk: &Chunk) {
    // Determine the state of the part being streamed in the current chunk.
    let current_part_state = match &chunk.chunk {
      Response::Assistant(ass) if ass.content.is_some() => Some(ResponsePartState::Content),
      Response::Assistant(ass) if ass.refusal.is_some() => Some(ResponsePartState::Refusal),
      Response::ToolCall(tool_chunk) if !tool_chunk.tool_calls.is_empty() => {
        let current_tool_id = &tool_chunk.tool_calls[0].id;
        if current_tool_id.is_empty() {
          // Continuation of a tool call. The state is the same as the last one.
          if let Some(ResponsePartState::ToolCall { .. }) = self.last_part_state {
            self.last_part_state.clone()
          } else {
            None // Should not happen in a valid stream
          }
        } else if let Response::ToolCall(acc_tool) = &self.response {
          let maybe_index = acc_tool
            .tool_calls
            .iter()
            .position(|t| &t.id == current_tool_id);

          if let Some(index) = maybe_index {
            Some(ResponsePartState::ToolCall { index })
          } else {
            // New tool call
            Some(ResponsePartState::ToolCall {
              index: acc_tool.tool_calls.len(),
            })
          }
        } else {
          // This must be the first tool chunk, so it will be at index 0.
          Some(ResponsePartState::ToolCall { index: 0 })
        }
      }
      _ => None,
    };

    // If the state has changed, the previous part is now finished.
    if self.last_part_state != current_part_state {
      self.just_finished = match &self.last_part_state {
        Some(ResponsePartState::Content) => JustFinished::Content,
        Some(ResponsePartState::Refusal) => JustFinished::Refusal,
        Some(ResponsePartState::ToolCall { index }) => {
          // The tool call at `index` is now complete. We construct the
          // `FinishedToolCall` from the accumulated response.
          if let Response::ToolCall(acc_tool) = &self.response {
            acc_tool
              .tool_calls
              .get(*index)
              .map(|tool| {
                JustFinished::ToolCall(FinishedToolCall {
                  id: tool.id.clone(),
                  index: *index,
                  name: tool.name.clone(),
                  arguments: tool.arguments.clone(),
                })
              })
              .unwrap_or_default()
          } else {
            JustFinished::None
          }
        }
        None => JustFinished::None,
      };
    } else {
      self.just_finished = JustFinished::None;
    }

    // Update the last state for the next chunk.
    self.last_part_state = current_part_state;
  }
}

/// Ensures a `Vec<T>` is large enough to have an element at `index`.
/// If the vector is too small, it is resized with default values.

#[cfg(test)]
mod tests {
  use super::*;
  use crate::messages::{ToolCallData, ToolCallMessage};

  fn make_content_chunk(run_id: Uuid, turn_id: Uuid, content: &str) -> Chunk {
    Chunk::new(
      run_id,
      turn_id,
      Response::Assistant(AssistantMessage {
        content: Some(AssistantContentOrParts::Content(content.to_string())),
        refusal: None,
      }),
    )
  }

  fn make_refusal_chunk(run_id: Uuid, turn_id: Uuid, refusal: &str) -> Chunk {
    Chunk::new(
      run_id,
      turn_id,
      Response::Assistant(AssistantMessage {
        content: None,
        refusal: Some(refusal.to_string()),
      }),
    )
  }

  fn make_tool_chunk(
    run_id: Uuid,
    turn_id: Uuid,
    _index: usize,
    id: &str,
    name: &str,
    args: &str,
  ) -> Chunk {
    Chunk::new(
      run_id,
      turn_id,
      Response::ToolCall(ToolCallMessage {
        tool_calls: vec![ToolCallData {
          id: id.to_string(),
          name: name.to_string(),
          arguments: args.to_string(),
        }],
      }),
    )
  }

  #[test]
  fn test_accumulate_content() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let mut acc = StreamAccumulator::new();

    let chunk1 = make_content_chunk(run_id, turn_id, "Hello, ");
    acc.add_chunk(&chunk1);
    assert_eq!(acc.take_just_finished(), JustFinished::None);

    let chunk2 = make_content_chunk(run_id, turn_id, "world!");
    acc.add_chunk(&chunk2);
    assert_eq!(acc.take_just_finished(), JustFinished::None);

    if let Response::Assistant(ass) = acc.response {
      assert_eq!(
        ass.content,
        Some(AssistantContentOrParts::Content(
          "Hello, world!".to_string()
        ))
      );
    } else {
      panic!("Expected Assistant response");
    }
  }

  #[test]
  fn test_just_finished_content() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let mut acc = StreamAccumulator::new();

    let chunk1 = make_content_chunk(run_id, turn_id, "Hello");
    acc.add_chunk(&chunk1);
    assert_eq!(acc.take_just_finished(), JustFinished::None);

    let chunk2 = make_refusal_chunk(run_id, turn_id, "I cannot");
    acc.add_chunk(&chunk2);
    assert_eq!(acc.take_just_finished(), JustFinished::Content);
    assert_eq!(acc.just_finished_content(), None); // already taken
  }

  #[test]
  fn test_accumulate_tool_calls() {
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let mut acc = StreamAccumulator::new();

    // First tool call part 1
    let chunk1 = make_tool_chunk(run_id, turn_id, 0, "call_1", "get_weather", r#"{"loca"#);
    acc.add_chunk(&chunk1);
    assert_eq!(acc.take_just_finished(), JustFinished::None);

    // First tool call part 2
    let chunk2 = make_tool_chunk(run_id, turn_id, 0, "", "", r#"tion":"SF"}"#);
    acc.add_chunk(&chunk2);
    assert_eq!(acc.take_just_finished(), JustFinished::None);

    // Second tool call
    let chunk3 = make_tool_chunk(run_id, turn_id, 1, "call_2", "get_time", "{}");
    acc.add_chunk(&chunk3);
    let finished = acc.take_just_finished();
    assert_eq!(
      finished,
      JustFinished::ToolCall(FinishedToolCall {
        id: "call_1".to_string(),
        index: 0,
        name: "get_weather".to_string(),
        arguments: r#"{"location":"SF"}"#.to_string(),
      })
    );

    // Final state check
    if let Response::ToolCall(tool) = acc.response {
      assert_eq!(tool.tool_calls.len(), 2);
      assert_eq!(tool.tool_calls[0].id, "call_1");
      assert_eq!(tool.tool_calls[0].name, "get_weather");
      assert_eq!(tool.tool_calls[0].arguments, r#"{"location":"SF"}"#);
      assert_eq!(tool.tool_calls[1].id, "call_2");
      assert_eq!(tool.tool_calls[1].name, "get_time");
    } else {
      panic!("Expected ToolCall response");
    }
  }

  #[test]
  fn test_mismatched_ids() {
    let run_id1 = Uuid::now_v7();
    let turn_id1 = Uuid::now_v7();
    let run_id2 = Uuid::now_v7();
    let mut acc = StreamAccumulator::new();

    let chunk1 = make_content_chunk(run_id1, turn_id1, "Hello");
    assert!(acc.add_chunk(&chunk1));

    let chunk2 = make_content_chunk(run_id2, turn_id1, "World");
    assert!(!acc.add_chunk(&chunk2)); // Should fail

    // check that the state was not modified by the failed chunk
    if let Response::Assistant(ass) = acc.response {
      assert_eq!(
        ass.content,
        Some(AssistantContentOrParts::Content("Hello".to_string()))
      );
    }
  }
}
