mod get_concept_by_uuid;
mod get_interaction_by_uuid;
mod get_theme_by_uuid;

pub use get_concept_by_uuid::GetConceptByUuid;
pub use get_interaction_by_uuid::GetInteractionByUuid;
pub use get_theme_by_uuid::GetThemeByUuid;

use serde::{Deserialize, Serialize};

/// Comparison operators for date filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOperator {
  Equals,
  NotEquals,
  GreaterThan,
  LessThan,
  GreaterThanEqual,
  LessThanEqual,
}

impl AsRef<str> for ComparisonOperator {
  fn as_ref(&self) -> &str {
    match self {
      Self::Equals => "=",
      Self::NotEquals => "<>",
      Self::GreaterThan => ">",
      Self::LessThan => "<",
      Self::GreaterThanEqual => ">=",
      Self::LessThanEqual => "<=",
    }
  }
}

/// Date filter for temporal queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateFilter {
  pub date: jiff::Timestamp,
  pub comparison_operator: ComparisonOperator,
}

/// Search filters for graph queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
  /// Filter by concept labels (entity types in Graphiti)
  pub node_labels: Option<Vec<String>>,
  /// Filter by edge types (relation types)
  pub edge_types: Option<Vec<String>>,
  /// Filter by valid_at timestamp (outer Vec is OR, inner Vec is AND)
  pub valid_at: Option<Vec<Vec<DateFilter>>>,
  /// Filter by invalid_at timestamp
  pub invalid_at: Option<Vec<Vec<DateFilter>>>,
  /// Filter by created_at timestamp
  pub created_at: Option<Vec<Vec<DateFilter>>>,
  /// Filter by expired_at timestamp
  pub expired_at: Option<Vec<Vec<DateFilter>>>,
}
