# Copilot Instructions for Slipstream

## Project Architecture
- **Slipstream** is a modular Rust platform for agentic compute and state cloud, exposing a REST API.
- **Core Crates:**
  - `slipstream-core`: Temporal knowledge graph engine, dual-database (KuzuDB for graph, LanceDB for vectors), async/actor-based, 2PC for atomicity.
  - `slipstream-memory`: Temporal knowledge graph with versioning, disaster recovery, and rich node/edge types (Interaction, Concept, Theme).
  - `slipstream-store`: High-performance storage and graph operations, integrates with core and memory.
  - `slipstream-ai`: Agent orchestration, event-driven, provider abstraction, tool system, and memory management for AI agents.
  - `slipstream-metadata`: Registry for agent/tool definitions and configurations.
- **Data Flow:**
  - Graph and vector DBs are tightly coupled for consistency and performance.
  - All changes are versioned and auditable; recovery and rollback are first-class features.

## Developer Workflows
- **Build:** Use `cargo build` at the workspace root. Each crate is independently buildable.
- **Test:** Use `cargo test` at the workspace or crate level. Tests are in `src/` or `tests/`.
- **Debug:** Standard Rust debugging applies. For async code, prefer `tokio-console` or `RUST_LOG` for tracing.
- **Run:** Main server entrypoint is in `slipstream-server/src/main.rs`.
- **Local Compose:** Use `compose.local.yml` for local dev environments (services, DBs).

## Project Conventions
- **Async-first:** All IO and compute is async (tokio). Use `.await` everywhere.
- **Actor Model:** Concurrency and separation of concerns via actors (see `slipstream-core` and `slipstream-memory`).
- **2-Phase Commit:** All cross-DB operations use 2PC for atomicity.
- **Versioning:** All data changes are versioned for audit and recovery.
- **Extensibility:** New agent types, tools, and providers are added via trait impls and registration in the registry crate.
- **Testing:** Use property-based and round-trip tests for serialization (see `content.rs` tests for patterns).

## Integration Points
- **External DBs:** KuzuDB (graph), LanceDB (vector/embeddings). See crate docs for config.
- **REST API:** Exposed by `slipstream-server`.
- **Agent Providers:** Pluggable via `slipstream-ai` provider abstraction.

## Examples
- **Graph+Vector Query:** See `slipstream-core` and `slipstream-memory` for unified API usage.
- **Agent Orchestration:** See `slipstream-ai` for event-driven agent workflows.
- **Registry Usage:** See `slipstream-metadata` for agent/tool registration patterns.

## Key Files/Dirs
- `/crates/`: All core logic, organized by domain.
- `/compose.local.yml`: Local dev environment.
- `/README.md` and crate-level `README.md`: Architectural and usage docs.
- `/src/content.rs`: Example of advanced serialization and test patterns.

---
For questions about non-obvious patterns, see crate-level `README.md` or `/src/` for idiomatic examples. When in doubt, prefer async, actor-based, and versioned approaches.
