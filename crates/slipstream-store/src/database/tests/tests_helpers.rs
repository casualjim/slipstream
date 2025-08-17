#![cfg(test)]

use std::sync::Arc;

use arrow::datatypes::Schema as ArrowSchema;
use arrow_array::RecordBatch;
use futures::StreamExt as _;

use crate::Config;
use crate::operations::{DatabaseOperation, GraphIndexQuery, PrimaryStoreQuery, QueryOperation};
use crate::streams::ResultStream;
use crate::traits::DatabaseCommand;

// Initialize pretty test logging and backtraces once per process.
// Safe to call multiple times in tests due to subscriber init guarding.
// centralized in crate::tests via ctor in lib.rs
pub(crate) fn init_logging() { /* no-op; centralized */
}

pub(crate) fn test_config(path: &std::path::Path) -> Config {
  Config::new_test(path.to_path_buf())
}

// ---- Shared DatabaseCommand helpers for tests ----

pub(crate) struct CreateSchema {
  pub graph_ddl: Vec<&'static str>,
  pub meta_table: &'static str,
  pub meta_schema: Arc<ArrowSchema>,
}

impl DatabaseCommand for CreateSchema {
  type Output = ();
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Migration {
      graph_ddl: self.graph_ddl.clone(),
      meta_setup: Box::new({
        let table = self.meta_table;
        let schema = self.meta_schema.clone();
        move |conn| {
          let schema = schema.clone();
          Box::pin(async move {
            conn.create_empty_table(table, schema).execute().await?;
            Ok(())
          })
        }
      }),
      transformer: Box::new(|_| ()),
    }
  }
}

pub(crate) struct CountGraphItems {
  pub label: &'static str,
}

impl DatabaseCommand for CountGraphItems {
  type Output = ResultStream<usize>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    use crate::transformers::graph_stream;
    DatabaseOperation::Query(QueryOperation::IndexOnly {
      query: GraphIndexQuery {
        cypher: format!("MATCH (n:{}) RETURN count(n)", self.label),
        params: vec![],
      },
      transformer: Box::new(graph_stream::extract_count()),
    })
  }
}

pub(crate) struct CountTableRows {
  pub table: &'static str,
}

impl DatabaseCommand for CountTableRows {
  type Output = ResultStream<usize>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    DatabaseOperation::Query(QueryOperation::StoreOnly {
      query: PrimaryStoreQuery {
        table: self.table,
        filter: None,
        limit: None,
        offset: None,
        vector_search: None,
      },
      transformer: Box::new(|batch_stream| {
        Box::pin(async_stream::try_stream! {
          let mut total = 0usize;
          futures::pin_mut!(batch_stream);
          while let Some(batch) = batch_stream.next().await { total += batch?.num_rows(); }
          yield total;
        })
      }),
    })
  }
}

pub(crate) struct CountSystemMetadataForTable {
  pub table: &'static str,
}

impl DatabaseCommand for CountSystemMetadataForTable {
  type Output = ResultStream<usize>;
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    use crate::transformers::graph_stream;
    DatabaseOperation::Query(QueryOperation::IndexOnly {
      query: GraphIndexQuery {
        cypher: "MATCH (sm:SystemMetadata {table_name: $table_name}) RETURN count(sm)".to_string(),
        params: vec![("table_name", kuzu::Value::String(self.table.to_string()))],
      },
      transformer: Box::new(graph_stream::extract_count()),
    })
  }
}

// Append rows to an existing Lance table via Migration meta_setup
// This allows tests to simulate Lance-only changes (e.g., crash scenarios)
pub(crate) struct AppendRows {
  pub table: &'static str,
  pub batch: RecordBatch,
}

impl DatabaseCommand for AppendRows {
  type Output = ();
  type SaveData = crate::operations::NoData;

  fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
    let table = self.table;
    let batch = self.batch.clone();
    DatabaseOperation::Migration {
      graph_ddl: vec![],
      meta_setup: Box::new(move |conn| {
        let batch = batch.clone();
        Box::pin(async move {
          // Open existing table and append the provided batch
          let schema = batch.schema();
          let batch_iter =
            arrow_array::RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
          let table_handle = conn.open_table(table).execute().await?;
          let _ = table_handle.add(Box::new(batch_iter)).execute().await?;
          Ok(())
        })
      }),
      transformer: Box::new(|_| ()),
    }
  }
}
