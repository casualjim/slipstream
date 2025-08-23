use crate::nodes::Interaction;
use slipstream_store::{
  DatabaseCommand, DatabaseOperation, PrimaryStoreQuery, QueryOperation, ResultStream, ToDatabase,
  transformers::record_batch::to_typed_stream,
};
use uuid::Uuid;

/// Command to get a single interaction by UUID
pub struct GetInteractionByUuid(pub Uuid);

impl DatabaseCommand for GetInteractionByUuid {
  type Output = ResultStream<Interaction>;
  type SaveData = slipstream_store::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    let filter = format!("uuid = '{}'", self.0);
    DatabaseOperation::Query(QueryOperation::StoreOnly {
      query: PrimaryStoreQuery {
        table: Interaction::meta_table_name(),
        filter: Some(filter),
        limit: Some(1),
        offset: None,
        vector_search: None,
      },
      transformer: Box::new(to_typed_stream()),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    migrations::interaction_migration::RunInteractionMigration,
    mutations::save_interaction::SaveInteraction, nodes::ContentType,
  };
  use futures::StreamExt;
  use jiff::Timestamp;
  use slipstream_store::Database;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_get_interaction_by_uuid() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // Run migrations
    db.execute(RunInteractionMigration)
      .await
      .expect("Failed to run migrations");

    // Create and save a test interaction
    let test_uuid = Uuid::now_v7();
    let now = Timestamp::now();
    let interaction = Interaction {
      uuid: test_uuid,
      name: "Test Interaction".to_string(),
      content: "Test content".to_string(),
      source: ContentType::Text,
      source_description: "Test source".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["test".to_string()],
      created_at: now,
      valid_at: now,
      reference_edges: vec![],
      version: Uuid::now_v7(),
    };

    db.execute(SaveInteraction {
      interaction: Arc::new(interaction.clone()),
    })
    .await
    .expect("Failed to save interaction");

    // Test getting the interaction
    let mut stream = db
      .execute(GetInteractionByUuid(test_uuid))
      .await
      .expect("Failed to get interaction");

    let retrieved = stream
      .next()
      .await
      .expect("Stream should have an item")
      .expect("Failed to get interaction from stream");

    assert_eq!(retrieved.uuid, interaction.uuid);
    assert_eq!(retrieved.name, interaction.name);
    assert_eq!(retrieved.content, interaction.content);
    assert_eq!(retrieved.source, interaction.source);
    assert_eq!(retrieved.source_description, interaction.source_description);
    assert_eq!(retrieved.group_id, interaction.group_id);
    assert_eq!(retrieved.labels, interaction.labels);
    assert_eq!(retrieved.reference_edges, interaction.reference_edges);

    // Test getting non-existent interaction
    let non_existent_uuid = Uuid::now_v7();
    let mut stream = db
      .execute(GetInteractionByUuid(non_existent_uuid))
      .await
      .expect("Query should succeed even if interaction not found");

    let maybe_interaction = stream.next().await;
    assert!(maybe_interaction.is_none());
  }

  #[tokio::test]
  async fn test_get_interaction_with_complex_data() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    db.execute(RunInteractionMigration)
      .await
      .expect("Failed to run migrations");

    // Create interaction with complex data
    let test_uuid = Uuid::now_v7();
    let now = Timestamp::now();
    let interaction = Interaction {
      uuid: test_uuid,
      name: "Complex Interaction".to_string(),
      content: r#"{"event": "user_action", "details": {"action": "click", "target": "button"}}"#
        .to_string(),
      source: ContentType::Json,
      source_description: "User interaction JSON".to_string(),
      group_id: "test-group".to_string(),
      labels: vec![
        "json".to_string(),
        "event".to_string(),
        "user_action".to_string(),
      ],
      created_at: now,
      valid_at: now,
      reference_edges: vec!["ref1".to_string(), "ref2".to_string(), "ref3".to_string()],
      version: Uuid::now_v7(),
    };

    db.execute(SaveInteraction {
      interaction: Arc::new(interaction.clone()),
    })
    .await
    .expect("Failed to save interaction");

    // Retrieve and verify
    let mut stream = db
      .execute(GetInteractionByUuid(test_uuid))
      .await
      .expect("Failed to get interaction");

    let retrieved = stream
      .next()
      .await
      .expect("Stream should have an item")
      .expect("Failed to get interaction from stream");

    assert_eq!(retrieved.labels.len(), 3);
    assert_eq!(retrieved.reference_edges.len(), 3);
    assert_eq!(retrieved.source, ContentType::Json);
    assert!(retrieved.content.contains("user_action"));
  }
}
