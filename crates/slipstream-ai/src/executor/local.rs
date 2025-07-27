use std::sync::Arc;

use async_trait::async_trait;
use slipstream_core::messages::Aggregator;
use uuid::Uuid;
use validator::Validate as _;

use crate::agent::Agent;
use crate::executor::{AgentRequest, AgentResponse, Executor};
use crate::{CompletionParams, Error, Result, completer::Completer};

pub struct Local {
  providers: dashmap::DashMap<String, Arc<dyn Completer>>,
}

#[async_trait]
impl Executor for Local {
  async fn execute(&self, mut params: AgentRequest) -> AgentResponse {
    params.validate()?;

    let thread = params.session.fork();
    let active_agent = params.agent.clone();

    let provider = &params.agent.model().provider;

    let completer = self
      .providers
      .get(provider)
      .map(|c| c.clone())
      .ok_or_else(|| Error::UnknownProvider(provider.clone()))?;

    let reactor = ReactorLoop {
      active_agent: Some(active_agent),
      completer,
      thread,
    };
    match reactor.run().await {
      Ok((session, result)) => {
        params.session.join(&session);
        Ok(result)
      }
      Err(e) => Err(e),
    }
  }
}

struct ReactorLoop {
  active_agent: Option<Arc<dyn Agent>>,
  completer: Arc<dyn Completer>,
  thread: Aggregator,
}

impl ReactorLoop {
  async fn run(mut self) -> Result<(Aggregator, String)> {
    // Implementation of the reactor loop logic
    let result = String::new();

    while self.thread.turn_len() < 100 && self.active_agent.is_some() {
      let run_id = Uuid::now_v7();
      let agent = self.active_agent.take().unwrap();
      let _stream = self.completer.chat_completion(CompletionParams {
        run_id,
        agent,
        session: &mut self.thread,
        tool_choice: None,
        stream: false,
      });
    }

    Ok((self.thread, result))
  }
}
