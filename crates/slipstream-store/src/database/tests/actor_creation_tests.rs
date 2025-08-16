#![cfg(test)]

use tempfile::tempdir;

use super::tests_helpers::*;
use crate::Database;

#[tokio::test]
async fn test_database_actor_creation() {
  init_logging();
  let temp_dir = tempdir().expect("Failed to create temp directory");
  let config = test_config(temp_dir.path());

  let actor = Database::new(&config)
    .await
    .expect("Failed to create Database");

  // Validate GraphDb accessibility via public execute API
  assert!(
    actor
      .execute(CountGraphItems {
        label: "SystemMetadata"
      })
      .await
      .is_ok()
  );

  // Validate MetaDb accessibility via public execute API (expecting error for nonexistent table)
  assert!(
    actor
      .execute(CountTableRows {
        table: "nonexistent"
      })
      .await
      .is_err()
  );
}
