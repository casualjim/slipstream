mod edges;
mod engine;
mod error;
mod migrations;
mod mutations;
mod nodes;
mod queries;
mod repository;
mod service;

pub use engine::Engine;

#[cfg(test)]
mod tests {
  use ctor::ctor;
  use tracing_subscriber::prelude::*;

  #[ctor]
  fn init_color_backtrace() {
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let subscriber = tracing_subscriber::fmt::layer()
      .pretty()
      .with_test_writer()
      .with_filter(env_filter);

    tracing_subscriber::registry().with(subscriber).init();
    color_backtrace::install();
  }
}
