use crate::nodes::Theme;
use slipstream_store::{
  DatabaseCommand, DatabaseOperation, PrimaryStoreQuery, QueryOperation, ToDatabase,
};
use uuid::Uuid;

/// Query to get a theme by UUID
pub struct GetThemeByUuid(pub Uuid);

impl DatabaseCommand for GetThemeByUuid {
  type Output = slipstream_store::ResultStream<Theme>;
  type SaveData = slipstream_store::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    // Build filter for LanceDB query
    let filter = Some(format!("uuid = '{}'", self.0));

    DatabaseOperation::Query(QueryOperation::StoreOnly {
      query: PrimaryStoreQuery {
        table: Theme::meta_table_name(),
        filter,
        limit: Some(1),
        offset: None,
        vector_search: None,
      },
      transformer: Box::new(slipstream_store::transformers::record_batch::to_typed_stream()),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{migrations::RunThemeMigration, mutations::SaveTheme};
  use futures::StreamExt;
  use jiff::Timestamp;
  use slipstream_store::Database;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_get_theme_by_uuid() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // Run migrations
    db.execute(RunThemeMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Failed to run migrations");

    // Create and save a theme
    let test_uuid = Uuid::now_v7();
    let theme = Theme {
      uuid: test_uuid,
      name: "Test Theme".to_string(),
      summary: "A test theme for testing retrieval".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["Theme".to_string()],
      created_at: Timestamp::now(),
      name_embedding: None,
    };

    db.execute(SaveTheme {
      theme: Arc::new(theme.clone()),
      embedding_dimensions: config.embedding.dimensions,
    })
    .await
    .expect("Failed to save theme");

    // Query for the theme
    let mut stream = db
      .execute(GetThemeByUuid(test_uuid))
      .await
      .expect("Failed to execute query");

    // Get the first (and only) result
    let result = stream
      .next()
      .await
      .expect("Stream should have an item")
      .expect("Failed to get theme");

    assert_eq!(result.uuid, test_uuid);
    assert_eq!(result.name, "Test Theme");
    assert_eq!(result.summary, "A test theme for testing retrieval");
    assert_eq!(result.labels, vec!["Theme".to_string()]);
    assert_eq!(result.group_id, "test-group");
  }

  #[tokio::test]
  async fn test_get_theme_by_uuid_not_found() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    db.execute(RunThemeMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Failed to run migrations");

    // Query for a non-existent UUID
    let random_uuid = Uuid::now_v7();
    let mut stream = db
      .execute(GetThemeByUuid(random_uuid))
      .await
      .expect("Failed to execute query");

    // Should return None since the theme doesn't exist
    let result = stream.next().await;
    assert!(result.is_none(), "Should not find any theme");
  }
}
