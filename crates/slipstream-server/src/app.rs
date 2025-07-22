use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
  memory: Arc<slipstream_memory::Engine>,
}

impl AppState {
  pub async fn new() -> eyre::Result<Self> {
    Ok(Self {
      memory: Arc::new(slipstream_memory::Engine::new()),
    })
  }
}

impl AppState {
  pub fn memory(&self) -> &slipstream_memory::Engine {
    Arc::as_ref(&self.memory)
  }
}
