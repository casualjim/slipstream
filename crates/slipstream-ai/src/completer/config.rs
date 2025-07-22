pub struct CompleterConfig {
  pub api_key: String,
  pub base_url: Option<String>,
  pub model: String,
  pub small_model: String,
  pub temperature: f32,
  pub max_tokens: u32,
}
