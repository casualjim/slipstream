mod http_agent;
mod http_model;
mod http_tool;

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
