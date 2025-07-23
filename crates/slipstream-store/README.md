<!-- If you have a logo, place it here. Example: <img src="/path/to/logo.png" alt="Slipstream Core Logo" width="120" /> -->

## Slipstream Core

> **Temporal, high-performance knowledge graph engine for modern AI and data applications.**

---

### Overview

Slipstream Core is the foundational Rust library for building temporal knowledge graph systems. It powers the Slipstream platform with:

- **Graph Indexing**: Fast, expressive graph queries via [KuzuDB](https://github.com/kuzudb/kuzu)
- **Vector Storage**: High-performance embeddings and metadata with [LanceDB](https://github.com/lancedb/lancedb)
- **Dual-Database Architecture**: Combines graph and vector DBs for rich, scalable data modeling
- **2-Phase Commit (2PC)**: Atomic, disaster-resistant operations across both databases
- **Versioned Recovery**: Full audit trail and roll-forward recovery using LanceDB's versioning
- **Async by Design**: Built on [tokio](https://tokio.rs/) for scalable, concurrent workloads

> [!TIP]
> Slipstream Core is ideal for applications needing strong consistency, auditability, and advanced graph+vector search.

---

### Features

- **Unified API**: Single interface for graph and vector operations
- **Actor-based Design**: Clean separation of concerns and concurrency
- **Audit & Rollback**: Every change is versioned for traceability and recovery
- **Extensible**: Designed for integration with custom data models and pipelines
- **Async/await**: Non-blocking operations throughout

---

### Database Design: LanceDB + KuzuDB

**Source of Truth**: LanceDB is the source of truth for all data. Every field is stored in LanceDB.

**Covering Index**: KuzuDB serves as a covering index for graph traversal and query operations. Only properties used in queries are stored in KuzuDB:

- **Store in KuzuDB**: Properties used in WHERE clauses, ORDER BY, graph traversal patterns, or JOIN conditions
  - Examples: uuid, group_id, created_at, temporal fields (valid_at, invalid_at, expired_at)

- **Store in LanceDB only**: Data payload properties that are returned but not queried
  - Examples: fact text, content, attributes, interactions arrays, embeddings

This design ensures optimal query performance while keeping the graph database lean.

---

### Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
slipstream-core = "*"
```

Initialize and use the database:

```rust
use slipstream_core::{Config, Database};

#[tokio::main]
async fn main() -> slipstream_core::Result<()> {
    let config = Config::new(/* ... */);
    let db = Database::new(&config).await?;
    // ... use db for graph and vector operations ...
    Ok(())
}
```

---

### Architecture

Slipstream Core coordinates two best-in-class databases:

- **KuzuDB**: Handles graph structure, relationships, and fast graph queries
- **LanceDB**: Stores embeddings, metadata, and provides versioned, columnar storage

All operations are coordinated for atomicity and consistency, with 2PC ensuring that changes are either fully applied or rolled back across both systems.

---

### Use Cases

- AI knowledge graphs with temporal versioning
- Hybrid search (graph + vector)
- Data lineage and audit trails
- Disaster recovery for graph/vector data

---

### Resources

- [KuzuDB](https://github.com/kuzudb/kuzu) — Graph database engine
- [LanceDB](https://github.com/lancedb/lancedb) — Vector database for embeddings
- [Slipstream Project](https://github.com/casualjim/slipstream)

---

> [!NOTE]
> For advanced configuration, migration, and integration details, see the main project documentation and examples.


