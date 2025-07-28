use crate::Config;
use crate::Result;
use kuzu::{Connection, Database, SystemConfig, Value};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// GraphDb provides a thread-safe async interface to KuzuDB
///
/// This is a simplified version that lives in the db module,
/// avoiding dependencies on other modules.
#[derive(Clone)]
pub struct GraphDb {
  // The database instance - wrapped in Arc<RwLock> for thread-safe access
  pub(super) db: Arc<RwLock<Database>>,
  pub(super) path: PathBuf,
}

impl std::fmt::Debug for GraphDb {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GraphDb").field("path", &self.path).finish()
  }
}

impl GraphDb {
  pub fn new(config: &Config) -> Result<Self> {
    let db_path = PathBuf::from(&config.graph_data_dir());

    // Create the database instance
    let db = Database::new(&db_path, SystemConfig::default())?;

    // Wrap the database in Arc<RwLock> for thread-safe access
    let db = Arc::new(RwLock::new(db));

    Ok(Self { db, path: db_path })
  }

  /// Execute a write query (CREATE, MERGE, DELETE, etc.)
  /// For Kuzu as a secondary index, we don't care about results
  pub async fn execute_write(&self, query: &str, params: Vec<(&'static str, Value)>) -> Result<()> {
    let db = self.db.clone();
    let query = query.to_string();

    tokio::task::spawn_blocking(move || {
      let db_guard = db.write();
      let conn = Connection::new(&db_guard)?;

      if params.is_empty() {
        conn.query(&query)?;
      } else {
        let mut prepared = conn.prepare(&query)?;
        let params: Vec<(&str, Value)> = params
          .iter()
          .map(|(k, v)| (k.as_ref(), v.clone()))
          .collect();
        conn.execute(&mut prepared, params)?;
      };

      Ok(())
    })
    .await
    .expect("Graph database write task should not panic")
  }

  /// Execute a query and return a channel receiver for results
  ///
  /// This method spawns a blocking task that holds the connection and
  /// streams results one by one through a channel. The receiver can be
  /// converted to a futures Stream using tokio_stream.
  pub async fn execute_query(
    &self,
    query: &str,
    params: Vec<(&'static str, Value)>,
  ) -> Result<tokio::sync::mpsc::UnboundedReceiver<Result<Vec<Value>>>> {
    let db = self.db.clone();
    let query = query.to_string();

    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn_blocking(move || {
      // This runs in a blocking thread
      let result: Result<()> = (|| {
        let db_guard = db.read();
        let conn = Connection::new(&db_guard)?;

        let mut query_result = if params.is_empty() {
          conn.query(&query)?
        } else {
          let mut prepared = conn.prepare(&query)?;
          let params: Vec<(&str, Value)> = params
            .iter()
            .map(|(k, v)| (k.as_ref(), v.clone()))
            .collect();
          conn.execute(&mut prepared, params)?
        };

        // Stream results one by one
        while let Some(row) = query_result.next() {
          if sender.send(Ok(row)).is_err() {
            // Receiver dropped
            break;
          }
        }

        Ok(())
      })();

      // If there was an error executing the query, send it
      if let Err(e) = result {
        let _ = sender.send(Err(e));
      }
    });

    Ok(receiver)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;
  use std::path::PathBuf;
  use tempfile::tempdir;
  use tokio_stream::wrappers::UnboundedReceiverStream;

  fn test_config(path: &std::path::Path) -> Config {
    Config::new_test(path.to_path_buf())
  }

  async fn setup_test_db() -> (GraphDb, tempfile::TempDir) {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = GraphDb::new(&config).expect("Failed to create GraphDb");

    // Create a simple schema for testing
    db.execute_write(
      "CREATE NODE TABLE TestNode (id INT64, name STRING, PRIMARY KEY(id))",
      vec![],
    )
    .await
    .expect("Failed to create test schema");

    (db, temp_dir)
  }

  #[tokio::test]
  async fn test_basic_query() {
    let (db, _temp_dir) = setup_test_db().await;

    // Insert data
    db.execute_write("CREATE (:TestNode {id: 1, name: 'Node1'})", vec![])
      .await
      .expect("Failed to insert data");

    // Query data
    let receiver = db
      .execute_query("MATCH (n:TestNode) RETURN n.id, n.name", vec![])
      .await
      .expect("Failed to query data");

    let mut result = UnboundedReceiverStream::new(receiver);
    let row = result
      .next()
      .await
      .expect("Should have one row")
      .expect("Row should be Ok");
    assert_eq!(row.len(), 2);

    match &row[0] {
      Value::Int64(v) => assert_eq!(*v, 1),
      _ => panic!("Expected Int64"),
    }

    match &row[1] {
      Value::String(v) => assert_eq!(v, "Node1"),
      _ => panic!("Expected String"),
    }
  }

  #[tokio::test]
  async fn test_parameterized_query() {
    let (db, _temp_dir) = setup_test_db().await;

    // Insert data with parameters
    db.execute_write(
      "CREATE (:TestNode {id: $id, name: $name})",
      vec![
        ("id", Value::Int64(1)),
        ("name", Value::String("Node1".to_string())),
      ],
    )
    .await
    .expect("Failed to insert data");

    // Query with parameters
    let receiver = db
      .execute_query(
        "MATCH (n:TestNode) WHERE n.id = $id RETURN n.name",
        vec![("id", Value::Int64(1))],
      )
      .await
      .expect("Failed to query data");

    let mut result = UnboundedReceiverStream::new(receiver);
    let row = result
      .next()
      .await
      .expect("Should have one row")
      .expect("Row should be Ok");
    match &row[0] {
      Value::String(v) => assert_eq!(v, "Node1"),
      _ => panic!("Expected String"),
    }
  }

  #[tokio::test]
  async fn test_concurrent_reads() {
    let (db, _temp_dir) = setup_test_db().await;

    // Insert multiple test nodes
    for i in 1..=10 {
      db.execute_write(
        &format!("CREATE (:TestNode {{id: {}, name: 'Node{}' }})", i, i),
        vec![],
      )
      .await
      .expect("Failed to insert test data");
    }

    // Perform concurrent reads
    let mut handles = Vec::new();

    for i in 1..=5 {
      let db_clone = db.clone();
      let handle = tokio::spawn(async move {
        let receiver = db_clone
          .execute_query(
            &format!("MATCH (n:TestNode) WHERE n.id = {} RETURN n.name", i),
            vec![],
          )
          .await?;

        let mut stream = UnboundedReceiverStream::new(receiver);
        let row = stream
          .next()
          .await
          .ok_or_else(|| crate::error::Error::Generic("No results".to_string()))??;

        Ok::<Vec<Value>, crate::error::Error>(row)
      });
      handles.push(handle);
    }

    // Wait for all reads to complete
    let results = futures::future::try_join_all(handles)
      .await
      .expect("Tasks should not panic");

    // Verify all operations succeeded
    for result in results {
      assert!(result.is_ok(), "Read operation should succeed");
    }
  }

  #[tokio::test]
  async fn test_sequential_writes() {
    let (db, _temp_dir) = setup_test_db().await;

    // Perform sequential writes
    for i in 1..=5 {
      db.execute_write(
        &format!("CREATE (:TestNode {{id: {}, name: 'Node{}' }})", i, i),
        vec![],
      )
      .await
      .expect("Write operation should succeed");
    }

    // Verify all nodes were created
    let receiver = db
      .execute_query("MATCH (n:TestNode) RETURN count(n)", vec![])
      .await
      .expect("Read should succeed");

    let mut stream = UnboundedReceiverStream::new(receiver);
    let row = stream
      .next()
      .await
      .expect("Should have result")
      .expect("Row should be Ok");
    match &row[0] {
      Value::Int64(v) => assert_eq!(*v, 5, "Should have created 5 nodes"),
      _ => panic!("Expected Int64 value for node count"),
    }
  }

  #[tokio::test]
  async fn test_invalid_query() {
    let (db, _temp_dir) = setup_test_db().await;

    let receiver = db
      .execute_query("INVALID QUERY SYNTAX", vec![])
      .await
      .expect("Should return receiver even for invalid query");

    // The error appears when we try to read from the stream
    let mut stream = UnboundedReceiverStream::new(receiver);
    let result = stream.next().await;

    assert!(result.is_some(), "Should have an error result");
    assert!(
      result.unwrap().is_err(),
      "Invalid query should produce error in stream"
    );
  }

  #[tokio::test]
  async fn test_concurrent_reads_and_writes() {
    let (db, _temp_dir) = setup_test_db().await;

    // Spawn 5 concurrent write operations
    let mut write_handles = Vec::new();
    for i in 1..=5 {
      let db_clone = db.clone();
      let handle = tokio::spawn(async move {
        db_clone
          .execute_write(
            &format!(
              "CREATE (:TestNode {{id: {}, name: 'ConcurrentNode{}' }})",
              i, i
            ),
            vec![],
          )
          .await
      });
      write_handles.push(handle);
    }

    // Spawn 10 concurrent read operations
    let mut read_handles = Vec::new();
    for _ in 0..10 {
      let db_clone = db.clone();
      let handle = tokio::spawn(async move {
        let receiver = db_clone
          .execute_query("MATCH (n:TestNode) RETURN count(n)", vec![])
          .await?;

        let mut stream = UnboundedReceiverStream::new(receiver);
        let _row = stream
          .next()
          .await
          .ok_or_else(|| crate::error::Error::Generic("No results".to_string()))??;

        Ok::<(), crate::error::Error>(())
      });
      read_handles.push(handle);
    }

    // Wait for all operations to complete
    let write_results = futures::future::try_join_all(write_handles)
      .await
      .expect("Write tasks should not panic");
    let read_results = futures::future::try_join_all(read_handles)
      .await
      .expect("Read tasks should not panic");

    // All operations should complete without errors
    for result in write_results {
      assert!(result.is_ok(), "Write operation should succeed");
    }

    for result in read_results {
      assert!(result.is_ok(), "Read operation should succeed");
    }
  }

  #[tokio::test]
  async fn test_high_load() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create initial data
    for i in 1..=10 {
      db.execute_write(
        &format!("CREATE (:TestNode {{id: {}, name: 'LoadNode{}' }})", i, i),
        vec![],
      )
      .await
      .expect("Write should succeed");
    }

    // Perform many operations in quick succession
    let mut handles = Vec::new();

    for i in 0..50 {
      let db_clone = db.clone();
      let op_type = i % 3; // Mix of reads and writes

      let handle = tokio::spawn(async move {
        let result: Result<(), crate::error::Error> = match op_type {
          0 => {
            let receiver = db_clone
              .execute_query("MATCH (n:TestNode) RETURN count(n)", vec![])
              .await?;
            let mut stream = UnboundedReceiverStream::new(receiver);
            let _row = stream
              .next()
              .await
              .ok_or_else(|| crate::error::Error::Generic("No results".to_string()))??;
            Ok(())
          }
          1 => {
            let receiver = db_clone
              .execute_query(
                &format!(
                  "MATCH (n:TestNode) WHERE n.id = {} RETURN n.name",
                  (i % 10) + 1
                ),
                vec![],
              )
              .await?;
            let mut stream = UnboundedReceiverStream::new(receiver);
            let _row = stream
              .next()
              .await
              .ok_or_else(|| crate::error::Error::Generic("No results".to_string()))??;
            Ok(())
          }
          _ => {
            db_clone
              .execute_write(
                &format!(
                  "CREATE (:TestNode {{id: {}, name: 'WriteNode{}' }})",
                  100 + i,
                  i
                ),
                vec![],
              )
              .await?;
            Ok(())
          }
        };
        result
      });

      handles.push(handle);

      // Add small delay to simulate slightly staggered requests
      if i % 5 == 0 {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
      }
    }

    // Wait for all operations to complete
    let results = futures::future::try_join_all(handles)
      .await
      .expect("Tasks should not panic");

    // All operations should succeed - we're a bomb-proof database
    for (i, result) in results.iter().enumerate() {
      assert!(
        result.is_ok(),
        "Operation {} failed: {:?}",
        i,
        result.as_ref().err()
      );
    }
  }

  #[tokio::test]
  async fn test_streaming_functionality() {
    let (db, _temp_dir) = setup_test_db().await;

    // Insert test data
    for i in 1..=5 {
      db.execute_write(
        &format!("CREATE (:TestNode {{id: {}, name: 'Node{}' }})", i, i),
        vec![],
      )
      .await
      .expect("Failed to insert test data");
    }

    // Read back using streaming
    let receiver = db
      .execute_query(
        "MATCH (n:TestNode) RETURN n.id, n.name ORDER BY n.id",
        vec![],
      )
      .await
      .expect("Read should succeed");

    // Use stream to process results
    let mut count = 0;
    let mut ids = Vec::new();
    let mut stream = UnboundedReceiverStream::new(receiver);

    while let Some(result) = stream.next().await {
      let row = result.expect("Row should be Ok");
      if let Value::Int64(id) = &row[0] {
        ids.push(*id);
      }
      count += 1;
    }

    assert_eq!(count, 5, "Should have 5 rows");
    assert_eq!(ids, vec![1, 2, 3, 4, 5], "IDs should be in order");
  }

  #[tokio::test]
  async fn test_database_creation_fails_with_invalid_path() {
    // Create a config with an invalid path
    let invalid_config = Config::new_test(PathBuf::from("/path/that/should/not/exist/for/testing"));

    let result = GraphDb::new(&invalid_config);
    assert!(
      result.is_err(),
      "GraphDb::new should fail with invalid path"
    );
  }

  // Tests to explore Kuzu syntax for Episode-like operations

  #[tokio::test]
  async fn test_list_contains_function() {
    let _temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(_temp_dir.path());
    let db = GraphDb::new(&config).expect("Failed to create GraphDb");

    // Create schema with group_id field for our test
    db.execute_write(
      "CREATE NODE TABLE GroupTestNode (id INT64, group_id STRING, PRIMARY KEY(id))",
      vec![],
    )
    .await
    .expect("Failed to create schema");

    // Insert test data for list_contains testing
    db.execute_write(
      "CREATE (:GroupTestNode {id: 1, group_id: 'group-a'})",
      vec![],
    )
    .await
    .expect("Failed to insert test data");
    db.execute_write(
      "CREATE (:GroupTestNode {id: 2, group_id: 'group-b'})",
      vec![],
    )
    .await
    .expect("Failed to insert test data");
    db.execute_write(
      "CREATE (:GroupTestNode {id: 3, group_id: 'group-a'})",
      vec![],
    )
    .await
    .expect("Failed to insert test data");

    // Test simple IN clause (this should work)

    let receiver = db
      .execute_query(
        "MATCH (n:GroupTestNode) WHERE n.group_id IN ['group-a'] RETURN n.id",
        vec![],
      )
      .await
      .expect("IN clause should work");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut count = 0;
    while let Some(row_result) = result.next().await {
      row_result.expect("Row should be Ok");
      count += 1;
    }

    assert_eq!(count, 2, "Should find 2 episodes with group-a");

    // Test list_contains function (this might fail)

    let receiver = db
      .execute_query(
        "MATCH (n:GroupTestNode) WHERE list_contains(['group-a'], n.group_id) RETURN n.id",
        vec![],
      )
      .await;

    match receiver {
      Ok(receiver) => {
        let mut result = UnboundedReceiverStream::new(receiver);
        let mut count = 0;
        while let Some(row_result) = result.next().await {
          match row_result {
            Ok(_row) => count += 1,
            Err(e) => {
              break;
            }
          }
        }

        if count > 0 {
          assert_eq!(
            count, 2,
            "list_contains should find 2 episodes with group-a"
          );
        }
      }
      Err(e) => {

        // This is fine - it means Kuzu doesn't support list_contains
        // We need to use IN clause instead
      }
    }
  }

  #[tokio::test]
  async fn test_episode_schema_and_basic_operations() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create Episode-like schema to match our real schema
    db.execute_write(
      r#"
      CREATE NODE TABLE Episode (
        uuid UUID,
        group_id STRING,
        source STRING,
        valid_at TIMESTAMP,
        created_at TIMESTAMP,
        kumos_version STRING,
        PRIMARY KEY (uuid)
      )
      "#,
      vec![],
    )
    .await
    .expect("Failed to create Episode schema");

    let test_uuid = uuid::Uuid::now_v7();
    let now = time::OffsetDateTime::now_utc();

    // Insert an episode
    db.execute_write(
      r#"
      CREATE (:Episode {
        uuid: $uuid,
        group_id: $group_id,
        source: $source,
        valid_at: $valid_at,
        created_at: $created_at,
        kumos_version: $version
      })
      "#,
      vec![
        ("uuid", Value::UUID(test_uuid)),
        ("group_id", Value::String("test-group".to_string())),
        ("source", Value::String("Message".to_string())),
        ("valid_at", Value::Timestamp(now)),
        ("created_at", Value::Timestamp(now)),
        ("version", Value::String("v1.0.0".to_string())),
      ],
    )
    .await
    .expect("Failed to insert episode");

    // Test basic query
    let receiver = db
      .execute_query("MATCH (n:Episode) RETURN n.uuid, n.group_id", vec![])
      .await
      .expect("Failed to query episodes");

    let mut result = UnboundedReceiverStream::new(receiver);
    let row = result
      .next()
      .await
      .expect("Should have one row")
      .expect("Row should be Ok");

    assert_eq!(row.len(), 2);
    match &row[0] {
      Value::UUID(uuid) => assert_eq!(*uuid, test_uuid),
      _ => panic!("Expected UUID, got {:?}", row[0]),
    }
    match &row[1] {
      Value::String(group_id) => assert_eq!(group_id, "test-group"),
      _ => panic!("Expected String, got {:?}", row[1]),
    }
  }

  #[tokio::test]
  async fn test_kuzu_group_id_filtering_syntax() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create Episode schema
    db.execute_write(
      "CREATE NODE TABLE Episode (uuid UUID, group_id STRING, PRIMARY KEY (uuid))",
      vec![],
    )
    .await
    .expect("Failed to create schema");

    // Insert test episodes with different group_ids
    let test_data = vec![
      (uuid::Uuid::now_v7(), "group-a"),
      (uuid::Uuid::now_v7(), "group-b"),
      (uuid::Uuid::now_v7(), "group-a"),
      (uuid::Uuid::now_v7(), "group-c"),
    ];

    for (uuid, group_id) in &test_data {
      db.execute_write(
        "CREATE (:Episode {uuid: $uuid, group_id: $group_id})",
        vec![
          ("uuid", Value::UUID(*uuid)),
          ("group_id", Value::String(group_id.to_string())),
        ],
      )
      .await
      .expect("Failed to insert episode");
    }

    // Test 1: Simple equality filter

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) WHERE n.group_id = 'group-a' RETURN n.uuid",
        vec![],
      )
      .await
      .expect("Failed to query with equality");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut count = 0;
    while let Some(row_result) = result.next().await {
      let _row = row_result.expect("Row should be Ok");
      count += 1;
    }
    assert_eq!(count, 2, "Should find 2 episodes with group-a");

    // Test 2: IN clause syntax

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) WHERE n.group_id IN ['group-a', 'group-b'] RETURN n.uuid",
        vec![],
      )
      .await
      .expect("Failed to query with IN clause");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut count = 0;
    while let Some(row_result) = result.next().await {
      let _row = row_result.expect("Row should be Ok");
      count += 1;
    }
    assert_eq!(count, 3, "Should find 3 episodes with group-a or group-b");

    // Test 3: Try list_contains function (this might be the issue)

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) WHERE list_contains(['group-a', 'group-b'], n.group_id) RETURN n.uuid",
        vec![],
      )
      .await;

    match receiver {
      Ok(receiver) => {
        let mut result = UnboundedReceiverStream::new(receiver);
        let mut count = 0;
        while let Some(row_result) = result.next().await {
          match row_result {
            Ok(_row) => count += 1,
            Err(e) => {
              break;
            }
          }
        }
      }
      Err(e) => {

        // This might be the issue - let's try alternative syntax
      }
    }

    // Test 4: Alternative - using ANY or EXISTS

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) WHERE n.group_id = ANY(['group-a', 'group-b']) RETURN n.uuid",
        vec![],
      )
      .await;

    match receiver {
      Ok(receiver) => {
        let mut result = UnboundedReceiverStream::new(receiver);
        let mut count = 0;
        while let Some(row_result) = result.next().await {
          match row_result {
            Ok(_row) => count += 1,
            Err(e) => {
              break;
            }
          }
        }
      }
      Err(e) => {}
    }
  }

  #[tokio::test]
  async fn test_kuzu_uuid_ordering() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create Episode schema
    db.execute_write(
      "CREATE NODE TABLE Episode (uuid UUID, name STRING, PRIMARY KEY (uuid))",
      vec![],
    )
    .await
    .expect("Failed to create schema");

    // Insert episodes with known UUIDs to test ordering
    let mut uuids = Vec::new();
    for i in 0..5 {
      // Small delay to ensure different UUID v7 timestamps
      tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
      let uuid = uuid::Uuid::now_v7();
      uuids.push(uuid);

      db.execute_write(
        "CREATE (:Episode {uuid: $uuid, name: $name})",
        vec![
          ("uuid", Value::UUID(uuid)),
          ("name", Value::String(format!("Episode {}", i))),
        ],
      )
      .await
      .expect("Failed to insert episode");
    }

    // Test ordering by UUID DESC

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) RETURN n.uuid, n.name ORDER BY n.uuid DESC",
        vec![],
      )
      .await
      .expect("Failed to query with UUID ordering");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut returned_uuids = Vec::new();
    while let Some(row_result) = result.next().await {
      let row = row_result.expect("Row should be Ok");
      if let Value::UUID(uuid) = &row[0] {
        returned_uuids.push(*uuid);
        if let Value::String(name) = &row[1] {}
      }
    }

    assert_eq!(returned_uuids.len(), 5, "Should return all 5 episodes");

    // Verify descending order (newest first)
    for i in 0..returned_uuids.len() - 1 {
      assert!(
        returned_uuids[i] > returned_uuids[i + 1],
        "UUIDs should be in descending order"
      );
    }
  }

  #[tokio::test]
  async fn test_episode_table_and_data() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config = test_config(temp_dir.path());
    let db = GraphDb::new(&config).expect("Failed to create GraphDb");

    // Create Episode schema like the migration does
    db.execute_write(
      r#"
      CREATE NODE TABLE IF NOT EXISTS Episode (
          uuid UUID,
          group_id STRING,
          source STRING,
          valid_at TIMESTAMP,
          created_at TIMESTAMP,
          kumos_version STRING,
          PRIMARY KEY (uuid)
      )"#,
      vec![],
    )
    .await
    .expect("Failed to create Episode schema");

    // Insert some test episodes
    let now = time::OffsetDateTime::now_utc();
    let episode1_uuid = uuid::Uuid::now_v7();
    let episode2_uuid = uuid::Uuid::now_v7();

    db.execute_write(
      "CREATE (:Episode {uuid: $uuid, group_id: $group_id, source: $source, valid_at: $valid_at, created_at: $created_at, kumos_version: $version})",
      vec![
        ("uuid", kuzu::Value::UUID(episode1_uuid)),
        ("group_id", kuzu::Value::String("group-a".to_string())),
        ("source", kuzu::Value::String("Message".to_string())),
        ("valid_at", kuzu::Value::Timestamp(now)),
        ("created_at", kuzu::Value::Timestamp(now)),
        ("version", kuzu::Value::String("v1.0.0".to_string())),
      ],
    )
    .await
    .expect("Failed to insert episode 1");

    db.execute_write(
      "CREATE (:Episode {uuid: $uuid, group_id: $group_id, source: $source, valid_at: $valid_at, created_at: $created_at, kumos_version: $version})",
      vec![
        ("uuid", kuzu::Value::UUID(episode2_uuid)),
        ("group_id", kuzu::Value::String("group-a".to_string())),
        ("source", kuzu::Value::String("Message".to_string())),
        ("valid_at", kuzu::Value::Timestamp(now)),
        ("created_at", kuzu::Value::Timestamp(now)),
        ("version", kuzu::Value::String("v1.0.0".to_string())),
      ],
    )
    .await
    .expect("Failed to insert episode 2");

    // Test the exact query used in GetEpisodesByGroupIds

    let group_ids_list = "'group-a'";
    let cypher = format!(
      "MATCH (n:Episode)
       WHERE list_contains([{}], n.group_id)
       RETURN n.uuid
       ORDER BY n.uuid DESC",
      group_ids_list
    );

    let receiver = db
      .execute_query(&cypher, vec![])
      .await
      .expect("Query should work");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut count = 0;
    let mut found_uuids = Vec::new();
    while let Some(row_result) = result.next().await {
      let row = row_result.expect("Row should be Ok");
      if let Some(kuzu::Value::UUID(uuid)) = row.first() {
        found_uuids.push(*uuid);
        count += 1;
      }
    }

    assert_eq!(count, 2, "Should find 2 episodes with group-a");
    assert!(found_uuids.contains(&episode1_uuid));
    assert!(found_uuids.contains(&episode2_uuid));

    // Also test a basic count query

    let receiver = db
      .execute_query("MATCH (n:Episode) RETURN count(n)", vec![])
      .await
      .expect("Count query should work");
    let mut result = UnboundedReceiverStream::new(receiver);
    let row = result
      .next()
      .await
      .expect("Should have count result")
      .expect("Row should be Ok");
    if let kuzu::Value::Int64(total_count) = &row[0] {
      assert_eq!(*total_count, 2, "Should have 2 total episodes");
    } else {
      panic!("Expected Int64 count");
    }
  }

  #[tokio::test]
  async fn test_kuzu_timestamp_operations() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create Episode schema with timestamps
    db.execute_write(
      "CREATE NODE TABLE Episode (uuid UUID, valid_at TIMESTAMP, created_at TIMESTAMP, PRIMARY KEY (uuid))",
      vec![],
    )
    .await
    .expect("Failed to create schema");

    let base_time = time::OffsetDateTime::now_utc();
    let mut episodes = Vec::new();

    // Insert episodes with different timestamps
    for i in 0..3 {
      let uuid = uuid::Uuid::now_v7();
      let valid_at = base_time - time::Duration::hours(i);
      let created_at = base_time;

      episodes.push((uuid, valid_at));

      db.execute_write(
        "CREATE (:Episode {uuid: $uuid, valid_at: $valid_at, created_at: $created_at})",
        vec![
          ("uuid", Value::UUID(uuid)),
          ("valid_at", Value::Timestamp(valid_at)),
          ("created_at", Value::Timestamp(created_at)),
        ],
      )
      .await
      .expect("Failed to insert episode");
    }

    // Test timestamp filtering (like our retrieve_episodes query)

    let reference_time = base_time - time::Duration::minutes(30); // Between hour 0 and 1

    let receiver = db
      .execute_query(
        "MATCH (n:Episode) WHERE n.valid_at <= $reference_time RETURN n.uuid ORDER BY n.valid_at DESC",
        vec![("reference_time", Value::Timestamp(reference_time))],
      )
      .await
      .expect("Failed to query with timestamp filter");

    let mut result = UnboundedReceiverStream::new(receiver);
    let mut count = 0;
    while let Some(row_result) = result.next().await {
      let row = row_result.expect("Row should be Ok");
      count += 1;
      if let Value::UUID(uuid) = &row[0] {}
    }

    // Should find 2 episodes (the ones from 1 and 2 hours ago)
    assert_eq!(
      count, 2,
      "Should find episodes with valid_at <= reference_time"
    );
  }
}
