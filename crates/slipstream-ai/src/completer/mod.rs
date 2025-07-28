mod openailike;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::{CompletionParams, ResultStream, events::StreamEvent};
pub use openailike::OpenAILikeCompleter;

#[derive(Debug, Clone, TypedBuilder, Default, Serialize, Deserialize)]
pub struct StructuredOutput {
  pub name: String,
  pub description: String,
  pub parameters: schemars::Schema,
}

#[async_trait]
pub trait Completer: Send + Sync {
  async fn chat_completion<'a>(&self, params: CompletionParams<'a>) -> ResultStream<StreamEvent>;
}
