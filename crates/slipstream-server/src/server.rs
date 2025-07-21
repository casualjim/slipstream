use std::{
  net::{Ipv4Addr, SocketAddr, TcpListener},
  path::PathBuf,
  sync::Arc,
  time::Duration,
};

use aide::scalar::Scalar;
use axum::{
  BoxError, Extension, Router,
  extract::Request as AxumRequest,
  handler::HandlerWithoutStateExt as _,
  response::{Json, Redirect},
  routing::get,
};

use axum_helmet::{Helmet, HelmetLayer};
use axum_otel_metrics::{HttpMetricsLayer, HttpMetricsLayerBuilder};
use axum_server::{Handle, tls_rustls::RustlsConfig};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use futures::StreamExt;
use http::{StatusCode, Uri};
use listenfd::ListenFd;
use rustls::ServerConfig;
use rustls_acme::{AcmeConfig, caches::DirCache};
use tokio::{signal, task::JoinHandle, time::sleep};
use tower_http::compression::CompressionLayer;
use tracing::{debug, error, info};

use crate::app::AppState;

async fn serve_openapi(
  Extension(api): Extension<Arc<aide::openapi::OpenApi>>,
) -> Json<aide::openapi::OpenApi> {
  Json((*api).clone())
}

#[derive(Clone, Copy)]
struct Ports {
  http: u16,
  https: u16,
}

#[derive(Default, Debug)]
pub struct Config {
  /// Enable TLS/HTTPS
  pub tls_enabled: bool,

  /// Path to store LanceDB data
  /// Domains
  pub domains: Vec<String>,

  /// Contact info
  pub email: Vec<String>,

  /// Cache directory
  pub cache: Option<PathBuf>,

  /// Use Let's Encrypt production environment
  /// (see https://letsencrypt.org/docs/staging-environment/)
  pub production: bool,

  /// Use Let's Encrypt production environment
  /// (see https://letsencrypt.org/docs/staging-environment/)
  // #[clap(long, require_equals=true, default_value_t = TlsMode::None, default_missing_value="none", value_parser)]
  // tls_mode: TlsMode,

  /// The private key when tls-mode is keypair
  pub tls_key: Option<PathBuf>,

  /// The public key when tls-mode is keypair
  pub tls_cert: Option<PathBuf>,

  /// The port to listen on for secure traffic
  pub https_port: u16,

  /// The port to listen on for unecrypted traffic
  pub http_port: u16,
}

pub async fn run(
  args: Config,
  shutdown_token: Option<tokio_util::sync::CancellationToken>,
) -> eyre::Result<()> {
  // Extract all needed fields from args first
  let tls_enabled = args.tls_enabled;
  let http_port = args.http_port;
  let https_port = args.https_port;

  // Get TLS config if needed (before moving indexer)
  let tls_config_result = if tls_enabled {
    Some(make_tls_config(&args).await?)
  } else {
    None
  };

  let state = AppState::new();

  let metrics = metrics_layer();

  let handle = Handle::new();
  tokio::spawn(graceful_shutdown(handle.clone(), shutdown_token.clone()));

  aide::generate::infer_responses(true);

  let mut api = crate::routes::openapi();

  let api_routes = crate::routes::router(state.clone()).finish_api(&mut api);

  let app = Router::new()
    .merge(api_routes)
    .route("/api/openapi.json", get(serve_openapi))
    .route(
      "/api/docs",
      Scalar::new("/api/openapi.json").axum_route().into(),
    )
    .layer(HelmetLayer::new(Helmet::default()))
    .layer(OtelInResponseLayer)
    .layer(OtelAxumLayer::default())
    .layer(Extension(Arc::new(api.clone()))) // Arc is important for performance
    .layer(metrics)
    .layer(CompressionLayer::new().quality(tower_http::CompressionLevel::Default))
    .with_state(state);

  // this will enable us to keep application running during recompile: systemfd --no-pid -s http::8080 -s https::8443 -- cargo watch -x run
  let mut listenfd = ListenFd::from_env();

  if let Some((config, _jh)) = tls_config_result {
    // TLS is enabled - set up both HTTP (for redirect) and HTTPS listeners

    let http_listener = listenfd.take_tcp_listener(0)?.unwrap_or_else(|| {
      let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, http_port));
      TcpListener::bind(addr).unwrap_or_else(|_| panic!("failed to bind to {}", addr))
    });
    let https_listener = listenfd.take_tcp_listener(1)?.unwrap_or_else(|| {
      let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, https_port));
      TcpListener::bind(addr).unwrap_or_else(|_| panic!("failed to bind to {}", addr))
    });

    let ports = Ports {
      http: http_port,
      https: https_port,
    };
    // Create a handle for the redirect server
    let redirect_handle = Handle::new();
    // Spawn graceful shutdown for redirect server
    tokio::spawn(graceful_shutdown(
      redirect_handle.clone(),
      shutdown_token.clone(),
    ));
    // Spawn a server to redirect http requests to https
    tokio::spawn(redirect_http_to_https(
      ports,
      http_listener,
      redirect_handle,
    ));

    // run https server
    debug!(
      addr = %https_listener.local_addr().unwrap(),
      "starting HTTPS server",
    );

    let mut server = axum_server::from_tcp_rustls(https_listener, config).handle(handle.clone());
    server.http_builder().http2().enable_connect_protocol();

    // Run the server
    let server_future = server.serve(app.into_make_service_with_connect_info::<SocketAddr>());
    server_future.await?;
  } else {
    // TLS is disabled - run plain HTTP server only
    let http_listener = listenfd.take_tcp_listener(0)?.unwrap_or_else(|| {
      let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, http_port));
      TcpListener::bind(addr).unwrap_or_else(|_| panic!("failed to bind to {}", addr))
    });

    debug!(
      addr = %http_listener.local_addr().unwrap(),
      "starting HTTP server (TLS disabled)",
    );

    let server = axum_server::from_tcp(http_listener)
      .handle(handle.clone())
      .serve(app.into_make_service_with_connect_info::<SocketAddr>());
    server.await?;
  }

  debug!("Server run function completing");
  Ok(())
}

fn metrics_layer() -> HttpMetricsLayer {
  HttpMetricsLayerBuilder::new().build()
}

async fn make_tls_config(args: &Config) -> eyre::Result<(RustlsConfig, JoinHandle<()>)> {
  let (config, mut maybe_state) = match (&args.tls_cert, &args.tls_key) {
    (None, None) => {
      // we're in acme mode
      let state = AcmeConfig::new(args.domains.iter())
        .contact(args.email.iter().map(|e| format!("mailto:{}", e)))
        .cache_option(args.cache.clone().map(DirCache::new))
        .directory_lets_encrypt(args.production)
        .state();

      let tls_config = Arc::new(
        ServerConfig::builder()
          .with_no_client_auth()
          .with_cert_resolver(state.resolver()),
      );
      tracing::debug!("starting server with letsencrypt");

      (RustlsConfig::from_config(tls_config), Some(state))
    }
    (Some(cert_path), Some(key_path)) => {
      tracing::debug!("starting server: key={key_path:?}, cert={cert_path:?}");

      (
        RustlsConfig::from_pem_file(cert_path, key_path)
          .await
          .unwrap(),
        None,
      )
    }
    _ => {
      return Err(eyre::anyhow!(
        "Both --tls-cert and --tls-key must be provided together"
      ));
    }
  };

  let jh = tokio::spawn(async move {
    if maybe_state.is_none() {
      return;
    }
    let mut state = maybe_state.take().unwrap();
    loop {
      match state.next().await.unwrap() {
        Ok(ok) => info!("event: {ok:?}"),
        Err(err) => error!("error: {:?}", err),
      }
    }
  });

  Ok((config, jh))
}

async fn redirect_http_to_https(ports: Ports, listener: TcpListener, handle: Handle) {
  fn make_https(host: String, uri: Uri, ports: Ports) -> Result<Uri, BoxError> {
    let mut parts = uri.into_parts();

    parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

    if parts.path_and_query.is_none() {
      parts.path_and_query = Some("/".parse().unwrap());
    }

    let https_host = host.replace(&ports.http.to_string(), &ports.https.to_string());
    parts.authority = Some(https_host.parse()?);

    Ok(Uri::from_parts(parts)?)
  }

  let redirect = move |request: AxumRequest| async move {
    let host = request.uri().host().unwrap().to_string();
    let uri = request.uri().clone();
    match make_https(host, uri, ports) {
      Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
      Err(error) => {
        tracing::warn!(%error, "failed to convert URI to HTTPS");
        Err(StatusCode::BAD_REQUEST)
      }
    }
  };

  tracing::debug!("listening on {}", listener.local_addr().unwrap());

  let server = axum_server::from_tcp(listener)
    .handle(handle)
    .serve(redirect.into_make_service());

  if let Err(e) = server.await {
    error!("HTTP redirect server error: {}", e);
  }
  info!("HTTP redirect server stopped");
}

async fn graceful_shutdown(
  handle: Handle,
  external_token: Option<tokio_util::sync::CancellationToken>,
) {
  match external_token {
    Some(token) => {
      // Wait only for the external token - signals are handled elsewhere
      token.cancelled().await;
      info!("Shutdown requested via external token");
    }
    None => {
      // Only set up signal handlers if no external token is provided
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
    }
  }

  info!("waiting for connections to close");
  handle.graceful_shutdown(Some(Duration::from_secs(10)));
  loop {
    let count = handle.connection_count();
    if count == 0 {
      break;
    }
    debug!("alive connections count={count}",);
    sleep(Duration::from_secs(1)).await;
  }
  debug!("graceful shutdown complete");
}
