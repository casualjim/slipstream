# Agent Commands & Style Guide

This document standardizes local tooling, build/test workflows, and coding style for Slipstream (Rust crates + Cloudflare Workers).

## Prerequisites
- Toolchain manager: mise (repo provides tasks)
- Rust toolchain: stable with nextest
- Node runtime: Bun
- Cloudflare Wrangler CLI for workers

See mise tool pinning: [`.mise.toml`](.mise.toml:13).

## Quickstart
```bash
# Install tool versions (Rust, Node, Wrangler, etc.)
mise install

# Install JS deps for all workspaces
bun install

# Build everything (Rust workspace + typecheck Workers)
mise run build

# Run all tests (Rust nextest + Workers via Vitest)
mise run test
```

## Essential Commands
```bash
# Build
mise run build          # Full build (Rust + TS typecheck)  [see .mise/tasks/build/_default](.mise/tasks/build/_default:11)
mise run build:rust     # Rust only                        [see Cargo workspace](Cargo.toml:3)

# Test
mise run test           # All tests (Rust + Workers)       [see .mise/tasks/test/_default](.mise/tasks/test/_default:12)
mise run test:rust      # Rust tests (nextest)             [see .mise/tasks/test/_default](.mise/tasks/test/_default:12)
mise run test:rust --package <crate_name>  # Single crate
cargo test --package slipstream-memory --test integration_test  # Single Rust test target

# Workers tests
mise run test:workers agent-registry-api
mise run test:workers agent-registry-api path/to/file.test.ts
# Implementation: bun runs Vitest via worker task  [see .mise/tasks/test/workers](.mise/tasks/test/workers:20)

# Lint/Format
mise run lint           # All linters
mise run format         # Format all
```

## Dev Server & Testing Setup

### Agent Registry Auth (Critical)
Check if agent registry is running:

```bash
ps aux | grep agent-registry-api
```

If not running locally:
```bash
export SLIPSTREAM_BASE_URL=http://localhost:8787/api/v1
export SLIPSTREAM_API_KEY=test-api-key
bun run --cwd workers/agent-registry-api dev > dev.log &
```
Notes:
- Local Worker config at [`workers/agent-registry-api/wrangler.jsonc`](workers/agent-registry-api/wrangler.jsonc:6)
- Package scripts: [`workers/agent-registry-api/package.json`](workers/agent-registry-api/package.json:5)
- Example vars present in [`.dev.vars`](.dev.vars:28)

### Running Worker Dev Server Manually
```bash
# From repo root
bun run --cwd workers/agent-registry-api dev
# Swagger UI usually at http://localhost:8787/ per project README
```
Reference: [`workers/agent-registry-api/README.md`](workers/agent-registry-api/README.md:23)

### Running Rust Server (if applicable)
Some flows target the Rust server crate:
```bash
cargo run --package slipstream-server
```
Reference: [`crates/slipstream-server/src/main.rs`](crates/slipstream-server/src/main.rs) and workspace members [`Cargo.toml`](Cargo.toml:3)

### Testing Workers
- Vitest pool for Workers configured here:
  - [`workers/agent-registry-api/tests/vitest.config.mts`](workers/agent-registry-api/tests/vitest.config.mts:13)
  - D1 migrations applied in setup: [`workers/agent-registry-api/tests/apply-migrations.ts`](workers/agent-registry-api/tests/apply-migrations.ts:1)
- Run:
```bash
mise run test:workers agent-registry-api
mise run test:workers agent-registry-api path/to/file.test.ts
```

## Project Structure
- `crates/`:
  - `slipstream-core`: Data structures [see (`crates/slipstream-core/src/lib.rs`)](crates/slipstream-core/src/lib.rs)
  - `slipstream-ai`: Agent orchestration [see (`crates/slipstream-ai/src/lib.rs`)](crates/slipstream-ai/src/lib.rs)
  - `slipstream-memory`: Knowledge graph [see (`crates/slipstream-memory/src/lib.rs`)](crates/slipstream-memory/src/lib.rs)
  - `slipstream-store`: Database abstractions [see (`crates/slipstream-store/src/lib.rs`)](crates/slipstream-store/src/lib.rs)
  - `slipstream-server`: REST API [see (`crates/slipstream-server/src/main.rs`)](crates/slipstream-server/src/main.rs)
  - `slipstream-metadata`: Registry types [see (`crates/slipstream-metadata/src/lib.rs`)](crates/slipstream-metadata/src/lib.rs)
- `workers/`:
  - `agent-registry-api`: Cloudflare Worker API (Hono + Chanfana + D1) [see (`workers/agent-registry-api/src/index.ts`)](workers/agent-registry-api/src/index.ts:153)
  - `registry`: additional worker (Rust-based binding present in workspace) [see (`workers/registry/src/lib.rs`)](workers/registry/src/lib.rs:55)
- `docs/`:
  - Plans and implementation docs for agent registry:
    - [`docs/plans/agent-registry.md`](docs/plans/agent-registry.md:3)
    - [`docs/plans/agent-registry-implementation.md`](docs/plans/agent-registry-implementation.md:3)

## Code Style
- Rust: use `eyre::Result`, async/await, avoid `unwrap`
- TypeScript: strict types, no `any`, prefer `.all<Type>()` when using D1 prepared statements (pattern seen in Workers tooling)
- Package manager: Bun (bun install, bun run)
- Naming: snake_case (Rust), camelCase (TypeScript)
- Error handling: `thiserror` for custom errors (Rust)
- Comments: Document public APIs only

## Worker Conventions
- Config via Wrangler JSONC: [`workers/agent-registry-api/wrangler.jsonc`](workers/agent-registry-api/wrangler.jsonc:6)
- D1 bindings:
  - Dev DB binding: [`wrangler.jsonc`](workers/agent-registry-api/wrangler.jsonc:15)
  - Preview binding: [`wrangler.jsonc`](workers/agent-registry-api/wrangler.jsonc:36)
- Tests use @cloudflare/vitest-pool-workers; single worker mode: [`vitest.config.mts`](workers/agent-registry-api/tests/vitest.config.mts:14)

## Environment & Auth
Minimal auth for local testing:
```bash
export SLIPSTREAM_BASE_URL=http://localhost:8787/api/v1
export SLIPSTREAM_API_KEY=test-api-key
```
Example values are also tracked in [`.dev.vars`](.dev.vars:28). Do not commit real secrets.

## Troubleshooting
- Worker dev server not reachable:
  - Confirm wrangler dev is running: `ps aux | grep agent-registry-api`
  - Check port and route in [`workers/agent-registry-api/README.md`](workers/agent-registry-api/README.md:23)
- Tests fail with D1 binding error:
  - Ensure migrations are applied in test setup; see [`apply-migrations.ts`](workers/agent-registry-api/tests/apply-migrations.ts:1)
  - Verify wrangler config path in Vitest pool; see [`vitest.config.mts`](workers/agent-registry-api/tests/vitest.config.mts:17)
- Rust workspace build issues:
  - Run `mise run build:rust` to isolate
  - Confirm workspace members in [`Cargo.toml`](Cargo.toml:3)

## Agent Stance - Collaborative & Explicit
- Never assume: Always check if agent registry is running before tests
- Ask before acting: Confirm environment setup before running commands
- Verify state: `ps aux | grep agent-registry-api` to check if server is running
- Explicit setup: Always export required env vars before testing
- Collaborative: Explain what you're doing and why; document assumptions

## References
- Build task: [`.mise/tasks/build/_default`](.mise/tasks/build/_default:11)
- Test tasks: [`.mise/tasks/test/_default`](.mise/tasks/test/_default:12), [`.mise/tasks/test/workers`](.mise/tasks/test/workers:20)
- Worker package scripts: [`workers/agent-registry-api/package.json`](workers/agent-registry-api/package.json:5)
- Worker config: [`workers/agent-registry-api/wrangler.jsonc`](workers/agent-registry-api/wrangler.jsonc:6)
- Docs: [`docs/plans/agent-registry.md`](docs/plans/agent-registry.md:3), [`docs/plans/agent-registry-implementation.md`](docs/plans/agent-registry-implementation.md:3)
- Env examples: [`.dev.vars`](.dev.vars:28)
