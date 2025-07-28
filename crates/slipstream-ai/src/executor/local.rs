use std::{sync::Arc, usize};

use async_trait::async_trait;
use either::Either;
use futures::StreamExt;
use slipstream_core::{
  definitions::Provider,
  messages::{
    Aggregator, AssistantContentOrParts, AssistantContentPart, AssistantMessage, Erasable,
    MessageBuilder, Response, ToolCallMessage,
  },
};
use uuid::Uuid;
use validator::Validate as _;

use crate::{
  CompletionParams, Error, Result,
  agent::Agent,
  completer::Completer,
  events::{StreamError, StreamEvent},
  executor::{AgentRequest, AgentResponse, Executor},
};

/// A local executor that runs agents within the current process.
pub struct Local {
  providers: dashmap::DashMap<String, Arc<dyn Completer>>,
}

impl Local {
  /// Creates a new `Local` executor.
  pub fn new() -> Self {
    Self {
      providers: dashmap::DashMap::new(),
    }
  }

  /// Registers a completer for a given provider.
  pub fn with_provider(self, provider: Provider, completer: Arc<dyn Completer>) -> Self {
    // The key for the provider map is the debug representation of the enum.
    let name = format!("{:?}", provider);
    self.providers.insert(name, completer);
    self
  }
}

impl Default for Local {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Executor for Local {
  async fn execute(&self, mut params: AgentRequest) -> AgentResponse {
    params.validate()?;

    let thread = params.session.fork();
    let active_agent = params.agent.clone();
    let sender = params.sender.clone();

    // Look up the completer by the provider name.
    let provider_name = &params.agent.model().provider;
    let completer = self
      .providers
      .get(provider_name)
      .map(|c| c.value().clone())
      .ok_or_else(|| Error::UnknownProvider(provider_name.clone()))?;

    let reactor = ReactorLoop {
      run_id: params.run_id,
      active_agent,
      completer,
      thread,
      max_turns: params.max_turns.unwrap_or(usize::MAX),
      sender: sender.clone(),
    };

    match reactor.run().await {
      Ok((session, result)) => {
        // Join the forked thread back into the main session on success.
        params.session.join(&session);
        Ok(result)
      }
      Err(e) => {
        // Send error event before returning
        let _ = sender.send(StreamEvent::Error(StreamError::new(
          params.run_id,
          params.session.id(),
          format!("{:?}", e),
        )));
        Err(e)
      }
    }
  }
}

/// The internal state machine for the local executor.
struct ReactorLoop {
  run_id: Uuid,
  active_agent: Arc<dyn Agent>,
  completer: Arc<dyn Completer>,
  thread: Aggregator,
  max_turns: usize,
  sender: tokio::sync::broadcast::Sender<StreamEvent>,
}

impl ReactorLoop {
  /// Runs the agent-model interaction loop until a final response is produced
  /// or an error occurs.
  async fn run(mut self) -> Result<(Aggregator, String)> {
    while self.thread.turn_len() < self.max_turns {
      // Validate current agent and provider
      self.validate_agent_and_provider()?;

      // Get chat completion stream and process it
      match self.process_completion_stream().await? {
        StreamCompletion::Break(result) => {
          return Ok((self.thread.clone(), result));
        }
        StreamCompletion::Continue => {
          // Agent transfer occurred, continue with new agent
          continue;
        }
      }
    }

    Err(Error::AgentTool(format!(
      "Max turns ({}) exceeded",
      self.max_turns
    )))
  }

  fn validate_agent_and_provider(&self) -> Result<()> {
    let model = self.active_agent.model();
    if model.provider.is_empty() {
      let err = Error::AgentTool("agent model provider cannot be empty".to_string());
      self.publish_error(&err);
      return Err(err);
    }
    Ok(())
  }

  async fn process_completion_stream(&mut self) -> Result<StreamCompletion> {
    let mut stream = self
      .completer
      .chat_completion(CompletionParams {
        run_id: self.run_id,
        agent: self.active_agent.clone(),
        session: &mut self.thread,
        tool_choice: None,
        stream: true,
      })
      .await;

    while let Some(event_result) = stream.next().await {
      let event = event_result?;

      // Forward the event immediately
      if let Err(_) = self.sender.send(event.clone()) {
        // Receiver dropped, but continue processing
      }

      match self.process_stream_event(event).await? {
        EventProcessing::Continue => continue,
        EventProcessing::Break(result) => return Ok(StreamCompletion::Break(result)),
        EventProcessing::AgentTransfer => return Ok(StreamCompletion::Continue),
      }
    }

    // Stream ended without a final response - this shouldn't happen in normal operation
    Err(Error::AgentTool("Stream ended unexpectedly".to_string()))
  }

  async fn process_stream_event(&mut self, event: StreamEvent) -> Result<EventProcessing> {
    match event {
      StreamEvent::Delim(_) => Ok(EventProcessing::Continue),
      StreamEvent::Chunk(_) => {
        // Chunks are already forwarded, nothing else to do here
        Ok(EventProcessing::Continue)
      }
      StreamEvent::Error(e) => Err(Error::AgentTool(e.err)),
      StreamEvent::Response(response) => match response.response {
        Response::Assistant(assistant_message) => {
          self.handle_assistant_response(response.checkpoint, assistant_message)
        }
        Response::ToolCall(tool_call_message) => {
          self
            .handle_tool_call_response(response.checkpoint, tool_call_message)
            .await
        }
      },
    }
  }

  fn handle_assistant_response(
    &mut self,
    checkpoint: slipstream_core::messages::Checkpoint,
    assistant_message: AssistantMessage,
  ) -> Result<EventProcessing> {
    // Merge checkpoint into our thread
    checkpoint.merge_into(&mut self.thread);

    // Build and add the assistant message
    let builder = MessageBuilder::new()
      .with_run_id(self.run_id)
      .with_turn_id(self.thread.id())
      .with_sender(self.active_agent.name());

    // Handle refusal first
    if let Some(refusal) = &assistant_message.refusal {
      self
        .thread
        .add_message(builder.assistant_refusal(refusal.clone()).erase());
      return Err(Error::Refusal(refusal.clone()));
    }

    // Extract text content for the final result
    let final_text = match &assistant_message.content {
      Some(AssistantContentOrParts::Content(text)) => {
        self
          .thread
          .add_message(builder.assistant_message(text.clone()).erase());
        text.clone()
      }
      Some(AssistantContentOrParts::Parts(parts)) => {
        let mut text = String::new();
        for part in parts {
          if let AssistantContentPart::Text(text_part) = part {
            text.push_str(&text_part.text);
          }
        }
        self
          .thread
          .add_message(builder.assistant_message_multipart(parts.clone()).erase());
        text
      }
      Some(AssistantContentOrParts::Refusal(refusal)) => {
        self
          .thread
          .add_message(builder.assistant_refusal(refusal.clone()).erase());
        return Err(Error::Refusal(refusal.clone()));
      }
      None => {
        self
          .thread
          .add_message(builder.assistant_message("".to_string()).erase());
        String::new()
      }
    };

    Ok(EventProcessing::Break(final_text))
  }

  async fn handle_tool_call_response(
    &mut self,
    checkpoint: slipstream_core::messages::Checkpoint,
    tool_call_message: ToolCallMessage,
  ) -> Result<EventProcessing> {
    // Fork the thread for tool processing
    let mut forked = self.thread.fork();
    checkpoint.merge_into(&mut forked);

    // Add the tool call message
    let builder = MessageBuilder::new()
      .with_run_id(self.run_id)
      .with_turn_id(forked.id())
      .with_sender(self.active_agent.name());

    forked.add_message(
      builder
        .clone()
        .tool_call(tool_call_message.tool_calls.clone())
        .erase(),
    );

    // Execute tool calls
    let next_agent = self
      .execute_tool_calls(&mut forked, &tool_call_message.tool_calls)
      .await?;

    // Join the forked thread back
    self.thread.join(&forked);

    // Handle agent transfer
    if let Some(new_agent) = next_agent {
      self.active_agent = new_agent;
      return Ok(EventProcessing::AgentTransfer);
    }

    Ok(EventProcessing::Continue)
  }

  async fn execute_tool_calls(
    &self,
    thread: &mut Aggregator,
    tool_calls: &[slipstream_core::messages::ToolCallData],
  ) -> Result<Option<Arc<dyn Agent>>> {
    for tool_call in tool_calls {
      // Execute the tool call
      let result = self.active_agent.call_tool(tool_call).await?;

      match result {
        Either::Left(value) => {
          // Regular tool result - add response message
          let response_text =
            serde_json::to_string(&value).unwrap_or_else(|_| format!("{:?}", value));
          let builder = MessageBuilder::new()
            .with_run_id(self.run_id)
            .with_turn_id(thread.id())
            .with_sender(self.active_agent.name());

          thread.add_message(
            builder
              .tool_response(&tool_call.id, &tool_call.name, &response_text)
              .erase(),
          );
        }
        Either::Right(new_agent) => {
          // Agent transfer - return the new agent immediately
          return Ok(Some(new_agent));
        }
      }
    }

    Ok(None)
  }

  fn publish_error(&self, err: &Error) {
    let error_event = StreamEvent::Error(StreamError::new(
      self.run_id,
      self.thread.id(),
      format!("{:?}", err),
    ));
    let _ = self.sender.send(error_event);
  }
}

#[derive(Debug)]
enum StreamCompletion {
  Break(String),
  Continue,
}

#[derive(Debug)]
enum EventProcessing {
  Continue,
  Break(String),
  AgentTransfer,
}
