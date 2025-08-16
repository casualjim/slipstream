mod config;
// Clean database module implementation
// This module provides a clean actor-based database interface with proper separation of concerns
mod database;

// moved to src/database/
mod embeddings;
mod error;
mod graph;
// moved to src/database/
mod meta;
mod operations;
mod ordered;
mod streams;
mod traits;

mod queries;
// Transformers are exported so we can retain the naming
pub mod transformers;

pub use database::Database;
pub use error::{Error, Result};

// Re-export key traits and types for external use
pub use operations::{
  DatabaseOperation, MutationOperation, NoData, PrimaryStoreQuery, QueryOperation,
};
pub use traits::{CommandExecutor, DatabaseCommand, FromRecordBatchRow, ToDatabase};

// Internal re-exports for tests
#[cfg(test)]
pub use queries::{empty_filter, extract_uuids_to_in_clause};
pub use streams::ResultStream;

// Test module with common initialization
#[cfg(test)]
mod tests {
  use ctor::ctor;
  use tracing_subscriber::prelude::*;

  // Initialize pretty test logging and backtraces once per process.
  #[ctor]
  fn init_logging() {
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let subscriber = tracing_subscriber::fmt::layer()
      .pretty()
      .with_test_writer()
      .with_filter(env_filter);

    let _ = tracing_subscriber::registry().with(subscriber).try_init();
    let _ = std::panic::catch_unwind(|| color_backtrace::install());
  }
}

// Internal use of our embedding function
use embeddings::create_embedding_function_from_config;

use std::sync::Arc;

pub use crate::config::Config;

  
