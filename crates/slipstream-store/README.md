# Slipstream Store

Slipstream Store provides a temporal knowledge graph system with:

## Overview

- **Graph Database**: Powered by [KuzuDB](https://github.com/kuzudb/kuzu) for graph queries (index)
- **Vector Storage**: Using [LanceDB](https://github.com/lancedb/lancedb) for embeddings and metadata (source of truth)
- **Dual-Database Architecture**: GraphDB serves as an index while LanceDB stores complete data
- **2-Phase Commit (2PC)**: Disaster-resistant atomic operations across both databases with automatic rollback
- **Version-Based Recovery**: Complete audit trail using LanceDB's roll-forward versioning
- **Async Architecture**: Built on tokio and axum for high performance
