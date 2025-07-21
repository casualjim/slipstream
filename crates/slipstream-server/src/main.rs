use slipstream_server::server;
use slipstream_server::server::Config;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("Failed to install AWS LC crypto provider");

  let config = Config::default();

  let (shutdown_token, signal_handle) = create_shutdown_token();

  server::run(config, Some(shutdown_token)).await?;

  signal_handle.abort();
  Ok(())
}

/// Create a cancellation token that triggers on Ctrl+C or SIGTERM
fn create_shutdown_token() -> (CancellationToken, tokio::task::JoinHandle<()>) {
  let token = CancellationToken::new();
  let token_clone = token.clone();

  let handle = tokio::spawn(async move {
    let ctrl_c = async {
      signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
      signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("failed to install signal handler")
        .recv()
        .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
      _ = ctrl_c => {},
      _ = terminate => {},
    }

    info!("Shutdown signal received");
    token_clone.cancel();
  });

  (token, handle)
}
