use crate::messages;
use async_openai::config::OpenAIConfig;
use async_trait::async_trait;

use crate::{Completer, Result, completer::config::CompleterConfig};

pub struct OpenAILikeCompleter {
  client: async_openai::Client<OpenAIConfig>,
}

impl OpenAILikeCompleter {
  pub fn new(config: &CompleterConfig) -> Self {
    let mut openai_config = OpenAIConfig::default().with_api_key(config.api_key.clone());
    if let Some(base_url) = &config.base_url {
      openai_config = openai_config.with_api_base(base_url.clone());
    }

    let client = async_openai::Client::with_config(openai_config);
    Self { client }
  }
}

#[async_trait]
impl Completer for OpenAILikeCompleter {
  async fn complete(
    &self,
    _prompt: &str,
  ) -> Result<Vec<messages::MessageEnvelope<messages::ModelMessage>>> {
    Ok(vec![])
  }
}
