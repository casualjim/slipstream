# Knowledge Graph API v1 — Traceability Matrix

This document maps the technology-neutral requirements from the Knowledge Graph API v1 specification to concrete Operations and acceptance tests. It is organized for black‑box conformance and phased delivery planning.

- Spec reference: knowledge-graph-api.md (v1)
- Status: Draft for review

## 1. Legend

- Requirement IDs
  - FR-x: Functional Requirement
  - NFR-x: Non‑Functional Requirement
- Operation IDs
  - OP-Healthcheck, OP-AddMessages, OP-AddEpisodes, OP-AddEntityNode, OP-SearchFacts, OP-GetEntityEdge, OP-GetEpisodes, OP-GetMemory, OP-DeleteEntityEdge, OP-DeleteEpisode, OP-DeleteGroup, OP-ClearAll
- Parameters to finalize: N (max retries), W (idempotency window), S (max payload size), M (max items), T_ingest_visibility (ingestion-to-visible bound), w (graph distance weight)

## 2. Requirements List

### Functional Requirements (FR)
- FR-1: Ingest episodes (text/json/message) in batches and accept them asynchronously.
- FR-2: Ingest chat messages and accept them asynchronously.
- FR-3: Deterministically extract nodes/edges from episodes; validate structured output; no partial mutations on extraction failures.
- FR-4: Upsert nodes and edges deterministically with referential integrity and lineage.
- FR-5: Maintain bitemporal semantics (fact time and system time) and a contradiction policy that expires superseded facts.
- FR-6: Hybrid fact search (keyword + semantic) with RRF fusion and deterministic tie-breaking; optional graph-distance rerank.
- FR-7: Deterministic GetMemory query composition from messages.
- FR-8: Retrieve entity edge by UUID.
- FR-9: Retrieve last N episodes for a group.
- FR-10: Maintenance: delete entity edge; idempotent.
- FR-11: Maintenance: delete episode (no cascade to derived artifacts in v1).
- FR-12: Maintenance: delete group with cascade (episodes → edges → nodes); clear-all (admin).
- FR-13: Healthcheck operation.

### Non‑Functional Requirements (NFR)
- NFR-1: Single golden path; no fallbacks or legacy paths.
- NFR-2: Multi‑tenancy: all operations and uniqueness scoped by group_id.
- NFR-3: Determinism: idempotent writes, strict schemas, deterministic ranking/ties.
- NFR-4: Durability & retries: at‑least‑once steps; logical exactly-once via upserts; retries with backoff; parked after N.
- NFR-5: Input limits: S (payload), M (batch size), field caps; reject on violation.
- NFR-6: Observability: structured logs and metrics; request correlation.
- NFR-7: Time handling: ISO 8601 UTC, ms precision.
- NFR-8: Versioning policy: v1 single surface; breaking changes require v2.

## 3. Operations Catalog (for reference)
- OP-Healthcheck
- OP-AddMessages
- OP-AddEpisodes
- OP-AddEntityNode
- OP-SearchFacts
- OP-GetEntityEdge
- OP-GetEpisodes
- OP-GetMemory
- OP-DeleteEntityEdge
- OP-DeleteEpisode
- OP-DeleteGroup
- OP-ClearAll

## 4. Traceability Matrix

Each requirement maps to Operations and acceptance tests. Tests are black‑box and action-oriented (inputs/outputs/status), technology‑neutral.

### FR-1: Ingest episodes asynchronously
- Operations: OP-AddEpisodes
- Acceptance tests:
  - FR-1.1 Accepted batch
    - Input: AddEpisodes with K valid items
    - Expect: status=ACCEPTED, output.accepted=K
  - FR-1.2 Visibility within SLO
    - After ≤ T_ingest_visibility, facts derived from the items are returned by OP-SearchFacts with default filters
  - FR-1.3 Idempotent replay by uuid
    - Re-submit same items with identical uuids → no duplicate nodes/edges; OP-SearchFacts results identical (order and content)
  - FR-1.4 Idempotent replay by idempotency_key
    - Submit with idempotency_key, then replay with same key → no duplicates; same search results

### FR-2: Ingest chat messages asynchronously
- Operations: OP-AddMessages
- Acceptance tests:
  - FR-2.1 Accepted batch of messages → status=ACCEPTED, accepted=K
  - FR-2.2 Visibility SLO and idempotency mirror FR-1.2–1.4

### FR-3: Deterministic extraction; no partial mutations on failure
- Operations: OP-AddEpisodes, OP-AddMessages (ingestion), extraction stage
- Acceptance tests:
  - FR-3.1 Valid structured output → nodes/edges materialize deterministically; lineage set to source episode uuid
  - FR-3.2 Invalid structured output → retries up to N; then PARKED; no nodes/edges created
  - FR-3.3 Node mention resolution deterministic: (group_id, exact normalized name) match before new node creation

### FR-4: Deterministic upsert; referential integrity; lineage
- Operations: Upsert logic in ingestion; OP-AddEntityNode
- Acceptance tests:
  - FR-4.1 Node upsert keying
    - With uuid → merges fields; created_at immutable
    - Without uuid → keyed by (group_id, normalized_name_hash)
  - FR-4.2 Edge upsert requires existing nodes or deterministic creation
  - FR-4.3 Uniqueness key for edges: (group_id, source_uuid, name, target_uuid, valid_at) unless uuid provided
  - FR-4.4 Lineage present on nodes/edges (source_episode_uuid)

### FR-5: Bitemporal semantics and contradictions
- Operations: Applied to retrieval and ingestion
- Acceptance tests:
  - FR-5.1 Default retrieval returns facts valid now (fact time) and not expired (system time)
  - FR-5.2 Contradiction handling
    - New conflicting fact F′ causes superseded facts F to receive expired_at=now (system time)
    - If F′ negates F in fact time, F.invalid_at set accordingly
  - FR-5.3 Tie-breaking: when overlapping, return the one with latest created_at; if equal, lowest uuid

### FR-6: Hybrid search with RRF; deterministic ties; optional rerank
- Operations: OP-SearchFacts
- Acceptance tests:
  - FR-6.1 Keyword-only corpus → ranking stable and deterministic
  - FR-6.2 Embedding-only corpus → ranking stable and deterministic
  - FR-6.3 Hybrid corpus → RRF fusion applied; deterministic rank
  - FR-6.4 Optional graph-distance rerank with weight w; ties by uuid asc

### FR-7: Deterministic GetMemory composition
- Operations: OP-GetMemory → OP-SearchFacts
- Acceptance tests:
  - FR-7.1 Given messages, composed query equals concatenation policy; OP-SearchFacts on that query equals OP-GetMemory result

### FR-8: Retrieve entity edge by UUID
- Operations: OP-GetEntityEdge
- Acceptance tests:
  - FR-8.1 Existing uuid → FactResult
  - FR-8.2 Unknown uuid → status=ERROR, error_code="NOT_FOUND"

### FR-9: Retrieve last N episodes by group
- Operations: OP-GetEpisodes
- Acceptance tests:
  - FR-9.1 Valid group and last_n → returns most recent N episodes for that group (policy-defined ordering)

### FR-10: Delete entity edge (idempotent)
- Operations: OP-DeleteEntityEdge
- Acceptance tests:
  - FR-10.1 Delete existing edge → success=true, status=OK
  - FR-10.2 Delete same edge again → success=true, status=OK (idempotent)

### FR-11: Delete episode (no cascade in v1)
- Operations: OP-DeleteEpisode
- Acceptance tests:
  - FR-11.1 Delete existing episode → success=true
  - FR-11.2 Derived nodes/edges remain (no cascade); clearly documented behavior

### FR-12: Delete group cascade; clear-all
- Operations: OP-DeleteGroup, OP-ClearAll
- Acceptance tests:
  - FR-12.1 DeleteGroup removes episodes → edges → nodes; subsequent OP-SearchFacts & OP-GetEpisodes for that group return empty
  - FR-12.2 ClearAll (admin) removes all tenant data; subsequent reads are empty

### FR-13: Healthcheck
- Operations: OP-Healthcheck
- Acceptance tests:
  - FR-13.1 Returns status=OK with output.status="healthy"

### NFR-1: Single golden path
- Operations: All
- Acceptance tests:
  - NFR-1.1 No alternate providers/flows are required to pass any test; only the specified path exists

### NFR-2: Multi‑tenancy (group_id scoping)
- Operations: All write operations require group_id; reads accept group_ids where applicable
- Acceptance tests:
  - NFR-2.1 Data from different group_ids never co-mingle in retrieval or uniqueness checks

### NFR-3: Determinism
- Operations: Ingestion, SearchFacts, GetMemory
- Acceptance tests:
  - NFR-3.1 Replays with same uuid/idempotency_key produce identical graph state and ordering
  - NFR-3.2 Ranking tie-breaks by uuid asc consistently

### NFR-4: Durability & retries
- Operations: Ingestion pipeline
- Acceptance tests:
  - NFR-4.1 After restart, accepted items resume processing; no duplicate effects
  - NFR-4.2 Failed steps retry with backoff up to N; terminal state is PARKED without partial graph state

### NFR-5: Limits & validation
- Operations: Ingestion and retrieval where applicable
- Acceptance tests:
  - NFR-5.1 Payload > S → status=ERROR with limit error
  - NFR-5.2 Items > M → status=ERROR with limit error
  - NFR-5.3 Field caps exceeded → status=ERROR (reject, no truncate)
  - NFR-5.4 Unknown fields → status=ERROR (schema strict)
  - NFR-5.5 Non-UTC datetimes → status=ERROR

### NFR-6: Observability
- Operations: All
- Acceptance tests:
  - NFR-6.1 Logs include request_id, operation, group_id, status, latency_ms, error_code?
  - NFR-6.2 Metrics counters/gauges exist and increment appropriately (at least in a test environment)

### NFR-7: Time handling
- Operations: All time-bearing operations
- Acceptance tests:
  - NFR-7.1 All reported times are ISO 8601 UTC with ms precision

### NFR-8: Versioning policy
- Operations: Entire surface identified as v1
- Acceptance tests:
  - NFR-8.1 No alternate or legacy operation surfaces are required to pass tests; breaking changes documented as v2-only in future

## 5. Example Test Scenarios (Black‑Box)

These scenarios illustrate inputs and expected outputs/status. They are protocol-agnostic and can be executed via any harness.

### TS-1 Ingest and visibility
- Step 1: OP-AddEpisodes with 3 items (valid)
  - Expect: status=ACCEPTED, accepted=3
- Step 2: Within ≤ T_ingest_visibility, OP-SearchFacts with a query matching derived facts
  - Expect: status=OK, facts include items from Step 1 following default bitemporal filters

### TS-2 Idempotent replay by uuid
- Step 1: OP-AddEpisodes with uuids U1..Uk
- Step 2: Replay the same request (same uuids)
  - Expect: no duplicates; OP-SearchFacts results identical in order and content

### TS-3 Extraction failure parking
- Step 1: OP-AddEpisodes with 1 item that causes extraction schema mismatch
  - Expect: retries up to N; then PARKED; OP-SearchFacts returns no facts from that item

### TS-4 Contradiction handling
- Step 1: Ingest fact F: (A —likes→ B) with valid_at=t1
- Step 2: Ingest conflicting F′: (A —dislikes→ B) at a later time
  - Expect: prior F expired_at set (system time now); invalid_at set according to rule if explicit negation; retrieval returns only current fact by default

### TS-5 Hybrid search determinism
- Prepare: corpus with both keyword and embedding signals
- Step: OP-SearchFacts with query Q, max_facts=K
  - Expect: RRF fusion ranking applied; deterministic order; if rerank with center_node_uuid set, graph distance applied with weight w

### TS-6 GetMemory deterministic composition
- Step: OP-GetMemory with messages [m1..mn]
  - Expect: Same results as OP-SearchFacts on the concatenated query per composition policy

### TS-7 Maintenance cascade delete
- Step 1: Populate group G with episodes/nodes/edges
- Step 2: OP-DeleteGroup { group_id: G }
  - Expect: success=true; subsequent reads for G return empty

### TS-8 Limits and schema strictness
- Step: Submit payload > S or items > M or unknown fields
  - Expect: status=ERROR with appropriate error_code; no state change

## 6. Phasing and Deliverables (Guidance)

- P1: Core ingestion (OP-AddEpisodes/OP-AddMessages), extraction validation, node/edge upsert, Healthcheck
- P2: Hybrid search (OP-SearchFacts) with RRF; GetEntityEdge; GetEpisodes; GetMemory
- P3: Contradictions, bitemporal filtering defaults; maintenance operations (DeleteEntityEdge, DeleteEpisode, DeleteGroup, ClearAll)
- P4: Idempotency, retries, durability behaviors; limits/validation; observability metrics and logs
- P5: Conformance test harness and scenario suite

## 7. Open Parameters (to fix during planning)
- N, W, S, M, T_ingest_visibility, w
- Optional as‑of parameters in v1 (default: deferred)
- DeleteEpisode cascade policy (default: no cascade)

