#[tokio::main]
async fn main() -> eyre::Result<()> {
  let client = genai::Client::builder().build();

  client
    .all_model_names(genai::adapter::AdapterKind::Gemini)
    .await?;
  Ok(())
}
