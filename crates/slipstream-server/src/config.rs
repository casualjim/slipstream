use clap::{Args, Parser};
use confique::Config as _;
use secrecy::ExposeSecret;
use std::path::PathBuf;

#[derive(confique::Config, Debug, Clone)]
pub struct AppConfig {
  #[config(nested)]
  pub server: Server,
  #[config(nested)]
  pub auth: Auth,
  #[config(nested)]
  pub uploads: Uploads,
}

#[derive(confique::Config, Debug, Clone)]
pub struct Server {
  // Networking
  #[config(default = 8080, env = "SLIPSTREAM__HTTP_PORT")]
  pub http_port: u16,
  #[config(default = 8443, env = "SLIPSTREAM__HTTPS_PORT")]
  pub https_port: u16,
  #[config(default = 9090, env = "SLIPSTREAM__MONITORING_PORT")]
  pub monitoring_port: u16,

  // TLS toggle
  #[config(default = false, env = "SLIPSTREAM__TLS_ENABLED")]
  pub tls_enabled: bool,

  // TLS/ACME configuration (optional)
  #[config(default = [])]
  pub domains: Vec<String>,
  #[config(default = [])]
  pub email: Vec<String>,

  pub cache: Option<PathBuf>,
  #[config(default = false)]
  pub production: bool,
  pub tls_key: Option<PathBuf>,
  pub tls_cert: Option<PathBuf>,
}

#[derive(confique::Config, Debug, Clone)]
pub struct Auth {
  #[config(
    default = "https://wagyu-tunhvc.zitadel.cloud",
    env = "SLIPSTREAM__AUTH__ISSUER_URL"
  )]
  pub issuer_url: String,
  #[config(
    default = "https://wagyu-tunhvc.zitadel.cloud",
    env = "SLIPSTREAM__AUTH__AUDIENCE"
  )]
  pub audience: String,
  #[config(default = 60, env = "SLIPSTREAM__AUTH__LEEWAY_SECONDS")]
  pub leeway_seconds: u64,
  // How often to refresh JWKS (seconds). Set to 0 to disable background refresh.
  #[config(default = 600, env = "SLIPSTREAM__AUTH__JWKS_REFRESH_SECONDS")]
  pub jwks_refresh_seconds: u64,
}

#[derive(Debug, Parser, Default, Clone)]
struct CliArgs {
  #[command(flatten)]
  server: ServerArgs,
  #[command(flatten)]
  auth: AuthArgs,
}

#[derive(Debug, Args, Default, Clone)]
struct ServerArgs {
  #[arg(long = "http-port")]
  http_port: Option<u16>,
  #[arg(long = "https-port")]
  https_port: Option<u16>,
  #[arg(long = "monitoring-port")]
  monitoring_port: Option<u16>,
  #[arg(long = "tls-enabled")]
  tls_enabled: Option<bool>,
}

#[derive(Debug, Args, Default, Clone)]
struct AuthArgs {
  #[arg(long = "issuer-url")]
  issuer_url: Option<String>,
  #[arg(long = "audience")]
  audience: Option<String>,
  #[arg(long = "leeway-seconds")]
  leeway_seconds: Option<u64>,
}

pub fn load_config() -> eyre::Result<AppConfig> {
  load_config_with_args(std::env::args_os())
}

pub fn load_config_with_args<I, T>(args: I) -> eyre::Result<AppConfig>
where
  I: IntoIterator<Item = T>,
  T: Into<std::ffi::OsString> + Clone,
{
  // files + env
  let mut cfg = AppConfig::builder()
    .env()
    .file("config/local.toml")
    .file("/etc/slipstream/secrets.toml")
    .file("/etc/slipstream/config.toml")
    .file("config/default.toml")
    .load()
    .map_err(|e| eyre::eyre!(e.to_string()))?;

  // CLI overlay
  let cli = CliArgs::parse_from(args);
  if let Some(v) = cli.server.http_port {
    cfg.server.http_port = v;
  }
  if let Some(v) = cli.server.https_port {
    cfg.server.https_port = v;
  }
  if let Some(v) = cli.server.monitoring_port {
    cfg.server.monitoring_port = v;
  }
  if let Some(v) = cli.server.tls_enabled {
    cfg.server.tls_enabled = v;
  }

  if let Some(v) = cli.auth.issuer_url {
    cfg.auth.issuer_url = v;
  }
  if let Some(v) = cli.auth.audience {
    cfg.auth.audience = v;
  }
  if let Some(v) = cli.auth.leeway_seconds {
    cfg.auth.leeway_seconds = v;
  }

  // Validate uploads config once
  cfg.uploads.validate()?;

  Ok(cfg)
}

#[derive(confique::Config, Debug, Clone)]
pub struct Uploads {
  #[config(env = "UPLOADS_BUCKET")]
  pub bucket: String,
  #[config(env = "UPLOADS_S3_ENDPOINT")]
  pub s3_endpoint: String,
  #[config(env = "UPLOADS_S3_REGION")]
  pub s3_region: String,
  #[config(env = "UPLOADS_S3_ACCESS_KEY_ID")]
  pub s3_access_key_id: secrecy::SecretString,
  #[config(env = "UPLOADS_S3_SECRET_ACCESS_KEY")]
  pub s3_secret_access_key: secrecy::SecretString,
  #[config(default = true, env = "UPLOADS_S3_FORCE_PATH_STYLE")]
  pub s3_force_path_style: bool,
  // Per-file size limit, in bytes. Defaults to 5 MiB.
  #[config(default = 5_242_880, env = "UPLOADS_MAX_FILE_SIZE_BYTES")]
  pub max_file_size_bytes: usize,
}

impl Uploads {
  pub fn validate(&self) -> eyre::Result<()> {
    if self.bucket.trim().is_empty() {
      return Err(eyre::eyre!("UPLOADS_BUCKET must not be empty"));
    }
    if self.s3_region.trim().is_empty() {
      return Err(eyre::eyre!("UPLOADS_S3_REGION must not be empty"));
    }
    if self.s3_access_key_id.expose_secret().trim().is_empty() {
      return Err(eyre::eyre!("UPLOADS_S3_ACCESS_KEY_ID must not be empty"));
    }
    if self.s3_secret_access_key.expose_secret().trim().is_empty() {
      return Err(eyre::eyre!("UPLOADS_S3_SECRET_ACCESS_KEY must not be empty"));
    }
    if self.max_file_size_bytes == 0 {
      return Err(eyre::eyre!("UPLOADS_MAX_FILE_SIZE_BYTES must be > 0"));
    }
    let url = url::Url::parse(&self.s3_endpoint)
      .map_err(|e| eyre::eyre!("UPLOADS_S3_ENDPOINT is not a valid URL: {e}"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
      return Err(eyre::eyre!("UPLOADS_S3_ENDPOINT must be http or https"));
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    env, fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
  };
  use uuid::Uuid;

  // Serialize env-dependent tests
  static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
  fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
  }

  fn write_temp_toml(contents: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("slipstream-test-{}", Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, contents).unwrap();
    path
  }

  // Provide minimal uploads env so AppConfig can load without altering defaults.
  fn with_min_uploads_env<F, R>(f: F) -> R
  where
    F: FnOnce() -> R,
  {
    unsafe {
      env::set_var("UPLOADS_BUCKET", "test-bucket");
      env::set_var("UPLOADS_S3_ENDPOINT", "http://127.0.0.1:9000");
      env::set_var("UPLOADS_S3_REGION", "us-east-1");
      env::set_var("UPLOADS_S3_ACCESS_KEY_ID", "minioadmin");
      env::set_var("UPLOADS_S3_SECRET_ACCESS_KEY", "minioadmin");
    }
    let out = f();
    unsafe {
      env::remove_var("UPLOADS_BUCKET");
      env::remove_var("UPLOADS_S3_ENDPOINT");
      env::remove_var("UPLOADS_S3_REGION");
      env::remove_var("UPLOADS_S3_ACCESS_KEY_ID");
      env::remove_var("UPLOADS_S3_SECRET_ACCESS_KEY");
    }
    out
  }

  #[test]
  fn config_defaults_when_no_sources() {
    // No files, CLI empty. We set only uploads env to satisfy required fields.
    let _g = env_lock();
    let cfg = with_min_uploads_env(|| AppConfig::builder().env().load().unwrap());
    assert_eq!(cfg.server.http_port, 8080);
    assert_eq!(cfg.server.https_port, 8443);
    assert_eq!(cfg.server.monitoring_port, 9090);
    assert!(!cfg.server.tls_enabled);
  }

  #[test]
  fn file_overrides_defaults() {
    let _g = env_lock();
    let path = write_temp_toml(
      r#"[server]
http_port = 18080
https_port = 18443
monitoring_port = 19090
tls_enabled = true
"#,
    );

    let cfg = with_min_uploads_env(|| AppConfig::builder().env().file(&path).load().unwrap());

    assert_eq!(cfg.server.http_port, 18080);
    assert_eq!(cfg.server.https_port, 18443);
    assert_eq!(cfg.server.monitoring_port, 19090);
    assert!(cfg.server.tls_enabled);
  }

  #[test]
  fn env_overrides_files() {
    let _g = env_lock();
    let path = write_temp_toml(
      r#"[server]
http_port = 18080
"#,
    );

    unsafe {
      env::set_var("SLIPSTREAM__HTTP_PORT", "28080");
    }

    let cfg = with_min_uploads_env(|| AppConfig::builder().env().file(&path).load().unwrap());

    assert_eq!(cfg.server.http_port, 28080);

    unsafe {
      env::remove_var("SLIPSTREAM__HTTP_PORT");
    }
  }

  #[test]
  fn cli_overrides_env() {
    let _g = env_lock();
    unsafe {
      env::set_var("SLIPSTREAM__HTTP_PORT", "28080");
    }

    let cfg = with_min_uploads_env(|| super::load_config_with_args(["slipstream-server", "--http-port", "38080"]).unwrap());

    assert_eq!(cfg.server.http_port, 38080);

    unsafe {
      env::remove_var("SLIPSTREAM__HTTP_PORT");
    }
  }
}
