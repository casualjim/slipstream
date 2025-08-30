use clap::{Args, Parser};
use confique::Config as _;
use std::path::PathBuf;

#[derive(confique::Config, Debug, Clone)]
pub struct AppConfig {
  #[config(nested)]
  pub server: Server,
  #[config(nested)]
  pub auth: Auth,
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

  Ok(cfg)
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

  #[test]
  fn config_defaults_when_no_sources() {
    // No files, no env, CLI empty
    let cfg = AppConfig::builder().env().load().unwrap();
    assert_eq!(cfg.server.http_port, 8080);
    assert_eq!(cfg.server.https_port, 8443);
    assert_eq!(cfg.server.monitoring_port, 9090);
    assert!(!cfg.server.tls_enabled);
  }

  #[test]
  fn file_overrides_defaults() {
    let path = write_temp_toml(
      r#"[server]
http_port = 18080
https_port = 18443
monitoring_port = 19090
tls_enabled = true
"#,
    );

    let cfg = AppConfig::builder().env().file(&path).load().unwrap();

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

    let cfg = AppConfig::builder().env().file(&path).load().unwrap();

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

    let cfg = super::load_config_with_args(["slipstream-server", "--http-port", "38080"]).unwrap();

    assert_eq!(cfg.server.http_port, 38080);

    unsafe {
      env::remove_var("SLIPSTREAM__HTTP_PORT");
    }
  }
}
