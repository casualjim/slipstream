mod conversions;

use std::sync::Arc;

use crate::{CompletionParams, Error, ProviderConfig, agent::Agent, messages};
use async_openai::{
  Client,
  config::OpenAIConfig,
  types::{
    ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequest,
    CreateChatCompletionRequestArgs, FunctionObject, ResponseFormat, ResponseFormatJsonSchema,
  },
};
use async_trait::async_trait;
use secrecy::ExposeSecret;

use crate::{Completer, Result};

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
    let mut messages = vec![];
    for message in &session.messages() {
      messages.push(conversions::message_to_openai(message));
    }

    let model = agent.model();
    let mut req = CreateChatCompletionRequestArgs::default()
      .model(&model.model)
      .messages(messages)
      .max_tokens(model.max_tokens)
      .temperature(model.temperature)
      .top_p(model.top_p)
      .n(model.n)
      .build()?;

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
      let metadata = schema.as_value().get("metadata");
      let description = metadata
        .and_then(|m| m.get("description"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

      let title = metadata
        .and_then(|m| m.get("title"))
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
}

#[async_trait]
impl Completer for OpenAILikeCompleter {
  async fn complete<'a>(&self, params: CompletionParams<'a>) -> Result<()> {
    let messages = conversions::messages_to_openai(&params.session.messages());
    let model_config = params.agent.model();

    let request = CreateChatCompletionRequestArgs::default()
      .model(&model_config.model)
      .messages(messages)
      .max_tokens(model_config.max_tokens)
      .temperature(model_config.temperature)
      .top_p(model_config.top_p)
      .n(model_config.n)
      .build()?;

    let response = self.client.chat().create(request).await?;

    response.choices.first().ok_or_else(|| Error::NoChoices)?;

    // for choice in response.choices {
    //   if let Some(message) = choice.message {
    //     params
    //       .sender
    //       .send(Event::Message(messages::Message::from(message)))?;
    //   }
    // }

    Ok(())
  }
}
