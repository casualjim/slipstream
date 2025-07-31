# Agent Commands & Style Guide

## Essential Commands
```bash
# Build
mise run build          # Full build
mise run build:rust     # Rust only

# Test
mise run test           # All tests
mise run test:rust      # Rust tests
mise run test:rust --package <crate_name>  # Single crate
cargo test --package slipstream-memory --test integration_test  # Single test

# Lint/Format
mise run lint           # All linters
mise run format         # Format all
```

## Dev Server & Testing Setup

### Agent Registry Auth (Critical)
Check if agent registry is running: `ps aux | grep agent-registry-api`

If not running:
```bash
export SLIPSTREAM_BASE_URL=http://localhost:8787/api/v1
export SLIPSTREAM_API_KEY=test-api-key
bun run --cwd workers/agent-registry-api dev > dev.log &
```

### Testing Workers
```bash
mise run test:workers agent-registry-api
mise run test:workers agent-registry-api path/to/file.test.ts
```

### Running Server
```bash
cargo run --package slipstream-server
```

## Code Style
- **Rust**: `eyre::Result`, async/await, no unwraps
- **TypeScript**: Strict types, no `any`, use `.all<Type>()`
- **Package Manager**: Use `bun` not `npm` (bun install, bun run)
- **Naming**: snake_case (Rust), camelCase (TypeScript)
- **Error handling**: `thiserror` for custom errors
- **Comments**: Document public APIs only

## Project Structure
- **slipstream-core**: Data structures
- **slipstream-ai**: Agent orchestration
- **slipstream-memory**: Knowledge graph (KuzuDB + LanceDB)
- **slipstream-store**: Database abstractions
- **slipstream-server**: REST API
- **slipstream-metadata**: Registry types

## Agent Stance - Collaborative & Explicit
- **Never assume**: Always check if agent registry is running before tests
- **Ask before acting**: Confirm environment setup before running commands
- **Verify state**: Run `ps aux | grep agent-registry-api` to check if server is running
- **Explicit setup**: Always export required env vars before testing
- **Collaborative**: Explain what you're doing and why, seek confirmation on unclear steps
- **Document assumptions**: State any assumptions you're making before proceeding
