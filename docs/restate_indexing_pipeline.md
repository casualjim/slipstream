# Restate Indexing Pipeline: Upload Ingestor → Chunk Batcher

Status: Draft
Date: 2025-08-28
Owners: Search/Indexing

## Summary
- Build a durable, idempotent pipeline in Restate that:
  1) Receives an upload notification for a file in an S3‑compatible store,
  2) Streams, parses, and chunks content,
  3) Emits per‑chunk messages to a batching Restate component that assembles token‑bounded batches for the embedding service.
- Current runtime: TypeScript on Bun, deployed on Knative. Future direction: Rust (WASI‑P2) with unchanged service contracts.

## Goals
- Exactly‑once chunk emission per uploaded object version (by `docId`).
- Low latency on small files; steady throughput on large files.
- Batches sized by token budget and item count, with time‑based flush.
- Simple, observable rollback and retry semantics.

## Non‑Goals (This Doc)
- Embedding dispatch and vector DB write path (covered in a separate design).
- Advanced document parsing (PDF/HTML/OCR). We start with plaintext and a hook point for parsers.

## Terminology
- Restate Service: Regular function handlers with durable execution and side‑effect wrapping.
- Virtual Object (VO): Per‑key, single‑writer stateful component in Restate.
- Upload Event: Notification from S3‑compatible store (S3/MinIO/Ceph) about a new object/version.

## Architecture

Components
- Upload Ingestor (Service):
  - Method `uploaded(UploadNotification)`
  - Streams object by range, parses/normalizes text, chunks by token budget.
  - Emits each `ChunkEnvelope` to Batcher VO keyed by `{tenantId}:{indexId}:{model}`.
  - Sends `endDocument` marker when done to trigger final flush if needed.

- Chunk Batcher (Virtual Object):
  - Key: `{tenantId}:{indexId}:{model}` (or `{tenant}:{index}:{provider:model}` if multi‑provider).
  - Accumulates chunks into a pending batch; flushes on size/time/end‑of‑document.
  - Emits `BatchRequest` to the downstream Embedding stage via a single durable side‑effect.

Sequence (happy path)
```
 S3/MinIO ──upload event──▶ Knative Adapter ──▶ Restate::UploadIngestor.uploaded
   │                                                     │
   └──────(GET range)◀────── Bun runtime (ingestor) ◀───┘
                                                         │ chunk+seq, ctx.call
                                                         ▼
                                              Restate::ChunkBatcher (VO)
                                                         │ flush (size/time/EOD)
                                                         ▼
                                           Embedding Dispatch (downstream system)
```

## Data Contracts

UploadNotification
```
{
  tenantId: string,
  indexId: string,
  bucket: string,
  key: string,
  versionId?: string,     // S3 style versioning (or omitted for MinIO if not enabled)
  etag?: string,          // strong/weak; used for idempotency when versionId is absent
  sizeBytes: number,
  contentType?: string,
  model: string,          // embedding model key that drives batch sizing
  sourceId?: string,      // external correlation
  createdAt: string       // ISO timestamp
}
```

ChunkEnvelope
```
{
  docId: string,          // sha256(tenantId|bucket|key|versionId|etag)
  chunkId: string,        // `${docId}:${byteStart}-${byteEnd}` or hash of (text+offsets)
  seq: number,            // 0..N, monotonic per doc
  text: string,
  byteRange: [number, number],
  tokenCount: number,     // estimate
  metadata: {
    tenantId: string,
    indexId: string,
    model: string,
    sourceId?: string
  }
}
```

BatchRequest
```
{
  batchId: string,          // `${voKey}:${counter}:${time}`
  tenantId: string,
  indexId: string,
  model: string,
  inputs: Array<{
    docId: string,
    chunkId: string,
    seq: number,
    text: string
  }>,
  tokensTotal: number,
  createdAt: string
}
```

## Durable Flow & Idempotency

- Deterministic core: All side effects (S3 reads, downstream HTTP/queue) are wrapped in Restate `ctx` side‑effects so replay doesn’t duplicate effects.
- Document identity: `docId = sha256(tenantId|bucket|key|versionId|etag)`; the ingestor records progress per `docId`.
- Progress tracking in Ingestor:
  - `nextSeq`: next sequence number to emit.
  - `nextByteStart`: next object offset to read (for range GET resume).
  - On retry/replay: skip chunks with `seq < nextSeq`.
- Batcher dedupe: `seenChunkIds` bounded LRU/HT (windowed) to drop duplicates safely.
- Exactly‑once batch emission: `flush()` uses a single durable side‑effect with `batchId` idempotency key upstream.
- Ordering: Global ordering is not required. Per‑doc `seq` is preserved for auditability.

Failure Scenarios
- Ingestor crash mid‑read: replay resumes from `nextByteStart`; already emitted chunks are dropped by Batcher via `seenChunkIds`.
- Batcher crash mid‑flush: on replay, `flush()` side‑effect is retried with same `batchId` → idempotent downstream.
- Partial parsing errors: chunker may skip unreadable segments; record per‑doc warnings in telemetry and attach to `endDocument` report.

## Chunking Strategy
- Target chunk size: ~800 tokens; max ~1200; 10–15% overlap. Configurable per model.
- Parsers: pluggable text extractor chain (initially: plaintext). For HTML/PDF, add adapters upstream (future work).
- Token estimation: fast estimator for batching; exact tokenization can occur at embedding call.

## Batching Strategy
- Flush on any of:
  - `tokensInBatch + tokenCount > maxTokensPerBatch`
  - `itemsInBatch >= maxItemsPerBatch`
  - Timer `flushAfterMs` elapses since last append
  - `endDocument` with pending items
- VO single‑writer semantics guarantee serialized `addChunk`/`flush` per key.

## Configuration
- Chunking: `targetChunkTokens` (800), `maxChunkTokens` (1200), `overlapTokens` (100)
- Batching: `maxTokensPerBatch` (model‑specific, e.g., 32k/64k), `maxItemsPerBatch` (e.g., 128), `flushAfterMs` (250–500ms)
- Concurrency: Ingestor stream concurrency (range reads), VO concurrency is per key (single writer)
- Backpressure: Cap in‑flight `addChunk` awaits in ingestor (e.g., window of 32)

## Interfaces (TypeScript/Bun, Restate)

UploadIngestor Service (pseudo‑TS)
```
export interface UploadIngestor {
  uploaded(evt: UploadNotification): Promise<{ docId: string, chunks: number }>
}
```

ChunkBatcher VO (pseudo‑TS)
```
export interface ChunkBatcher {
  addChunk(chunk: ChunkEnvelope): Promise<void>
  endDocument(doc: { docId: string, seqMax: number }): Promise<void>
}
```

Implementation Notes (TS/Bun)
- Use Restate TS SDK; register service and VO; deploy as Knative service container.
- S3 client: AWS SDK v3 (S3) or direct signed `fetch` for range GET; Bun supports Node APIs.
- Side effects: wrap S3 calls and downstream enqueue with `ctx.run` (or equivalent) to ensure idempotency.

## Knative + S3 Eventing
- Event source options:
  - AWS S3 → EventBridge → API Destination calling `UploadIngestor.uploaded` HTTP endpoint
  - MinIO → bucket notification webhook → Knative adapter → Restate call
- Adapter: tiny stateless Bun service that maps CloudEvent → `UploadNotification` and invokes Restate (or call Restate service HTTP ingress directly if exposed).
- Networking: Knative service needs outbound to S3 and Restate server; Restate server reachable for deployments and service calls.

## Observability
- Metrics: 
  - Ingestor: bytes read, chunks emitted, resume count, read latency
  - Batcher: batches emitted, tokens/items per batch, flush reasons, dedupe hits
- Tracing: propagate `traceId` from upload event; annotate spans with `docId`, `batchId`, VO key
- Logging: structured JSON with `tenantId`, `indexId`, `docId`, `seq`, `batchId`

## Security
- Validate source: verify event signature or trusted source IP for upload notifications.
- Least‑privilege S3 creds: read‑only for specified buckets; scoped IAM/policy for MinIO/Ceph.
- PII handling: redact logs; encrypt data in transit; consider KMS for at‑rest if we stage content.

## Testing Strategy
- Unit: chunker (boundary/overlap), token estimator, batcher flush logic.
- Integration (local): run Restate server; ingest sample files; verify `nextSeq`/resume, batch contents, and idempotent flush.
- Fault injection: kill ingestor mid‑stream; kill batcher mid‑flush; simulate duplicate events.

## Migration Plan (TS → Rust/WASI‑P2)
- Keep service contracts stable (`UploadNotification`, `ChunkEnvelope`, `BatchRequest`).
- Mirror interfaces in Rust SDK; share JSON schemas/OpenAPI for cross‑lang compatibility.
- Stage migration by VO key: move batcher first (state is VO‑scoped), then ingestor.

## Open Questions
- Embedding provider constraints (max tokens/input, batch size, QPS) to finalize `maxTokensPerBatch` and backoff.
- Supported file types in first milestone (plaintext only vs HTML/PDF).
- Downstream transport: direct HTTP vs queue/topic for batches.

## Next Steps
1) Confirm `UploadNotification` fields from the chosen S3 backend.
2) Lock embedding provider limits and batch sizing.
3) Implement TS/Bun services (`upload-ingestor.ts`, `chunk-batcher.ts`) with minimal plaintext parser.
4) Add Knative adapter for S3/MinIO events; wire to Restate endpoint.
5) Integration tests against local Restate server with sample files.

