---
post_title: "Slipstream Store"
author1: "Slipstream Team"
post_slug: "slipstream-store"
microsoft_alias: "na"
featured_image: ""
categories:
  - rust
  - databases
  - ai
tags:
  - kuzu
  - lancedb
  - graph
  - vector
  - 2pc
ai_note: "This README was created with AI assistance."
summary: "Unified storage engine for Slipstream combining KuzuDB (graph) and LanceDB (vector/columnar) with 2-phase commit and versioned recovery."
post_date: "2025-08-08"
---

## Slipstream Store

> Unified, async-first storage engine that coordinates KuzuDB (graph) and LanceDB (vector/columnar) with 2‑phase commit and versioned recovery.

### Overview

Slipstream Store provides a clean, high-level facade over two systems:

- KuzuDB for graph indexing, traversal, and expressive Cypher queries
- LanceDB for the source-of-truth data, embeddings, and versioned columnar storage

It delivers atomic, consistent operations across both through a pragmatic 2‑phase commit (2PC), plus roll-forward recovery using LanceDB versions.

> [!TIP]
> Use Slipstream Store when you need graph + vector search with strong consistency and operational simplicity.

### Key Features

- Unified API for graph-only, store-only, and index‑then‑store query patterns
- 2PC across KuzuDB and LanceDB with deterministic rollback
- Versioned recovery and auditability leveraging LanceDB versions
- Vector search (cosine, etc.) with optional filters and limits
- Async from the ground up (Tokio), built for concurrency

### Data Model & Patterns

- Source of Truth: All fields reside in LanceDB. KuzuDB stores only query-critical properties (e.g., uuid, group_id, timestamps) as a covering index.
- Queries:
  - IndexOnly: Cypher over KuzuDB
  - StoreOnly: Filters/limits over LanceDB
  - IndexThenStore: Use graph results to build an efficient Lance filter (e.g., uuid IN (...)) and stream results
- Mutations:
  - Single or Bulk upserts into LanceDB, mirrored into KuzuDB within the same 2PC

### Quick Start

Add to Cargo.toml:

```toml
[dependencies]
slipstream-store = "*"
```

Minimal usage:

```rust
use slipstream_store::{Config, Database};

#[tokio::main]
async fn main() -> slipstream_store::Result<()> {
    // Data lives under data_dir/{graphs,meta}
    let config = Config::new_test("./data");
    let db = Database::new(&config).await?;

    // Run queries/mutations via typed commands (see ops in `operations.rs`)
    // db.execute(MyCommand { ... }).await?;
    Ok(())
}
```

### Configuration

Embeddings are pluggable via `EmbeddingConfig` and register automatically with LanceDB.

- Default provider: Ollama (`http://localhost:11434/v1`, model `snowflake-arctic-embed:xs`, dim 384)
- Override provider/base/model/api_key/dimensions as needed

> [!NOTE]
> LanceDB tables must exist before upserts; create empty tables during setup/migrations.

### Testing

- Unit tests cover 2PC, concurrency, vector search, and rollback semantics
- Run tests for this crate only:

```sh
cargo test -p slipstream-store
```

### Architecture at a Glance

- KuzuDB: Graph schema, traversal, and write serialization
- LanceDB: Upserts with MVCC, vector indices, and time-travel via versions
- Store orchestrates both, ensuring atomic changes and consistent reads

### Resources

- KuzuDB: https://github.com/kuzudb/kuzu
- LanceDB: https://github.com/lancedb/lancedb
- Slipstream: https://github.com/casualjim/slipstream


