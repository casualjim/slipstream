# Slipstream Memory

A high-performance temporal knowledge graph system for building AI applications with long-term memory capabilities.

## Overview

Slipstream Memory provides a robust foundation for applications requiring sophisticated knowledge management and retrieval. Built on proven graph database technologies, it offers a dual-database architecture that ensures both performance and reliability.

### Key Features

- **Temporal Knowledge Graph**: Track relationships and facts over time with built-in versioning
- **Dual-Database Architecture**: GraphDB (KuzuDB) for fast queries + Vector DB (LanceDB) for embeddings storage
- **ACID Transactions**: 2-Phase Commit protocol ensures data consistency across databases
- **Disaster Recovery**: Automatic rollback and version-based recovery with complete audit trails
- **High Performance**: Async architecture built on modern Rust foundations
- **Rich Data Model**: Support for Interactions, Concepts, Themes, and their relationships

## Architecture

```
┌─────────────┐    ┌─────────────┐
│   GraphDB   │    │  Vector DB  │
│   (KuzuDB)  │◄──►│  (LanceDB)  │
│   [Index]   │    │ [Source]    │
└─────────────┘    └─────────────┘
       │                  │
       └──────────────────┘
              2PC
```

**GraphDB** serves as a high-performance index for graph traversals and complex queries, while **Vector DB** acts as the source of truth storing complete data with embeddings.

## Core Concepts

### Node Types

- **Interaction** (formerly Episode): Temporal data points representing events, messages, or observations
- **Concept** (formerly Entity): Real-world entities with attributes and vector embeddings
- **Theme** (formerly Community): Clusters of related concepts forming semantic groups

### Edge Types

- **Mentions**: Links Interactions to referenced Concepts
- **Relates**: Fact-based relationships between Concepts with temporal validity
- **Includes**: Membership relationships from Themes to Concepts

## Getting Started

Add to your `Cargo.toml`:

```toml
[dependencies]
slipstream-memory = "0.1.0"
```

### Basic Usage

```rust
use slipstream_memory::Engine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the memory engine
    let engine = Engine::new();

    // The engine provides access to the temporal knowledge graph
    // and handles dual-database operations automatically

    Ok(())
}
```

### Working with Interactions

```rust
use slipstream_memory::nodes::{Interaction, ContentType};
use uuid::Uuid;
use jiff::Timestamp;

// Create a new interaction
let interaction = Interaction {
    uuid: Uuid::now_v7(),
    name: "User Message".to_string(),
    content: "What is the weather like today?".to_string(),
    source: ContentType::Message,
    group_id: "chat-session-123".to_string(),
    valid_at: Timestamp::now(),
    ..Default::default()
};
```

### Working with Concepts

```rust
use slipstream_memory::nodes::Concept;
use std::collections::HashMap;

// Create a concept with attributes
let mut attributes = HashMap::new();
attributes.insert("type".to_string(), serde_json::json!("location"));
attributes.insert("coordinates".to_string(), serde_json::json!([40.7128, -74.0060]));

let concept = Concept {
    name: "New York City".to_string(),
    labels: vec!["City".to_string(), "Location".to_string()],
    attributes,
    summary: "Major city in New York state".to_string(),
    ..Default::default()
};
```

## Data Model

The memory system supports rich temporal semantics:

- **Versioning**: All changes are versioned for complete audit trails
- **Temporal Validity**: Facts and relationships can have validity periods
- **Group Isolation**: Data can be partitioned by group_id for multi-tenant scenarios
- **Embedding Integration**: Automatic vector embedding generation and storage

## Advanced Features

### 2-Phase Commit Protocol

All operations use a distributed transaction protocol to ensure consistency across both databases:

1. **Prepare Phase**: Validate and prepare changes in both databases
2. **Commit Phase**: Atomically commit or rollback based on success

### Disaster Recovery

The system provides multiple levels of recovery:

- **Automatic Rollback**: Failed transactions are automatically rolled back
- **Version Recovery**: Roll forward/backward to any point in time
- **Audit Trail**: Complete history of all changes for forensic analysis

> [!NOTE]
> LanceDB serves as the authoritative source of truth, while KuzuDB provides high-performance graph indexing.

## Performance Characteristics

- **Query Performance**: Graph traversals optimized through KuzuDB indexing
- **Scale**: Handles large knowledge graphs efficiently
- **Memory Usage**: Optimized Arrow format for minimal memory overhead
- **Concurrent Access**: Lock-free reads with MVCC (Multi-Version Concurrency Control)

## Dependencies

The module relies on several key technologies:

- [KuzuDB](https://github.com/kuzudb/kuzu) - Graph database for high-performance queries
- [LanceDB](https://github.com/lancedb/lancedb) - Vector database for embeddings and metadata
- [Apache Arrow](https://arrow.apache.org/) - Columnar memory format for efficiency
- [Jiff](https://github.com/BurntSushi/jiff) - Modern date/time handling
- [UUID v7](https://uuid7.com/) - Time-ordered identifiers

## Integration

Slipstream Memory integrates seamlessly with:

- **slipstream-ai**: AI/ML pipeline for embedding generation and reasoning
- **slipstream-store**: Low-level storage abstractions and utilities

## Inspiration

This implementation draws inspiration from several groundbreaking projects:

- [GraphRAG](https://www.falkordb.com/blog/what-is-graphrag/) - Retrieval-augmented generation with knowledge graphs
- [Graphiti](https://github.com/getzep/graphiti) - Temporal knowledge graphs for AI applications
