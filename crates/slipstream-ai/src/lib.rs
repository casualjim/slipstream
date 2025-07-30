mod agent;
mod completer;
mod embedder;
mod engine;
mod error;
mod events;
mod executor;
mod reranker;

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
pub use error::{Error, Result};
use futures::Stream;
use secrecy::SecretString;

use slipstream_core::messages::{self, Aggregator};
use typed_builder::TypedBuilder;
use uuid::Uuid;

pub use agent::*;
pub use completer::OpenAILikeCompleter;
pub use engine::Engine;
pub use events::*;
pub use executor::{AgentRequest, ExecutionContext, Executor, Local};

pub type DefaultStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// Type alias for the most common case - a stream of Results
pub type ResultStream<T> = DefaultStream<Result<T>>;

#[derive(Debug, Clone, Default, TypedBuilder)]
pub struct ProviderConfig {
  pub name: String,
  #[builder(setter(into))]
  pub api_key: SecretString,
  #[builder(default, setter(into))]
  pub base_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub enum ReasoningEffort {
  #[default]
  None,
  Low,
  Medium,
  High,
}

#[derive(Debug, Clone, Default, TypedBuilder)]
pub struct CompleterConfig {
  #[builder(setter(into))]
  pub provider: String,
  #[builder(setter(into))]
  pub model: String,
  #[builder(default = 0.2, setter(into))]
  pub temperature: f32,
  #[builder(default, setter(into))]
  pub max_tokens: Option<u32>,
  #[builder(default = 0.3, setter(into))]
  pub top_p: f32,
  #[builder(default = 1, setter(into))]
  pub n: u8,
  #[builder(default, setter(into))]
  pub reasoning_effort: ReasoningEffort,
}

pub struct CompletionParams<'a> {
  pub run_id: Uuid,
  pub agent: Arc<dyn Agent>,
  pub session: &'a mut Aggregator,
  pub tool_choice: Option<String>,
  pub stream: bool,
}

#[async_trait]
pub trait Completer: Send + Sync {
  async fn complete<'a>(&self, params: CompletionParams<'a>) -> Result<()>;
}
