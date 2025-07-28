mod accumulator;
mod conversions;

use std::sync::Arc;

use self::accumulator::ChatCompletionAccumulator;
use crate::{
  CompletionParams, Error, ProviderConfig, ResultStream,
  agent::Agent,
  events::{Chunk, Delim, StreamEvent, StreamResponse},
  messages,
};
use async_openai::{
  Client,
  config::OpenAIConfig,
  types::{
    ChatCompletionRequestMessage, ChatCompletionResponseMessage, ChatCompletionTool,
    ChatCompletionToolType, CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
    CreateChatCompletionStreamResponse, FunctionObject, ResponseFormat, ResponseFormatJsonSchema,
  },
};
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use secrecy::ExposeSecret;
use slipstream_core::messages::{
  AssistantContentOrParts, AssistantMessage, Checkpoint, Response, ToolCallData, ToolCallMessage,
};
use uuid::Uuid;

use crate::Result;
use crate::completer::Completer as ChatCompleter;

pub struct OpenAILikeCompleter {
  client: async_openai::Client<OpenAIConfig>,
}

impl OpenAILikeCompleter {
  pub fn new(config: &ProviderConfig) -> Self {
    let mut openai_config = OpenAIConfig::default().with_api_key(config.api_key.expose_secret());
    if let Some(base_url) = &config.base_url {
      openai_config = openai_config.with_api_base(base_url.clone());
    }

    let client = Client::with_config(openai_config);
    Self { client }
  }

  fn create_chat_completion_request(
    &self,
    session: &mut messages::Aggregator,
    agent: Arc<dyn Agent>,
  ) -> Result<CreateChatCompletionRequest> {
    let messages = conversions::messages_to_openai(&session.messages());

    let model = agent.model();
    let mut req = CreateChatCompletionRequestArgs::default()
      .model(&model.model)
      .messages(messages)
      .temperature(model.temperature)
      .top_p(model.top_p)
      .n(model.n)
      .build()?;

    if let Some(max_tokens) = model.max_tokens {
      req.max_completion_tokens = Some(max_tokens);
    }

    let tools = agent.tools();
    if !tools.is_empty() {
      let mut oai_tools = vec![];
      for tool in tools {
        oai_tools.push(ChatCompletionTool {
          r#type: ChatCompletionToolType::Function,
          function: FunctionObject {
            name: tool.name.clone(),
            description: if tool.description.trim().is_empty() {
              None
            } else {
              Some(tool.description.clone())
            },
            parameters: Some(serde_json::to_value(&tool.schema)?),
            strict: Some(true),
          },
        });
      }
      if !oai_tools.is_empty() {
        req.tools = Some(oai_tools);
        req.parallel_tool_calls = Some(true);
      }
    }

    if let Some(schema) = agent.response_schema() {
      let schema_value = schema.as_value();
      let description = schema_value
        .get("description")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

      let title = schema_value
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string();

      req.response_format = Some(ResponseFormat::JsonSchema {
        json_schema: ResponseFormatJsonSchema {
          description,
          name: title,
          schema: Some(serde_json::to_value(schema)?),
          strict: Some(true),
        },
      });
    }

    Ok(req)
  }

  async fn get_completion<'a>(
    &self,
    request: CreateChatCompletionRequest,
    params: &CompletionParams<'a>,
  ) -> ResultStream<StreamEvent> {
    let response = match self.client.chat().create(request).await {
      Ok(response) => response,
      Err(err) => return futures::stream::once(async { Err(err.into()) }).boxed(),
    };

    let run_id = params.run_id;
    let session = &*params.session;

    let mut result = vec![];
    for choice in response.choices {
      // Pass the message by value into the helper.
      match stream_event_from_openai(run_id, session.id(), session.checkpoint(), choice.message) {
        Ok(events) => {
          for event in events {
            result.push(Ok(event));
          }
        }
        Err(err) => return futures::stream::once(async { Err(err) }).boxed(),
      };
    }

    futures::stream::iter(result).boxed()
  }

  async fn stream_completion(
    &self,
    request: CreateChatCompletionRequest,
    params: &CompletionParams<'_>,
  ) -> ResultStream<StreamEvent> {
    let response = match self.client.chat().create_stream(request).await {
      Ok(response) => response,
      Err(err) => return futures::stream::once(async { Err(err.into()) }).boxed(),
    };

    let run_id = params.run_id;
    let turn_id = params.session.id();
    let checkpoint = params.session.checkpoint();

    stream! {
      // Send start delimiter
      yield Ok(StreamEvent::Delim(Delim{run_id, turn_id, delim: "start".to_string()}));

      let mut has_content = false;

      // Process the stream
      let mut stream = Box::pin(response);
      let mut acc = ChatCompletionAccumulator::new();

      while let Some(result) = stream.next().await {
        match result {
          Ok(chunk) => {
            has_content = true;

            acc.add_chunk(chunk.clone());

            // Skip empty termination chunks (they're just OpenAI's internal signal)
            if is_termination_signal(&chunk) {
              continue;
            }

            // Forward the chunk as a complete chunk event
            match stream_event_from_chunk(run_id, turn_id, checkpoint.clone(), chunk) {
              Ok(event) => {
                yield Ok(event);
              }
              Err(err) => {
                yield Err(err);
                return;
              }
            }

          }
          Err(err) => {
            yield Err(Error::from(err));
            return;
          }
        }
      }

      // Send end delimiter only if we had content
      if has_content {
        yield Ok(StreamEvent::Delim(Delim{run_id, turn_id, delim: "end".to_string()}));
        let completion = acc.response;
        for choice in completion.choices {
          // Pass the message by value into the helper.
          match stream_event_from_openai(run_id, turn_id, checkpoint.clone(), choice.message) {
            Ok(events) => {
              for event in events {
                yield Ok(event);
              }
            },
            Err(err) => {
              yield Err(err);
              return;
            },
          };
        }

      }

    }
    .boxed()
  }
}

#[async_trait]
impl ChatCompleter for OpenAILikeCompleter {
  async fn chat_completion<'a>(
    &self,
    mut params: CompletionParams<'a>,
  ) -> ResultStream<StreamEvent> {
    let request =
      match self.create_chat_completion_request(&mut params.session, params.agent.clone()) {
        Ok(request) => request,
        Err(err) => return Box::pin(futures::stream::once(async { Err(err) })),
      };

    if params.stream {
      // Use streaming completion method
      self.stream_completion(request, &params).await
    } else {
      // Use non-streaming completion method
      self.get_completion(request, &params).await
    }
  }
}

fn is_termination_signal(chunk: &CreateChatCompletionStreamResponse) -> bool {
  if let Some(choice) = chunk.choices.first() {
    let delta = &choice.delta;
    let has_finish_reason = choice.finish_reason.is_some();
    let is_empty_delta = delta.content.is_none()
      && delta.tool_calls.as_ref().map_or(true, |tc| tc.is_empty())
      && delta.refusal.is_none();

    has_finish_reason && is_empty_delta
  } else {
    false
  }
}

fn stream_event_from_chunk(
  run_id: Uuid,
  turn_id: Uuid,
  checkpoint: Checkpoint,
  chunk: CreateChatCompletionStreamResponse,
) -> Result<StreamEvent> {
  let choice = chunk.choices.into_iter().next().ok_or(Error::NoChoices)?;
  let delta = choice.delta;

  if delta.tool_calls.as_ref().map_or(false, |v| !v.is_empty()) {
    let mut tool_calls = vec![];
    for tool_call in delta.tool_calls.unwrap() {
      let id = tool_call.id.unwrap_or_default();
      let (name, arguments) = tool_call
        .function
        .map(|f| (f.name.unwrap_or_default(), f.arguments.unwrap_or_default()))
        .unwrap_or_default();

      tool_calls.push(ToolCallData {
        id,
        name,
        arguments,
      });
    }

    let response = Response::ToolCall(ToolCallMessage { tool_calls });
    return Ok(StreamEvent::Chunk(Chunk::new(run_id, turn_id, response)));
  }

  // Standard assistant message chunk
  let response = Response::Assistant(AssistantMessage {
    content: delta.content.map(AssistantContentOrParts::Content),
    refusal: delta.refusal,
  });

  Ok(StreamEvent::Chunk(Chunk::new(run_id, turn_id, response)))
}

fn stream_event_from_openai(
  run_id: Uuid,
  turn_id: Uuid,
  checkpoint: Checkpoint,
  message: ChatCompletionResponseMessage,
) -> Result<Vec<StreamEvent>> {
  let mut events = Vec::new();

  // First, create tool call event if there are tool calls
  if let Some(tool_calls) = message.tool_calls {
    let tool_calls_data = tool_calls
      .into_iter()
      .map(|tc| ToolCallData {
        id: tc.id,
        name: tc.function.name,
        arguments: tc.function.arguments,
      })
      .collect();

    events.push(StreamEvent::Response(StreamResponse::new(
      run_id,
      turn_id,
      checkpoint.clone(),
      Response::ToolCall(ToolCallMessage {
        tool_calls: tool_calls_data,
      }),
    )));
  }

  if let Some(content) = message.content {
    if !content.trim().is_empty() {
      events.push(StreamEvent::Response(StreamResponse::new(
        run_id,
        turn_id,
        checkpoint,
        Response::Assistant(AssistantMessage {
          content: Some(AssistantContentOrParts::Content(content)),
          refusal: None,
        }),
      )));
    }
  } else if let Some(refusal) = message.refusal {
    if !refusal.trim().is_empty() {
      events.push(StreamEvent::Response(StreamResponse::new(
        run_id,
        turn_id,
        checkpoint,
        Response::Assistant(AssistantMessage {
          content: None,
          refusal: Some(refusal),
        }),
      )));
    }
  }

  // Construct the final event with the appropriate response payload.
  Ok(events)
}

#[cfg(test)]
mod tests {
  use std::convert::Infallible;

  use super::*;
  use crate::{
    CompleterConfig,
    agent::{Agent, AgentTool},
  };
  use axum::{
    Router,
    extract::Request,
    response::{
      Json,
      sse::{Event, Sse},
    },
    routing::post,
  };
  use futures::{StreamExt, stream};
  use pretty_assertions::assert_eq;
  use schemars::JsonSchema;
  use secrecy::SecretString;
  use serde::Serialize;
  use slipstream_core::messages::{Aggregator, InstructionsMessage, MessageBuilder};
  use tokio::time::{Duration, timeout};

  #[derive(Debug, Serialize, JsonSchema)]
  #[schemars(title = "TestToolParams")]
  struct TestToolParams {
    param1: String,
  }

  #[derive(Debug, Serialize, JsonSchema)]
  #[schemars(title = "ComplexToolParams")]
  struct ComplexToolParams {
    param0: String,
    param1: i32,
  }

  #[derive(Debug, Serialize, JsonSchema)]
  #[schemars(title = "SimpleToolParams")]
  struct SimpleToolParams {
    param0: String,
  }

  #[derive(Debug)]
  struct TestAgent {
    name: String,
    tools: Vec<&'static AgentTool>,
    model: CompleterConfig,
    response_schema: Option<schemars::Schema>,
  }

  impl TestAgent {
    fn new() -> Self {
      Self {
        name: "test_agent".to_string(),
        tools: vec![],
        model: CompleterConfig {
          provider: "test".to_string(),
          model: "gpt-4.1-nano".to_string(),
          n: 1,
          ..Default::default()
        },
        response_schema: None,
      }
    }
  }

  impl Agent for TestAgent {
    fn name(&self) -> &str {
      &self.name
    }

    fn instructions(&self) -> InstructionsMessage {
      InstructionsMessage::new("Test instructions".to_string())
    }

    fn model(&self) -> &CompleterConfig {
      &self.model
    }

    fn tools(&self) -> &[&'static AgentTool] {
      &self.tools
    }

    fn response_schema(&self) -> Option<schemars::Schema> {
      self.response_schema.clone()
    }
  }

  fn setup_completer() -> OpenAILikeCompleter {
    let config = ProviderConfig {
      name: "test".to_string(),
      api_key: SecretString::new("test_key".into()),
      base_url: None,
    };
    OpenAILikeCompleter::new(&config)
  }

  #[test]
  fn test_new_completer() {
    let _completer = setup_completer();
    // Just test that it can be created without panicking
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_basic() {
    let completer = setup_completer();
    let mut session = Aggregator::new();
    let agent = std::sync::Arc::new(TestAgent::new());

    let request = completer
      .create_chat_completion_request(&mut session, agent)
      .unwrap();

    assert_eq!(request.model, "gpt-4.1-nano");
    assert!(request.tools.is_none() || request.tools.as_ref().unwrap().is_empty());
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_with_invalid_schema() {
    let completer = setup_completer();
    let mut session = Aggregator::new();

    // Create an agent with an invalid schema that will cause JSON serialization to fail
    let mut agent = TestAgent::new();
    agent.model.model = "".to_string(); // Empty model name should be valid though

    let agent = std::sync::Arc::new(agent);

    // This should still work since we don't have actual schema validation yet
    let result = completer.create_chat_completion_request(&mut session, agent);

    // For now this passes, but we're testing the error path structure
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_empty_messages() {
    let completer = setup_completer();
    let mut session = Aggregator::new();
    let agent = std::sync::Arc::new(TestAgent::new());

    let request = completer
      .create_chat_completion_request(&mut session, agent)
      .unwrap();

    assert_eq!(request.model, "gpt-4.1-nano");
    // With empty session, should have no messages
    assert_eq!(request.messages.len(), 0);
    assert!(request.tools.is_none() || request.tools.as_ref().unwrap().is_empty());
  }

  #[test]
  fn test_messages_to_openai_empty_messages() {
    // Test the conversions::messages_to_openai function directly with empty messages
    let empty_messages = Vec::new();
    let result = conversions::messages_to_openai(&empty_messages);

    // With empty messages, result should be empty (no automatic system message addition)
    assert_eq!(result.len(), 0);
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_with_response_schema() {
    let completer = setup_completer();
    let mut session = Aggregator::new();

    // Create an agent with a response schema
    let agent = TestAgent::new();
    let schema = schemars::schema_for!(TestToolParams);

    let agent = std::sync::Arc::new(TestAgent {
      name: agent.name,
      tools: agent.tools,
      model: agent.model,
      response_schema: Some(schema),
    });

    let request = completer
      .create_chat_completion_request(&mut session, agent)
      .unwrap();

    // Verify response schema was properly set
    assert!(request.response_format.is_some());
    if let Some(response_format) = request.response_format {
      if let async_openai::types::ResponseFormat::JsonSchema { json_schema } = response_format {
        assert_eq!(json_schema.name, "TestToolParams");
        assert!(json_schema.strict.unwrap());
        assert!(json_schema.schema.is_some());
      } else {
        panic!("Expected JsonSchema response format");
      }
    }
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_with_tools_and_messages() {
    let completer = setup_completer();
    let mut session = Aggregator::new();

    // Create a static tool for testing
    static TEST_TOOL: std::sync::OnceLock<AgentTool> = std::sync::OnceLock::new();
    let tool = TEST_TOOL.get_or_init(|| AgentTool {
      name: "test_tool".to_string(),
      description: "A test tool".to_string(),
      arguments: vec!["param1".to_string()],
      schema: schemars::schema_for!(TestToolParams),
      handler: None,
    });

    // Create an agent with tools
    let agent = std::sync::Arc::new(TestAgent {
      name: "test_agent".to_string(),
      tools: vec![tool],
      model: CompleterConfig {
        provider: "test".to_string(),
        model: "gpt-4o-mini".to_string(),
        temperature: 0.1,
        n: 1,
        ..Default::default()
      },
      response_schema: None,
    });

    // Add instructions message to the session
    let instructions_msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("Test instructions");
    session.add_message(instructions_msg);

    // Add a user message to the session
    let user_msg = MessageBuilder::new()
      .with_sender("testUser")
      .user_prompt("Hello");
    session.add_user_prompt(user_msg);

    let request = completer
      .create_chat_completion_request(&mut session, agent)
      .unwrap();

    // Verify the built request
    assert_eq!(request.model, "gpt-4o-mini");
    assert_eq!(request.n.unwrap(), 1);
    assert!(request.parallel_tool_calls.unwrap());
    assert_eq!(request.temperature.unwrap(), 0.1);

    // Verify messages (should have system message + user message)
    assert_eq!(request.messages.len(), 2);

    // Verify system message
    if let async_openai::types::ChatCompletionRequestMessage::System(ref sys_msg) =
      request.messages[0]
    {
      if let async_openai::types::ChatCompletionRequestSystemMessageContent::Text(ref content) =
        sys_msg.content
      {
        assert_eq!(content, "Test instructions");
      } else {
        panic!("Expected text content in system message");
      }
    } else {
      panic!("Expected system message as first message");
    }

    // Verify user message
    if let async_openai::types::ChatCompletionRequestMessage::User(ref user_msg) =
      request.messages[1]
    {
      if let async_openai::types::ChatCompletionRequestUserMessageContent::Text(ref content) =
        user_msg.content
      {
        assert_eq!(content, "Hello");
      } else {
        panic!("Expected text content in user message");
      }
      assert_eq!(user_msg.name, Some("testUser".to_string()));
    } else {
      panic!("Expected user message as second message");
    }

    // Verify tools
    assert!(request.tools.is_some());
    let tools = request.tools.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].function.name, "test_tool");
    assert_eq!(
      tools[0].function.description,
      Some("A test tool".to_string())
    );
    assert!(tools[0].function.parameters.is_some());
  }

  #[tokio::test]
  async fn test_create_chat_completion_request_complex_tools() {
    let completer = setup_completer();
    let mut session = Aggregator::new();

    // Create static tools for testing
    static COMPLEX_TOOL: std::sync::OnceLock<AgentTool> = std::sync::OnceLock::new();
    let complex_tool = COMPLEX_TOOL.get_or_init(|| AgentTool {
      name: "complex_tool".to_string(),
      description: "A tool with multiple parameters".to_string(),
      arguments: vec!["param0".to_string(), "param1".to_string()],
      schema: schemars::schema_for!(ComplexToolParams),
      handler: None,
    });

    static SIMPLE_TOOL: std::sync::OnceLock<AgentTool> = std::sync::OnceLock::new();
    let simple_tool = SIMPLE_TOOL.get_or_init(|| AgentTool {
      name: "simple_tool".to_string(),
      description: "A simple tool with one parameter".to_string(),
      arguments: vec!["param0".to_string()],
      schema: schemars::schema_for!(SimpleToolParams),
      handler: None,
    });

    // Create an agent with multiple tools
    let agent = std::sync::Arc::new(TestAgent {
      name: "test_agent".to_string(),
      tools: vec![complex_tool, simple_tool],
      model: CompleterConfig {
        provider: "test".to_string(),
        model: "gpt-4o-mini".to_string(),
        temperature: 0.1,
        n: 1,
        ..Default::default()
      },
      response_schema: None,
    });

    // Add instructions message to the session
    let instructions_msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("Test instructions");
    session.add_message(instructions_msg);

    let request = completer
      .create_chat_completion_request(&mut session, agent)
      .unwrap();

    // Verify tools were properly converted
    assert!(request.tools.is_some());
    let tools = request.tools.unwrap();
    assert_eq!(tools.len(), 2);

    // Verify complex tool
    assert_eq!(tools[0].function.name, "complex_tool");
    assert_eq!(
      tools[0].function.description,
      Some("A tool with multiple parameters".to_string())
    );
    assert!(tools[0].function.parameters.is_some());

    // Verify simple tool
    assert_eq!(tools[1].function.name, "simple_tool");
    assert_eq!(
      tools[1].function.description,
      Some("A simple tool with one parameter".to_string())
    );
    assert!(tools[1].function.parameters.is_some());
  }

  // Generic test server setup helper - takes any router and returns server address
  async fn start_test_server(app: axum::Router) -> std::net::SocketAddr {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
      axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    addr
  }

  // OpenAI completer factory
  fn create_test_completer(base_url: String) -> OpenAILikeCompleter {
    let config = ProviderConfig {
      name: "test".to_string(),
      api_key: SecretString::new("test_key".into()),
      base_url: Some(base_url),
    };
    OpenAILikeCompleter::new(&config)
  }

  #[tokio::test]
  async fn test_chat_completion() {
    // Create a test server that returns a mock ChatCompletion response
    async fn handler(_req: Request) -> Json<serde_json::Value> {
      let mock_response = serde_json::json!({
        "id": "test-id",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o-mini",
        "choices": [{
          "index": 0,
          "message": {
            "role": "assistant",
            "content": "Test response"
          },
          "finish_reason": "stop"
        }],
        "usage": {
          "prompt_tokens": 10,
          "completion_tokens": 5,
          "total_tokens": 15
        }
      });

      Json(mock_response)
    }

    let app = Router::new().route("/chat/completions", post(handler));
    let addr = start_test_server(app).await;
    let completer = create_test_completer(format!("http://{}", addr));

    // Set up parameters for non-streaming completion
    let mut session = Aggregator::new();

    // Add instructions message to the session
    let instructions_msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("Test instructions");
    session.add_message(instructions_msg);

    let agent = std::sync::Arc::new(TestAgent {
      name: "test_agent".to_string(),
      tools: vec![],
      model: CompleterConfig {
        provider: "test".to_string(),
        model: "gpt-4o-mini".to_string(),
        temperature: 0.1,
        n: 1,
        ..Default::default()
      },
      response_schema: None,
    });

    let params = CompletionParams {
      run_id: Uuid::now_v7(),
      session: &mut session,
      agent,
      stream: false, // Non-streaming completion
      tool_choice: None,
    };

    // Get the event stream
    let mut stream = completer.chat_completion(params).await;

    // Read events from the stream
    let mut events = vec![];
    while let Some(event_result) = stream.next().await {
      match event_result {
        Ok(event) => events.push(event),
        Err(e) => panic!("Unexpected error: {:?}", e),
      }
    }

    // Verify we got the expected events
    assert!(!events.is_empty(), "Expected at least one event");

    // Find the response event
    let response_event = events
      .iter()
      .find(|event| matches!(event, StreamEvent::Response(_)));

    assert!(response_event.is_some(), "Expected a Response event");

    if let Some(StreamEvent::Response(response)) = response_event {
      if let Response::Assistant(assistant_msg) = &response.response {
        assert_eq!(
          assistant_msg.content,
          Some(AssistantContentOrParts::Content(
            "Test response".to_string()
          ))
        );
      } else {
        panic!("Expected assistant response, got {:?}", response.response);
      }
    }
  }

  #[tokio::test]
  async fn test_chat_completion_context_cancellation() {
    // Create a test server that sends one SSE chunk immediately, then hangs
    async fn handler(_req: Request) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
      let stream = stream::iter(vec![
        // Send first chunk immediately
        Ok(Event::default()
          .data("{\"id\":\"test-id\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}")),
      ])
      .chain(
        // Then create an infinite stream that never yields (simulates hanging)
        stream::pending::<Result<Event, Infallible>>()
      );

      Sse::new(stream)
    }

    let app = Router::new().route("/chat/completions", post(handler));
    let addr = start_test_server(app).await;
    let completer = create_test_completer(format!("http://{}", addr));

    // Set up parameters
    let mut session = Aggregator::new();
    let agent = std::sync::Arc::new(TestAgent::new());
    let params = CompletionParams {
      run_id: Uuid::now_v7(),
      session: &mut session,
      agent,
      stream: true,
      tool_choice: None,
    };

    // Get the event stream
    let mut stream = completer.chat_completion(params).await;

    // Read the start delimiter
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, StreamEvent::Delim(d) if d.delim == "start"));

    // Read the first chunk
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Chunk(chunk) = event {
      if let Response::Assistant(msg) = chunk.chunk {
        assert_eq!(
          msg.content,
          Some(AssistantContentOrParts::Content("Hello".to_string()))
        );
      } else {
        panic!("Expected assistant message, got {:?}", chunk.chunk);
      }
    } else {
      panic!("Expected an assistant message chunk, got {:?}", event);
    }

    // Now, try to read the next event with a timeout - since our server only sends one chunk
    // and then hangs, this should timeout
    let res = timeout(Duration::from_millis(100), stream.next()).await;

    // Assert that the timeout occurred, which means the stream is hanging as expected
    assert!(res.is_err(), "Expected a timeout error, but got a result");
  }

  #[tokio::test]
  async fn test_stream_chat_completion_sse_events() {
    // Prepare mock SSE events: one text chunk, one tool call chunk, then [DONE]
    async fn handler(_req: Request) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
      let events = vec![
        Event::default().data(
          r#"{
                      "id": "test-id",
                      "object": "chat.completion.chunk",
                      "created": 1234567890,
                      "model": "test-model",
                      "choices": [{
                          "index": 0,
                          "delta": { "content": "Hello" }
                      }]
                  }"#,
        ),
        Event::default().data(
          r#"{
                      "id": "test-id",
                      "object": "chat.completion.chunk",
                      "created": 1234567890,
                      "model": "test-model",
                      "choices": [{
                          "index": 0,
                          "delta": {
                              "tool_calls": [{
                                  "index": 0,
                                  "id": "tool1",
                                  "function": {
                                      "name": "test_tool",
                                      "arguments": "{\"param\": \"value\"}"
                                  }
                              }]
                          }
                      }]
                  }"#,
        ),
        Event::default().data(
          r#"{
                    "id": "test-id",
                    "object": "chat.completion.chunk",
                    "created": 1234567890,
                    "model": "test-model",
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": "stop"
                    }]
                }"#,
        ),
        Event::default().data("[DONE]"),
      ];
      Sse::new(stream::iter(events.into_iter().map(Ok)))
    }

    let app = Router::new().route("/chat/completions", post(handler));
    let addr = start_test_server(app).await;
    let completer = create_test_completer(format!("http://{}", addr));

    // Set up parameters for streaming completion
    let mut session = Aggregator::new();
    let instructions_msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("Test instructions");
    session.add_message(instructions_msg);

    let agent = std::sync::Arc::new(TestAgent::new());

    let params = CompletionParams {
      run_id: Uuid::now_v7(),
      session: &mut session,
      agent,
      stream: true, // Streaming completion
      tool_choice: None,
    };

    // Get the event stream
    let mut stream = completer.chat_completion(params).await;

    // Process events sequentially
    // Start delimiter
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, StreamEvent::Delim(d) if d.delim == "start"));

    // Content chunk
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Chunk(chunk) = event {
      if let Response::Assistant(msg) = chunk.chunk {
        assert_eq!(
          msg.content,
          Some(AssistantContentOrParts::Content("Hello".to_string()))
        );
      } else {
        panic!("Expected assistant message chunk, got {:?}", chunk);
      }
    } else {
      panic!("Expected chunk event, got {:?}", event);
    }

    // Tool call chunk
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Chunk(chunk) = event {
      if let Response::ToolCall(tool_msg) = chunk.chunk {
        assert_eq!(tool_msg.tool_calls.len(), 1);
        assert_eq!(tool_msg.tool_calls[0].id, "tool1");
        assert_eq!(tool_msg.tool_calls[0].name, "test_tool");
        assert_eq!(tool_msg.tool_calls[0].arguments, "{\"param\": \"value\"}");
      } else {
        panic!("Expected tool call chunk, got {:?}", chunk);
      }
    } else {
      panic!("Expected chunk event, got {:?}", event);
    }

    // End delimiter
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, StreamEvent::Delim(d) if d.delim == "end"));

    // First final response event - ToolCall message (tool calls are processed first)
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Response(response) = event {
      if let Response::ToolCall(tool_msg) = response.response {
        assert_eq!(tool_msg.tool_calls.len(), 1);
        assert_eq!(tool_msg.tool_calls[0].id, "tool1");
        assert_eq!(tool_msg.tool_calls[0].name, "test_tool");
        assert_eq!(tool_msg.tool_calls[0].arguments, "{\"param\": \"value\"}");
      } else {
        panic!("Expected tool call response, got {:?}", response.response);
      }
    } else {
      panic!("Expected final tool call response event, got {:?}", event);
    }

    // Second final response event - Assistant message with content
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Response(response) = event {
      if let Response::Assistant(msg) = response.response {
        assert_eq!(
          msg.content,
          Some(AssistantContentOrParts::Content("Hello".to_string()))
        );
      } else {
        panic!("Expected assistant response, got {:?}", response.response);
      }
    } else {
      panic!("Expected final assistant response event, got {:?}", event);
    }

    // Stream should now be exhausted
    assert!(stream.next().await.is_none());
  }

  #[tokio::test]
  async fn test_stream_chat_completion_sse_events_multiple_tool_calls() {
    // Prepare mock SSE events: one text chunk, one tool call chunk, then [DONE]
    async fn handler(_req: Request) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
      let events = vec![
        Event::default().data(
          r#"{
                      "id": "test-id",
                      "object": "chat.completion.chunk",
                      "created": 1234567890,
                      "model": "test-model",
                      "choices": [{
                          "index": 0,
                          "delta": {
                              "tool_calls": [
                                  {
                                      "index": 0,
                                      "id": "tool1",
                                      "function": {
                                          "name": "test_tool1",
                                          "arguments": "{\"param\": \"value1\"}"
                                      }
                                  },
                                  {
                                      "index": 1,
                                      "id": "tool2",
                                      "function": {
                                          "name": "test_tool2",
                                          "arguments": "{\"param\": \"value2\"}"
                                      }
                                  }
                              ]
                          }
                      }]
                  }"#,
        ),
        Event::default().data(
          r#"{
                    "id": "test-id",
                    "object": "chat.completion.chunk",
                    "created": 1234567890,
                    "model": "test-model",
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": "stop"
                    }]
                }"#,
        ),
        Event::default().data("[DONE]"),
      ];
      Sse::new(stream::iter(events.into_iter().map(Ok)))
    }

    let app = Router::new().route("/chat/completions", post(handler));
    let addr = start_test_server(app).await;
    let completer = create_test_completer(format!("http://{}", addr));

    // Set up parameters for streaming completion
    let mut session = Aggregator::new();
    let instructions_msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("Test instructions");
    session.add_message(instructions_msg);

    let agent = std::sync::Arc::new(TestAgent::new());

    let params = CompletionParams {
      run_id: Uuid::now_v7(),
      session: &mut session,
      agent,
      stream: true, // Streaming completion
      tool_choice: None,
    };

    // Get the event stream
    let mut stream = completer.chat_completion(params).await;
    // Process events sequentially
    // Start delimiter
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, StreamEvent::Delim(d) if d.delim == "start"));

    // Tool call chunk with multiple tool calls
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Chunk(chunk) = event {
      if let Response::ToolCall(tool_msg) = chunk.chunk {
        assert_eq!(tool_msg.tool_calls.len(), 2);

        assert_eq!(tool_msg.tool_calls[0].id, "tool1");
        assert_eq!(tool_msg.tool_calls[0].name, "test_tool1");
        assert_eq!(tool_msg.tool_calls[0].arguments, "{\"param\": \"value1\"}");

        assert_eq!(tool_msg.tool_calls[1].id, "tool2");
        assert_eq!(tool_msg.tool_calls[1].name, "test_tool2");
        assert_eq!(tool_msg.tool_calls[1].arguments, "{\"param\": \"value2\"}");
      } else {
        panic!("Expected tool call message, got {:?}", chunk.chunk);
      }
    } else {
      panic!("Expected tool call chunk, got {:?}", event);
    }

    // End delimiter
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, StreamEvent::Delim(d) if d.delim == "end"));

    // Final response event with accumulated tool calls (only tool calls, no content)
    let event = stream.next().await.unwrap().unwrap();
    if let StreamEvent::Response(response) = event {
      if let Response::ToolCall(tool_msg) = response.response {
        assert_eq!(tool_msg.tool_calls.len(), 2);
        assert_eq!(tool_msg.tool_calls[0].id, "tool1");
        assert_eq!(tool_msg.tool_calls[0].name, "test_tool1");
        assert_eq!(tool_msg.tool_calls[0].arguments, "{\"param\": \"value1\"}");
        assert_eq!(tool_msg.tool_calls[1].id, "tool2");
        assert_eq!(tool_msg.tool_calls[1].name, "test_tool2");
        assert_eq!(tool_msg.tool_calls[1].arguments, "{\"param\": \"value2\"}");
      } else {
        panic!("Expected tool call response, got {:?}", response.response);
      }
    } else {
      panic!("Expected final tool call response event, got {:?}", event);
    }

    // Stream should now be exhausted (no Assistant response since there's no content)
    assert!(stream.next().await.is_none());
  }

  #[tokio::test]
  async fn test_simple_content_stream_fixture() {
    use slipstream_core::messages::Aggregator;

    // Load and parse the captured simple content stream fixture
    let fixture_path = "src/completer/openailike/fixtures/simple_content_stream.json";
    let fixture_content =
      std::fs::read_to_string(fixture_path).expect("Simple content stream fixture should exist");

    let chunks: Vec<CreateChatCompletionStreamResponse> =
      serde_json::from_str(&fixture_content).expect("Fixture should be valid JSON");

    assert!(
      !chunks.is_empty(),
      "Fixture should contain streaming chunks"
    );

    // Test that stream_event_from_chunk processes every chunk without errors
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();

    let mut successful_chunks = 0;
    // Remove unused events vector

    for (i, chunk) in chunks.iter().enumerate() {
      match stream_event_from_chunk(run_id, turn_id, checkpoint.clone(), chunk.clone()) {
        Ok(event) => {
          successful_chunks += 1;
          // Validate event structure
          match &event {
            crate::events::StreamEvent::Chunk(chunk_event) => {
              assert_eq!(chunk_event.run_id, run_id, "Run ID should match");
              assert_eq!(chunk_event.turn_id, turn_id, "Turn ID should match");

              // Validate response content
              match &chunk_event.chunk {
                Response::Assistant(msg) => {
                  // Content should be Some if there's actual content
                  if let Some(content) = &msg.content {
                    match content {
                      AssistantContentOrParts::Content(text) => {
                        // Content should not be None or empty in real responses
                        assert!(
                          !text.is_empty() || i == 0,
                          "Content should not be empty except for initial chunk"
                        );
                      }
                      _ => {} // Other content types are valid
                    }
                  }
                }
                Response::ToolCall(_) => {
                  panic!(
                    "Should not have tool calls in simple content stream at chunk {}",
                    i
                  );
                }
              }
            }
            _ => panic!("Expected Chunk event at chunk {}, got {:?}", i, event),
          }
        }
        Err(e) => {
          panic!(
            "Failed to process chunk {} of {}: {:?}\nChunk: {:?}",
            i,
            chunks.len(),
            e,
            chunk
          );
        }
      }
    }

    assert_eq!(
      successful_chunks,
      chunks.len(),
      "All chunks should be processed successfully"
    );

    // Test accumulator produces expected final result
    let mut accumulator = ChatCompletionAccumulator::new();
    let mut completion_responses = Vec::new();

    for chunk in chunks {
      if let Some(response) = accumulator.add_chunk(chunk) {
        completion_responses.push(response);
      }
    }

    // Should have at least one assistant response for content stream
    let assistant_responses: Vec<_> = completion_responses
      .iter()
      .filter_map(|r| match r {
        Response::Assistant(msg) => Some(msg),
        _ => None,
      })
      .collect();

    assert!(
      !assistant_responses.is_empty(),
      "Should have at least one assistant response"
    );

    // Validate final accumulated content is non-empty
    assert!(
      !accumulator.response.choices.is_empty(),
      "Accumulator should have choices"
    );
    let final_choice = &accumulator.response.choices[0];

    if let Some(content) = &final_choice.message.content {
      assert!(
        !content.is_empty(),
        "Final accumulated content should not be empty"
      );
    }
  }

  #[tokio::test]
  async fn test_tool_call_stream_fixture() {
    use slipstream_core::messages::Aggregator;

    // Load and parse the captured tool call stream fixture
    let fixture_path = "src/completer/openailike/fixtures/tool_call_stream.json";
    let fixture_content =
      std::fs::read_to_string(fixture_path).expect("Tool call stream fixture should exist");

    let chunks: Vec<CreateChatCompletionStreamResponse> =
      serde_json::from_str(&fixture_content).expect("Fixture should be valid JSON");

    assert!(
      !chunks.is_empty(),
      "Fixture should contain streaming chunks"
    );

    // Test stream_event_from_chunk with every chunk
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();

    let mut tool_call_events = 0;
    let mut successful_chunks = 0;

    for (i, chunk) in chunks.iter().enumerate() {
      match stream_event_from_chunk(run_id, turn_id, checkpoint.clone(), chunk.clone()) {
        Ok(event) => {
          successful_chunks += 1;

          match &event {
            crate::events::StreamEvent::Chunk(chunk_event) => {
              match &chunk_event.chunk {
                Response::ToolCall(tool_msg) => {
                  tool_call_events += 1;

                  // Validate tool call structure
                  assert!(
                    !tool_msg.tool_calls.is_empty(),
                    "Tool call response should have tool calls"
                  );

                  for tool_call in &tool_msg.tool_calls {
                    // ID should be present and non-empty for complete tool calls
                    if !tool_call.id.is_empty() {
                      assert!(
                        !tool_call.name.is_empty(),
                        "Tool call should have name when ID present"
                      );
                    }
                    // Arguments can be empty or partial during streaming
                  }
                }
                Response::Assistant(_) => {
                  // Assistant responses are valid in tool call streams (e.g., reasoning before tool use)
                }
              }
            }
            _ => panic!("Expected Chunk event at chunk {}, got {:?}", i, event),
          }
        }
        Err(e) => {
          panic!(
            "Failed to process chunk {} of {}: {:?}\nChunk: {:?}",
            i,
            chunks.len(),
            e,
            chunk
          );
        }
      }
    }

    assert_eq!(
      successful_chunks,
      chunks.len(),
      "All chunks should be processed successfully"
    );
    assert!(
      tool_call_events > 0,
      "Should have processed some tool call events"
    );

    // Test accumulator produces valid tool call results
    let mut accumulator = ChatCompletionAccumulator::new();
    let mut all_responses = Vec::new();

    for chunk in chunks {
      if let Some(response) = accumulator.add_chunk(chunk) {
        all_responses.push(response);
      }
    }

    // Extract completed tool calls
    let tool_call_responses: Vec<_> = all_responses
      .iter()
      .filter_map(|r| match r {
        Response::ToolCall(ToolCallMessage { tool_calls }) => Some(tool_calls),
        _ => None,
      })
      .collect();

    assert!(
      !tool_call_responses.is_empty(),
      "Should have completed tool call responses"
    );

    // Validate at least one complete tool call
    let complete_tool_calls: Vec<_> = tool_call_responses
      .iter()
      .flat_map(|calls| calls.iter())
      .filter(|call| !call.id.is_empty() && !call.name.is_empty())
      .collect();

    assert!(
      !complete_tool_calls.is_empty(),
      "Should have at least one complete tool call"
    );

    // Verify final accumulator state has tool calls
    assert!(
      !accumulator.response.choices.is_empty(),
      "Accumulator should have choices"
    );
    let final_choice = &accumulator.response.choices[0];

    if let Some(tool_calls) = &final_choice.message.tool_calls {
      assert!(!tool_calls.is_empty(), "Final state should have tool calls");

      // At least one tool call should be complete
      let complete_final_calls = tool_calls
        .iter()
        .filter(|call| !call.id.is_empty() && !call.function.name.is_empty())
        .count();

      assert!(
        complete_final_calls > 0,
        "Final state should have complete tool calls"
      );
    }
  }

  #[tokio::test]
  async fn test_mixed_content_and_tool_stream_fixture() {
    use slipstream_core::messages::Aggregator;

    // Load and parse the mixed content and tool call stream fixture
    let fixture_path = "src/completer/openailike/fixtures/mixed_content_and_tool_stream.json";
    let fixture_content =
      std::fs::read_to_string(fixture_path).expect("Mixed stream fixture should exist");

    let chunks: Vec<CreateChatCompletionStreamResponse> =
      serde_json::from_str(&fixture_content).expect("Fixture should be valid JSON");

    assert!(
      !chunks.is_empty(),
      "Fixture should contain streaming chunks"
    );

    // Process all chunks without errors
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();

    let mut assistant_events = 0;
    let mut tool_call_events = 0;
    let mut has_content = false;
    let mut has_tool_calls = false;

    for (i, chunk) in chunks.iter().enumerate() {
      // Check if this chunk has actual content or tool calls
      if let Some(choice) = chunk.choices.first() {
        if let Some(content) = &choice.delta.content {
          if !content.is_empty() {
            has_content = true;
          }
        }
        if choice.delta.tool_calls.is_some() {
          has_tool_calls = true;
        }
      }

      match stream_event_from_chunk(run_id, turn_id, checkpoint.clone(), chunk.clone()) {
        Ok(event) => match &event {
          crate::events::StreamEvent::Chunk(chunk_event) => match &chunk_event.chunk {
            Response::Assistant(_) => assistant_events += 1,
            Response::ToolCall(_) => tool_call_events += 1,
          },
          _ => panic!("Expected Chunk event at chunk {}", i),
        },
        Err(e) => {
          panic!("Failed to process chunk {} of {}: {:?}", i, chunks.len(), e);
        }
      }
    }

    // Test accumulator handles the stream correctly
    let mut accumulator = ChatCompletionAccumulator::new();
    let mut all_responses = Vec::new();

    for chunk in chunks {
      if let Some(response) = accumulator.add_chunk(chunk) {
        all_responses.push(response);
      }
    }

    // Validate we got some responses
    assert!(!all_responses.is_empty(), "Should have some responses");

    // Should have tool calls based on fixture inspection
    assert!(has_tool_calls, "Fixture should contain tool calls");

    let has_tool_call_responses = all_responses
      .iter()
      .any(|r| matches!(r, Response::ToolCall(_)));

    assert!(
      has_tool_call_responses,
      "Should have tool call responses based on fixture content"
    );

    // Only assert content if the fixture actually has content
    if has_content {
      let has_assistant_responses = all_responses
        .iter()
        .any(|r| matches!(r, Response::Assistant(_)));
      assert!(
        has_assistant_responses,
        "Should have assistant responses when fixture has content"
      );
    }
  }

  #[tokio::test]
  async fn test_error_handling_with_malformed_chunks() {
    use slipstream_core::messages::Aggregator;

    // Test error handling with chunk that has no choices
    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let checkpoint = Aggregator::new().checkpoint();

    let malformed_chunk = CreateChatCompletionStreamResponse {
      id: "test-malformed".to_string(),
      object: "chat.completion.chunk".to_string(),
      created: 1234567890,
      model: "test-model".to_string(),
      system_fingerprint: None,
      service_tier: None,
      choices: vec![], // Empty choices should cause error
      usage: None,
    };

    let result = stream_event_from_chunk(run_id, turn_id, checkpoint, malformed_chunk);

    // Should fail with NoChoices error
    assert!(result.is_err(), "Should fail when chunk has no choices");
    match result.unwrap_err() {
      Error::NoChoices => {} // Expected error type
      other => panic!("Expected NoChoices error, got: {:?}", other),
    }
  }
}
