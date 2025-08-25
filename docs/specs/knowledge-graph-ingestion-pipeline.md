# Knowledge Graph Ingestion Pipeline (Technology-Neutral) v1 — Requirements

This document defines the technology-agnostic requirements for the ingestion pipeline that powers the Knowledge Graph API v1. It specifies responsibilities, contracts, stage semantics, durability, idempotency, retries, observability, and acceptance criteria. It deliberately avoids protocol, datastore, framework, and provider choices and prescribes a single golden path (no fallbacks).

- Version: v1 (pipeline)
- Status: Draft for review
- Companion spec: knowledge-graph-api.md (Operations)

## 1) Scope and Principles

- Purpose: Transform episodic inputs (text/json/messages) into a bitemporal knowledge graph (nodes/edges) through an ordered set of deterministic stages.
- Golden path: One extraction strategy, one embedding path (optional), one upsert policy; no alternates.
- Tech-agnostic: No assumptions about queues, databases, or LLM/embedding providers.
- Multi-tenancy: `group_id` strictly scopes all processing and uniqueness.
- Determinism: Idempotent, replay-safe, and stable outputs under retry/restart.
- Time: ISO 8601 UTC, millisecond precision.
- Security/PII: Minimize sensitive content exposure in logs/metrics; redact where possible.

## 2) Glossary

- Item: A single ingestion unit (message or episode).
- Receipt: Acknowledgement of queued items (non-blocking acceptance).
- Stage: A processing step in the pipeline state machine.
- Parked: Terminal non-destructive failure state after exhausting retries.
- Lineage: Linkage from derived nodes/edges to the source episode uuid.

## 3) External Interfaces (to/from Pipeline)

- Inputs (from Operations):
  - AddEpisodes: batch of items (group_id + items[])
  - AddMessages: batch of messages (group_id + messages[])
- Outputs:
  - ACCEPTED receipts with counts and optional receipt_id.
  - Derived graph mutations (nodes/edges) become visible to retrieval once completed.
  - No separate completion callback required in v1; visibility is via retrieval Operations.

## 4) High-Level Architecture

- Item-level state machine (single path):
  - accepted → extracted → embedded → upserted → completed
  - Failure substates: extract_failed (retryable), embed_failed (retryable), upsert_failed (retryable), parked (terminal)
- Isolation: Processing is per item; no cross-item transactions.
- Ordering: No strict ordering guarantee across items; per-item determinism required.

## 5) Stage Contracts

### 5.1 Acceptance Stage
- Responsibilities:
  - Validate request envelope/schema and limits before enqueuing.
  - Assign item identifier(s) and compute `content_hash` based on normalized content.
  - Enqueue for processing; return ACCEPTED receipt.
- Idempotency:
  - Deduplicate by `uuid` when present; otherwise by `idempotency_key` (if provided), else by `content_hash` within a dedup window `W`.
  - Replays must not create duplicate work or graph effects.
- Limits:
  - Enforce payload size `S` and max items `M` per request; reject on violation.

### 5.2 Extraction Stage
- Input: Item (episode/message) with `group_id`, source kind, body, timestamps.
- Output (structured):
  - nodes: [ { tmp_ref or uuid?, name, summary?, attributes? } ]
  - edges: [ { name, fact, source_ref (uuid|tmp_ref), target_ref (uuid|tmp_ref), valid_at?, invalid_at?, qualifiers? } ]
- Rules:
  - Strict schema validation; normalization of names/facts.
  - Deterministic disambiguation: resolve node mentions by `(group_id, exact normalized name)`; otherwise create new node via node-upsert rules.
  - No partial effects on failure; if output invalid after `N` retries (with backoff), PARK the item.
- Privacy:
  - Avoid logging raw bodies; record hashes and minimal metadata.

### 5.3 Embedding Stage (Optional)
- Inputs: node name and/or summary; optionally fact text.
- Outputs: `embedding_reference` or `content_hash` (opaque), associated with entity or edge.
- Rules:
  - Idempotency key: `(group_id, entity_uuid, content_hash)`.
  - Cache hits MUST avoid duplicate compute.
  - Failures retry up to `N`; then PARKED; absence of embeddings degrades affected items to keyword-only search.

### 5.4 Upsert Stage
- Inputs: normalized nodes/edges (and optional embedding references) + source episode uuid.
- Node upsert:
  - Key: `(group_id, uuid)` if provided; else `(group_id, normalized_name_hash)`.
  - Merge policy: `uuid` and `created_at` immutable; update `summary/attributes` if changed.
- Edge upsert:
  - Requires existing source/target nodes; create/resolve as per node-upsert rules.
  - Uniqueness key: `(group_id, source_uuid, name, target_uuid, valid_at)` unless `uuid` provided.
  - Bitemporal updates: contradiction handling may expire prior edges (see §6).
- Lineage:
  - Persist `source_episode_uuid` on all created/updated nodes/edges.

## 6) Bitemporal Semantics and Contradictions

- Fact time: `valid_at..invalid_at`. System time: `created_at..expired_at`.
- Default retrieval: facts valid “now” and not expired.
- Contradictions:
  - Conflict key: normalized `(source_uuid, name, target_uuid, qualifiers?)`.
  - On ingesting a conflicting fact F′, set `expired_at = now` (system time) on superseded facts F. If F′ explicitly negates F in fact time, set `invalid_at` accordingly.
  - Preserve complete history; tie-break by latest `created_at`, else lowest `uuid`.

## 7) Idempotency and Exactly-Once Effects

- Item-level idempotency via `uuid`, `idempotency_key`, or `content_hash` within window `W`.
- Stage idempotency: each stage MUST be re-entrant; redundant executions produce the same resulting graph state.
- Exactly-once logical effects are guaranteed via deterministic upsert keys and uniqueness constraints.

## 8) Concurrency, Ordering, and Partitioning

- Items may process concurrently; no strict ordering across items.
- Optional partitioning by `group_id` may be used to limit cross-tenant interference (conceptual; tech-neutral).
- Concurrency control (conceptual semaphore) to bound external calls (e.g., extraction/embedding) and protect downstream resources.
- Avoid head-of-line blocking between unrelated items.

## 9) Backpressure and Flow Control

- Queue capacity thresholds MUST be defined; when exceeded, callers receive immediate rejection at acceptance stage (with error output), or admission is throttled (policy to be decided, single path to be documented).
- Admission control SHOULD consider per-tenant quotas to ensure fairness.

## 10) Retry, Backoff, and Error Taxonomy

- Retryable errors (transient): timeouts, rate limits, temporary unavailability.
- Non-retryable errors: schema violations, unsupported formats, permanent validation failures.
- Policy:
  - Exponential backoff with jitter up to `N` attempts per stage.
  - On exhausting attempts, move to PARKED; no partial graph effects.
  - Error details recorded in an operator-visible store/log (without sensitive payloads).

## 11) Data Retention and Lifecycle

- Idempotency records retained for window `W`.
- Parked items retained for operator review for a retention period `R` (to be set).
- Transient artifacts (extraction outputs, embedding intermediates) have retention `R_t` appropriate for replay and audit.
- Receipts stored for auditability of acceptance decisions with minimal metadata.

## 12) Observability

- Logs per item and stage:
  - `request_id`, `group_id`, item_id, stage, status, latency_ms, error_code?, hashes (not raw payloads).
- Metrics (counters/gauges/histograms):
  - `ingestion_accepted_total`, `ingestion_rejected_total`
  - `stage_processed_total{stage}`, `stage_failed_total{stage, error_code}`
  - `parked_items_total`
  - `stage_latency_ms{stage}` p50/p90/p99
  - `upsert_conflicts_total`, `extraction_failures_total`, `embedding_failures_total`
- Tracing: end-to-end correlation via `request_id` and item_id.

## 13) Multi-Tenancy and Isolation

- `group_id` is mandatory on writes; used for scoping uniqueness and resource accounting.
- Isolation expectations:
  - Per-tenant quotas for admission and compute (policy to be defined).
  - Metrics and logs include `group_id` for isolation analysis.

## 14) Security and Privacy

- Do not log raw episode/message content; use hashes and minimal metadata.
- Clearly mark fields that can contain PII; implement redaction policy for observability outputs.
- Access control and encryption at rest are out of scope for v1 (implementation-specific), but the pipeline MUST enable redaction hooks.

## 15) Configuration and Tunables (to be fixed)

- `S`: Max payload size per request.
- `M`: Max items per ingestion request.
- `N`: Max retries per stage.
- `W`: Idempotency/dedup window.
- `R`: Retention for parked items.
- `R_t`: Retention for transient artifacts.
- Concurrency limits per stage (e.g., extraction, embedding, upsert).
- `T_ingest_visibility`: Time bound for item visibility in retrieval after ACCEPTED.

## 16) Service Level Objectives (Targets to be finalized)

- Acceptance latency (p95): ≤ T_accept_p95.
- Ingest-to-visible (p95): ≤ `T_ingest_visibility`.
- Error budgets: monthly failure rate thresholds for PARKED items and stage failures.

## 17) Versioning and Schema Evolution

- Pipeline payload schemas are versioned with the API v1.
- Breaking changes require a new major version (v2); no in-place behavioral drift.
- No feature flags or alternate paths in v1.

## 18) Acceptance Criteria (Pipeline-Focused)

1. Acceptance & Dedup
   - Given a valid batch within limits, items are ACCEPTED with counts.
   - Replays by uuid, idempotency_key, or identical content_hash within `W` do not produce duplicate effects.
2. Extraction
   - Valid structured outputs deterministically produce the same nodes/edges and lineage across retries/restarts.
   - Invalid outputs retry up to `N`; then PARKED with no graph mutations.
3. Embedding (optional)
   - Duplicate content produces cache hits; failures retry then PARKED; keyword-only search remains possible for affected items.
4. Upsert & Contradictions
   - Node and edge upserts enforce uniqueness keys and referential integrity.
   - Contradictory facts expire prior edges per §6; history preserved.
5. Durability & Restart
   - After process restart, accepted items resume processing without duplicate effects.
6. Concurrency & Backpressure
   - Concurrency limits are respected; when capacity is exceeded, a single documented policy (reject or throttle) is applied consistently.
7. Observability
   - Logs/metrics are emitted per stage and item; sensitive content is not exposed.

## 19) Phasing and Deliverables

- P1: Acceptance and Extraction stages; idempotency and limits; basic logs/metrics.
- P2: Embedding stage (optional path), caching semantics; enhanced observability.
- P3: Upsert stage with contradiction handling and lineage.
- P4: Durability/retry policies; parking; restart-resume tests; backpressure policy.
- P5: SLO validation and performance tests; retention policies and operator tooling for parked items.

---

### Appendix A: State Machine (Textual)

- accepted: Item admitted; minimal metadata and hashes recorded; enqueued for processing.
- extracted: Structured nodes/edges produced; normalized and validated.
- embedded (optional): Embeddings computed and associated; cache consulted first.
- upserted: Nodes and edges persisted/upserted atomically for the item; contradictions resolved.
- completed: Item’s effects are fully visible to retrieval.
- Failure substates: extract_failed, embed_failed, upsert_failed (with retry counters and reasons); parked when retries exhausted.

### Appendix B: Structured Extraction Output Schema (Abstract)

```json
{
  "nodes": [
    {
      "tmp_ref": "string?",
      "uuid": "string?",
      "name": "string",
      "summary": "string?",
      "attributes": { }
    }
  ],
  "edges": [
    {
      "name": "string",
      "fact": "string",
      "source_ref": "uuid|tmp_ref",
      "target_ref": "uuid|tmp_ref",
      "valid_at": "ISO 8601 UTC?",
      "invalid_at": "ISO 8601 UTC?",
      "qualifiers": { }
    }
  ]
}
```

### Appendix C: Error Taxonomy (Examples)

- Retryable: transient network/availability, rate limiting, timeouts from external services.
- Non-retryable: schema violation, unsupported source kind, invalid timestamp formats, permanent extraction validation failures.

### Appendix D: Hashing & Idempotency Keys (Policy)

- `content_hash` for episodes computed over normalized `(body, source, reference_time)`.
- Node name hash computed over `(group_id, normalized name)`.
- Embedding idempotency key `(group_id, entity_uuid, content_hash)`.
- Idempotency records persist within `W` to suppress duplicates and ensure replay determinism.

