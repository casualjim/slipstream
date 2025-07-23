mod openailike;

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::{CompletionParams, events::StreamEvent};
use eyre::Result;

#[async_trait]
pub trait Completer {
  async fn chat_completion<'a>(&self, params: CompletionParams<'a>) -> ResultStream<StreamEvent>;
}
