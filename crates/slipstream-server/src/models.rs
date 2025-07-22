use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The group ids for the memories to search
/// Query for searching facts in memory groups.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchQuery {
  /// Optional list of group IDs to search.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub group_ids: Option<Vec<String>>,
  /// The search query string.
  pub query: String,
  /// Maximum number of facts to return (default: 10).
  #[serde(default = "default_max_facts")]
  pub max_facts: usize,
}

fn default_max_facts() -> usize {
  10
}

/// A single fact result from a memory search.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FactResult {
  /// The unique identifier for the fact.
  pub uuid: String,
  /// The name of the fact.
  pub name: String,
  /// The fact content.
  pub fact: String,
  /// When the fact became valid.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub valid_at: Option<jiff::Timestamp>,
  /// When the fact became invalid.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub invalid_at: Option<jiff::Timestamp>,
  /// When the fact was created.
  pub created_at: jiff::Timestamp,
  /// When the fact expired.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub expired_at: Option<jiff::Timestamp>,
}

/// The results of a memory search.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResults {
  /// The list of facts found.
  pub facts: Vec<FactResult>,
}

/// The group id of the memory to get
/// Request to retrieve facts from a memory group.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetMemoryRequest {
  /// The group id to retrieve from.
  pub group_id: String,
  /// Maximum number of facts to return (default: 10).
  #[serde(default = "default_max_facts")]
  pub max_facts: usize,
  /// The uuid of the node to center the retrieval on.
  pub center_node_uuid: Option<String>,
  /// The messages to build the retrieval query from.
  pub messages: Vec<Message>,
}

/// The facts that were retrieved from the graph
/// The facts that were retrieved from the graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetMemoryResponse {
  /// The facts retrieved.
  pub facts: Vec<FactResult>,
}

/// Generic result type for API responses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Result {
  /// Whether the operation was successful.
  pub success: bool,
  /// A message describing the result.
  pub message: String,
}

/// The role of a message sender.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Role {
  /// System message.
  System,
  /// User message, with username.
  User(String),
  /// Assistant message.
  Assistant,
}

fn default_timestamp() -> jiff::Timestamp {
  jiff::Timestamp::now()
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
  /// The message content.
  pub content: String,
  /// The unique id of the message.
  pub id: Option<Uuid>,
  /// The name of the sender.
  pub name: String,
  /// The role of the sender.
  pub role: Role,
  /// The timestamp of the message.
  #[serde(default = "default_timestamp")]
  pub timestamp: jiff::Timestamp,
  /// Optional description of the message source.
  pub source_description: Option<String>,
}

/// Request to add messages to a group.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddMessages {
  /// The messages to add.
  pub messages: Vec<Message>,
  /// The group id to add messages to.
  pub group_id: String,
}

/// Request to add a concept to a group.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddConcept {
  /// The concept id.
  pub id: String,
  /// The group id.
  pub group_id: String,
  /// The concept name.
  pub name: String,
  /// The concept summary.
  pub summary: String,
}
