# Replicated DB Design (LanceDB + Kuzu)

This document describes a cloud‑native replication strategy built around LanceDB on object storage as the source of truth, with Kuzu as a derived index on each node. It assumes a single writer and one or more read replicas.

## Goals
- Cloud‑native reads: open LanceDB tables directly from object storage with intelligent local caching.
- Simple, reliable ordering: UUIDv7 `tx_id` for total order and happens‑before semantics without coordination.
- Kuzu as derived state: rebuild/fast‑forward locally from LanceDB; no cross‑node Kuzu coordination.
- Explicit control-plane signal: publish a tiny “version map” event; keep data plane in object storage.
- Streaming all the way: never materialize entire tables in memory (no collect inside the database layer).
- Clear deletion semantics: tombstones for idempotent deletes.
- Fast bootstrap: publish occasional Kuzu snapshots to avoid full replays for large datasets.

## Components
- Writer node (single):
  - Executes 2PC across LanceDB (primary store) and Kuzu (graph index).
  - Updates `SystemMetadata(table_name, lance_version)` in Kuzu inside the same transaction.
  - After a successful commit, emits a small “version map” event (control plane) with the `tx_id` and the new target versions per table.
- Read replicas:
  - Treat LanceDB (on object storage) as the source of truth; open tables directly from remote and rely on Lance’s local cache for chunks.
  - Treat Kuzu as derived: on each version map event, pin tables to target versions and update Kuzu by streaming from Lance and issuing idempotent MERGEs/DELETEs.
- Object storage:
  - Hosts Lance datasets and optional periodic Kuzu snapshots for faster bootstrap.
- Pub/sub (e.g., NATS/JetStream):
  - Transports version map events (control plane only; no row data).

## Identifiers & Ordering
- `tx_id`: UUIDv7 for total ordering and happens‑before semantics.
- One writer means a single global order of commits.

## Data Plane vs Control Plane
- Data plane: LanceDB tables in object storage (full datasets, versions, tombstones).
- Control plane: version map events showing “which table versions make up commit `tx_id`”.
- Keep events tiny; avoid duplicating the data plane in pub/sub.

## Write Path (Writer)
1. Execute 2PC:
   - LanceDB upsert/merge (MVCC) using Arrow RecordBatches; returns new table version.
   - Kuzu transaction with domain cypher (MERGE/CREATE/DELETE) and an update to `SystemMetadata(table_name, lance_version)` for each mutated table.
2. On success, emit a version map event (see below).
3. Do not emit before 2PC commit.

## Version Map Event
- Subject: `slipstream.versionmap` (example)
- Purpose: signal to replicas which versions to pin for a coherent view.
- Schema (JSON, serde‑friendly):
  - `tx_id: uuid` (UUIDv7)
  - `timestamp: string` (UTC ISO‑8601)
  - `tables: Array<{ table: string, version: u64 }>`
  - `notes?: string`
- Example:
```json
{
  "tx_id": "018f6c6e-6ea8-7a0a-b2f8-bc4b9b98b111",
  "timestamp": "2025-08-08T12:34:56Z",
  "tables": [
    { "table": "episodes", "version": 42 },
    { "table": "entities", "version": 17 },
    { "table": "tombstones", "version": 9 }
  ]
}
```

## Read Replica Apply Flow (Streaming, No Collect)
1. Receive version map event with `tx_id` and target versions.
2. For each table listed:
   - `open_table("s3://…/table")` (or configured remote), then `checkout(version)` and `restore()` to make that version visible. Lance will fetch only the fragments needed; the local cache persists across queries.
3. Rebuild / fast‑forward Kuzu:
   - For data tables: stream Lance RecordBatches and issue idempotent `MERGE` queries in Kuzu keyed by primary key. Keep batches small; avoid collect.
   - For tombstones: stream and issue `MATCH (n:Label {pk...}) DETACH DELETE n` (or other chosen delete pattern).
4. Record `last_applied_tx_id = tx_id` locally.
5. Ack the event only after success.

Notes:
- If an event is missed, the next event will contain higher versions; replicas can still pin to those and rebuild.
- Optionally add periodic polling (compare `table.version()`) as a safety net.

## Deletions via Tombstones
- Tombstone table in LanceDB (append‑only), versioned like other tables.
- Minimal schema:
  - `tx_id: uuid`, `table: string`, `label: string`, `pk: json`, `ts: timestamp`
- Replica logic:
  - On version bump, stream new tombstones (or just stream whole table pinned to the target version) and apply idempotent deletes in Kuzu.
- Idempotency: reapplying the same tombstone has no effect.

## Embeddings
- Externalize embedding computation (do not register Lance embedding functions).
- Store vectors as `FixedSizeList<Float32, D>` columns with strict dimension validation on write; never truncate.
- Vector search remains available via `table.vector_search().column("embedding")` with optional filters; replicas pin the table version before searching.
- Prior art: external embedding approach similar to code in
  https://github.com/casualjim/breeze-rustle/blob/main/crates/breeze-indexer/src/models/code_chunk.rs

## Bootstrap & Snapshots
- To avoid full replays on large datasets, publish periodic Kuzu snapshots:
  - Layout: `s3://bucket/kuzu-snapshots/{tx_id}/...` (include `snapshot-meta.json` with `{ tx_id }`).
  - Bootstrap:
    - Restore Kuzu snapshot with `tx_id_s`.
    - Start consuming version map events with `tx_id > tx_id_s` and apply them.
- Lance needs no snapshot: replicas open remote tables and pin versions as signaled.

## Incremental Indexing (Optional Future)
- Add `tx_id` or `updated_at` columns to data tables.
- Replicas can query “rows where `tx_id > last_applied_tx_id`” to avoid scanning entire tables.
- Full rebuild remains the fallback and correctness baseline.

## Roles & Guardrails
- Reader role: replicas refuse all direct write/mutation code paths; only the indexer updates Kuzu locally.
- Blocking pool tuning: streaming Kuzu queries run in `spawn_blocking`; size the blocking pool and/or limit concurrent streams.
- Optimizer task: run Lance `optimize(All)` on a cadence with a cancellation token and a `shutdown()` hook for clean exits.

## Failure Handling & Idempotency
- Delivery semantics: at‑least‑once for version map events; design for duplicates.
- Idempotent Kuzu upserts (`MERGE`) and deletes make replays safe.
- On replica graph failure: rollback Lance to the captured version using `checkout(version)` + `restore()` (mirrors writer 2PC behavior).
- If a replica misses events entirely, it can still align by pinning tables to the latest versions (from a fresh event or periodic poll) and rebuilding.

## Open Questions / Future Work
- Incremental tailing vs full rebuild:
  - If we need very fast catch‑up, add `tx_id` columns to tables or a compact commit log to query deltas efficiently.
- Auto‑tuning Lance optimization cadence based on table stats (row deltas, compaction backlog).
- Kuzu snapshot frequency and retention policy for large deployments.
- Finalize delete policy details (soft delete vs tombstones only vs mixed).

## Rationale Summary
- LanceDB’s cloud‑native design (remote reads + local cache) is leveraged by using object storage as the data plane and tiny version map events as the control plane.
- UUIDv7 `tx_id` provides total order without coordination.
- Kuzu remains derived; replicas pin table versions and (re)index via streaming without memory spikes.
- Occasional Kuzu snapshots keep bootstrap time bounded for large graphs.

