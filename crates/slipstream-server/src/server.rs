use std::{
  net::{Ipv4Addr, SocketAddr, TcpListener},
  sync::Arc,
  time::Duration,
};

use aide::scalar::Scalar;
use axum::{
  BoxError, Extension, Router,
  extract::Request as AxumRequest,
  handler::HandlerWithoutStateExt as _,
  middleware,
  response::{Json, Redirect},
  routing::get,
};
use limes::{axum::authentication_middleware, jwks::JWKSWebAuthenticator};

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
use tracing::{debug, error, info, warn};

use crate::{app::AppState, config::AppConfig};

async fn serve_openapi(
  Extension(api): Extension<Arc<aide::openapi::OpenApi>>,
) -> Json<aide::openapi::OpenApi> {
  Json((*api).clone())
}

async fn build_app(state: AppState, auth: &crate::config::Auth) -> eyre::Result<Router> {
  // Generate OpenAPI and compose routes
  aide::generate::infer_responses(true);
  let mut api = crate::routes::openapi();
  let protected_routes = crate::routes::router(state.clone()).finish_api(&mut api);

  // Initialize JWKS authenticator from injected config
  let issuer = auth.issuer_url.clone();
  let audience = auth.audience.clone();

  let refresh = if auth.jwks_refresh_seconds == 0 {
    None
  } else {
    Some(std::time::Duration::from_secs(auth.jwks_refresh_seconds))
  };
  let mut jwks = JWKSWebAuthenticator::new(&issuer, refresh)
    .await
    .map_err(|e| eyre::eyre!(format!("failed to initialize JWKS auth: {e}")))?;
  if !audience.is_empty() {
    jwks = jwks.set_accepted_audiences(vec![audience.clone()]);
  }
  let protected_with_auth = protected_routes.layer(middleware::from_fn_with_state(
    jwks,
    authentication_middleware::<JWKSWebAuthenticator>,
  ));

  // Public routes remain accessible
  let public_routes = Router::new()
    .route("/api/v1/openapi.json", get(serve_openapi))
    .route(
      "/api/v1/docs",
      Scalar::new("/api/v1/openapi.json").axum_route().into(),
    );

  // Global layers
  let app = Router::new()
    .merge(public_routes)
    .merge(protected_with_auth)
    .layer(HelmetLayer::new(Helmet::default()))
    .layer(OtelInResponseLayer)
    .layer(OtelAxumLayer::default())
    .layer(Extension(Arc::new(api.clone())))
    .layer(metrics_layer())
    .layer(CompressionLayer::new().quality(tower_http::CompressionLevel::Default))
    .with_state(state);

  Ok(app)
}

fn build_admin_router() -> Router {
  Router::new().route("/healthz", get(|| async { "OK" }))
}

const IDX_HTTP: usize = 0;
const IDX_HTTPS: usize = 1;
const IDX_ADMIN: usize = 2;

#[derive(Clone, Copy)]
struct Ports {
  http: u16,
  https: u16,
}

pub async fn run(
  args: crate::config::AppConfig,
  shutdown_token: Option<tokio_util::sync::CancellationToken>,
) -> eyre::Result<()> {
  // Extract all needed fields from args first
  let tls_enabled = args.server.tls_enabled;
  let http_port = args.server.http_port;
  let https_port = args.server.https_port;

  // Get TLS config if needed (before moving indexer)
  let tls_config_result = if tls_enabled {
    Some(make_tls_config(&args.server).await?)
  } else {
    None
  };

  let state = AppState::new(&args).await?;

  let handle = Handle::new();
  tokio::spawn(graceful_shutdown(handle.clone(), shutdown_token.clone()));

  let app = build_app(state.clone(), &args.auth).await?;

  // Prepare listenfd and start admin server
  let mut listenfd = prepare_listenfd();
  start_admin_server(&mut listenfd, &args, shutdown_token.clone())?;

  if let Some((tls_config, _jh)) = tls_config_result {
    run_tls(
      app,
      &mut listenfd,
      http_port,
      https_port,
      handle.clone(),
      shutdown_token.clone(),
      tls_config,
    )
    .await?;
  } else {
    run_http(app, &mut listenfd, http_port, handle.clone()).await?;
  }

  debug!("Server run function completing");
  Ok(())
}

fn metrics_layer() -> HttpMetricsLayer {
  HttpMetricsLayerBuilder::new().build()
}

fn prepare_listenfd() -> ListenFd {
  ListenFd::from_env()
}

fn acquire_listener(
  listenfd: &mut ListenFd,
  idx: usize,
  port: u16,
  name: &str,
) -> eyre::Result<TcpListener> {
  if let Some(l) = listenfd.take_tcp_listener(idx)? {
    return Ok(l);
  }
  let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
  TcpListener::bind(addr).map_err(|e| eyre::eyre!("failed to bind {name} to {}: {}", addr, e))
}

fn start_admin_server(
  listenfd: &mut ListenFd,
  args: &AppConfig,
  shutdown_token: Option<tokio_util::sync::CancellationToken>,
) -> eyre::Result<()> {
  let listener = acquire_listener(
    listenfd,
    IDX_ADMIN,
    args.server.monitoring_port,
    "monitoring",
  )?;
  let router = build_admin_router();
  spawn_admin_server(listener, router, shutdown_token);
  Ok(())
}

fn spawn_admin_server(
  listener: TcpListener,
  router: Router,
  shutdown_token: Option<tokio_util::sync::CancellationToken>,
) {
  let admin_handle = Handle::new();
  tokio::spawn(graceful_shutdown(admin_handle.clone(), shutdown_token));
  tokio::spawn(async move {
    debug!(addr = %listener.local_addr().unwrap(), "starting admin HTTP server");
    if let Err(e) = axum_server::from_tcp(listener)
      .handle(admin_handle)
      .serve(router.into_make_service())
      .await
    {
      error!("Admin server error: {}", e);
    }
    info!("Admin server stopped");
  });
}

async fn run_tls(
  app: Router,
  listenfd: &mut ListenFd,
  http_port: u16,
  https_port: u16,
  handle: Handle,
  shutdown_token: Option<tokio_util::sync::CancellationToken>,
  tls_config: RustlsConfig,
) -> eyre::Result<()> {
  let http_listener = acquire_listener(listenfd, IDX_HTTP, http_port, "HTTP")?;
  let https_listener = acquire_listener(listenfd, IDX_HTTPS, https_port, "HTTPS")?;

  let ports = Ports {
    http: http_port,
    https: https_port,
  };

  let redirect_handle = Handle::new();
  tokio::spawn(graceful_shutdown(
    redirect_handle.clone(),
    shutdown_token.clone(),
  ));
  tokio::spawn(redirect_http_to_https(
    ports,
    http_listener,
    redirect_handle,
  ));

  debug!(addr = %https_listener.local_addr().unwrap(), "starting HTTPS server");
  let mut server = axum_server::from_tcp_rustls(https_listener, tls_config).handle(handle.clone());
  server.http_builder().http2().enable_connect_protocol();
  server
    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
    .await?;
  Ok(())
}

async fn run_http(
  app: Router,
  listenfd: &mut ListenFd,
  http_port: u16,
  handle: Handle,
) -> eyre::Result<()> {
  let http_listener = acquire_listener(listenfd, IDX_HTTP, http_port, "HTTP")?;
  debug!(addr = %http_listener.local_addr().unwrap(), "starting HTTP server (TLS disabled)");
  axum_server::from_tcp(http_listener)
    .handle(handle.clone())
    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
    .await?;
  Ok(())
}

async fn make_tls_config(
  args: &crate::config::Server,
) -> eyre::Result<(RustlsConfig, JoinHandle<()>)> {
  let (config, maybe_state) = match (&args.tls_cert, &args.tls_key) {
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

      let config = RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .map_err(|e| eyre::eyre!("Failed to load TLS certificates: {}", e))?;

      (config, None)
    }
    _ => {
      return Err(eyre::anyhow!(
        "Both --tls-cert and --tls-key must be provided together"
      ));
    }
  };

  // Clone values for the reloading task before moving into the async block
  let config_clone = config.clone();
  let cert_path = args.tls_cert.clone();
  let key_path = args.tls_key.clone();

  let jh = tokio::spawn(async move {
    // Start periodic reloading task if we're in keypair mode
    if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
      tokio::spawn(async move {
        // Run periodic reload every hour
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));

        // Skip the first tick which happens immediately
        interval.tick().await;

        loop {
          interval.tick().await;

          match config_clone
            .reload_from_pem_file(&cert_path, &key_path)
            .await
          {
            Ok(_) => info!("TLS certificates reloaded successfully"),
            Err(e) => warn!("Failed to reload TLS certificates: {}", e),
          }
        }
      });
    }

    // Handle ACME state if present
    if let Some(mut state) = maybe_state {
      loop {
        match state.next().await.unwrap() {
          Ok(ok) => info!("event: {ok:?}"),
          Err(err) => error!("error: {:?}", err),
        }
      }
    } else {
      // For keypair mode, we need to keep the task alive
      // The reloading is handled in the spawned task above
      futures::future::pending::<()>().await;
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

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::StatusCode;
  use tower::ServiceExt;

  // Stable issuer used in tests (matches application defaults)
  const TEST_ISSUER: &str = "https://wagyu-tunhvc.zitadel.cloud";

  fn test_issuer() -> String {
    TEST_ISSUER.to_string()
  }

  #[derive(serde::Deserialize)]
  struct TestUserJson {
    #[serde(rename = "keyId")]
    key_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    key: String,
  }
  fn load_test_user_key() -> (String, String, String) {
    let raw = include_str!("../test-user.json");
    let user: TestUserJson = serde_json::from_str(raw).expect("failed to parse test-user.json");
    (user.user_id, user.key_id, user.key)
  }

  #[derive(serde::Serialize)]
  struct ClientAssertionClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    jti: String,
    iat: i64,
    exp: i64,
  }

  async fn fetch_token_private_key_jwt(
    issuer: &str,
    client_id: &str,
    key_id: &str,
    private_key_pem: &str,
  ) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use uuid::Uuid;

    let disc_url = format!("{}/.well-known/openid-configuration", issuer);
    let disc_resp = reqwest::Client::new()
      .get(&disc_url)
      .send()
      .await
      .expect("discovery fetch failed");
    let disc_status = disc_resp.status();
    let disc_text = disc_resp
      .text()
      .await
      .unwrap_or_else(|e| format!("<<failed to read discovery body: {e}>>"));
    let disc: serde_json::Value = serde_json::from_str(&disc_text).unwrap_or_else(|e| {
      panic!(
        "discovery parse failed: status={} url={} error={} body={}",
        disc_status, disc_url, e, disc_text
      )
    });
    let token_endpoint = disc
      .get("token_endpoint")
      .and_then(|v| v.as_str())
      .unwrap_or_else(|| {
        panic!(
          "token_endpoint missing in discovery: status={} url={} body={}",
          disc_status, disc_url, disc_text
        )
      });

    // Per ZITADEL docs for JWT bearer grant, use the issuer/custom domain as audience
    let aud = issuer;

    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_secs() as i64;
    let claims = ClientAssertionClaims {
      iss: client_id,
      sub: client_id,
      aud: aud,
      jti: Uuid::now_v7().to_string(),
      iat: now,
      exp: now + 60,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key_id.to_string());
    let enc_key =
      EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).expect("invalid RSA private key PEM");
    let client_assertion =
      encode(&header, &claims, &enc_key).expect("failed to sign client assertion");

    // Use JWT bearer grant per ZITADEL docs: grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer
    const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

    let params = [
      ("grant_type", JWT_BEARER_GRANT),
      ("scope", "openid"),
      ("assertion", client_assertion.as_str()),
    ];

    let resp = reqwest::Client::new()
      .post(token_endpoint)
      .form(&params)
      .send()
      .await
      .expect("token request failed");
    let status = resp.status();
    let text = resp
      .text()
      .await
      .unwrap_or_else(|e| format!("<<failed to read body: {e}>>"));

    // Try parse as JSON; return access_token if present, else panic with full body
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
      if let Some(at) = v.get("access_token").and_then(|x| x.as_str()) {
        return at.to_string();
      }
      let error = v.get("error").and_then(|x| x.as_str()).unwrap_or("<none>");
      let error_description = v
        .get("error_description")
        .and_then(|x| x.as_str())
        .unwrap_or("<none>");
      panic!(
        "access_token missing: status={} token_endpoint={} client_id={} kid={} error={} error_description={} body={}",
        status, token_endpoint, client_id, key_id, error, error_description, v
      );
    } else {
      panic!(
        "non-JSON token response: status={} token_endpoint={} client_id={} kid={} body={}",
        status, token_endpoint, client_id, key_id, text
      );
    }
  }

  fn make_test_config() -> crate::config::AppConfig {
    crate::config::AppConfig {
      server: crate::config::Server {
        http_port: 0,
        https_port: 0,
        monitoring_port: 0,
        tls_enabled: false,
        domains: vec![],
        email: vec![],
        cache: None,
        production: false,
        tls_key: None,
        tls_cert: None,
      },
      auth: crate::config::Auth {
        issuer_url: test_issuer(),
        audience: String::new(),
        leeway_seconds: 60,
        jwks_refresh_seconds: 0,
      },
      uploads: crate::config::Uploads {
        bucket: "test-bucket".to_string(),
        s3_endpoint: "http://localhost:9000".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_access_key_id: secrecy::SecretString::from("minioadmin".to_string()),
        s3_secret_access_key: secrecy::SecretString::from("minioadmin".to_string()),
        s3_force_path_style: true,
        max_file_size_bytes: 6 * 1024 * 1024,
      },
    }
  }

  #[tokio::test]
  async fn public_openapi_is_accessible_without_auth() {
    let issuer = test_issuer();
    let cfg = make_test_config();
    let state = AppState::new(&cfg).await.unwrap();
    let auth = crate::config::Auth {
      issuer_url: issuer,
      audience: String::new(),
      leeway_seconds: 60,
      jwks_refresh_seconds: 0,
    };
    let app = build_app(state, &auth).await.unwrap();

    let req = axum::http::Request::builder()
      .method("GET")
      .uri("/api/v1/openapi.json")
      .body(axum::body::Body::empty())
      .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn protected_endpoint_without_token_returns_401() {
    let issuer = test_issuer();
    let cfg = make_test_config();
    let state = AppState::new(&cfg).await.unwrap();
    let auth = crate::config::Auth {
      issuer_url: issuer,
      audience: String::new(),
      leeway_seconds: 60,
      jwks_refresh_seconds: 0,
    };
    let app = build_app(state, &auth).await.unwrap();

    let req = axum::http::Request::builder()
      .method("POST")
      .uri("/api/v1/messages")
      .header(axum::http::header::CONTENT_TYPE, "application/json")
      .body(axum::body::Body::from(
        "{\"group_id\":\"g\",\"messages\":[]}",
      ))
      .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
  }

  #[tokio::test]
  async fn protected_endpoint_with_valid_jwt_reaches_handler() {
    let issuer = test_issuer();
    // Service user credentials for JWT Bearer Grant (obtain access token)
    let (user_id, key_id_json, private_key_pem) = load_test_user_key();

    let cfg = make_test_config();
    let state = AppState::new(&cfg).await.unwrap();
    // Accept any audience in tests by leaving it empty
    let auth = crate::config::Auth {
      issuer_url: issuer.clone(),
      audience: String::new(),
      leeway_seconds: 60,
      jwks_refresh_seconds: 0,
    };
    let app = build_app(state, &auth).await.unwrap();

    let token =
      fetch_token_private_key_jwt(&issuer, &user_id, &key_id_json, &private_key_pem).await;

    let req = axum::http::Request::builder()
      .method("POST")
      .uri("/api/v1/messages")
      .header(axum::http::header::CONTENT_TYPE, "application/json")
      .header(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {}", token),
      )
      .body(axum::body::Body::from(
        "{\"group_id\":\"g\",\"messages\":[]}",
      ))
      .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
  }

  #[tokio::test]
  async fn protected_endpoint_with_malformed_token_401() {
    let issuer = test_issuer();

    let cfg = make_test_config();
    let state = AppState::new(&cfg).await.unwrap();
    let auth = crate::config::Auth {
      issuer_url: issuer.clone(),
      audience: String::new(),
      leeway_seconds: 60,
      jwks_refresh_seconds: 0,
    };
    let app = build_app(state, &auth).await.unwrap();

    let req = axum::http::Request::builder()
      .method("POST")
      .uri("/api/v1/messages")
      .header(axum::http::header::CONTENT_TYPE, "application/json")
      .header(axum::http::header::AUTHORIZATION, "Bearer not-a-jwt")
      .body(axum::body::Body::from(
        "{\"group_id\":\"g\",\"messages\":[]}",
      ))
      .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
  }

  // #[tokio::test]
  // async fn uploads_streaming_minio() {
  //   // Use shared test context
  //   let ctx = crate::testing::TestCtx::new().await;

  //   // Build AppState using shared storage and bucket
  //   let state = AppState {
  //     storage: ctx.storage.clone(),
  //     bucket: ctx.bucket.clone(),
  //   };

  //   // Build app with auth and valid JWT
  //   let issuer = test_issuer();
  //   let auth = crate::config::Auth {
  //     issuer_url: issuer.clone(),
  //     audience: String::new(),
  //     leeway_seconds: 60,
  //   };
  //   let app = build_app(state, &auth).await.unwrap();
  //   let (user_id, key_id_json, private_key_pem) = load_test_user_key();
  //   let token =
  //     fetch_token_private_key_jwt(&issuer, &user_id, &key_id_json, &private_key_pem).await;

  //   let ws = "ws1";
  //   // 1) Multi-file upload success
  //   let a = b"hello world".to_vec();
  //   let b = b"goodbye".to_vec();
  //   let (body_bytes, boundary) = build_multipart(vec![
  //     ("a.txt", "text/plain", a.clone()),
  //     ("b.bin", "application/octet-stream", b.clone()),
  //   ]);

  //   let req = axum::http::Request::builder()
  //     .method("POST")
  //     .uri(format!("/api/v1/workspaces/{}/uploads", ws))
  //     .header(
  //       axum::http::header::CONTENT_TYPE,
  //       format!("multipart/form-data; boundary={}", boundary),
  //     )
  //     .header(
  //       axum::http::header::AUTHORIZATION,
  //       format!("Bearer {}", token),
  //     )
  //     .body(axum::body::Body::from(body_bytes))
  //     .unwrap();

  //   let resp = app.clone().oneshot(req).await.unwrap();
  //   assert_eq!(resp.status(), StatusCode::CREATED);
  //   let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
  //   let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  //   let arr = v.as_array().unwrap();
  //   assert_eq!(arr.len(), 2);

  //   // 2) Conflict on re-upload of existing file
  //   let (body_bytes2, boundary2) = build_multipart(vec![("a.txt", "text/plain", a.clone())]);
  //   let req2 = axum::http::Request::builder()
  //     .method("POST")
  //     .uri(format!("/api/v1/workspaces/{}/uploads", ws))
  //     .header(
  //       axum::http::header::CONTENT_TYPE,
  //       format!("multipart/form-data; boundary={}", boundary2),
  //     )
  //     .header(
  //       axum::http::header::AUTHORIZATION,
  //       format!("Bearer {}", token),
  //     )
  //     .body(axum::body::Body::from(body_bytes2))
  //     .unwrap();
  //   let resp2 = app.clone().oneshot(req2).await.unwrap();
  //   assert_eq!(resp2.status(), StatusCode::CONFLICT);

  //   // 3) Exceed 5MB should yield 413
  //   let big = vec![0u8; 5_000_001];
  //   let (body_bytes3, boundary3) =
  //     build_multipart(vec![("big.bin", "application/octet-stream", big)]);
  //   let req3 = axum::http::Request::builder()
  //     .method("POST")
  //     .uri(format!("/api/v1/workspaces/{}/uploads", ws))
  //     .header(
  //       axum::http::header::CONTENT_TYPE,
  //       format!("multipart/form-data; boundary={}", boundary3),
  //     )
  //     .header(
  //       axum::http::header::AUTHORIZATION,
  //       format!("Bearer {}", token),
  //     )
  //     .body(axum::body::Body::from(body_bytes3))
  //     .unwrap();
  //   let resp3 = app.clone().oneshot(req3).await.unwrap();
  //   assert_eq!(resp3.status(), StatusCode::PAYLOAD_TOO_LARGE);
  // }
}
