# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace (core, ai, memory, store, server, metadata). Tests live alongside crates and in `crates/<name>/tests/`.
- `workers/`: Cloudflare Workers. `agent-registry-api` (Hono + D1) and `registry`.
- `docs/`: Plans and implementation notes for the agent registry.
- Key configs: `.mise.toml`, `workers/agent-registry-api/wrangler.jsonc`, `.dev.vars` (example envs only).

## Build, Test, and Development Commands
- `mise install`: Install pinned Rust, Bun, Wrangler, etc.
- `bun install`: Install JS deps across workspaces.
- `mise run build`: Build Rust workspace + type-check Workers.
- `mise run build:rust`: Build Rust only.
- `mise run test`: All tests (Rust nextest + Workers via Vitest).
- `mise run test:rust [--package <crate>]`: Rust tests only.
- `mise run test:workers agent-registry-api [path/to/file.test.ts]`: Worker tests.
- Dev servers:
  - Worker: `export SLIPSTREAM_BASE_URL=http://localhost:8787/api/v1 && export SLIPSTREAM_API_KEY=test-api-key && bun run --cwd workers/agent-registry-api dev`
  - Rust server: `cargo run --package slipstream-server`

## Coding Style & Naming Conventions
- EXTREMELY IMPORTANT: Do not swallow errors. Fail spectacularly.
- Rust code is async streaming first, avoid using `collect` style patterns
- Strong typing everywhere.
- Rust: use `eyre::Result`, async/await, and `thiserror` for domain errors; avoid `unwrap`.
- TypeScript: strict mode, no `any`; D1 queries should prefer `.all<Type>()`. Package manager: Bun.
- Naming: snake_case (Rust), camelCase (TS).
- Lint/format: `mise run lint`, `mise run format`.

## Testing Guidelines
- Rust: nextest via `mise run test:rust`. Integration tests in `crates/<crate>/tests/`. Example: `cargo test --package slipstream-memory --test integration_test`.
- Workers: Vitest with `@cloudflare/vitest-pool-workers`. Config at `workers/agent-registry-api/tests/vitest.config.mts`; migrations applied in `apply-migrations.ts`. Test files: `*.test.ts`.
- Before testing e2e flows, ensure the agent registry dev server is running and required env vars are exported.

## Commit & Pull Request Guidelines
- Commits: clear, imperative subject; reference issues (e.g., `#123`). Include scope (crate/worker) and motivation.
- PRs: concise description, linked issues, repro steps, and screenshots/logs when relevant. Add tests for new behavior and update docs if user-facing.
- Keep changes focused; avoid unrelated refactors.

## Security & Configuration Tips
- Never commit real secrets. Use `.dev.vars` for examples.
- Verify Wrangler/D1 bindings in `workers/agent-registry-api/wrangler.jsonc` when running tests and dev.
