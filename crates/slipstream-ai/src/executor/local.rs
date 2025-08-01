use std::{sync::Arc, usize};

use async_trait::async_trait;
use either::Either;
use futures::StreamExt;
use slipstream_core::messages::{
  Aggregator, AssistantContentOrParts, AssistantContentPart, AssistantMessage, Erasable,
  MessageBuilder, Response, ToolCallMessage,
};
use uuid::Uuid;
use validator::Validate as _;

use crate::{
  CompletionParams, Error, Result, ResultStream,
  agent::Agent,
  events::{StreamError, StreamEvent},
  executor::{AgentRequest, Executor, context::ExecutionContext},
};

/// A local executor that runs agents within the current process.
#[derive(Debug, Clone, Copy, Default)]
pub struct Local;

#[async_trait]
impl Executor for Local {
  async fn execute(
    &self,
    mut context: ExecutionContext,
    params: AgentRequest,
  ) -> ResultStream<StreamEvent> {
    use async_stream::try_stream;

    let stream = try_stream! {
      params.validate()?;

      let mut thread = context.session.fork();
      let mut active_agent = context
        .get_agent(&params.agent)
        .ok_or_else(|| Error::UnknownAgent(params.agent.to_string()))?;

      let run_id = params.run_id;
      let max_turns = params.max_turns.unwrap_or(usize::MAX);

      while thread.turn_len() < max_turns {
        // Validate current agent and provider
        let model = active_agent.model();
        if model.provider.is_empty() {
          let err = Error::AgentTool("agent model provider cannot be empty".to_string());
          // yield error and fail
          yield StreamEvent::Error(StreamError::new(run_id, thread.id(), format!("{:?}", err)));
          Err(err)?;
        }

        // Get chat completion stream and process it
        let provider_name = &active_agent.model().provider;
        let completer = context
          .get_provider(provider_name.parse()?)
          .ok_or_else(|| Error::UnknownProvider(provider_name.clone()))?;

        let mut completion = completer
          .chat_completion(CompletionParams {
            run_id,
            agent: active_agent.clone(),
            session: &mut thread,
            tool_choice: None,
            stream: true,
          })
          .await;

        // Pipe through events and drive control flow
        let mut agent_transferred = false;
        while let Some(event_result) = completion.next().await {
          let event = event_result?;
          // forward to caller
          yield event.clone();

          match process_stream_event(run_id, &mut thread, &mut active_agent, event).await? {
            EventProcessing::Continue => continue,
            EventProcessing::Break(_final_text) => {
              // finalize session with the produced state
              context.session.join(&thread);
              // finish
              return;
            }
            EventProcessing::AgentTransfer => {
              agent_transferred = true;
              break;
            }
          }
        }

        if !agent_transferred {
          // Stream ended without a final response
          Err(Error::AgentTool("Stream ended unexpectedly".to_string()))?;
        }

        // If transferred, continue outer loop with new active_agent
      }
      Err(Error::AgentTool(format!("Max turns ({}) exceeded", max_turns)))?;
    };

    Box::pin(stream)
  }
}

// Helpers adapted to work with the yielding executor
async fn process_stream_event(
  run_id: Uuid,
  thread: &mut Aggregator,
  active_agent: &mut Arc<dyn Agent>,
  event: StreamEvent,
) -> Result<EventProcessing> {
  match event {
    StreamEvent::Delim(_) => Ok(EventProcessing::Continue),
    StreamEvent::Chunk(_) => Ok(EventProcessing::Continue),
    StreamEvent::Error(e) => Err(Error::AgentTool(e.err)),
    StreamEvent::Response(response) => match response.response {
      Response::Assistant(assistant_message) => handle_assistant_response(
        run_id,
        thread,
        active_agent,
        response.checkpoint,
        assistant_message,
      ),
      Response::ToolCall(tool_call_message) => {
        handle_tool_call_response(
          run_id,
          thread,
          active_agent,
          response.checkpoint,
          tool_call_message,
        )
        .await
      }
    },
  }
}

fn handle_assistant_response(
  run_id: Uuid,
  thread: &mut Aggregator,
  active_agent: &mut Arc<dyn Agent>,
  checkpoint: slipstream_core::messages::Checkpoint,
  assistant_message: AssistantMessage,
) -> Result<EventProcessing> {
  // Merge checkpoint into our thread
  checkpoint.merge_into(thread);

  // Build and add the assistant message
  let builder = MessageBuilder::new()
    .with_run_id(run_id)
    .with_turn_id(thread.id())
    .with_sender(active_agent.name());

  // Handle refusal first
  if let Some(refusal) = &assistant_message.refusal {
    thread.add_message(builder.assistant_refusal(refusal.clone()).erase());
    return Err(Error::Refusal(refusal.clone()));
  }

  // Extract text content for the final result
  let final_text = match &assistant_message.content {
    Some(AssistantContentOrParts::Content(text)) => {
      thread.add_message(builder.assistant_message(text.clone()).erase());
      text.clone()
    }
    Some(AssistantContentOrParts::Parts(parts)) => {
      let mut text = String::new();
      for part in parts {
        if let AssistantContentPart::Text(text_part) = part {
          text.push_str(&text_part.text);
        }
      }
      thread.add_message(builder.assistant_message_multipart(parts.clone()).erase());
      text
    }
    Some(AssistantContentOrParts::Refusal(refusal)) => {
      thread.add_message(builder.assistant_refusal(refusal.clone()).erase());
      return Err(Error::Refusal(refusal.clone()));
    }
    None => {
      thread.add_message(builder.assistant_message("".to_string()).erase());
      String::new()
    }
  };

  Ok(EventProcessing::Break(final_text))
}

async fn handle_tool_call_response(
  run_id: Uuid,
  thread: &mut Aggregator,
  active_agent: &mut Arc<dyn Agent>,
  checkpoint: slipstream_core::messages::Checkpoint,
  tool_call_message: ToolCallMessage,
) -> Result<EventProcessing> {
  // Fork the thread for tool processing
  let mut forked = thread.fork();
  checkpoint.merge_into(&mut forked);

  // Add the tool call message
  let builder = MessageBuilder::new()
    .with_run_id(run_id)
    .with_turn_id(forked.id())
    .with_sender(active_agent.name());

  forked.add_message(
    builder
      .clone()
      .tool_call(tool_call_message.tool_calls.clone())
      .erase(),
  );

  // Execute tool calls
  if let Some(new_agent) = execute_tool_calls(
    run_id,
    active_agent,
    &mut forked,
    &tool_call_message.tool_calls,
  )
  .await?
  {
    // Join the forked thread back before agent transfer
    thread.join(&forked);
    *active_agent = new_agent;
    return Ok(EventProcessing::AgentTransfer);
  }

  // Join the forked thread back
  thread.join(&forked);

  Ok(EventProcessing::Continue)
}

async fn execute_tool_calls(
  run_id: Uuid,
  active_agent: &Arc<dyn Agent>,
  thread: &mut Aggregator,
  tool_calls: &[slipstream_core::messages::ToolCallData],
) -> Result<Option<Arc<dyn Agent>>> {
  for tool_call in tool_calls {
    // Execute the tool call
    let result = active_agent.call_tool(tool_call).await?;

    match result {
      Either::Left(value) => {
        // Regular tool result - add response message
        let response_text =
          serde_json::to_string(&value).unwrap_or_else(|_| format!("{:?}", value));
        let builder = MessageBuilder::new()
          .with_run_id(run_id)
          .with_turn_id(thread.id())
          .with_sender(active_agent.name());

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

#[derive(Debug)]
enum EventProcessing {
  Continue,
  Break(String),
  AgentTransfer,
}
