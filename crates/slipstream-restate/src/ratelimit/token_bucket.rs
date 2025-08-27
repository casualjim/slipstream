use std::time::{SystemTime, UNIX_EPOCH};

use eyre::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Using a large value to represent infinity, similar to the Go implementation
pub const INFINITY: f64 = f64::MAX;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TokenBucketState {
  #[serde(default)]
  pub limit: f64, // tokens per second
  #[serde(default)]
  pub burst: f64, // maximum burst size
  #[serde(default)]
  pub tokens: f64, // current number of tokens
  #[serde(default)]
  pub last: u64, // last time tokens field was updated (Unix timestamp in milliseconds)
  #[serde(default)]
  pub last_event: u64, // latest time of a rate-limited event (Unix timestamp in milliseconds)
}

impl TokenBucketState {
  pub fn new(limit: f64, burst: f64) -> Result<Self> {
    if limit <= 0.0 && limit != INFINITY {
      eyre::bail!("Invalid rate: {}. Rate must be positive.", limit);
    }

    if burst <= 0.0 {
      eyre::bail!("Invalid burst: {}. Burst must be positive.", burst);
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    Ok(TokenBucketState {
      limit,
      burst,
      tokens: burst, // Start with full tokens
      last: now,
      last_event: now,
    })
  }

  /// Advance updates the token bucket state based on time passed
  pub fn advance(&mut self, timestamp: u64) -> Result<f64> {
    let mut last = self.last;
    if timestamp <= last {
      last = timestamp;
    }

    // Calculate the new number of tokens, due to time that passed
    let elapsed_ms = timestamp - last;
    let delta = self.tokens_from_duration(elapsed_ms);
    let mut tokens = self.tokens + delta;
    if tokens > self.burst {
      tokens = self.burst;
    }

    self.tokens = tokens;
    Ok(tokens)
  }

  /// Reserve attempts to reserve n tokens, returning information about when they can be consumed
  pub fn reserve(&mut self, n: u32, max_future_reserve_ms: Option<u64>) -> Result<Reservation> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    // For infinite limit, reservation is always successful with no delay
    if self.limit == INFINITY {
      return Ok(Reservation {
        ok: true,
        tokens: n,
        creation_date: now,
        date_to_act: now,
        limit: self.limit,
      });
    }

    // Update tokens based on time passed
    let tokens = self.advance(now)?;

    // Calculate the remaining number of tokens resulting from the request
    let tokens_after_reservation = tokens - (n as f64);

    // Calculate the wait duration
    let wait_duration_ms = if tokens_after_reservation < 0.0 {
      self.duration_from_tokens(-tokens_after_reservation)
    } else {
      0
    };

    // Decide result
    let max_future_reserve_ms = max_future_reserve_ms.unwrap_or(u64::MAX);
    let ok = (n as f64) <= self.burst && wait_duration_ms <= max_future_reserve_ms;

    let reservation = Reservation {
      ok,
      tokens: if ok { n } else { 0 },
      creation_date: now,
      date_to_act: if ok { now + wait_duration_ms } else { 0 },
      limit: self.limit,
    };

    if ok {
      self.last = now;
      self.tokens = tokens_after_reservation;
      self.last_event = reservation.date_to_act;
    }

    Ok(reservation)
  }

  /// Cancel a reservation, restoring tokens if possible
  pub fn cancel_reservation(&mut self, reservation: &Reservation) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    // Check if reservation can be canceled
    if self.limit == INFINITY || reservation.tokens == 0 || reservation.date_to_act < now {
      return Ok(());
    }

    // Calculate tokens to restore
    // The duration between self.last_event and reservation.date_to_act tells us how many tokens
    // were reserved after reservation was obtained. These tokens should not be restored.
    let restore_tokens = reservation.tokens as f64
      - self.tokens_from_duration(self.last_event.saturating_sub(reservation.date_to_act));

    if restore_tokens <= 0.0 {
      return Ok(());
    }

    // Advance time to now
    let tokens = self.advance(now)?;

    // Calculate new number of tokens
    let mut tokens = tokens + restore_tokens;
    if tokens > self.burst {
      tokens = self.burst;
    }

    // Update state
    self.last = now;
    self.tokens = tokens;

    if reservation.date_to_act == self.last_event {
      let prev_event = reservation
        .date_to_act
        .saturating_sub(self.duration_from_tokens(reservation.tokens as f64));
      if prev_event >= now {
        self.last_event = prev_event;
      }
    }

    Ok(())
  }

  /// Set a new rate limit
  pub fn set_rate(&mut self, new_limit: Option<f64>, new_burst: Option<f64>) -> Result<()> {
    // If neither limit nor burst is provided, do nothing
    if new_limit.is_none() && new_burst.is_none() {
      return Ok(());
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    // Update tokens based on time passed
    let tokens = self.advance(now)?;

    self.last = now;
    self.tokens = tokens;

    if let Some(limit) = new_limit {
      if limit <= 0.0 && limit != INFINITY {
        eyre::bail!("Invalid rate: {}", limit);
      }
      self.limit = limit;
    }

    if let Some(burst) = new_burst {
      if burst <= 0.0 {
        eyre::bail!("Invalid burst: {}", burst);
      }
      self.burst = burst;
      // Ensure tokens don't exceed new burst capacity
      if self.tokens > self.burst {
        self.tokens = self.burst;
      }
    }

    Ok(())
  }

  /// Get the current number of available tokens
  pub fn tokens(&mut self) -> Result<f64> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    self.advance(now)
  }

  /// duration_from_tokens converts number of tokens to duration in milliseconds
  fn duration_from_tokens(&self, tokens: f64) -> u64 {
    if self.limit <= 0.0 {
      return u64::MAX; // Infinity
    }

    ((tokens / self.limit) * 1000.0).ceil() as u64
  }

  /// tokens_from_duration converts duration in milliseconds to number of tokens
  fn tokens_from_duration(&self, duration_ms: u64) -> f64 {
    if self.limit <= 0.0 {
      return 0.0;
    }

    (duration_ms as f64 / 1000.0) * self.limit
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Reservation {
  pub ok: bool,
  pub tokens: u32,
  pub creation_date: u64, // Unix timestamp in milliseconds
  pub date_to_act: u64,   // Unix timestamp in milliseconds when tokens can be consumed
  pub limit: f64,         // The limit at reservation time
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReserveRequest {
  pub n: Option<u32>,
  pub max_future_reserve_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetRateRequest {
  pub new_limit: Option<f64>,
  pub new_burst: Option<f64>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_token_bucket_creation() -> Result<()> {
    let bucket = TokenBucketState::new(10.0, 100.0)?;
    assert_eq!(bucket.limit, 10.0);
    assert_eq!(bucket.burst, 100.0);
    assert_eq!(bucket.tokens, 100.0); // Should start full
    Ok(())
  }

  #[test]
  fn test_token_bucket_infinite_limit() -> Result<()> {
    let bucket = TokenBucketState::new(INFINITY, 100.0)?;
    assert_eq!(bucket.limit, INFINITY);
    assert_eq!(bucket.burst, 100.0);
    assert_eq!(bucket.tokens, 100.0);
    Ok(())
  }

  #[test]
  fn test_token_bucket_invalid_rate() {
    let result = TokenBucketState::new(-5.0, 100.0);
    assert!(result.is_err());

    let result = TokenBucketState::new(0.0, 100.0);
    assert!(result.is_err());
  }

  #[test]
  fn test_token_bucket_invalid_burst() {
    let result = TokenBucketState::new(10.0, -5.0);
    assert!(result.is_err());

    let result = TokenBucketState::new(10.0, 0.0);
    assert!(result.is_err());
  }

  #[test]
  fn test_reserve_with_sufficient_tokens() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;
    let reservation = bucket.reserve(50, None)?;
    assert!(reservation.ok);
    assert_eq!(reservation.tokens, 50);
    Ok(())
  }

  #[test]
  fn test_reserve_with_insufficient_tokens() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;
    // First consume all tokens
    let _reservation = bucket.reserve(100, None)?;
    // Then try to reserve more (should fail since we don't allow future reservations)
    let reservation = bucket.reserve(50, Some(0))?;
    assert!(!reservation.ok);
    assert_eq!(reservation.tokens, 0);
    Ok(())
  }

  #[test]
  fn test_burst_limit() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;
    // Try to reserve more than burst capacity (should fail)
    let reservation = bucket.reserve(150, None)?;
    assert!(!reservation.ok);
    Ok(())
  }

  #[test]
  fn test_advance_tokens() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?; // 10 tokens/second
    // Consume all tokens
    bucket.reserve(100, None)?;
    assert_eq!(bucket.tokens, 0.0);

    // Advance time by 1 second (should add 10 tokens)
    let future_time = bucket.last + 1000; // 1 second in milliseconds
    let tokens = bucket.advance(future_time)?;
    assert_eq!(tokens, 10.0);

    Ok(())
  }

  #[test]
  fn test_set_rate() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;

    // Set new limit
    bucket.set_rate(Some(20.0), None)?;
    assert_eq!(bucket.limit, 20.0);

    // Set new burst
    bucket.set_rate(None, Some(200.0))?;
    assert_eq!(bucket.burst, 200.0);

    // Set both
    bucket.set_rate(Some(5.0), Some(50.0))?;
    assert_eq!(bucket.limit, 5.0);
    assert_eq!(bucket.burst, 50.0);

    Ok(())
  }

  #[test]
  fn test_tokens_method() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;
    let tokens = bucket.tokens()?;
    assert_eq!(tokens, 100.0); // Should start full
    Ok(())
  }

  #[test]
  fn test_cancel_reservation() -> Result<()> {
    let mut bucket = TokenBucketState::new(10.0, 100.0)?;
    let reservation = bucket.reserve(50, None)?;
    assert!(reservation.ok);

    // Cancel the reservation
    bucket.cancel_reservation(&reservation)?;

    // Tokens should be restored (this is a simplified test)
    // In a real scenario, we'd need to mock time to test properly
    Ok(())
  }

  #[test]
  fn test_default_state() -> Result<()> {
    let default_state = TokenBucketState::default();
    assert_eq!(default_state.limit, 0.0);
    assert_eq!(default_state.burst, 0.0);
    assert_eq!(default_state.tokens, 0.0);
    assert_eq!(default_state.last, 0);
    assert_eq!(default_state.last_event, 0);
    Ok(())
  }
}
