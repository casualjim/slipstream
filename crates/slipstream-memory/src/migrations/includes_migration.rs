use crate::edges::Includes;
use lancedb::index::Index;
use slipstream_store::{DatabaseCommand, DatabaseOperation, NoData, Result, ToDatabase};

/// Command to run migration for the Includes edge type (formerly HAS_MEMBER)
/// IMPORTANT: This must run AFTER Theme and Concept node migrations
pub struct RunIncludesMigration;

impl DatabaseCommand for RunIncludesMigration {
  type Output = ();
  type SaveData = NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: vec![
        // Create INCLUDES edge table (Theme -> Concept)
        r#"
                CREATE REL TABLE IF NOT EXISTS INCLUDES(
                    FROM Theme TO Concept,
                    uuid UUID,
                    group_id STRING,
                    created_at TIMESTAMP,
                    PRIMARY KEY (uuid)
                )"#
          .into(),
      ],

      meta_setup: Box::new(move |conn| Box::pin(create_includes_lance_table(conn))),
      transformer: Box::new(|_| ()), // No transformation needed
    }
  }
}

/// Internal function to create the includes_edges table in LanceDB
async fn create_includes_lance_table(conn: &lancedb::Connection) -> Result<()> {
  let table_name = Includes::meta_table_name();

  // Check if table already exists
  let tables = conn.table_names().execute().await?;
  if tables.contains(&table_name.to_string()) {
    tracing::debug!("Includes edges table already exists, skipping creation");
    return Ok(());
  }

  // Create the table using create_empty_table with our schema
  let schema = Includes::meta_schema(());
  let table = conn
    .create_empty_table(table_name, schema.clone())
    .execute()
    .await?;

  // Create scalar indices for fast lookup
  for col in [
    "uuid",
    "source_theme_uuid",
    "target_concept_uuid",
    "group_id",
    "created_at",
  ] {
    if let Err(e) = table.create_index(&[col][..], Index::Auto).execute().await {
      tracing::warn!("Failed to create scalar index for {}: {}", col, e);
    }
  }

  tracing::info!("Created Includes edges table with indices");
  Ok(())
}

#[cfg(test)]
mod tests {
  use slipstream_store::Database;

  use super::*;

  #[tokio::test]
  async fn test_includes_migration() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // First run node migrations - edges require nodes to exist
    db.execute(crate::migrations::RunThemeMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Theme migration should succeed");
    db.execute(crate::migrations::RunConceptMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Concept migration should succeed");

    // Now run includes edge migration
    let result = db.execute(RunIncludesMigration).await;
    assert!(
      result.is_ok(),
      "Includes migration should succeed: {:?}",
      result.err()
    );

    // Running again should be idempotent
    let result = db.execute(RunIncludesMigration).await;
    assert!(
      result.is_ok(),
      "Second migration run should be idempotent: {:?}",
      result.err()
    );
  }
}
