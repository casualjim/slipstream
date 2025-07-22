use crate::nodes::Interaction;
use slipstream_store::{DatabaseCommand, DatabaseOperation, ToDatabase};
use std::sync::Arc;

/// Command to save an interaction to both databases
pub struct SaveInteraction {
  pub interaction: Arc<Interaction>,
}

impl DatabaseCommand for SaveInteraction {
  type Output = ();
  type SaveData = Interaction;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    // Use save_simple helper which is designed for operations with unit contexts
    DatabaseOperation::save_simple(
      Interaction::meta_table_name(),
      self.interaction.clone(),
      // Static cypher string - Interaction is the actual table name
      r#"
        MERGE (n:Interaction {uuid: $uuid})
        SET n.group_id = $group_id,
            n.valid_at = $valid_at,
            n.created_at = $created_at
      "#,
      slipstream_store::transformers::common::unit_result(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    migrations::RunInteractionMigration, nodes::ContentType, queries::GetInteractionByUuid,
  };
  use futures::StreamExt;
  use jiff::Timestamp;
  use slipstream_store::Database;
  use uuid::Uuid;

  #[tokio::test]
  async fn test_save_interaction() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    // Run migrations to create schema
    db.execute(RunInteractionMigration)
      .await
      .expect("Failed to run migrations");

    // Create a test interaction
    let test_uuid = Uuid::now_v7();
    let now = Timestamp::now();

    let interaction = Interaction {
      uuid: test_uuid,
      name: "Test Interaction".to_string(),
      content: "This is test content".to_string(),
      source: ContentType::Text,
      source_description: "Test source".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["test".to_string(), "interaction".to_string()],
      created_at: now,
      valid_at: now,
      reference_edges: vec!["edge1".to_string(), "edge2".to_string()],
      version: Uuid::now_v7(),
    };

    // Save the interaction
    let save_result = db
      .execute(SaveInteraction {
        interaction: Arc::new(interaction.clone()),
      })
      .await;
    assert!(
      save_result.is_ok(),
      "Failed to save interaction: {:?}",
      save_result.err()
    );

    // Verify we can retrieve it
    let mut get_stream = db
      .execute(GetInteractionByUuid(test_uuid))
      .await
      .expect("Failed to get saved interaction");

    let retrieved = get_stream
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
    assert_eq!(retrieved.version, interaction.version);
  }

  #[tokio::test]
  async fn test_save_interaction_upsert() {
    let data_dir = tempfile::tempdir().unwrap().keep();
    let config = slipstream_store::Config::new_test(data_dir);
    let db = Database::new(&config).await.unwrap();

    db.execute(RunInteractionMigration)
      .await
      .expect("Failed to run migrations");

    let test_uuid = Uuid::now_v7();
    let now = Timestamp::now();

    // First save
    let interaction1 = Interaction {
      uuid: test_uuid,
      name: "Original Interaction".to_string(),
      content: "Original content".to_string(),
      source: ContentType::Message,
      source_description: "Original source".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["original".to_string()],
      created_at: now,
      valid_at: now,
      reference_edges: vec![],
      version: Uuid::now_v7(),
    };

    db.execute(SaveInteraction {
      interaction: Arc::new(interaction1),
    })
    .await
    .expect("Failed to save first interaction");

    // Second save with same UUID (should update)
    let interaction2 = Interaction {
      uuid: test_uuid,
      name: "Updated Interaction".to_string(),
      content: "Updated content".to_string(),
      source: ContentType::Json,
      source_description: "Updated source".to_string(),
      group_id: "test-group".to_string(),
      labels: vec!["updated".to_string(), "modified".to_string()],
      created_at: now,
      valid_at: now,
      reference_edges: vec!["new_edge".to_string()],
      version: Uuid::now_v7(),
    };

    db.execute(SaveInteraction {
      interaction: Arc::new(interaction2.clone()),
    })
    .await
    .expect("Failed to save updated interaction");

    // Verify the interaction was updated (not duplicated)
    let mut get_stream = db
      .execute(GetInteractionByUuid(test_uuid))
      .await
      .expect("Failed to get updated interaction");

    let retrieved = get_stream
      .next()
      .await
      .expect("Stream should have an item")
      .expect("Failed to get interaction from stream");

    assert_eq!(retrieved.name, "Updated Interaction");
    assert_eq!(retrieved.content, "Updated content");
    assert_eq!(retrieved.source, ContentType::Json);
    assert_eq!(
      retrieved.labels,
      vec!["updated".to_string(), "modified".to_string()]
    );
    assert_eq!(retrieved.reference_edges, vec!["new_edge".to_string()]);
  }
}
