//! # OpenAI Chat Completion Accumulator
//!
//! A simple, efficient accumulator for OpenAI streaming chat completions.
//! Focuses on the common use cases without over-engineering.

use async_openai::types::{
  ChatChoice, ChatChoiceStream, ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
  ChatCompletionResponseMessage, ChatCompletionStreamResponseDelta, ChatCompletionToolType,
  CompletionUsage, CreateChatCompletionResponse, CreateChatCompletionStreamResponse, FunctionCall,
  Role,
};
use slipstream_core::messages::{
  AssistantContentOrParts, AssistantMessage, Response, ToolCallData, ToolCallMessage,
};
use std::collections::HashMap;

/// Simple accumulator for streaming chat completions
#[derive(Debug, Clone)]
pub struct ChatCompletionAccumulator {
  /// The accumulated response
  pub response: CreateChatCompletionResponse,
  /// Track incomplete tool calls by choice index and tool call index
  tool_calls_in_progress: HashMap<(u32, u32), PartialToolCall>,
  /// Track what was happening in the last chunk for each choice
  last_state: HashMap<u32, StreamState>,
}

#[derive(Debug, Clone, PartialEq)]
enum StreamState {
  Empty,
  Content,
  ToolCall { index: u32 },
  Refusal,
  Finished,
}

#[derive(Debug, Clone)]
struct PartialToolCall {
  id: Option<String>,
  name: Option<String>,
  arguments: String,
}

impl Default for ChatCompletionAccumulator {
  fn default() -> Self {
    Self {
      response: CreateChatCompletionResponse {
        id: String::new(),
        object: "chat.completion".to_string(),
        created: 0,
        model: String::new(),
        choices: vec![],
        usage: Some(CompletionUsage {
          prompt_tokens: 0,
          completion_tokens: 0,
          total_tokens: 0,
          completion_tokens_details: None,
          prompt_tokens_details: None,
        }),
        system_fingerprint: None,
        service_tier: None,
      },
      tool_calls_in_progress: HashMap::new(),
      last_state: HashMap::new(),
    }
  }
}

impl ChatCompletionAccumulator {
  pub fn new() -> Self {
    Self::default()
  }

  /// Add a streaming chunk and return any events that were triggered
  pub fn add_chunk(&mut self, chunk: CreateChatCompletionStreamResponse) -> Option<Response> {
    let mut completed_response = None;
    // Initialize from first chunk
    if self.response.id.is_empty() {
      self.response.id = chunk.id.clone();
      self.response.object = chunk.object.clone();
      self.response.created = chunk.created;
      self.response.model = chunk.model.clone();
      self.response.system_fingerprint = chunk.system_fingerprint.clone();
      self.response.service_tier = chunk.service_tier.clone();
    }

    // Update metadata
    self.response.model = chunk.model.clone();
    self.response.created = chunk.created;
    self.response.system_fingerprint = chunk.system_fingerprint.clone();

    // Handle usage updates
    if let Some(usage) = chunk.usage {
      if let Some(acc_usage) = &mut self.response.usage {
        acc_usage.completion_tokens = usage.completion_tokens;
        acc_usage.prompt_tokens = usage.prompt_tokens;
        acc_usage.total_tokens = usage.total_tokens;
        // Update details if present
        if let Some(details) = usage.completion_tokens_details {
          acc_usage.completion_tokens_details = Some(details);
        }
        if let Some(details) = usage.prompt_tokens_details {
          acc_usage.prompt_tokens_details = Some(details);
        }
      }
    }

    // Process all choices in the chunk
    for choice_stream in &chunk.choices {
      let choice_index = choice_stream.index;

      // Ensure we have enough choices in our response
      while self.response.choices.len() <= choice_index as usize {
        self.response.choices.push(ChatChoice {
          index: self.response.choices.len() as u32,
          message: ChatCompletionResponseMessage {
            role: Role::Assistant,
            content: Some(String::new()),
            tool_calls: None,
            refusal: None,
            #[allow(deprecated)]
            function_call: None,
            audio: None,
          },
          logprobs: None,
          finish_reason: None,
        });
      }

      // Determine current state before borrowing choice mutably
      let current_state = self.determine_state(&choice_stream.delta);

      let choice = &mut self.response.choices[choice_index as usize];
      choice.index = choice_index;
      choice.finish_reason = choice_stream.finish_reason.clone();
      let previous_state = self
        .last_state
        .get(&choice_index)
        .cloned()
        .unwrap_or(StreamState::Empty);

      // Check for state transitions that indicate completion
      match (&previous_state, &current_state) {
        // Content -> something else = content finished
        (StreamState::Content, new_state) if *new_state != StreamState::Content => {
          if let Some(content) = &choice.message.content {
            if !content.is_empty() {
              completed_response = Some(Response::Assistant(AssistantMessage {
                content: Some(AssistantContentOrParts::Content(content.clone())),
                refusal: None,
              }));
            }
          }
        }
        // Tool call -> different tool call or other state = previous tool call finished
        (StreamState::ToolCall { index: old_idx }, new_state) => {
          let finished = match new_state {
            StreamState::ToolCall { index: new_idx } => old_idx != new_idx,
            _ => true,
          };
          if finished {
            if let Some(response) = self.finish_tool_call(choice_index, *old_idx) {
              completed_response = Some(response);
            }
          }
        }
        // Refusal -> something else = refusal finished
        (StreamState::Refusal, new_state) if *new_state != StreamState::Refusal => {
          if let Some(refusal) = &choice.message.refusal {
            if !refusal.is_empty() {
              completed_response = Some(Response::Assistant(AssistantMessage {
                content: None,
                refusal: Some(refusal.clone()),
              }));
            }
          }
        }
        _ => {}
      }

      // Apply the delta
      self.apply_delta(choice_stream);

      // Check for finish reason
      if choice_stream.finish_reason.is_some() {
        // Finish any remaining tool calls for this choice
        let tool_keys: Vec<(u32, u32)> = self
          .tool_calls_in_progress
          .keys()
          .filter(|(choice_idx, _)| *choice_idx == choice_index)
          .cloned()
          .collect();
        for (choice_idx, tool_idx) in tool_keys {
          if let Some(response) = self.finish_tool_call(choice_idx, tool_idx) {
            completed_response = Some(response);
          }
        }

        // Update state for this choice
        self.last_state.insert(choice_index, StreamState::Finished);

        // Check if all choices are finished
        let _all_finished = self
          .response
          .choices
          .iter()
          .all(|choice| choice.finish_reason.is_some());

        // Note: ResponseFinished event removed - not needed with Response types
      } else {
        self.last_state.insert(choice_index, current_state);
      }
    }

    completed_response
  }

  fn determine_state(&self, delta: &ChatCompletionStreamResponseDelta) -> StreamState {
    if delta.content.is_some() {
      StreamState::Content
    } else if delta.refusal.is_some() {
      StreamState::Refusal
    } else if let Some(tool_calls) = &delta.tool_calls {
      if let Some(first_tool) = tool_calls.first() {
        StreamState::ToolCall {
          index: first_tool.index,
        }
      } else {
        StreamState::Empty
      }
    } else {
      StreamState::Empty
    }
  }

  fn apply_delta(&mut self, choice_stream: &ChatChoiceStream) {
    let choice_index = choice_stream.index as usize;
    let choice = &mut self.response.choices[choice_index];
    let delta = &choice_stream.delta;

    // Update role
    if let Some(role) = &delta.role {
      choice.message.role = role.clone();
    }

    // Accumulate content
    if let Some(content) = &delta.content {
      if let Some(existing_content) = &mut choice.message.content {
        existing_content.push_str(content);
      } else {
        choice.message.content = Some(content.clone());
      }
    }

    // Accumulate refusal
    if let Some(refusal) = &delta.refusal {
      if let Some(existing_refusal) = &mut choice.message.refusal {
        existing_refusal.push_str(refusal);
      } else {
        choice.message.refusal = Some(refusal.clone());
      }
    }

    // Handle tool calls
    if let Some(tool_call_chunks) = &delta.tool_calls {
      for chunk in tool_call_chunks {
        self.accumulate_tool_call_chunk(choice_stream.index, chunk);
      }
    }
  }

  fn accumulate_tool_call_chunk(
    &mut self,
    choice_index: u32,
    chunk: &ChatCompletionMessageToolCallChunk,
  ) {
    let partial = self
      .tool_calls_in_progress
      .entry((choice_index, chunk.index))
      .or_insert_with(|| PartialToolCall {
        id: None,
        name: None,
        arguments: String::new(),
      });

    // Update ID if present
    if let Some(id) = &chunk.id {
      partial.id = Some(id.clone());
    }

    // Update function details if present
    if let Some(function) = &chunk.function {
      if let Some(name) = &function.name {
        partial.name = Some(name.clone());
      }
      if let Some(arguments) = &function.arguments {
        partial.arguments.push_str(arguments);
      }
    }
  }

  fn finish_tool_call(&mut self, choice_index: u32, tool_index: u32) -> Option<Response> {
    if let Some(partial) = self
      .tool_calls_in_progress
      .remove(&(choice_index, tool_index))
    {
      // Only finish if we have complete information
      if let (Some(id), Some(name)) = (partial.id.clone(), partial.name.clone()) {
        // Add to the response's tool calls for the correct choice
        let choice = &mut self.response.choices[choice_index as usize];
        if choice.message.tool_calls.is_none() {
          choice.message.tool_calls = Some(vec![]);
        }

        let tool_calls = choice.message.tool_calls.as_mut().unwrap();
        tool_calls.push(ChatCompletionMessageToolCall {
          id: id.clone(),
          r#type: ChatCompletionToolType::Function,
          function: FunctionCall {
            name: name.clone(),
            arguments: partial.arguments.clone(),
          },
        });

        return Some(Response::ToolCall(ToolCallMessage {
          tool_calls: vec![ToolCallData {
            id,
            name,
            arguments: partial.arguments,
          }],
        }));
      }
    }
    None
  }

  /// Get the current accumulated content for a specific choice (if any)
  #[cfg(test)]
  pub fn current_content(&self, choice_index: u32) -> Option<&str> {
    self
      .response
      .choices
      .get(choice_index as usize)?
      .message
      .content
      .as_deref()
  }

  /// Get the current accumulated content for the first choice (convenience method)
  #[cfg(test)]
  pub fn first_content(&self) -> Option<&str> {
    self.current_content(0)
  }
}

#[cfg(test)]
mod tests {
  use async_openai::types::{FinishReason, FunctionCallStream};

  use super::*;

  fn create_content_chunk(
    id: &str,
    index: u32,
    content: &str,
  ) -> CreateChatCompletionStreamResponse {
    #[allow(deprecated)]
    CreateChatCompletionStreamResponse {
      id: id.to_string(),
      object: "chat.completion.chunk".to_string(),
      created: 1234567890,
      model: "gpt-4".to_string(),
      system_fingerprint: None,
      service_tier: None,
      choices: vec![ChatChoiceStream {
        index,
        delta: ChatCompletionStreamResponseDelta {
          role: Some(Role::Assistant),
          content: Some(content.to_string()),
          refusal: None,
          tool_calls: None,
          function_call: None,
        },
        logprobs: None,
        finish_reason: None,
      }],
      usage: None,
    }
  }

  fn create_tool_chunk(
    id: &str,
    choice_index: u32,
    tool_index: u32,
    tool_id: Option<String>,
    name: Option<String>,
    args: Option<String>,
  ) -> CreateChatCompletionStreamResponse {
    #[allow(deprecated)]
    CreateChatCompletionStreamResponse {
      id: id.to_string(),
      object: "chat.completion.chunk".to_string(),
      created: 1234567890,
      model: "gpt-4".to_string(),
      system_fingerprint: None,
      service_tier: None,
      choices: vec![ChatChoiceStream {
        index: choice_index,
        delta: ChatCompletionStreamResponseDelta {
          role: None,
          content: None,
          refusal: None,
          tool_calls: Some(vec![ChatCompletionMessageToolCallChunk {
            index: tool_index,
            id: tool_id,
            r#type: Some(ChatCompletionToolType::Function),
            function: Some(FunctionCallStream {
              name,
              arguments: args,
            }),
          }]),
          function_call: None,
        },
        logprobs: None,
        finish_reason: None,
      }],
      usage: None,
    }
  }

  fn create_finish_chunk(
    id: &str,
    index: u32,
    reason: FinishReason,
  ) -> CreateChatCompletionStreamResponse {
    #[allow(deprecated)]
    CreateChatCompletionStreamResponse {
      id: id.to_string(),
      object: "chat.completion.chunk".to_string(),
      created: 1234567890,
      model: "gpt-4".to_string(),
      system_fingerprint: None,
      service_tier: None,
      choices: vec![ChatChoiceStream {
        index,
        delta: ChatCompletionStreamResponseDelta {
          role: None,
          content: None,
          refusal: None,
          tool_calls: None,
          function_call: None,
        },
        logprobs: None,
        finish_reason: Some(reason),
      }],
      usage: None,
    }
  }

  #[test]
  fn test_simple_content_accumulation() {
    let mut acc = ChatCompletionAccumulator::new();

    let response = acc.add_chunk(create_content_chunk("test-id", 0, "Hello, "));
    assert!(response.is_none());

    let response = acc.add_chunk(create_content_chunk("test-id", 0, "world!"));
    assert!(response.is_none());

    let response = acc.add_chunk(create_finish_chunk("test-id", 0, FinishReason::Stop));
    assert!(response.is_some());

    match response.unwrap() {
      Response::Assistant(AssistantMessage { content, refusal }) => {
        match content {
          Some(AssistantContentOrParts::Content(text)) => {
            assert_eq!(text, "Hello, world!");
          }
          _ => panic!("Expected content"),
        }
        assert!(refusal.is_none());
      }
      _ => panic!("Expected Assistant response"),
    }

    assert_eq!(acc.first_content(), Some("Hello, world!"));
  }

  #[test]
  fn test_tool_call_accumulation() {
    let mut acc = ChatCompletionAccumulator::new();

    // Start a tool call
    let response = acc.add_chunk(create_tool_chunk(
      "test-id",
      0,
      0,
      Some("call_1".to_string()),
      Some("get_weather".to_string()),
      Some("{\"loc".to_string()),
    ));
    assert!(response.is_none());

    // Continue the tool call
    let response = acc.add_chunk(create_tool_chunk(
      "test-id",
      0,
      0,
      None,
      None,
      Some("ation\":\"SF\"}".to_string()),
    ));
    assert!(response.is_none());

    // Start a new tool call (should finish the first one)
    let response = acc.add_chunk(create_tool_chunk(
      "test-id",
      0,
      1,
      Some("call_2".to_string()),
      Some("get_time".to_string()),
      Some("{}".to_string()),
    ));
    assert!(response.is_some());

    match response.unwrap() {
      Response::ToolCall(ToolCallMessage { tool_calls }) => {
        assert_eq!(tool_calls.len(), 1);
        let tool_call = &tool_calls[0];
        assert_eq!(tool_call.id, "call_1");
        assert_eq!(tool_call.name, "get_weather");
        assert_eq!(tool_call.arguments, "{\"location\":\"SF\"}");
      }
      _ => panic!("Expected ToolCall response"),
    }

    // Finish the response
    let response = acc.add_chunk(create_finish_chunk("test-id", 0, FinishReason::ToolCalls));
    assert!(response.is_some());

    match response.unwrap() {
      Response::ToolCall(ToolCallMessage { tool_calls }) => {
        assert_eq!(tool_calls.len(), 1);
        let tool_call = &tool_calls[0];
        assert_eq!(tool_call.id, "call_2");
        assert_eq!(tool_call.name, "get_time");
        assert_eq!(tool_call.arguments, "{}");
      }
      _ => panic!("Expected ToolCall response"),
    }
  }

  #[test]
  fn test_content_to_tool_transition() {
    let mut acc = ChatCompletionAccumulator::new();

    // Add some content
    let response = acc.add_chunk(create_content_chunk(
      "test-id",
      0,
      "I'll check the weather for you.",
    ));
    assert!(response.is_none());

    // Switch to tool call (should finish content)
    let response = acc.add_chunk(create_tool_chunk(
      "test-id",
      0,
      0,
      Some("call_1".to_string()),
      Some("get_weather".to_string()),
      Some("{\"location\":\"SF\"}".to_string()),
    ));
    assert!(response.is_some());

    match response.unwrap() {
      Response::Assistant(AssistantMessage { content, refusal }) => {
        match content {
          Some(AssistantContentOrParts::Content(text)) => {
            assert_eq!(text, "I'll check the weather for you.");
          }
          _ => panic!("Expected content"),
        }
        assert!(refusal.is_none());
      }
      _ => panic!("Expected Assistant response"),
    }
  }

  #[test]
  fn test_comprehensive_streaming_like_go_test() {
    let mut acc = ChatCompletionAccumulator::new();
    let mut all_responses = Vec::new();

    // Simulate the streaming pattern from the Go test
    // First, stream content chunks (story about Santorini)
    let content_chunks = vec![
      "Let's take a journey to the beautiful island of Santorini in Greece.",
      "\n\nSantorini is a gem in the Aegean Sea, known for its stunning sunsets.",
      " Now, let's check the weather in Santorini.",
    ];

    for chunk_content in content_chunks {
      let response = acc.add_chunk(create_content_chunk("test-id", 0, chunk_content));
      if let Some(r) = response {
        all_responses.push(r);
      }
    }

    // Then transition to tool call
    let tool_response = acc.add_chunk(create_tool_chunk(
      "test-id",
      0,
      0,
      Some("call_FXoAjBUMcVv1k40fficJ9cSs".to_string()),
      Some("get_weather".to_string()),
      Some("{\"".to_string()),
    ));
    if let Some(r) = tool_response {
      all_responses.push(r);
    }

    // Continue tool call arguments across multiple chunks
    let tool_arg_chunks = vec!["location\":\"Santorini", ", Greece\"}"];

    for args in tool_arg_chunks {
      let response = acc.add_chunk(create_tool_chunk(
        "test-id",
        0,
        0,
        None,
        None,
        Some(args.to_string()),
      ));
      if let Some(r) = response {
        all_responses.push(r);
      }
    }

    // Finish with tool_calls reason
    let final_response = acc.add_chunk(create_finish_chunk("test-id", 0, FinishReason::ToolCalls));
    if let Some(r) = final_response {
      all_responses.push(r);
    }

    // Verify we got the expected responses
    let content_responses: Vec<_> = all_responses
      .iter()
      .filter_map(|r| match r {
        Response::Assistant(AssistantMessage { content, .. }) => match content {
          Some(AssistantContentOrParts::Content(text)) => Some(text),
          _ => None,
        },
        _ => None,
      })
      .collect();

    let tool_call_responses: Vec<_> = all_responses
      .iter()
      .filter_map(|r| match r {
        Response::ToolCall(ToolCallMessage { tool_calls }) => {
          if let Some(tool_call) = tool_calls.first() {
            Some((&tool_call.id, &tool_call.name, &tool_call.arguments))
          } else {
            None
          }
        }
        _ => None,
      })
      .collect();

    // Verify content was finished
    assert_eq!(content_responses.len(), 1);
    let expected_content = "Let's take a journey to the beautiful island of Santorini in Greece.\n\nSantorini is a gem in the Aegean Sea, known for its stunning sunsets. Now, let's check the weather in Santorini.";
    assert_eq!(content_responses[0], expected_content);

    // Verify tool call was finished
    assert_eq!(tool_call_responses.len(), 1);
    let (tool_id, tool_name, tool_args) = &tool_call_responses[0];
    assert_eq!(*tool_id, "call_FXoAjBUMcVv1k40fficJ9cSs");
    assert_eq!(*tool_name, "get_weather");
    assert_eq!(*tool_args, "{\"location\":\"Santorini, Greece\"}");

    // Verify final accumulated state matches expectations
    let choice = &acc.response.choices[0];
    assert_eq!(choice.message.content.as_ref().unwrap(), expected_content);

    let tool_calls = choice.message.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_FXoAjBUMcVv1k40fficJ9cSs");
    assert_eq!(tool_calls[0].function.name, "get_weather");
    assert_eq!(
      tool_calls[0].function.arguments,
      "{\"location\":\"Santorini, Greece\"}"
    );

    // Verify we detected "something finished" (like the Go test)
    let anything_finished = !content_responses.is_empty() || !tool_call_responses.is_empty();
    assert!(anything_finished, "No finish events were detected");
  }
}
