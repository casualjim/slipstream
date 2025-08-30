use clap::Parser;
use confique::Config as _;
use serde::Deserialize;

#[derive(confique::Config, Debug, Clone, Deserialize)]
#[config(partial_attr(derive(Parser, Debug)))]
pub struct AppConfig {
  #[serde(default)]
  #[config(nested, partial_attr(clap::flatten))]
  pub server: Server,
}

#[derive(confique::Config, Debug, Clone, Deserialize)]
pub struct Server {
  #[serde(default = "default_http_port")]
  pub http_port: u16,
  #[serde(default = "default_https_port")]
  pub https_port: u16,
  #[serde(default = "default_admin_port")]
  pub admin_http_port: u16,
  #[serde(default)]
  pub tls_enabled: bool,
}

fn default_http_port() -> u16 { 8080 }
fn default_https_port() -> u16 { 8443 }
fn default_admin_port() -> u16 { 9090 }

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
  // Load files + env as a Partial
  let files_and_env: AppConfigPartial = AppConfig::builder()
    .env(
      confique::env::Source::new("SLIPSTREAM")
        .with_separator("__")
        .try_convert(true),
    )
    .file("config/default.toml")
    .file_optional("config/local.toml")
    .file_optional("/etc/slipstream/config.toml")
    .file_optional("/etc/slipstream/secrets.toml")
    .load_partial()?;

  // Parse CLI as a Partial (highest precedence)
  let cli: AppConfigPartial = <AppConfigPartial as Parser>::parse();

  // Merge and complete
  let merged = files_and_env.merge(cli);
  let cfg = merged.try_complete()?;
  Ok(cfg)
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::Parser as _;
  use std::{env, fs, path::PathBuf, sync::{Mutex, OnceLock}};
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
    // Build only defaults (no files, no env, no CLI)
    let files_and_env: AppConfigPartial = AppConfig::builder().load_partial().unwrap();
    let cli: AppConfigPartial = <AppConfigPartial as Parser>::parse_from(["bin"]);
    let cfg = files_and_env.merge(cli).try_complete().unwrap();

    assert_eq!(cfg.server.http_port, 8080);
    assert_eq!(cfg.server.https_port, 8443);
    assert_eq!(cfg.server.admin_http_port, 9090);
    assert!(!cfg.server.tls_enabled);
  }

  #[test]
  fn file_overrides_defaults() {
    let path = write_temp_toml(
      r#"[server]
http_port = 18080
https_port = 18443
admin_http_port = 19090
tls_enabled = true
"#,
    );

    let files: AppConfigPartial = AppConfig::builder()
      .file(&path)
      .load_partial()
      .unwrap();

    let cli: AppConfigPartial = <AppConfigPartial as Parser>::parse_from(["bin"]);
    let cfg = files.merge(cli).try_complete().unwrap();

    assert_eq!(cfg.server.http_port, 18080);
    assert_eq!(cfg.server.https_port, 18443);
    assert_eq!(cfg.server.admin_http_port, 19090);
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

    env::set_var("SLIPSTREAM__SERVER__HTTP_PORT", "28080");

    let layered: AppConfigPartial = AppConfig::builder()
      .file(&path)
      .env(confique::env::Source::new("SLIPSTREAM").with_separator("__").try_convert(true))
      .load_partial()
      .unwrap();

    let cli: AppConfigPartial = <AppConfigPartial as Parser>::parse_from(["bin"]);
    let cfg = layered.merge(cli).try_complete().unwrap();

    assert_eq!(cfg.server.http_port, 28080);

    env::remove_var("SLIPSTREAM__SERVER__HTTP_PORT");
  }

  #[test]
  fn cli_overrides_env() {
    let _g = env_lock();
    env::set_var("SLIPSTREAM__SERVER__HTTP_PORT", "28080");

    let env_only: AppConfigPartial = AppConfig::builder()
      .env(confique::env::Source::new("SLIPSTREAM").with_separator("__").try_convert(true))
      .load_partial()
      .unwrap();

    let cli: AppConfigPartial = <AppConfigPartial as Parser>::parse_from([
      "bin",
      "--server-http-port",
      "38080",
    ]);

    let cfg = env_only.merge(cli).try_complete().unwrap();
    assert_eq!(cfg.server.http_port, 38080);

    env::remove_var("SLIPSTREAM__SERVER__HTTP_PORT");
  }
}
