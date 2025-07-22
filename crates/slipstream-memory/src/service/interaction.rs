use std::sync::Arc;

use slipstream_store::Database;

pub struct InteractionService {
  db: Arc<Database>,
}

impl InteractionService {
  pub fn new(db: Arc<Database>) -> Self {
    Self { db }
  }
}
