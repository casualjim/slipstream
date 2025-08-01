#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use crate::{
  Agent, AgentRequest, CompleterConfig, DefaultAgent, ExecutionContext, Executor, Local,
  OpenAILikeCompleter, ProviderConfig, ResultStream, events::StreamEvent,
};
use futures::StreamExt;
use slipstream_core::messages::{Aggregator, MessageBuilder, Response};
use slipstream_metadata::Provider;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

fn require_openai_key() -> String {
  std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set for these integration tests")
}

fn make_provider_config() -> ProviderConfig {
  let api_key = require_openai_key();
  ProviderConfig::builder()
    .name("openai".to_string())
    .api_key(api_key)
    .build()
}

fn register_openai(ctx: &ExecutionContext, cfg: &ProviderConfig) {
  let completer = Arc::new(OpenAILikeCompleter::new(cfg));
  ctx.register_provider(Provider::OpenAI, completer);
}

fn minimal_agent(agent_ref: &str) -> Arc<dyn Agent> {
  Arc::new(
    DefaultAgent::builder()
      .name(agent_ref)
      .instructions("You are a concise assistant.".to_string())
      .model(
        CompleterConfig::builder()
          .provider("openai".to_string())
          .model("gpt-4.1-nano".to_string())
          .temperature(0.0)
          .n(1)
          .build(),
      )
      .build(),
  )
}

async fn drain_events_to_vec(
  mut stream: crate::ResultStream<crate::events::StreamEvent>,
) -> eyre::Result<Vec<crate::events::StreamEvent>> {
  let mut out = Vec::new();
  while let Some(next) = stream.next().await {
    out.push(next?);
  }
  Ok(out)
}

#[tokio::test]
async fn streaming_assistant_success() -> eyre::Result<()> {
  // Fail fast if key missing
  let provider_cfg = make_provider_config();

  // Context and provider
  let mut ctx = ExecutionContext::new();
  register_openai(&ctx, &provider_cfg);

  // Agent
  let agent_ref = "it/minimal-agent";
  let agent = minimal_agent(agent_ref);
  ctx.register_agent(agent_ref.into(), agent.clone());

  // Seed a simple prompt to drive streaming without forcing exact output
  let run_id = Uuid::now_v7();
  let turn_id = ctx.session.id();
  let user_msg = MessageBuilder::new()
    .with_run_id(run_id)
    .with_turn_id(turn_id)
    .user_prompt("Say hello briefly.".to_string());
  ctx.session.add_user_prompt(user_msg);

  // Execute
  let req = AgentRequest::builder()
    .agent(agent_ref)
    .run_id(run_id)
    .stream(true)
    .build();
  let stream = Local.execute(ctx, req).await;
  // Bound total time so we don’t hang forever if networking misbehaves
  let events = timeout(Duration::from_secs(45), drain_events_to_vec(stream))
    .await
    .map_err(|_| eyre::eyre!("timeout draining stream"))??;

  // Assertions: event ordering has at least one chunk and a final response
  assert!(
    events
      .iter()
      .any(|e| matches!(e, crate::events::StreamEvent::Chunk(_))),
    "expected at least one Chunk event"
  );
  let final_resp = events
    .iter()
    .rev()
    .find_map(|e| match e {
      crate::events::StreamEvent::Response(r) => Some(r),
      _ => None,
    })
    .expect("expected a final Response event");

  if let Response::Assistant(msg) = &final_resp.response {
    let text = match &msg.content {
      Some(slipstream_core::messages::AssistantContentOrParts::Content(t)) => t.as_str(),
      _ => "",
    };
    assert!(text == "OK", "expected exact 'OK', got: {:?}", text);
  } else {
    panic!(
      "expected final assistant response, got: {:?}",
      final_resp.response
    );
  }

  Ok(())
}

#[tokio::test]
async fn error_missing_provider_fails() {
  // Build context with agent whose model has empty provider to trigger the guard in Local
  let mut ctx = ExecutionContext::new();

  // Agent with empty provider
  let agent_ref = "it/empty-provider";
  let agent = Arc::new(
    DefaultAgent::builder()
      .name(agent_ref)
      .instructions("no-op".to_string())
      .model(
        CompleterConfig::builder()
          .provider("".to_string()) // empty to trigger the validation
          .model("gpt-4.1-nano".to_string())
          .build(),
      )
      .build(),
  ) as Arc<dyn Agent>;
  ctx.register_agent(agent_ref.into(), agent);

  // Add a trivial user message
  let run_id = Uuid::now_v7();
  let turn_id = ctx.session.id();
  let user_msg = MessageBuilder::new()
    .with_run_id(run_id)
    .with_turn_id(turn_id)
    .user_prompt("hi".to_string());
  ctx.session.add_user_prompt(user_msg);

  let req = AgentRequest::builder()
    .agent(agent_ref)
    .run_id(run_id)
    .stream(true)
    .build();

  // Execute; we get a stream directly
  let mut stream = Local.execute(ctx, req).await;
  // First event should be the error produced by the validation
  let first = stream.next().await.expect("expected one event");
  match first {
    Err(e) => panic!(
      "expected a StreamEvent::Error yielded, got underlying error: {:?}",
      e
    ),
    Ok(evt) => match evt {
      crate::events::StreamEvent::Error(err_evt) => {
        assert!(
          err_evt.err.contains("agent model provider cannot be empty"),
          "unexpected error message: {}",
          err_evt.err
        );
      }
      other => panic!("expected error event, got {:?}", other),
    },
  }
}

#[tokio::test]
async fn max_turns_exceeded_when_no_final_response() -> eyre::Result<()> {
  // We force max_turns = 0 so loop condition fails immediately and we get the max turns error
  // Note: add minimal valid provider so the executor reaches the loop edge case
  let provider_cfg = make_provider_config();

  let mut ctx = ExecutionContext::new();
  register_openai(&ctx, &provider_cfg);

  let agent_ref = "it/max-turns";
  let agent = minimal_agent(agent_ref);
  ctx.register_agent(agent_ref.into(), agent);

  let run_id = Uuid::now_v7();
  let turn_id = ctx.session.id();
  ctx.session.add_user_prompt(
    MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .user_prompt("hi".to_string()),
  );

  let req = AgentRequest::builder()
    .agent(agent_ref)
    .run_id(run_id)
    .stream(true)
    .max_turns(0) // force immediate exceed
    .build();

  let stream = Local.execute(ctx, req).await;
  let result = drain_events_to_vec(stream).await;

  // When max turns is 0, the executor immediately errors with "Max turns (0) exceeded"
  match result {
    Ok(events) => {
      // The stream yields a single underlying error (from try_stream Err) not wrapped in StreamEvent::Error
      // We therefore expect the vector to be empty (no events) and the outer result to be Err.
      // If we get Ok(events), ensure it's empty (no yielded events) and treat that as unexpected success.
      assert!(
        events.is_empty(),
        "expected no events when exceeding max turns immediately, got: {:?}",
        events
      );
      // Force failure to indicate it didn't error as expected
      panic!("expected error for max turns exceeded, but stream completed ok");
    }
    Err(e) => {
      let msg = e.to_string();
      assert!(
        msg.contains("Max turns (0) exceeded"),
        "unexpected error: {}",
        msg
      );
    }
  }

  Ok(())
}
