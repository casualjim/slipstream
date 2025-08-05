use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::error::{Error, Result};

/// Reserved field names that cannot be used in entity type definitions
const RESERVED_FIELDS: &[&str] = &[
  "uuid",
  "name",
  "group_id",
  "labels",
  "created_at",
  "name_embedding",
  "summary",
  "attributes",
];

/// Definition of a custom entity type with its schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityTypeDefinition {
  /// The name of the entity type (e.g., "Person", "Organization")
  pub name: String,
  /// Human-readable description of what this entity type represents
  pub description: String,
  /// JSON Schema defining the attributes for this entity type
  pub schema: Value,
}

/// Validate entity type definitions to ensure no conflicts with reserved fields
pub fn validate_entity_types(entity_types: &HashMap<String, Value>) -> Result<()> {
  for (type_name, schema) in entity_types {
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
      for field_name in properties.keys() {
        if RESERVED_FIELDS.contains(&field_name.as_str()) {
          return Err(Error::EntityTypeValidationError(
            type_name.clone(),
            field_name.clone(),
          ));
        }
      }
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn test_validate_entity_types_success() {
    let entity_types = HashMap::from([(
      "Person".to_string(),
      json!({
          "properties": {
              "age": { "type": "number" },
              "occupation": { "type": "string" }
          }
      }),
    )]);

    assert!(validate_entity_types(&entity_types).is_ok());
  }

  #[test]
  fn test_validate_entity_types_reserved_field() {
    let entity_types = HashMap::from([(
      "Person".to_string(),
      json!({
          "properties": {
              "uuid": { "type": "string" },
              "age": { "type": "number" }
          }
      }),
    )]);

    let result = validate_entity_types(&entity_types);
    assert!(result.is_err());

    if let Err(Error::EntityTypeValidationError(type_name, field_name)) = result {
      assert_eq!(type_name, "Person");
      assert_eq!(field_name, "uuid");
    } else {
      panic!("Expected EntityTypeValidationError");
    }
  }
}
