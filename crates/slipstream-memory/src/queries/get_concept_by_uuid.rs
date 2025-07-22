use crate::nodes::Concept;
use slipstream_store::{
  DatabaseCommand, DatabaseOperation, PrimaryStoreQuery, QueryOperation, ToDatabase,
};
use uuid::Uuid;

/// Query to get a concept by UUID
pub struct GetConceptByUuid(pub Uuid);

impl DatabaseCommand for GetConceptByUuid {
  type Output = slipstream_store::ResultStream<Concept>;
  type SaveData = slipstream_store::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    // Build filter for LanceDB query
    let filter = Some(format!("uuid = '{}'", self.0));

    DatabaseOperation::Query(QueryOperation::StoreOnly {
      query: PrimaryStoreQuery {
        table: Concept::meta_table_name(),
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
  use crate::{migrations::RunConceptMigration, mutations::SaveConcept};
  use futures::StreamExt;
  use jiff::Timestamp;
  use slipstream_store::Database;
  use std::collections::HashMap;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_get_concept_by_uuid() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // Run migrations
    db.execute(RunConceptMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Failed to run migrations");

    // Create and save a concept
    let test_uuid = Uuid::now_v7();
    let concept = Concept {
      uuid: test_uuid,
      name: "Test Concept".to_string(),
      labels: vec!["Concept".to_string()],
      attributes: HashMap::from([("test".to_string(), serde_json::json!("value"))]),
      group_id: "test-group".to_string(),
      created_at: Timestamp::now(),
      name_embedding: None,
      summary: "A test concept".to_string(),
    };

    db.execute(SaveConcept {
      concept: Arc::new(concept.clone()),
      embedding_dimensions: config.embedding.dimensions,
    })
    .await
    .expect("Failed to save concept");

    // Query for the concept
    let mut stream = db
      .execute(GetConceptByUuid(test_uuid))
      .await
      .expect("Failed to execute query");

    // Get the first (and only) result
    let result = stream
      .next()
      .await
      .expect("Stream should have an item")
      .expect("Failed to get concept");

    assert_eq!(result.uuid, test_uuid);
    assert_eq!(result.name, "Test Concept");
    assert_eq!(result.labels, vec!["Concept".to_string()]);
    assert_eq!(result.group_id, "test-group");
  }

  #[tokio::test]
  async fn test_get_concept_by_uuid_not_found() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    db.execute(RunConceptMigration {
      embedding_function_name: config.embedding.provider.clone(),
    })
    .await
    .expect("Failed to run migrations");

    // Query for a non-existent UUID
    let random_uuid = Uuid::now_v7();
    let mut stream = db
      .execute(GetConceptByUuid(random_uuid))
      .await
      .expect("Failed to execute query");

    // Should return None since the concept doesn't exist
    let result = stream.next().await;
    assert!(result.is_none(), "Should not find any concept");
  }
}
