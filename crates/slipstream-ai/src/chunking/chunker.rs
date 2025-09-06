use std::path::Path;

use crate::grammars::get_language;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkInput {
  pub content: String,
  pub file_path: String,
  pub tokenizer: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
  pub content: String,
  pub tokens: Vec<u32>,
  pub start_byte: usize,
  pub end_byte: usize,
  pub start_line: usize,
  pub end_line: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkOutput {
  pub chunks: Vec<Chunk>,
  pub content_hash: [u8; 32],
}

/// Checks if a file is a text file.
pub fn is_text_file<P: AsRef<Path>>(path: P) -> bool {
  if let Ok(Some(file_type)) = infer::get_from_path(path.as_ref()) {
    file_type.matcher_type() == infer::MatcherType::Text
  } else {
    // If infer can't determine, check with hyperpolyglot
    hyperpolyglot::detect(path.as_ref()).is_ok()
  }
}

/// Detects the language of a file.
pub fn detect_language<P: AsRef<Path>>(path: P) -> Option<String> {
  if let Ok(Some(detection)) = hyperpolyglot::detect(path.as_ref()) {
    let language = detection.language();
    get_language(language).map(|_| language.to_string())
  } else {
    None
  }
}
