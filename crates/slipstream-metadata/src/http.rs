mod http_agent;
mod http_model;
mod http_tool;

use std::time::Duration;

use crate::Result;
// use http_cache::{CacheMode, HttpCache, HttpCacheOptions, MokaManager};
// use http_cache_reqwest::Cache;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};

pub use http_agent::HttpAgentRegistry as AgentRegistry;
pub use http_model::HttpModelRegistry as ModelRegistry;
pub use http_tool::HttpToolRegistry as ToolRegistry;

#[derive(Debug, Serialize, Deserialize)]
struct ResultInfo {
  pub count: Option<usize>,
  pub page: Option<usize>,
  pub per_page: Option<usize>,
  pub total_count: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct APIEnvelope<T> {
  pub success: bool,
  pub result: T,
  pub result_info: Option<ResultInfo>,
}

fn create_client(api_key: SecretString) -> Result<reqwest_middleware::ClientWithMiddleware> {
  let mut default_headers = HeaderMap::new();
  let api_key = HeaderValue::from_bytes(format!("Bearer {}", api_key.expose_secret()).as_bytes())
    .map_err(|e| {
    crate::Error::Io(std::io::Error::new(
      std::io::ErrorKind::Other,
      format!("HeaderValue error: {e}"),
    ))
  })?;
  default_headers.insert(AUTHORIZATION, api_key);

  // let cache_manager = MokaManager::default();

  Ok(
    ClientBuilder::new(
      reqwest::Client::builder()
        .default_headers(default_headers)
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(1))
        .read_timeout(Duration::from_secs(4))
        .build()
        .map_err(|e| {
          crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Reqwest client build error: {e}"),
          ))
        })?,
    )
    // .with(Cache(HttpCache {
    //   mode: CacheMode::Default,
    //   manager: cache_manager,
    //   options: HttpCacheOptions::default(),
    // }))
    .with(RetryTransientMiddleware::new_with_policy(
      ExponentialBackoff::builder()
        .base(2)
        .jitter(reqwest_retry::Jitter::Full)
        .retry_bounds(Duration::from_millis(500), Duration::from_secs(60))
        .build_with_max_retries(5),
    ))
    .build(),
  )
}
