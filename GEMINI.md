# Gemini Project Overview

This document provides a brief overview of the Slipstream project, its key technologies, and instructions for building, testing, and running the project.

## Project Description

Slipstream is an agentic compute and state cloud. It is built with Rust and exposes a REST API. The project also includes a workspace with workers written in TypeScript.

## Key Technologies

- **Backend:** Rust, Tokio, Axum (likely, given the web server context)
- **Frontend/Workers:** TypeScript, Bun
- **Data Storage:** KuzuDB, LanceDB
- **AI:** Integration with OpenAI (or compatible APIs)
- **Build & Tooling:** Mise, Cargo, Bun

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

### Building the Project

To build the entire project, including the Rust workspace and type-checking the workers, run:
```bash
mise run build
```

To build only the Rust components, run:
```bash
mise run build:rust
```
This executes `cargo build --release --workspace`.

### Running Tests

To run all tests for the project, use `mise test`:

```bash
mise run test
```
This will execute `cargo nextest run --workspace --release --no-fail-fast --no-capture` and also run the worker tests.

To run only the tests for the workers, run the following command from the root directory:

```bash
mise run test:workers
```

### Linting and Formatting

The project has separate linting and formatting tasks for Rust and JavaScript/TypeScript.

**Linting:**
- To run all linters: `mise run lint` (This runs `prefligit run --all-files`)
- To lint only JavaScript/TypeScript: `mise run lint:js` (This runs `biome check`)
- To lint only Rust: `mise run lint:rust` (This runs `cargo clippy`)

**Formatting:**
- To format all code: `mise run format`
- To format only JavaScript/TypeScript: `mise run format:js` (This runs `biome format --write`)
- To format only Rust: `mise run format:rust` (This runs `rustfmt` and `mise sort-deps`)



### Running the Project

To run the Slipstream server, you can use the following command:

```bash
cargo run --package slipstream-server
```

To run the workers, you can use the following command from the `workers/agent-registry-api` directory:

```bash
bun dev
```
