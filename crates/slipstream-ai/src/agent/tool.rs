use super::Agent;
use crate::Result;
use async_trait::async_trait;
use either::Either;
use serde::{Deserialize, Serialize};
use slipstream_metadata::RestateType;
use std::{fmt::Debug, marker::PhantomData, pin::Pin, sync::Arc};
use tokio::runtime;
use typed_builder::TypedBuilder;

#[async_trait]
pub trait AgentToolHandler: Debug + Send + Sync + 'static {
  type Input: serde::de::DeserializeOwned;
  type Output: serde::Serialize;

  async fn handle(&self, args: Self::Input) -> Result<Either<Self::Output, Arc<dyn Agent>>>;

  fn handle_blocking(&self, args: Self::Input) -> Result<Either<Self::Output, Arc<dyn Agent>>> {
    runtime::Builder::new_current_thread()
      .build()?
      .block_on(self.handle(args))
  }
}

// Private module to seal our traits
mod sealed {
  use super::*;

  pub trait Sealed: Send + Sync + 'static {}

  // Value variant - only implementable for Serialize types
  pub struct ValueReturn<T>(pub(crate) T);
  // Agent variant - only implementable for Arc<dyn Agent>
  pub struct AgentReturn(pub(crate) Arc<dyn Agent>);

  impl<T: serde::Serialize + Send + Sync + 'static> Sealed for ValueReturn<T> {}
  impl Sealed for AgentReturn {}
}
// Marker trait for function return types
pub trait ReturnType: sealed::Sealed {
  fn into_either(self) -> Result<Either<serde_json::Value, Arc<dyn Agent>>>;
}

// Implementation for regular serializable values
impl<T: serde::Serialize + Send + Sync + 'static> ReturnType for sealed::ValueReturn<T> {
  fn into_either(self) -> Result<Either<serde_json::Value, Arc<dyn Agent>>> {
    Ok(Either::Left(serde_json::to_value(self.0)?))
  }
}

// Implementation for Agent types
impl ReturnType for sealed::AgentReturn {
  fn into_either(self) -> Result<Either<serde_json::Value, Arc<dyn Agent>>> {
    Ok(Either::Right(self.0))
  }
}

pub struct ToolWrapper<F, R> {
  function: F,
  _phantom: PhantomData<R>,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

// Helper functions with conversion built in
pub fn wrap_value_fn<F, Fut, T>(
  f: F,
) -> ToolWrapper<
  impl Fn(serde_json::Value) -> BoxFuture<Result<sealed::ValueReturn<T>>> + Send + Sync,
  sealed::ValueReturn<T>,
>
where
  F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Result<T>> + Send + 'static,
  T: serde::Serialize + Send + 'static,
{
  let wrapped = move |args| {
    let fut = f(args);
    Box::pin(async move {
      let res = fut.await?;
      Ok(sealed::ValueReturn(res))
    }) as BoxFuture<_>
  };
  ToolWrapper::new(wrapped)
}

pub fn wrap_agent_fn<F, Fut>(
  f: F,
) -> ToolWrapper<
  impl Fn(serde_json::Value) -> BoxFuture<Result<sealed::AgentReturn>> + Send + Sync,
  sealed::AgentReturn,
>
where
  F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Result<Arc<dyn Agent>>> + Send + 'static,
{
  let wrapped = move |args| {
    let fut = f(args);
    Box::pin(async move {
      let res = fut.await?;
      Ok(sealed::AgentReturn(res))
    }) as BoxFuture<_>
  };
  ToolWrapper::new(wrapped)
}

impl<F, R> ToolWrapper<F, R> {
  pub fn new(function: F) -> Self {
    ToolWrapper {
      function,
      _phantom: PhantomData,
    }
  }
}

#[async_trait]
impl<F, R> AgentToolHandler for ToolWrapper<F, R>
where
  F: Fn(serde_json::Value) -> BoxFuture<Result<R>> + Send + Sync + 'static,
  R: ReturnType + Send + 'static,
{
  type Input = serde_json::Value;
  type Output = serde_json::Value;

  async fn handle(&self, args: Self::Input) -> Result<Either<Self::Output, Arc<dyn Agent>>> {
    let res = (self.function)(args).await?;
    res.into_either()
  }
}

impl<F, R> Debug for ToolWrapper<F, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FunctionWrapper").finish()
  }
}

#[derive(Debug, TypedBuilder, Serialize, Deserialize)]
pub struct AgentTool {
  pub name: String,
  pub description: String,
  pub arguments: Vec<String>,
  pub schema: schemars::Schema,
  #[builder(default)]
  #[serde(skip)]
  pub handler:
    Option<Box<dyn AgentToolHandler<Input = serde_json::Value, Output = serde_json::Value>>>,
}

impl Default for AgentTool {
  fn default() -> Self {
    AgentTool {
      name: String::default(),
      description: String::default(),
      arguments: Vec::default(),
      schema: schemars::json_schema!(true),
      handler: None,
    }
  }
}

impl Clone for AgentTool {
  fn clone(&self) -> Self {
    AgentTool {
      name: self.name.clone(),
      description: self.description.clone(),
      arguments: self.arguments.clone(),
      schema: self.schema.clone(),
      handler: None,
    }
  }
}

pub struct McpTool {
  pub name: String,
  pub description: String,
  pub arguments: Vec<String>,
  pub schema: schemars::Schema,
}

pub struct RestateTool {
  pub name: String,
  pub service_name: String,
  pub service_type: RestateType,
  pub description: String,
  pub schema: schemars::Schema,
  pub formatted_name: String,
}

pub struct ClientTool {
  pub name: String,
  pub description: String,
  pub arguments: Vec<String>,
  pub schema: schemars::Schema,
}
