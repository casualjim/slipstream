//! Transformers for converting between different stream types and data formats
//!
//! This module provides reusable transformers for common patterns in the database layer.

use crate::Result;
use crate::ResultStream;
use arrow_array::RecordBatch;
use futures::{Stream, StreamExt};
use std::pin::Pin;

/// Common transformer utilities
pub mod common {

  use super::*;

  /// Creates a transformer that yields a single unit value
  /// Useful for operations that don't return data but need to signal completion
  pub fn unit_transformer() -> impl FnOnce(()) -> ResultStream<()> {
    |_| Box::pin(futures::stream::once(async { Ok(()) }))
  }

  /// Creates a transformer that returns unit directly (not as a stream)
  /// Useful for mutation operations that don't need streaming results
  pub fn unit_result() -> impl FnOnce(()) -> () {
    |_| ()
  }

  /// Creates a transformer that converts any input to an empty stream
  /// Useful for skip operations or when no results are expected
  pub fn empty_transformer<T: Send + 'static>() -> impl FnOnce(()) -> ResultStream<T> {
    |_| Box::pin(futures::stream::empty())
  }

  /// Creates a transformer that yields a single value
  pub fn single_value_transformer<T: Send + 'static>(
    value: T,
  ) -> impl FnOnce(()) -> ResultStream<T> {
    move |_| Box::pin(futures::stream::once(async move { Ok(value) }))
  }

  /// Creates a transformer that counts items in a stream
  pub fn count_transformer<T: Send + 'static>()
  -> impl FnOnce(Pin<Box<dyn Stream<Item = Result<T>> + Send>>) -> ResultStream<usize> {
    |stream| {
      Box::pin(async_stream::try_stream! {
          let mut count = 0;
          let mut stream = std::pin::pin!(stream);
          while let Some(result) = stream.next().await {
              result?; // Propagate errors
              count += 1;
          }
          yield count;
      })
    }
  }

  /// Creates a transformer that collects stream items into a vector
  pub fn collect_transformer<T: Send + 'static>()
  -> impl FnOnce(Pin<Box<dyn Stream<Item = Result<T>> + Send>>) -> ResultStream<Vec<T>> {
    |stream| {
      Box::pin(async_stream::try_stream! {
          let mut items = Vec::new();
          let mut stream = std::pin::pin!(stream);
          while let Some(result) = stream.next().await {
              items.push(result?);
          }
          yield items;
      })
    }
  }
}

/// Ordered stream transformers for maintaining order based on a key stream
pub mod ordered {
  use super::*;
  use crate::ordered::OrderedStream;
  use std::hash::Hash;

  /// Creates a transformer for ordered streams where items need to be emitted in a specific order
  pub fn ordered_stream_transformer<K, V>(
    key_stream: impl Stream<Item = K> + Send + 'static,
  ) -> impl FnOnce(Pin<Box<dyn Stream<Item = Result<(K, V)>> + Send + Unpin>>) -> ResultStream<V>
  where
    K: Eq + Hash + Clone + Send + std::fmt::Debug + 'static,
    V: Send + 'static,
  {
    move |items_stream| {
      let ordered = OrderedStream::new(Box::pin(key_stream), items_stream);
      Box::pin(ordered)
    }
  }

  /// Creates a transformer that maintains order based on a UUID stream
  /// This is a common pattern for entity queries
  pub fn uuid_ordered_transformer<T: Send + 'static>(
    uuid_stream: impl Stream<Item = uuid::Uuid> + Send + 'static,
  ) -> impl FnOnce(Pin<Box<dyn Stream<Item = Result<(uuid::Uuid, T)>> + Send + Unpin>>) -> ResultStream<T>
  {
    ordered_stream_transformer(uuid_stream)
  }
}

/// RecordBatch transformers for working with Arrow data
pub mod record_batch {
  use super::*;
  use crate::traits::FromRecordBatchRow;

  /// Transforms a RecordBatch stream into a stream of typed objects
  pub fn to_typed_stream<T: FromRecordBatchRow + Send + 'static>()
  -> impl FnOnce(ResultStream<RecordBatch>) -> ResultStream<T> {
    |batch_stream| {
      Box::pin(async_stream::try_stream! {
          let mut batch_stream = std::pin::pin!(batch_stream);
          while let Some(batch) = batch_stream.next().await {
              let batch = batch?;
              for row in 0..batch.num_rows() {
                  yield T::from_record_batch_row(&batch, row)?;
              }
          }
      })
    }
  }

  /// Transforms a RecordBatch stream into a stream of (key, value) pairs
  pub fn to_keyed_stream<K, V>(
    key_extractor: impl Fn(&arrow_array::RecordBatch, usize) -> Result<K> + Send + 'static,
    value_extractor: impl Fn(&arrow_array::RecordBatch, usize) -> Result<V> + Send + 'static,
  ) -> impl FnOnce(ResultStream<RecordBatch>) -> Pin<Box<dyn Stream<Item = Result<(K, V)>> + Send>>
  where
    K: Send + 'static,
    V: Send + 'static,
  {
    move |batch_stream| {
      Box::pin(async_stream::try_stream! {
          let mut batch_stream = std::pin::pin!(batch_stream);
          while let Some(batch) = batch_stream.next().await {
              let batch = batch?;
              for row in 0..batch.num_rows() {
                  let key = key_extractor(&batch, row)?;
                  let value = value_extractor(&batch, row)?;
                  yield (key, value);
              }
          }
      })
    }
  }

  /// Transforms a RecordBatch stream by applying a filter
  pub fn filter_stream<T: FromRecordBatchRow + Send + 'static>(
    predicate: impl Fn(&T) -> bool + Send + 'static,
  ) -> impl FnOnce(ResultStream<RecordBatch>) -> ResultStream<T> {
    move |batch_stream| {
      Box::pin(async_stream::try_stream! {
          let mut batch_stream = std::pin::pin!(batch_stream);
          while let Some(batch) = batch_stream.next().await {
              let batch = batch?;
              for row in 0..batch.num_rows() {
                  let item = T::from_record_batch_row(&batch, row)?;
                  if predicate(&item) {
                      yield item;
                  }
              }
          }
      })
    }
  }

  /// Counts total rows in a RecordBatch stream
  pub fn count_rows() -> impl FnOnce(ResultStream<RecordBatch>) -> ResultStream<usize> {
    |batch_stream| {
      Box::pin(async_stream::try_stream! {
          use futures::StreamExt;
          let mut total = 0;
          let mut batch_stream = std::pin::pin!(batch_stream);
          while let Some(batch) = batch_stream.next().await {
              let batch = batch?;
              total += batch.num_rows();
          }
          yield total;
      })
    }
  }

  /// Transforms a RecordBatch stream with distance scores for vector search results
  pub fn with_similarity_scores<T: FromRecordBatchRow + Send + 'static>(
    threshold: Option<f32>,
  ) -> impl FnOnce(ResultStream<RecordBatch>) -> ResultStream<(T, f32)> {
    move |batch_stream| {
      Box::pin(async_stream::try_stream! {
          let mut batch_stream = std::pin::pin!(batch_stream);
          while let Some(batch) = batch_stream.next().await {
              let batch = batch?;

              // Get the _distance column if it exists
              let distance_col = batch.column_by_name("_distance");

              for row in 0..batch.num_rows() {
                  let item = T::from_record_batch_row(&batch, row)?;

                  // Extract distance and convert to similarity
                  let similarity = if let Some(col) = distance_col {
                      let array = col.as_any()
                          .downcast_ref::<arrow_array::Float32Array>()
                          .ok_or_else(|| crate::Error::InvalidMetaDbData(
                              "_distance column is not Float32".into()
                          ))?;

                      let distance = array.value(row);
                      // Convert cosine distance to similarity (0 = identical, 2 = opposite)
                      1.0 - (distance / 2.0)
                  } else {
                      // Default similarity if no distance column
                      1.0
                  };

                  // Apply threshold if provided
                  if let Some(t) = threshold {
                      if similarity >= t {
                          yield (item, similarity);
                      }
                  } else {
                      yield (item, similarity);
                  }
              }
          }
      })
    }
  }
}

/// Graph stream transformers for working with Kuzu query results
pub mod graph_stream {
  use super::*;

  /// Extracts UUIDs from a graph query result stream
  pub fn extract_uuids() -> impl FnOnce(
    Pin<Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send>>,
  ) -> Pin<Box<dyn Stream<Item = Result<uuid::Uuid>> + Send>> {
    |graph_stream| {
      Box::pin(async_stream::try_stream! {
          let mut graph_stream = std::pin::pin!(graph_stream);
          while let Some(row_result) = graph_stream.next().await {
              let row = row_result?;
              if let Some(kuzu::Value::UUID(id)) = row.first() {
                  yield *id;
              } else {
                  Err(crate::Error::Generic(
                      "Expected UUID value in first column".to_string()
                  ))?;
              }
          }
      })
    }
  }

  /// Collects UUIDs from a graph stream into a vector
  pub fn collect_uuids() -> impl FnOnce(
    Pin<Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send>>,
  ) -> Pin<
    Box<dyn futures::Future<Output = Result<Vec<uuid::Uuid>>> + Send>,
  > {
    |graph_stream| {
      Box::pin(async move {
        let mut uuids = Vec::new();
        let mut graph_stream = std::pin::pin!(graph_stream);

        while let Some(row_result) = graph_stream.next().await {
          let row = row_result?;
          if let Some(kuzu::Value::UUID(id)) = row.first() {
            uuids.push(*id);
          } else {
            return Err(crate::Error::Generic(
              "Expected UUID value in first column".to_string(),
            ));
          }
        }

        Ok(uuids)
      })
    }
  }

  /// Extract string values from first column of graph results
  pub fn extract_strings() -> impl FnOnce(
    Pin<Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send>>,
  ) -> Pin<Box<dyn Stream<Item = Result<String>> + Send>> {
    |graph_stream| {
      Box::pin(async_stream::try_stream! {
          let mut graph_stream = std::pin::pin!(graph_stream);
          while let Some(row_result) = graph_stream.next().await {
              let row = row_result?;
              if let Some(kuzu::Value::String(s)) = row.first() {
                  yield s.clone();
              } else {
                  Err(crate::Error::Generic(
                      "Expected String value in first column".to_string()
                  ))?;
              }
          }
      })
    }
  }

  /// Extract a single count (Int64) from graph results
  pub fn extract_count() -> impl FnOnce(
    Pin<Box<dyn Stream<Item = Result<Vec<kuzu::Value>>> + Send>>,
  ) -> Pin<Box<dyn Stream<Item = Result<usize>> + Send>> {
    |graph_stream| {
      Box::pin(async_stream::try_stream! {
          let mut graph_stream = std::pin::pin!(graph_stream);
          if let Some(row_result) = graph_stream.next().await {
              let row = row_result?;
              if let Some(kuzu::Value::Int64(count)) = row.first() {
                  yield *count as usize;
              } else {
                  Err(crate::Error::Generic(
                      "Expected Int64 count value".to_string()
                  ))?;
              }
          } else {
              yield 0; // No results means count is 0
          }
      })
    }
  }
}
