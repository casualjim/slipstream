use crate::edges::Mentions;
use lancedb::index::Index;
use slipstream_store::{DatabaseCommand, DatabaseOperation, NoData, Result, ToDatabase};

/// Command to run migration for the Mentions edge type (formerly MENTIONS)
/// IMPORTANT: This must run AFTER Interaction and Concept node migrations
pub struct RunMentionsMigration;

impl DatabaseCommand for RunMentionsMigration {
  type Output = ();
  type SaveData = NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: vec![
        // Create MENTIONS edge table (Interaction -> Concept) in KuzuDB
        r#"
          CREATE REL TABLE IF NOT EXISTS MENTIONS(
            FROM Interaction TO Concept,
            uuid UUID,
            group_id STRING,
            created_at TIMESTAMP,
            PRIMARY KEY (uuid)
          )"#,
      ],

      meta_setup: Box::new(move |conn| Box::pin(create_mentions_lance_table(conn))),
      transformer: Box::new(|_| ()), // No transformation needed
    }
  }
}

/// Internal function to create the mentions_edges table in LanceDB
async fn create_mentions_lance_table(conn: &lancedb::Connection) -> Result<()> {
  let table_name = Mentions::meta_table_name();

  // Check if table already exists
  let tables = conn.table_names().execute().await?;
  if tables.contains(&table_name.to_string()) {
    tracing::debug!("Mentions edges table already exists, skipping creation");
    return Ok(());
  }

  // Create the table using create_empty_table with our schema
  let schema = Mentions::meta_schema(());
  let table = conn
    .create_empty_table(table_name, schema.clone())
    .execute()
    .await?;

  // Create scalar indices for fast lookup
  for col in [
    "uuid",
    "source_interaction_uuid",
    "target_concept_uuid",
    "group_id",
    "created_at",
  ] {
    if let Err(e) = table.create_index(&[col][..], Index::Auto).execute().await {
      tracing::warn!("Failed to create scalar index for {}: {}", col, e);
    }
  }

  tracing::info!("Created Mentions edges table with indices");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use slipstream_store::Database;

  #[tokio::test]
  async fn test_mentions_migration() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // First run node migrations - edges require nodes to exist
    db.execute(crate::migrations::interaction_migration::RunInteractionMigration)
      .await
      .expect("Interaction migration should succeed");
    db.execute(crate::migrations::concept_migration::RunConceptMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Concept migration should succeed");

    // Now run mentions edge migration
    let result = db.execute(RunMentionsMigration).await;
    assert!(
      result.is_ok(),
      "Mentions migration should succeed: {:?}",
      result.err()
    );

    // Running again should be idempotent
    let result = db.execute(RunMentionsMigration).await;
    assert!(
      result.is_ok(),
      "Second migration run should be idempotent: {:?}",
      result.err()
    );
  }
}
