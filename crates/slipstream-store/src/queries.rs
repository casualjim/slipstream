//! Helper functions for building common query patterns

#[cfg(test)]
use crate::Result;
#[cfg(test)]
use futures::{Stream, StreamExt};
#[cfg(test)]
use std::fmt::Display;

/// Builds an IN clause for SQL/LanceDB queries from a stream of values
///
/// This function consumes a stream of values and builds a comma-separated
/// string of single-quoted values suitable for use in an IN clause.
///
/// # Example
/// ```ignore
/// let in_clause = build_in_clause_from_stream(uuid_stream).await?;
/// // Returns: "'uuid1', 'uuid2', 'uuid3'"
/// ```
#[cfg(test)]
pub async fn build_in_clause_from_stream<T, S>(mut stream: S) -> Result<(String, usize)>
where
  T: Display,
  S: Stream<Item = Result<T>> + Unpin,
{
  let mut in_clause = String::new();
  let mut count = 0;

  while let Some(item) = stream.next().await {
    let value = item?;
    if count > 0 {
      in_clause.push_str(", ");
    }
    in_clause.push('\'');
    in_clause.push_str(&value.to_string());
    in_clause.push('\'');
    count += 1;
  }

  Ok((in_clause, count))
}

/// Builds an IN clause from an iterator of values
///
/// # Example
/// ```ignore
/// let in_clause = build_in_clause_from_iter(&uuids);
/// // Returns: "'uuid1', 'uuid2', 'uuid3'"
/// ```
#[cfg(test)]
pub fn build_in_clause_from_iter<T, I>(values: I) -> String
where
  T: Display,
  I: IntoIterator<Item = T>,
{
  values
    .into_iter()
    .map(|v| format!("'{v}'"))
    .collect::<Vec<_>>()
    .join(", ")
}

/// Extracts UUIDs from a graph query result stream and builds an IN clause
///
/// This is a common pattern when querying the graph index first,
/// then using the results to query the primary store.
#[cfg(test)]
pub async fn extract_uuids_to_in_clause<S>(mut stream: S) -> Result<(String, Vec<uuid::Uuid>)>
where
  S: Stream<Item = Result<Vec<kuzu::Value>>> + Unpin,
{
  let mut uuids = Vec::new();
  let mut in_clause = String::new();

  while let Some(row_result) = stream.next().await {
    let row = row_result?;
    if let Some(kuzu::Value::UUID(id)) = row.first() {
      if !uuids.is_empty() {
        in_clause.push_str(", ");
      }
      in_clause.push('\'');
      in_clause.push_str(&id.to_string());
      in_clause.push('\'');
      uuids.push(*id);
    } else {
      return Err(crate::Error::Generic(
        "Expected UUID value in first column".to_string(),
      ));
    }
  }

  Ok((in_clause, uuids))
}

/// Creates a filter expression that always returns false
///
/// Useful when you need to return an empty result set
#[cfg(test)]
pub fn empty_filter() -> String {
  "1 = 0".to_string()
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::stream;

  #[tokio::test]
  async fn test_build_in_clause_from_stream() {
    let values = vec!["a", "b", "c"];
    let stream = stream::iter(values.into_iter().map(Ok::<_, crate::Error>));

    let (clause, count) = build_in_clause_from_stream(stream).await.unwrap();
    assert_eq!(clause, "'a', 'b', 'c'");
    assert_eq!(count, 3);
  }

  #[test]
  fn test_build_in_clause_from_iter() {
    let values = vec![1, 2, 3];
    let clause = build_in_clause_from_iter(&values);
    assert_eq!(clause, "'1', '2', '3'");
  }

  #[test]
  fn test_build_in_clause_from_iter_uuids() {
    use uuid::Uuid;

    let uuid1 = Uuid::nil();
    let uuid2 = Uuid::from_u128(1);
    let values = vec![uuid1, uuid2];

    let clause = build_in_clause_from_iter(&values);
    assert_eq!(
      clause,
      "'00000000-0000-0000-0000-000000000000', '00000000-0000-0000-0000-000000000001'"
    );
  }
}
