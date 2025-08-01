use eyre::Result;
use futures::StreamExt as _;
use slipstream_ai::{Engine, Prompt, ProviderConfig, StreamEvent};
use slipstream_metadata::Provider;

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt().with_env_filter("debug").init();

  tracing::info!("running basic/minimal example");

  let api_key =
    std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY environment variable must be set");

  let engine = Engine::default();

  // Create provider config (equivalent to openai.GPT4oMini())
  let provider_config = ProviderConfig::builder()
    .name("openai".to_string())
    .api_key(api_key)
    .build();

  engine
    .register_provider(Provider::OpenAI, provider_config.clone())
    .await?;

  // // Create completer config (equivalent to openai.GPT4oMini())
  // let completer_config = CompleterConfig::builder()
  //   .provider("openai".to_string())
  //   .model("gpt-4.1-nano".to_string())
  //   .temperature(0.2)
  //   .build();

  // // Create the minimal agent (equivalent to Go's minimalAgent)
  // let minimal_agent = Arc::new(
  //   DefaultAgent::builder()
  //     .name(agent_ref.clone())
  //     .instructions("You are a helpful assistant".to_string())
  //     .model(
  //       CompleterConfig::builder()
  //         .provider("openai".to_string())
  //         .model("gpt-4.1-nano".to_string())
  //         .temperature(0.2)
  //         .build(),
  //     )
  //     .build(),
  // ) as Arc<dyn Agent>;

  // // Create the OpenAI completer
  // let completer = Arc::new(OpenAILikeCompleter::new(&provider_config));

  let mut stream = engine
    .execute(
      "minimal-agent/0.0.1",
      Prompt::new(
        "What is the answer to the ultimate question of life, the universe, and everything?",
      )?
      .max_turns(5)
      .sender("basic-minimal-local"),
    )
    .await?;

  // Drain the stream so the executor can process events and drive control flow.
  while let Some(evt) = stream.next().await {
    match evt {
      Ok(StreamEvent::Delim(_)) => {}
      Ok(StreamEvent::Chunk(_)) => {}
      Ok(StreamEvent::Response(_)) => {
        // The executor will join the session and return once a final assistant response is processed.
        // We still keep draining until exhaustion to be explicit.
      }
      Ok(StreamEvent::Error(e)) => {
        tracing::error!("Stream error event: {}", e.err);
        break;
      }
      Err(e) => {
        tracing::error!("Underlying stream error: {:?}", e);
        break;
      }
    }
  }

  tracing::info!("Agent execution completed");

  Ok(())
}
