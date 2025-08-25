# Knowledge Graph API (Technology-Neutral) v1

This document specifies a technology-agnostic set of Operations that together form a bitemporal knowledge-graph service for ingesting episodic inputs, extracting entities and facts, maintaining a temporal graph, and retrieving relevant facts deterministically. It intentionally avoids protocol, transport, datastore, provider, or framework choices. No JVM dependencies are implied.

- Version: v1 (single surface; breaking changes require v2)
- Status: Draft for review
- Owner: Slipstream Entity Discovery

## 1. Scope and Principles

- Goal: Define Operations (commands) and data contracts to:
  - Ingest episodes (text/json/messages)
  - Extract and upsert entities and facts into a bitemporal knowledge graph
  - Search and retrieve facts deterministically
- Single golden path: one ingestion flow, one upsert policy, one retrieval approach (hybrid search with RRF). No fallbacks or legacy paths.
- Tech-agnostic: No datastore, protocol, framework, or LLM/provider specified.
- Multi-tenancy: `group_id` scopes all data, uniqueness, and queries.
- Determinism: Idempotent writes, strict schemas, deterministic ranking and tie-breaking.
- Time: ISO 8601 UTC, millisecond precision.
- Out of scope v1: authentication/authorization, MCP/IDE integrations, CI/CD, admin UI, multi-provider plug-ins.

## 2. Domain Model (Canonical Types)

### 2.1 Group
- `group_id: string` — tenant namespace key.

### 2.2 Episode
- Fields:
  - `uuid?: string`
  - `group_id: string`
  - `name?: string`
  - `body: string`
  - `source: "text" | "json" | "message"`
  - `source_description?: string`
  - `reference_time: datetime (ISO 8601 UTC)`
  - `created_at: datetime (ISO 8601 UTC)`
- Notes:
  - Immutable post-ingestion.
  - Serves as lineage anchor for derived graph mutations.

### 2.3 EntityNode
- Fields:
  - `uuid: string`
  - `group_id: string`
  - `name: string`
  - `summary?: string`
  - `attributes?: object` (opaque)
  - `created_at: datetime (ISO 8601 UTC)`
- May include `embedding_reference` or `content_hash` (opaque, provider-neutral).

### 2.4 EntityEdge (Fact)
- Fields:
  - `uuid: string`
  - `group_id: string`
  - `name: string` (relation)
  - `fact: string`
  - `source_node_uuid: string`
  - `target_node_uuid: string`
  - `valid_at?: datetime`
  - `invalid_at?: datetime`
  - `created_at: datetime`
  - `expired_at?: datetime`
- Bitemporal semantics over fact time and system time.

### 2.5 Invariants
- UUID uniqueness per type within `group_id`.
- Referential integrity: edges require existing nodes (or nodes deterministically created during upsert).
- Temporal consistency: as-of filtering respects fact and system time.
- Deletion v1: hard delete; group deletion cascades episodes → edges → nodes.
- Normalization: canonical rules for whitespace/case in names/facts; content hashing uses normalized content.

## 3. Operation Model (Uniform Interface)

All Operations use the same envelope.

### 3.1 Operation Request
```json
{
  "request_id": "string? (optional; generated if absent)",
  "idempotency_key": "string? (optional; ingestion operations SHOULD support)",
  "input": { /* operation-specific */ }
}
```

### 3.2 Operation Response
```json
{
  "request_id": "string",             
  "status": "OK" | "ACCEPTED" | "ERROR" | "PARKED",
  "output": { /* operation-specific */ },
  "error": {
    "error_code": "string",
    "message": "string",
    "details": { /* optional */ }
  }
}
```

- ACCEPTED denotes asynchronous processing has begun; final visibility is governed by the ingestion SLO.
- PARKED denotes a terminal non-destructive failure state after exhausting retries.

## 4. Operations (Commands)

### 4.1 Healthcheck
- Input: `{}`
- Output: `{ "status": "healthy" }`
- Status: `OK`

### 4.2 Ingestion

#### 4.2.1 AddMessages
- Input:
```json
{
  "group_id": "string",
  "messages": [
    {
      "uuid": "string?",
      "name": "string?",
      "role_type": "user" | "assistant" | "system",
      "role": "string?",
      "content": "string",
      "timestamp": "ISO 8601 UTC",
      "source_description": "string?"
    }
  ]
}
```
- Output: `{ "message": "string", "accepted": number }`
- Status: `ACCEPTED`

#### 4.2.2 AddEpisodes
- Input:
```json
{
  "group_id": "string",
  "items": [
    {
      "uuid": "string?",
      "name": "string?",
      "source": "text" | "json" | "message",
      "body": "string",
      "reference_time": "ISO 8601 UTC",
      "source_description": "string?"
    }
  ]
}
```
- Output: `{ "receipt_id": "string", "accepted": number }`
- Status: `ACCEPTED`

#### 4.2.3 AddEntityNode
- Input:
```json
{
  "uuid": "string",
  "group_id": "string",
  "name": "string",
  "summary": "string?",
  "attributes": { }
}
```
- Output: `EntityNode`
- Status: `OK` on idempotent create/update; `ERROR` with `error_code="CONFLICT"` on uniqueness violation.

### 4.3 Retrieval

#### 4.3.1 SearchFacts
- Input:
```json
{
  "group_ids": ["string"],
  "query": "string",
  "max_facts": 10,
  "center_node_uuid": "string?"
}
```
- Output:
```json
{ "facts": [ { "uuid": "string", "name": "string", "fact": "string", "valid_at": "ISO?", "invalid_at": "ISO?", "created_at": "ISO", "expired_at": "ISO?" } ] }
```
- Status: `OK`

#### 4.3.2 GetEntityEdge
- Input: `{ "uuid": "string" }`
- Output: `FactResult`
- Status: `OK` or `ERROR` with `error_code="NOT_FOUND"`.

#### 4.3.3 GetEpisodes
- Input: `{ "group_id": "string", "last_n": number }`
- Output: `{ "episodes": [ Episode ] }`
- Status: `OK`

#### 4.3.4 GetMemory
- Input:
```json
{
  "group_id": "string",
  "messages": [
    { "role_type": "user"|"assistant"|"system", "role": "string?", "content": "string", "timestamp": "ISO 8601 UTC" }
  ],
  "max_facts": 10,
  "center_node_uuid": "string?"
}
```
- Behavior: Compose a deterministic query from messages (Appendix A), then act as `SearchFacts`.
- Output: `{ "facts": [ FactResult ] }`
- Status: `OK`

### 4.4 Maintenance

#### 4.4.1 DeleteEntityEdge
- Input: `{ "uuid": "string" }`
- Output: `{ "message": "string", "success": true }`
- Status: `OK` (idempotent delete-if-exists)

#### 4.4.2 DeleteEpisode
- Input: `{ "uuid": "string" }`
- Output: `{ "message": "string", "success": true }`
- Status: `OK`
- Note: v1 does NOT cascade to derived nodes/edges.

#### 4.4.3 DeleteGroup
- Input: `{ "group_id": "string" }`
- Output: `{ "message": "string", "success": true }`
- Status: `OK` after cascade delete (episodes → edges → nodes)

#### 4.4.4 ClearAll (admin)
- Input: `{}`
- Output: `{ "message": "string", "success": true }`
- Status: `OK`

### 4.5 Canonical Types Referenced in Outputs
- `FactResult`:
```json
{ "uuid": "string", "name": "string", "fact": "string", "valid_at": "ISO?", "invalid_at": "ISO?", "created_at": "ISO", "expired_at": "ISO?" }
```
- `EntityNode`:
```json
{ "uuid": "string", "group_id": "string", "name": "string", "summary": "string?", "attributes": { }, "created_at": "ISO" }
```
- `Episode`:
```json
{ "uuid": "string", "group_id": "string", "name": "string?", "body": "string", "source": "text|json|message", "reference_time": "ISO", "created_at": "ISO", "source_description": "string?" }
```

## 5. Ingestion Pipeline (Durable, Single Path)

- Per-item state machine: `accepted → extracted → embedded → upserted → completed`.
- Failure substates: `extract_failed` (retryable), `embed_failed` (retryable), `upsert_failed` (retryable), `parked` (terminal, non-destructive).
- Item-level processing; no cross-item transactional guarantees.
- Idempotency: prefer item `uuid`; else `content_hash`. Replays are no-ops.
- Exactly-once logical effects via upserts and uniqueness constraints.
- Lineage: derived nodes/edges store `source_episode_uuid`.
- Visibility SLO: derived facts available within `T_ingest_visibility` (to be defined).

## 6. Bitemporal Semantics and Contradictions

- Fact time: `valid_at..invalid_at` (when true).
- System time: `created_at..expired_at` (when current in system).
- Default retrieval filters: valid now (fact-time intersects now) and not expired (system-time `expired_at` null).
- Contradiction policy:
  - Conflict key: normalized `(source_node_uuid, name, target_node_uuid, qualifiers?)`.
  - On new conflicting fact F′, expire superseded facts F: `expired_at = now` (system time). If F′ negates F in fact time, set `invalid_at` accordingly.
  - Preserve history; tie-breakers: latest `created_at`, else lowest `uuid` lexicographically.

## 7. Extraction Strategy and Validation

- Input: Episode with `source: text|json|message`.
- Structured output (provider-neutral):
```json
{
  "nodes": [ { "tmp_ref": "string?", "uuid": "string?", "name": "string", "summary": "string?", "attributes": { } } ],
  "edges": [ { "name": "string", "fact": "string", "source_ref": "uuid|tmp_ref", "target_ref": "uuid|tmp_ref", "valid_at": "ISO?", "invalid_at": "ISO?", "qualifiers": { } } ]
}
```
- Node reference resolution: match `(group_id, exact normalized name)`; else create new node via node-upsert rules.
- Strict validation; normalization of names/facts by policy.
- Failures: retry up to `N`; then `PARKED`; no partial graph effects.

## 8. Embedding Policy (Optional)

- Inputs: node name/summary; optionally fact text.
- Output: `embedding_reference` or `content_hash` (opaque).
- Idempotency key: `(group_id, entity_uuid, content_hash)`.
- Failures: retry → `PARKED`; absence of embeddings degrades to keyword-only for those items.

## 9. Upsert Logic and Referential Integrity

- Node upsert:
  - Key: `(group_id, uuid)` if provided; else `(group_id, normalized_name_hash)`.
  - Merge: `uuid` immutable; update `summary/attributes` if changed; `created_at` immutable.
- Edge upsert:
  - Requires source/target nodes; create/resolve via node-upsert rules.
  - Uniqueness key: `(group_id, source_uuid, name, target_uuid, valid_at)` unless `uuid` provided.
- Lineage: nodes/edges carry `source_episode_uuid`.

## 10. Retrieval and Search Behavior

- Hybrid search (facts):
  - Keyword: over fact text, node names, selected attributes (policy-defined).
  - Semantic: nearest-neighbor using embeddings (when available).
  - Fusion: RRF(rank_keyword, rank_semantic, k=60). If one modality missing, use the available one.
  - Optional rerank: graph distance from `center_node_uuid` with weight `w`; ties → `uuid` ascending.
- Node search recipes: fixed initial set (e.g., hybrid by name/summary).
- As-of parameters: deferred in v1; defaults apply (current validity/system time).
- `GetMemory`: build query from messages deterministically (Appendix A), then use `SearchFacts`.

## 11. Durability, Idempotency, Retries, Compensations

- Idempotency: support `idempotency_key` and/or per-item `uuid` on ingestion; dedup window `W` persisted.
- Execution guarantees: at-least-once step execution tolerated; exactly-once logical effects via upsert/uniqueness.
- Retries: bounded exponential backoff; max attempts `N` per step.
- Compensation: on failure, item remains retryable; parked items visible via metrics; no partial state leaks.
- Durability: accepted items survive restarts; processing resumes; no ordering guarantees across items.

## 12. Limits and Validation Policies

- Limits: maximum payload size `S`; maximum items per ingestion call `M`.
- Field caps: name, summary, fact, attributes; policy is reject (not truncate) with `status=ERROR` and limit error code.
- `max_facts`: default and hard bound defined.
- Strict schema validation: unknown fields rejected; datetimes must be ISO 8601 UTC.

## 13. Security and Multi-Tenancy

- `group_id` required on write operations; reads may accept multiple `group_ids` where relevant.
- Uniqueness/upserts/search scoped by `group_id` by default.
- Reserved for future: auth and tenant routing (unspecified in v1).
- Privacy: embeddings and facts are tenant-scoped.

## 14. Observability (Minimal)

- Logs per operation: `request_id`, operation name, `group_id`, `status`, `latency_ms`, outcome, `error_code?`.
- Metrics:
  - `ingestion_accepted_total`, `ingestion_processed_total`, `ingestion_failed_total`
  - `parked_items_total`
  - `search_latency_ms` p50/p90/p99
  - `extraction_failures_total`, `embedding_failures_total`, `upsert_conflicts_total`
- Tracing: correlate via `request_id` (backend unspecified).

## 15. Acceptance Criteria (Operation-Oriented)

- Ingestion
  - Given `AddEpisodes` with `K` items → returns `ACCEPTED` with `accepted=K`; within `T_ingest_visibility`, derived facts appear in `SearchFacts`. Replays with same `uuid`s or `idempotency_key` produce no duplicates and identical result order.
  - Items yielding invalid extraction output retry up to `N`, then `PARKED`; they create no nodes/edges; `SearchFacts` returns no facts from them.
- Retrieval/Search
  - On a corpus with mixed embedding presence, `SearchFacts` returns deterministic RRF ranking; if only one modality present, uses that; ties → `uuid` ascending.
  - Defaults return only facts valid now and not expired; contradictory historical facts do not appear.
  - `GetMemory` produces the same results as `SearchFacts` on the composed query.
- Maintenance
  - `DeleteGroup` removes all resources; subsequent `GetEpisodes`/`SearchFacts` for that group return empty.
  - `DeleteEntityEdge` is idempotent: repeated calls yield `OK` with `success=true`.
- Errors
  - Schema violations → `ERROR` with `error_code="INVALID_ARGUMENT"` (or equivalent).
  - Conflicts → `ERROR` with `error_code="CONFLICT"`.
  - Not found → `ERROR` with `error_code="NOT_FOUND"`.

## 16. Conformance Test Outline (Black-Box)

- Schema validation for all operations (happy/failure; unknown fields).
- Idempotency/dedup tests (uuids, idempotency_key; replay behavior).
- Bitemporal logic (overlaps, default as-of; contradiction resolution expiring older facts).
- Search (keyword-only, embedding-only, hybrid; RRF determinism; rerank; tie-breaking).
- Maintenance (cascade delete semantics; idempotent deletes; clear-all behavior).
- Limits (payload size `S`, item count `M`, field caps).

## 17. Versioning and Compatibility Policy

- Single v1 surface; no alternate or legacy paths.
- Breaking changes require v2; no behavioral drift within v1.
- No hidden feature flags.

## 18. Open Parameters to Finalize

- `S` (max payload size), `M` (max items per ingestion call), `N` (max retries per step), `W` (idempotency dedup window), `T_ingest_visibility` (ingest-to-visible bound), `w` (rerank weight).
- Whether to expose explicit as-of parameters in v1 (default: deferred).
- Whether `DeleteEpisode` should optionally cascade to derived artifacts in v1 (default: no cascade).

---

### Appendix A: Deterministic Message-to-Query Composition

For `GetMemory`, compose query as follows, in message order:

```
<role_type>(<role or empty>): <content>\n
```

Concatenate all lines; use the resulting string as `query` for `SearchFacts`.

