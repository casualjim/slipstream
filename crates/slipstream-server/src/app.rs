#[derive(Clone)]
pub struct AppState {}

impl AppState {
  pub async fn new() -> eyre::Result<Self> {
    Ok(Self {})
  }
}

impl AppState {}
