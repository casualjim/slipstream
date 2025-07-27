# Slipstream Rust Project Overview

This document provides a comprehensive overview of the Slipstream project's Rust components, including architecture, key modules, and implementation details. Slipstream is an agentic compute and state cloud built with Rust, exposing a REST API with a focus on temporal knowledge graph systems.

## Project Architecture

Slipstream follows a modular monolith architecture with a Rust workspace containing multiple crates:

1. **slipstream-core** - Core data structures and messaging components
2. **slipstream-ai** - Agent orchestration and AI integration
3. **slipstream-memory** - Temporal knowledge graph with dual-database architecture
4. **slipstream-store** - Low-level database abstractions and utilities
5. **slipstream-server** - REST API server implementation
6. **slipstream-metadata** - Agent/tool registry types

## Key Technologies

- **Backend Framework**: Rust with Tokio async runtime
- **Web Framework**: Axum for REST API
- **Databases**: 
  - KuzuDB (graph database for indexing)
  - LanceDB (vector database for storage)
- **Serialization**: Serde for JSON serialization/deserialization
- **AI Integration**: async-openai for OpenAI-compatible APIs
- **Observability**: Tracing, OpenTelemetry
- **Testing**: tokio-test, mockall

## Core Components

### 1. slipstream-core

This crate provides fundamental data structures for the Slipstream system:

- **Definitions**: Model definitions, agent configurations, and tool specifications
- **Messages**: Comprehensive message system for agent communication with:
  - Content and assistant message types
  - Text, image, audio, and video content parts
  - Request/response message envelopes with metadata
- **Registry**: Trait definitions for agent, model, and tool registries

### 2. slipstream-ai

Implements agent orchestration and AI integration:

- **Agents**: Agent trait and implementations with instructions, tools, and model configurations
- **Completers**: AI model completion interfaces
- **Embedders**: Text embedding generation
- **Rerankers**: Result reranking capabilities
- **Events**: Event system for agent communication
- **Executors**: Task execution mechanisms

### 3. slipstream-memory

Implements a temporal knowledge graph system with dual-database architecture:

- **Nodes**: Interaction, Concept, and Theme node types
- **Edges**: Relationships between nodes (Mentions, Relates, Includes)
- **Engine**: Core memory engine coordinating both databases
- **Migrations**: Database schema migration system
- **Mutations**: Data modification operations
- **Queries**: Data retrieval operations
- **Service**: High-level interaction interfaces

Architecture:
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

### 4. slipstream-store

Provides low-level database abstractions:

- **Database**: Unified interface for KuzuDB and LanceDB operations
- **Operations**: Query and mutation operation definitions
- **Traits**: Database command and execution traits
- **Graph**: KuzuDB integration
- **Meta**: LanceDB integration
- **Streams**: Result streaming utilities

Key features:
- 2-Phase Commit (2PC) protocol for consistency
- Automatic rollback and version-based recovery
- Async architecture with MVCC (Multi-Version Concurrency Control)

### 5. slipstream-server

REST API server implementation:

- **App State**: Shared application state with memory engine
- **Routes**: API endpoint definitions
- **Server**: HTTP server configuration and startup
- **Models**: API data transfer objects

### 6. slipstream-metadata

Agent/tool registry types:

- Agent configurations
- Tool definitions

## Data Model

### Node Types

1. **Interaction** (formerly Episode): Temporal data points representing events, messages, or observations
2. **Concept** (formerly Entity): Real-world entities with attributes and vector embeddings
3. **Theme** (formerly Community): Clusters of related concepts forming semantic groups

### Edge Types

1. **Mentions**: Links Interactions to referenced Concepts
2. **Relates**: Fact-based relationships between Concepts with temporal validity
3. **Includes**: Membership relationships from Themes to Concepts

## Key Implementation Details

### Database Architecture

Slipstream uses a dual-database architecture for optimal performance:

- **KuzuDB** serves as a high-performance index for graph traversals and complex queries
- **LanceDB** acts as the source of truth storing complete data with embeddings

The system implements a 2-Phase Commit (2PC) protocol to ensure consistency across both databases.

### Message System

The messaging system provides a rich, typed interface for agent communication with:
- Support for various content types (text, images, audio, video)
- Request/response patterns
- Tool calling and responses
- Message metadata (timestamps, sender info)

### Agent System

Agents are defined with:
- Instructions and system messages
- Available tools with schemas
- Model configurations
- Response schemas for structured output

### 2-Phase Commit Implementation

The 2PC implementation ensures atomic operations across both databases:
1. Begin KuzuDB transaction (serializes write transactions)
2. Execute GraphDB operations within the transaction
3. Execute LanceDB operations (auto-commits)
4. If GraphDB fails after LanceDB succeeded, rollback LanceDB
5. Commit or rollback KuzuDB based on success/failure

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Bun](https://bun.sh/docs/installation)
- [Mise](https://mise.jdx.dev/getting-started.html)

### Installation

1.  **Install tools:**
    ```bash
    mise install
    ```

2.  **Install dependencies:**
    ```bash
    cargo fetch
    bun install
    ```

### Building

To build the entire project, including the Rust workspace and type-checking the workers, run:
```bash
mise run build
```

To build only the Rust components, run:
```bash
mise run build:rust
```

### Testing

To run all tests for the project:
```bash
mise run test
```

To run only the tests for the workers:
```bash
mise run test:workers
```

To test only the Rust components:
```bash
mise run test:rust
```

To test a single rust crate:
```bash
mise run test:rust --package <crate_name>
```

### Running

To run the Slipstream server:
```bash
cargo run --package slipstream-server
```

### Linting and Formatting

The project uses mise tasks for linting and formatting:

**Linting:**
- To run all linters: `mise run lint`
- To lint only Rust: `mise run lint:rust`

**Formatting:**
- To format all code: `mise run format`
- To format only Rust: `mise run format:rust`

## Development Guidelines

1. Follow Rust best practices and idioms
2. Use async/await for all I/O operations
3. Implement proper error handling with `eyre::Result`
4. Write comprehensive tests for new features
5. Document public APIs with rustdoc comments
6. Follow the existing code style and patterns

## Testing Strategy

The project uses a comprehensive testing approach:
- Unit tests for individual components
- Integration tests for cross-component functionality
- Property-based testing where applicable
- Mocking for external dependencies
- Concurrent testing for database operations

## Observability

The system includes comprehensive tracing and logging:
- Structured logging with tracing
- OpenTelemetry integration
- Error reporting with color-backtrace in development
