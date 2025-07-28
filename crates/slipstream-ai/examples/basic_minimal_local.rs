use std::sync::Arc;

use eyre::Result;
use slipstream_ai::{
  Agent, AgentRequest, CompleterConfig, DefaultAgent, Executor, Local, OpenAILikeCompleter,
  ProviderConfig, StreamEvent,
};
use slipstream_core::{
  definitions::Provider,
  messages::{Aggregator, Erasable, MessageBuilder},
};
use tokio::sync::broadcast;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize logging (similar to Go example's zerolog setup)
  tracing_subscriber::fmt().with_env_filter("debug").init();

  tracing::info!("running basic/minimal example");

  // Get API key from environment (similar to Go's godotenv autoload)
  let api_key =
    std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY environment variable must be set");

  // Create provider config (equivalent to openai.GPT4oMini())
  let provider_config = ProviderConfig::builder()
    .name("openai".to_string())
    .api_key(api_key)
    .build();

  // Create completer config (equivalent to openai.GPT4oMini())
  let completer_config = CompleterConfig::builder()
    .provider("openai".to_string())
    .model("gpt-4.1-nano".to_string())
    .temperature(0.2)
    .build();

  // Create the minimal agent (equivalent to Go's minimalAgent)
  let minimal_agent = Arc::new(
    DefaultAgent::builder()
      .name("minimal-agent".to_string())
      .instructions("You are a helpful assistant".to_string())
      .model(completer_config)
      .build(),
  ) as Arc<dyn Agent>;

  // Create the OpenAI completer
  let completer = Arc::new(OpenAILikeCompleter::new(&provider_config));

  // Create the local executor (equivalent to bubo.Local())
  let executor = Local::new().with_provider(Provider::OpenAI, completer);

  // Create a session aggregator and add the user message
  let mut session = Aggregator::new();
  let user_message = MessageBuilder::new()
    .with_run_id(Uuid::now_v7())
    .with_turn_id(session.id())
    .user_prompt(
      "What is the answer to the ultimate question of life, the universe, and everything?"
        .to_string(),
    );

  session.add_message(user_message.erase());

  // Create a broadcast channel for streaming events
  let (sender, mut receiver) = broadcast::channel::<StreamEvent>(1000);

  // Create the agent request
  let request = AgentRequest::builder()
    .agent(minimal_agent)
    .session(session)
    .stream(true)
    .sender(sender)
    .build();

  // Spawn a task to handle streaming events (equivalent to msgfmt.Console)
  let stream_handle = tokio::spawn(async move {
    let mut final_result = String::new();

    while let Ok(event) = receiver.recv().await {
      match event {
        StreamEvent::Delim(delim) => {
          tracing::debug!("Delimiter: {}", delim.delim);
        }
        StreamEvent::Chunk(chunk) => {
          // Extract and print chunk content as it streams
          if let slipstream_core::messages::Response::Assistant(ref assistant) = chunk.chunk {
            if let Some(slipstream_core::messages::AssistantContentOrParts::Content(ref text)) =
              assistant.content
            {
              print!("{}", text);
              final_result.push_str(text);
            }
          }
        }
        StreamEvent::Response(response) => {
          // Final response received
          if let slipstream_core::messages::Response::Assistant(ref assistant) = response.response {
            if let Some(slipstream_core::messages::AssistantContentOrParts::Content(ref text)) =
              assistant.content
            {
              if !text.is_empty() && !final_result.ends_with(text) {
                println!("{}", text);
                final_result = text.clone();
              }
            }
          }
          break;
        }
        StreamEvent::Error(error) => {
          tracing::error!("Stream error: {}", error.err);
          break;
        }
      }
    }

    final_result
  });

  // Execute the agent request (equivalent to p.Run())
  match executor.execute(request).await {
    Ok(result) => {
      tracing::info!("Agent execution completed successfully");

      // Wait for streaming to complete and get the final result
      let streamed_result = stream_handle.await?;

      // Print final result if we didn't get it from streaming
      if !streamed_result.is_empty() {
        println!("\nFinal result: {}", result);
      } else {
        println!("{}", result);
      }
    }
    Err(err) => {
      tracing::error!("Failed to execute agent: {:?}", err);
      return Err(err);
    }
  }

  Ok(())
}
