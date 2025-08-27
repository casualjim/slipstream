# Restate Token Bucket Rate Limiter Implementation

## Overview
This document outlines the tasks to implement a token bucket rate limiter in Rust for the slipstream-restate crate, based on the TypeScript example from the Restate examples repository. The implementation will follow Restate's virtual object pattern and Rust best practices with proper error handling.

## Current Issues with Implementation
1. Trait method signatures don't match Restate virtual object conventions
2. Implementation is not properly structured as a Restate virtual object
3. Method names and signatures don't align with the TypeScript API

## Implementation Tasks

### 1. Fix Trait Definition
Based on Restate documentation, update the trait to match virtual object conventions:
- Remove explicit `&self` parameter from trait methods (Restate adds this automatically)
- Ensure proper async method signatures
- Add `#[shared]` attribute to read-only methods

### 2. Core Token Bucket Implementation
Based on the TypeScript example, implement these exact methods:
- `tryReserve(ctx: ObjectContext, tokens: number): Promise<boolean>` - Attempts to reserve tokens, returns true if successful
- `release(ctx: ObjectContext, tokens: number): Promise<void>` - Releases tokens back to the bucket
- `getAvailablePermits(ctx: ObjectContext): Promise<number>` - Returns current number of available tokens

### 3. Additional Utility Methods (Optional)
- `setRate(ctx: ObjectContext, limit: number, burst: number): Promise<void>` - Sets rate limit parameters
- `state(ctx: ObjectContext): Promise<TokenBucketState>` - Returns current state for debugging

### 4. Data Structures
Ensure these structures match the TypeScript implementation:
- `TokenBucketState`:
  - `limit`: tokens per second
  - `burst`: maximum burst size
  - `tokens`: current number of tokens
  - `last`: last time tokens field was updated (Unix timestamp in milliseconds)
  - `lastEvent`: latest time of a rate-limited event (Unix timestamp in milliseconds)
- `Reservation`:
  - `ok`: boolean
  - `tokens`: number
  - `creationDate`: number (timestamp)
  - `dateToAct`: number (timestamp)
  - `limit`: number

### 5. State Management
- Use Restate's key-value state store properly
- Each virtual object instance should manage its own token bucket state
- Ensure state is persisted correctly between calls

## Test Tasks

### 1. Unit Tests
- Test token bucket core logic with various scenarios
- Test `tryReserve` with sufficient and insufficient tokens
- Test `release` functionality to add tokens back
- Test `getAvailablePermits` for accuracy
- Test token refill calculations over time

### 2. Integration Tests
- Test Restate virtual object handlers with proper context
- Verify state persistence works correctly
- Test concurrent access patterns
- Verify request/response handling matches TypeScript behavior

## Acceptance Criteria

### 1. Functional Requirements
- [ ] Virtual object properly implements Restate's concurrency model
- [ ] `tryReserve` method returns boolean based on token availability
- [ ] `release` method correctly adds tokens back to the bucket
- [ ] `getAvailablePermits` returns current token count
- [ ] Token bucket algorithm correctly refills tokens over time
- [ ] Burst capacity is properly respected
- [ ] API signatures match TypeScript example exactly

### 2. Quality Requirements
- [ ] All unit tests pass with comprehensive coverage
- [ ] Integration tests pass with Restate context
- [ ] Proper error handling with `eyre::Result` and `thiserror`
- [ ] Code follows repository style guidelines (snake_case, 2-space indentation)
- [ ] No `unwrap()` or `expect()` in public APIs
- [ ] Module is properly exported from `ratelimit/mod.rs`
- [ ] Documentation comments for all public functions and methods

### 3. Restate Integration Requirements
- [ ] Virtual object trait follows Restate conventions
- [ ] ObjectContext is properly used for state management
- [ ] Methods can be called via Restate ingress with correct paths
- [ ] State is properly isolated per object key