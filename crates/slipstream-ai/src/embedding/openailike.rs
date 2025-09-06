use crate::embedding::{Config, EmbedInput, EmbedOutput, EmbeddingService};
use async_trait::async_trait;
use http::{
  HeaderValue,
  header::{AUTHORIZATION, CONTENT_TYPE},
};
use secrecy::ExposeSecret;

pub struct Client {
  client: reqwest_middleware::ClientWithMiddleware,
  base_url: String,
}

impl Client {
  pub fn new(config: Config) -> eyre::Result<Self> {
    let client = reqwest::Client::builder()
      .default_headers({
        let mut headers = http::HeaderMap::new();
        if let Some(api_key) = &config.api_key {
          headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key.expose_secret())).unwrap(),
          );
        }
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
      })
      .user_agent(format!(
        "Slipstream/Embeddings {}",
        env!("CARGO_PKG_VERSION")
      ))
      .timeout(config.timeout)
      .build()?;

    let client = reqwest_middleware::ClientBuilder::new(client)
      // Add any middleware here, e.g., for logging, retries, etc.
      .build();

    Ok(Self {
      client,
      base_url: config.base_url,
    })
  }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddingObject {
  embedding: Vec<f32>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbedApiResponse {
  data: Vec<EmbeddingObject>,
}

#[async_trait]
impl EmbeddingService for Client {
  async fn embed(&self, text: EmbedInput) -> eyre::Result<EmbedOutput> {
    let base_url = &self.base_url;
    let response = self
      .client
      .post(format!("{base_url}/embeddings"))
      .json(&text)
      .send()
      .await?;

    let output: EmbedApiResponse = response.json().await?;
    Ok(EmbedOutput {
      embeddings: output.data.into_iter().map(|obj| obj.embedding).collect(),
    })
  }
}
