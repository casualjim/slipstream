---
applyTo: '**'
description: "Instructions for agentic coding agents working in this repository"
---

# Agent Coding Guidelines

## Build, Test, and Development Commands
- `mise install`: Install pinned Rust, Bun, Wrangler, etc.
- `bun install`: Install JS deps across workspaces.
- `mise run build`: Build Rust workspace + type-check Workers.
- `mise run build:rust`: Build Rust only.
- `mise run test`: All tests (Rust nextest + Workers via Vitest).
- `mise run test:rust [--package <crate>]`: Rust tests only (use `-- --nocapture` to see output).
- `mise run test:rust --package slipstream-core --lib -- --exact messages::content::tests::test_content_serialization` to run a single test
- `mise run test:workers agent-registry-api [path/to/file.test.ts]`: Worker tests.
- Dev servers:
  - Worker: `export SLIPSTREAM_BASE_URL=http://localhost:8787/api/v1 && export SLIPSTREAM_API_KEY=test-api-key && bun run --cwd workers/agent-registry-api dev`
  - Rust server: `cargo run --package slipstream-server`

## Code Style & Formatting
- Rust: 
  - Use `eyre::Result` for error handling, `thiserror` for domain errors
  - No `unwrap()` or `expect()` in public APIs
  - Async streaming first - avoid `collect()` patterns
  - Imports: Group std/core, external crates, and internal modules separately
  - Formatting: `mise run format` (uses rustfmt with max_width=100, tab_spaces=2)
  - Strict error handling - fail spectacularly, don't swallow errors
- TypeScript:
  - Strict mode with no `any` or `unknown`
  - Bun package manager
  - Double quotes for strings
  - D1 queries should prefer `.all<Type>()`
- General:
  - 2-space indentation (except Python which uses 4)
  - LF line endings with final newline
  - Trim trailing whitespace
  - UTF-8 encoding

## Naming Conventions
- Rust: snake_case for variables/functions, PascalCase for types
- TypeScript: camelCase for variables/functions, PascalCase for types
- Files: snake_case for Rust, camelCase for TypeScript

## Error Handling
- Rust: Use `eyre::Result` for function returns, `thiserror` for domain-specific errors
- TypeScript: Proper error catching and handling without swallowing
- Never ignore errors - propagate or handle explicitly

## References
This file combines repository guidelines with specific agent instructions for working with the codebase effectively.
