use restate_sdk::prelude::*;
use slipstream_restate::{Greeter, GreeterImpl};

#[tokio::main]
async fn main() -> eyre::Result<()> {
  tracing_subscriber::fmt::init();

  HttpServer::new(Endpoint::builder().bind(GreeterImpl.serve()).build())
    .listen_and_serve("0.0.0.0:9081".parse()?)
    .await;

  Ok(())
}
