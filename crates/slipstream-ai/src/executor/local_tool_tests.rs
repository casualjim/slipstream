#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use crate::{
  Agent, AgentRequest, CompleterConfig, DefaultAgent, ExecutionContext, Executor, Local,
  OpenAILikeCompleter, ProviderConfig, agent::AgentTool, events::StreamEvent,
};
use futures::StreamExt;
use slipstream_core::messages::{MessageBuilder, Response, ToolCallMessage};
use slipstream_metadata::Provider;
use uuid::Uuid;

// NOTE: These tests use the real OpenAI provider and expect OPENAI_API_KEY to be set.
// The prompts strongly instruct the model to emit tool calls first via the provided function schema.

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

fn tool_enabled_agent(_agent_ref: &str, tools: Vec<&'static AgentTool>) -> Arc<dyn Agent> {
  // DefaultAgent doesn’t accept tools directly; provide a strong instruction to call the tool.
  let tool_hint = if let Some(t) = tools.first() {
    format!(
      "You MUST call the {} tool first with its documented arguments {:?} before answering.",
      t.name, t.arguments
    )
  } else {
    "You MUST call the required tool before answering.".to_string()
  };

  Arc::new(
    DefaultAgent::builder()
      .name("it/tool-agent/0.0.1")
      .instructions(tool_hint)
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

// Regular tool: returns a JSON-serializable string result
static REGULAR_TOOL: std::sync::OnceLock<AgentTool> = std::sync::OnceLock::new();
// Agent transfer tool: signals handoff by name; our Agent::call_tool for DefaultAgent returns Left,
// so to simulate transfer we will create a second agent and instruct the model to call only the regular tool.
// The executor performs transfer only when Agent::call_tool returns Either::Right(new_agent).
// Since DefaultAgent cannot do that, we cover ordering and tool-response assertions here;
// agent-handoff will be handled in a separate test where we build a custom Agent impl if needed.

#[tokio::test]
async fn streaming_toolcall_then_assistant_regular_tool() -> eyre::Result<()> {
  let provider_cfg = make_provider_config();

  // Define a simple tool schema with one required argument "question"
  let tool = REGULAR_TOOL.get_or_init(|| AgentTool {
    name: "lookup".to_string(),
    description: "Perform a lookup; returns a short string result".to_string(),
    arguments: vec!["question".to_string()],
    schema: schemars::schema_for!(ToolArgs),
    handler: None, // model will call it; DefaultAgent::call_tool returns a JSON value string
  });

  let mut ctx = ExecutionContext::new();
  register_openai(&ctx, &provider_cfg);

  // Use a valid AgentRef slug/version ("slug/version")
  let agent_ref = "it/tool-agent/0.0.1";
  let agent = tool_enabled_agent(agent_ref, vec![tool]);
  ctx.register_agent(agent_ref.into(), agent);

  // Prompt that pushes tool usage
  let run_id = Uuid::now_v7();
  let turn_id = ctx.session.id();
  ctx.session.add_user_prompt(
    MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .user_prompt(
        "Use the lookup tool with question='what is 2+2', then after tool responses, answer with \
         exactly: 4"
          .to_string(),
      ),
  );

  // Build a request using the exact same valid AgentRef used in register_agent
  let req = AgentRequest::builder()
    .agent(agent_ref)
    .run_id(run_id)
    .stream(true)
    .max_turns(2)
    .build();

  let mut stream = Local.execute(ctx, req).await;

  // We expect a sequence including: Delim, some Chunks, a Response with ToolCallMessage,
  // and a final Assistant Response.
  let mut saw_tool_response = false;
  let mut saw_assistant_final = false;

  while let Some(evt) = stream.next().await {
    let evt = evt?;
    match evt {
      StreamEvent::Chunk(_) => {
        // ignore
      }
      StreamEvent::Delim(_) => { /* ignore */ }
      StreamEvent::Error(e) => panic!("stream error: {}", e.err),
      StreamEvent::Response(resp) => match resp.response {
        Response::ToolCall(ToolCallMessage { tool_calls }) => {
          // Expect at least one tool call referencing our tool
          assert!(
            tool_calls.iter().any(|tc| tc.name == "lookup"),
            "expected a tool call to 'lookup', got {:?}",
            tool_calls
          );
          saw_tool_response = true;
        }
        Response::Assistant(msg) => {
          let text = match &msg.content {
            Some(slipstream_core::messages::AssistantContentOrParts::Content(t)) => t.trim(),
            _ => "",
          };
          if !text.is_empty() {
            saw_assistant_final = true;
            // Accept either exactly "4" or minimal variation
            assert!(
              text == "4" || text.trim_matches('\"') == "4",
              "expected final answer '4', got {:?}",
              text
            );
            break;
          }
        }
      },
    }
  }

  assert!(
    saw_tool_response,
    "expected at least one tool call response"
  );
  assert!(saw_assistant_final, "expected a final assistant response");

  Ok(())
}

#[derive(serde::Serialize, schemars::JsonSchema)]
#[schemars(title = "ToolArgs")]
struct ToolArgs {
  question: String,
}
