use std::collections::HashMap;
use std::hash::Hash;
use std::task::{Context, Poll};

use futures::Stream;
use pin_project_lite::pin_project;

use crate::Result;
use crate::streams::{DefaultStream, ResultStream};

pin_project! {
    /// A stream that yields items in a specific order by joining two streams:
    /// - A stream of keys defining the desired order
    /// - A stream of (key, item) pairs in arbitrary order
    pub struct OrderedStream<T, K> {
        #[pin]
        keys: DefaultStream<K>,
        #[pin]
        items: ResultStream<(K, T)>,
        buffer: HashMap<K, T>,
        items_done: bool,
        current_key: Option<K>,
    }
}

impl<T, K> OrderedStream<T, K>
where
  K: Hash + Eq,
{
  pub fn new<KS, IS>(keys: KS, items: IS) -> Self
  where
    KS: Stream<Item = K> + Send + 'static,
    IS: Stream<Item = Result<(K, T)>> + Send + 'static,
  {
    Self {
      keys: Box::pin(keys),
      items: Box::pin(items),
      buffer: HashMap::new(),
      items_done: false,
      current_key: None,
    }
  }
}

impl<T, K> Stream for OrderedStream<T, K>
where
  T: Send,
  K: Hash + Eq + Send + std::fmt::Debug,
{
  type Item = Result<T>;

  fn poll_next(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut this = self.project();

    loop {
      // Get the current key we're processing, or get the next one if we don't have one
      if this.current_key.is_none() {
        match this.keys.as_mut().poll_next(cx) {
          Poll::Ready(Some(key)) => {
            *this.current_key = Some(key);
          }
          Poll::Ready(None) => {
            // No more keys
            return Poll::Ready(None);
          }
          Poll::Pending => {
            return Poll::Pending;
          }
        }
      }

      // We have a current key, process it until we find it or exhaust items stream
      if let Some(key) = this.current_key.as_ref() {
        // Check if we already have this item buffered
        if let Some(item) = this.buffer.remove(key) {
          *this.current_key = None; // Move to next key
          return Poll::Ready(Some(Ok(item)));
        }

        // Don't have the item in buffer, poll items stream until we find it or stream ends
        if !*this.items_done {
          loop {
            match this.items.as_mut().poll_next(cx) {
              Poll::Ready(Some(Ok((item_key, item)))) => {
                if item_key == *key {
                  // Found the item we're looking for!
                  *this.current_key = None; // Move to next key
                  return Poll::Ready(Some(Ok(item)));
                } else {
                  // Buffer this item for later and continue looking
                  this.buffer.insert(item_key, item);
                  continue; // Keep looking for our key
                }
              }
              Poll::Ready(Some(Err(e))) => {
                return Poll::Ready(Some(Err(e)));
              }
              Poll::Ready(None) => {
                *this.items_done = true;
                break; // Exit items polling loop
              }
              Poll::Pending => {
                return Poll::Pending;
              }
            }
          }
        }

        // Items stream is exhausted, check if the item is now in buffer
        if let Some(item) = this.buffer.remove(key) {
          *this.current_key = None; // Move to next key
          return Poll::Ready(Some(Ok(item)));
        }

        // Item not found and items stream is done, skip this key and move to next one
        *this.current_key = None; // Move to next key
        continue; // Go back to get next key
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::{StreamExt, stream};

  #[tokio::test]
  async fn test_ordered_stream() {
    // Unordered data stream
    let items = vec![Ok(("b", "Beta")), Ok(("a", "Alpha")), Ok(("c", "Gamma"))];
    let items_stream = stream::iter(items);

    // Define the order we want
    let keys = vec!["a", "b", "c"];
    let key_stream = stream::iter(keys);

    // Create ordered stream
    let ordered = OrderedStream::new(key_stream, items_stream);

    // Collect results
    let results: Vec<_> = ordered.collect::<Vec<_>>().await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap(), &"Alpha");
    assert_eq!(results[1].as_ref().unwrap(), &"Beta");
    assert_eq!(results[2].as_ref().unwrap(), &"Gamma");
  }

  #[tokio::test]
  async fn test_ordered_stream_with_missing_items() {
    // Items stream missing "b"
    let items = vec![Ok(("a", "Alpha")), Ok(("c", "Gamma"))];
    let items_stream = stream::iter(items);

    let keys = vec!["a", "b", "c", "d"];
    let key_stream = stream::iter(keys);

    let ordered = OrderedStream::new(key_stream, items_stream);

    let results: Vec<_> = ordered.filter_map(|r| async { r.ok() }).collect().await;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], "Alpha");
    assert_eq!(results[1], "Gamma");
  }

  #[tokio::test]
  async fn test_ordered_stream_buffering() {
    // Items arrive out of order
    let items = vec![Ok(("c", "Gamma")), Ok(("b", "Beta")), Ok(("a", "Alpha"))];
    let items_stream = stream::iter(items);

    let keys = vec!["a", "b", "c"];
    let key_stream = stream::iter(keys);

    let ordered = OrderedStream::new(key_stream, items_stream);

    let results: Vec<_> = ordered.collect::<Vec<_>>().await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap(), &"Alpha");
    assert_eq!(results[1].as_ref().unwrap(), &"Beta");
    assert_eq!(results[2].as_ref().unwrap(), &"Gamma");
  }

  #[tokio::test]
  async fn test_ordered_stream_with_channel() {
    // Test async pattern with spawned coroutine and channel
    // This mimics the real-world usage in GetEpisodesByGroupIds
    let keys = vec!["key1", "key2"];
    let key_stream = futures::stream::iter(keys);

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(async move {
      // Send items in different order than keys
      let items = vec![
        ("key2", "Second Item"), // This sends first
        ("key1", "First Item"),  // This sends second
      ];

      for (key, value) in items {
        if tx.send(Ok((key, value))).is_err() {
          return;
        }
      }
      // Keep sender alive briefly to ensure items are received
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    });

    // Small delay to let coroutine send items before we start polling
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

    let items_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    let ordered = OrderedStream::new(key_stream, Box::pin(items_stream));
    let results: Vec<_> = ordered.collect::<Vec<_>>().await;

    // Should get results in key order, not send order
    assert_eq!(results.len(), 2, "Should get both items back");
    assert_eq!(results[0].as_ref().unwrap(), &"First Item"); // key1's value
    assert_eq!(results[1].as_ref().unwrap(), &"Second Item"); // key2's value
  }

  #[tokio::test]
  async fn test_channel_timing_issue() {
    // Test to ensure channel works correctly even when sender is dropped quickly
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<(&str, &str)>>();

    // Spawn coroutine that sends items and immediately drops sender
    tokio::spawn(async move {
      let _ = tx.send(Ok(("key1", "value1")));
      let _ = tx.send(Ok(("key2", "value2")));
      // tx gets dropped here
    });

    // Create stream from receiver
    let mut items_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    // Poll the stream manually
    use futures::StreamExt;
    let mut count = 0;
    while let Some(item) = items_stream.next().await {
      count += 1;
      assert!(item.is_ok());
    }

    assert_eq!(
      count, 2,
      "Should receive both items even if sender is dropped"
    );
  }
}
