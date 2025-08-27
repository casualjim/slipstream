use restate_sdk::prelude::*;

use crate::ratelimit::{INFINITY, TokenBucketState};

// Virtual object trait following Restate conventions
#[restate_sdk::object]
pub trait RateLimiter {
  // Initialize the rate limiter with specific parameters
  async fn init(params: Json<InitParams>) -> Result<(), HandlerError>;

  // Try to reserve tokens, returning true if successful
  async fn try_reserve(tokens: u32) -> Result<bool, HandlerError>;

  // Release tokens back to the bucket
  async fn release(tokens: u32) -> Result<(), HandlerError>;

  // Get the number of available permits
  #[shared]
  async fn get_available_permits() -> Result<u32, HandlerError>;

  // Set a new rate for the limiter
  async fn set_rate(params: Json<SetRateParams>) -> Result<(), HandlerError>;

  // Get the current state of the rate limiter
  #[shared]
  async fn state() -> Result<Json<TokenBucketState>, HandlerError>;
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InitParams {
  pub limit: f64,
  pub burst: f64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SetRateParams {
  pub limit: f64,
  pub burst: Option<f64>,
}

// Implementation struct
pub struct RateLimiterImpl;

// Implementation of the virtual object
impl RateLimiter for RateLimiterImpl {
  async fn init(
    &self,
    ctx: ObjectContext<'_>,
    params: Json<InitParams>,
  ) -> Result<(), HandlerError> {
    let InitParams { limit, burst } = params.into_inner();

    // Validate parameters
    if limit <= 0.0 && limit != INFINITY {
      return Err(eyre::eyre!("Invalid rate: {}. Rate must be positive.", limit).into());
    }

    if burst <= 0.0 {
      return Err(eyre::eyre!("Invalid burst: {}. Burst must be positive.", burst).into());
    }

    // Create and store new state
    let state = TokenBucketState::new(limit, burst)?;
    ctx.set("state", Json(state));

    Ok(())
  }

  async fn try_reserve(&self, ctx: ObjectContext<'_>, tokens: u32) -> Result<bool, HandlerError> {
    // Get current state
    let mut state: TokenBucketState = ctx.get("state").await?.map(|Json(s)| s).unwrap_or_default();

    // If this is a new state, initialize it with default values
    if state.limit == 0.0 && state.burst == 0.0 {
      state = TokenBucketState::new(10.0, 100.0)?; // Default values
    }

    // Attempt to reserve tokens
    let reservation = state.reserve(tokens, None)?;

    // Update state in storage
    ctx.set("state", Json(state));

    Ok(reservation.ok)
  }

  async fn release(&self, ctx: ObjectContext<'_>, tokens: u32) -> Result<(), HandlerError> {
    // Get current state
    let mut state: TokenBucketState = ctx.get("state").await?.map(|Json(s)| s).unwrap_or_default();

    // If this is a new state, initialize it with default values
    if state.limit == 0.0 && state.burst == 0.0 {
      state = TokenBucketState::new(10.0, 100.0)?; // Default values
    }

    // Add tokens back to the bucket
    state.tokens += tokens as f64;
    if state.tokens > state.burst {
      state.tokens = state.burst;
    }

    // Update state in storage
    ctx.set("state", Json(state));

    Ok(())
  }

  async fn get_available_permits(&self, ctx: SharedObjectContext<'_>) -> Result<u32, HandlerError> {
    // Get current state
    let mut state: TokenBucketState = ctx.get("state").await?.map(|Json(s)| s).unwrap_or_default();

    // If this is a new state, initialize it with default values
    if state.limit == 0.0 && state.burst == 0.0 {
      state = TokenBucketState::new(10.0, 100.0)?; // Default values
    }

    // Get current available tokens
    let tokens = state.tokens()?;

    Ok(tokens as u32)
  }

  async fn set_rate(
    &self,
    ctx: ObjectContext<'_>,
    params: Json<SetRateParams>,
  ) -> Result<(), HandlerError> {
    let SetRateParams { limit, burst } = params.into_inner();

    // Validate parameters
    if limit <= 0.0 && limit != INFINITY {
      return Err(eyre::eyre!("Invalid rate: {}. Rate must be positive.", limit).into());
    }

    if let Some(burst) = burst && burst <= 0.0 {
      return Err(eyre::eyre!("Invalid burst: {}. Burst must be positive.", burst).into());
    }

    // Get current state or create new one
    let mut state: TokenBucketState = ctx.get("state").await?.map(|Json(s)| s).unwrap_or_default();

    // If this is a new state, initialize it with provided values
    if state.limit == 0.0 && state.burst == 0.0 {
      state = TokenBucketState::new(limit, burst.unwrap_or(100.0))?; // Default burst of 100
    } else {
      state.limit = limit;
      if let Some(new_burst) = burst {
        state.burst = new_burst;
      }
    }

    // Update state in storage
    ctx.set("state", Json(state));

    Ok(())
  }

  async fn state(
    &self,
    ctx: SharedObjectContext<'_>,
  ) -> Result<Json<TokenBucketState>, HandlerError> {
    let state: Option<Json<TokenBucketState>> = ctx.get("state").await?;
    Ok(state.unwrap_or_default())
  }
}
