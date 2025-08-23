use crate::nodes::Interaction;
use lancedb::index::Index;
use lancedb::index::scalar::FtsIndexBuilder;
use slipstream_store::{DatabaseCommand, DatabaseOperation, NoData, Result, ToDatabase};

/// Command to run migration for the Interaction node type (formerly Episode)
pub struct RunInteractionMigration;

impl DatabaseCommand for RunInteractionMigration {
  type Output = ();
  type SaveData = NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: vec![
        // Create the Interaction node table in KuzuDB
        // Only include properties used in WHERE, ORDER BY, or graph traversal queries
        r#"
                CREATE NODE TABLE IF NOT EXISTS Interaction (
                    uuid UUID,
                    group_id STRING,
                    valid_at TIMESTAMP,
                    created_at TIMESTAMP,
                    PRIMARY KEY (uuid)
                )"#,
      ],

      meta_setup: Box::new(move |conn| Box::pin(create_interactions_lance_table(conn))),
      transformer: Box::new(|_| ()), // No transformation needed
    }
  }
}

/// Internal function to create the interactions table in LanceDB
async fn create_interactions_lance_table(conn: &lancedb::Connection) -> Result<()> {
  let table_name = Interaction::meta_table_name();

  // Check if table already exists
  let tables = conn.table_names().execute().await?;
  if tables.contains(&table_name.to_string()) {
    tracing::debug!("Interactions table already exists, skipping creation");
    return Ok(());
  }

  // Create the table using create_empty_table with our schema
  let schema = Interaction::meta_schema(());
  let table = conn
    .create_empty_table(table_name, schema.clone())
    .execute()
    .await?;

  // Create scalar indices for fast lookup
  for col in ["uuid", "group_id", "created_at", "valid_at", "name"] {
    if let Err(e) = table.create_index(&[col][..], Index::Auto).execute().await {
      tracing::warn!("Failed to create scalar index for {}: {}", col, e);
    }
  }

  // Create full-text search indices for text fields
  for col in ["content", "source", "source_description", "name"] {
    if let Err(e) = table
      .create_index(&[col][..], Index::FTS(FtsIndexBuilder::default()))
      .execute()
      .await
    {
      tracing::warn!("Failed to create FTS index for {}: {}", col, e);
    }
  }

  tracing::info!("Created Interaction table with indices");
  Ok(())
}

#[cfg(test)]
mod tests {
  use slipstream_store::Database;

  use super::*;

  #[tokio::test]
  async fn test_interaction_migration() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // Run migrations through the Database interface
    let result = db.execute(RunInteractionMigration).await;
    assert!(
      result.is_ok(),
      "Interaction migration should succeed: {:?}",
      result.err()
    );

    // Running again should be idempotent
    let result = db.execute(RunInteractionMigration).await;
    assert!(
      result.is_ok(),
      "Second migration run should be idempotent: {:?}",
      result.err()
    );
  }
}
