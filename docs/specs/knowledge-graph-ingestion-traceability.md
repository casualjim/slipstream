# Ingestion Pipeline v1 — Traceability Matrix

This document maps the technology‑neutral ingestion pipeline requirements to concrete stages and black‑box acceptance tests. It is protocol‑agnostic and intended for conformance planning.

- Spec reference: knowledge-graph-ingestion-pipeline.md (v1)
- Status: Draft for review

## 1. Legend

- Requirement IDs
  - PFR‑x: Pipeline Functional Requirement
  - PNFR‑x: Pipeline Non‑Functional Requirement
- Stage IDs
  - ST‑ACCEPT: Acceptance
  - ST‑EXTRACT: Extraction
  - ST‑EMBED: Embedding (optional)
  - ST‑UPSERT: Upsert
  - ST‑COMPLETE: Completion/visibility
  - ST‑FAIL‑EXTRACT / ST‑FAIL‑EMBED / ST‑FAIL‑UPSERT: Failure substates
- Cross‑cutting IDs
  - CC‑IDEMPOTENCY, CC‑RETRY, CC‑BACKPRESSURE, CC‑OBSERVABILITY, CC‑TENANCY
- Parameters to finalize
  - N (max retries), W (idempotency window), S (max payload size), M (max items per request), R (retention parked), R_t (retention transient), T_ingest_visibility (ingest→visible), T_accept_p95, stage concurrency limits

## 2. Requirements List

### Functional (PFR)
- PFR‑1 Acceptance: validate schema/limits, admit items, return ACCEPTED, queue for processing.
- PFR‑2 Dedup at acceptance via uuid/idempotency_key/content_hash within window W.
- PFR‑3 Extraction: produce normalized structured nodes/edges; deterministic mention resolution; no partial effects on failure.
- PFR‑4 Embedding (optional): compute embedding_reference/content_hash; cache; degrade to keyword‑only on absence/failure.
- PFR‑5 Upsert: deterministic node/edge upsert; referential integrity; lineage; uniqueness keys.
- PFR‑6 Contradictions: apply bitemporal invalidation/expiration rules when conflicts occur.
- PFR‑7 Completion: derived effects become visible to retrieval within T_ingest_visibility.

### Non‑Functional (PNFR)
- PNFR‑1 Determinism: replay‑safe, idempotent, stable outputs across retries/restarts.
- PNFR‑2 Durability: accepted items survive restarts and resume; exactly‑once logical effects via upserts.
- PNFR‑3 Retries: bounded backoff with jitter up to N attempts per stage; PARKED after exhaustion.
- PNFR‑4 Backpressure: documented single policy when capacity exceeded (reject or throttle).
- PNFR‑5 Limits: enforce S (payload), M (items), field caps; reject, not truncate.
- PNFR‑6 Observability: structured logs, metrics per stage and item; redact sensitive data.
- PNFR‑7 Multi‑tenancy: isolation and scoping by group_id; quotas/fairness guidance.
- PNFR‑8 Time handling: ISO 8601 UTC, ms precision at all stages.
- PNFR‑9 Versioning: single v1 behavior; breaking changes → v2.

## 3. Stages Catalog (for reference)
- ST‑ACCEPT → ST‑EXTRACT → [ST‑EMBED]? → ST‑UPSERT → ST‑COMPLETE
- Failure substates: ST‑FAIL‑EXTRACT, ST‑FAIL‑EMBED, ST‑FAIL‑UPSERT → PARKED

## 4. Traceability Matrix (Requirements → Stages → Tests)

### PFR‑1 Acceptance
- Stages: ST‑ACCEPT
- Cross‑cutting: CC‑IDEMPOTENCY, CC‑TENANCY, CC‑BACKPRESSURE, CC‑OBSERVABILITY
- Tests:
  - TM‑1.1 Valid batch within limits is admitted → status=ACCEPTED, count matches.
  - TM‑1.2 Items carry item_id and content_hash; group_id present.
  - TM‑1.3 On capacity exceed, the documented policy is applied (reject or throttle), consistently.

### PFR‑2 Dedup at acceptance
- Stages: ST‑ACCEPT
- Cross‑cutting: CC‑IDEMPOTENCY
- Tests:
  - TM‑2.1 Replay same uuid within W → no new work, no new effects.
  - TM‑2.2 Replay same idempotency_key within W → no new work/effects.
  - TM‑2.3 Identical content → same content_hash within W → no duplicates.

### PFR‑3 Extraction
- Stages: ST‑EXTRACT (and ST‑FAIL‑EXTRACT)
- Tests:
  - TM‑3.1 Valid output → deterministic nodes/edges; normalized fields; lineage links to source episode uuid.
  - TM‑3.2 Mention resolution: existing node by (group_id, exact normalized name) is preferred over creation.
  - TM‑3.3 Invalid output → retries ≤ N with backoff; then PARKED; no graph mutations.

### PFR‑4 Embedding (optional)
- Stages: ST‑EMBED (and ST‑FAIL‑EMBED)
- Tests:
  - TM‑4.1 Cache hit when same (group_id, entity_uuid, content_hash); no duplicate compute.
  - TM‑4.2 Transient failures retry ≤ N; then PARKED if persistent; item can still complete with keyword‑only for affected parts.

### PFR‑5 Upsert
- Stages: ST‑UPSERT (and ST‑FAIL‑UPSERT)
- Tests:
  - TM‑5.1 Node upsert keys enforced; uuid immutable; created_at immutable; summary/attributes merge.
  - TM‑5.2 Edge upsert requires source/target; creates/links deterministically if missing.
  - TM‑5.3 Edge uniqueness key enforced: (group_id, source_uuid, name, target_uuid, valid_at) unless uuid provided.
  - TM‑5.4 Lineage (source_episode_uuid) recorded for all mutations.

### PFR‑6 Contradictions
- Stages: ST‑UPSERT
- Tests:
  - TM‑6.1 Ingest conflicting fact F′ → superseded F gets expired_at=now (system time).
  - TM‑6.2 If F′ explicitly negates F in fact time, F.invalid_at set appropriately.
  - TM‑6.3 History preserved; tie‑break via latest created_at, else lowest uuid.

### PFR‑7 Completion/Visibility
- Stages: ST‑COMPLETE
- Tests:
  - TM‑7.1 Derived facts become retrievable within T_ingest_visibility.
  - TM‑7.2 Visibility is consistent across restarts.

### PNFR‑1 Determinism
- Stages: All
- Tests:
  - TM‑8.1 Full replay of an accepted batch yields identical graph state and ordering.
  - TM‑8.2 Reordered delivery of the same items yields identical final state.

### PNFR‑2 Durability
- Stages: ST‑ACCEPT..ST‑COMPLETE
- Tests:
  - TM‑9.1 Simulated crash mid‑pipeline; after restart, items resume processing; no duplicate effects.

### PNFR‑3 Retries
- Stages: ST‑EXTRACT, ST‑EMBED, ST‑UPSERT
- Tests:
  - TM‑10.1 Transient errors trigger bounded backoff retries; success leads to progress; exhaustion → PARKED.

### PNFR‑4 Backpressure
- Stages: ST‑ACCEPT (admission)
- Tests:
  - TM‑11.1 Under load beyond capacity, the single chosen policy is consistently applied and observable in metrics.

### PNFR‑5 Limits & validation
- Stages: ST‑ACCEPT (envelope), ST‑EXTRACT (structured output)
- Tests:
  - TM‑12.1 Payload > S → rejected at acceptance with limit error; no work enqueued.
  - TM‑12.2 Items > M → rejected; no work enqueued.
  - TM‑12.3 Field caps exceeded → rejected; no work enqueued.
  - TM‑12.4 Unknown fields → rejected (schema strict).
  - TM‑12.5 Non‑UTC datetime → rejected.

### PNFR‑6 Observability
- Stages: All
- Tests:
  - TM‑13.1 Logs include request_id, group_id, item_id, stage, status, latency_ms, error_code?; payloads redacted.
  - TM‑13.2 Metrics counters/gauges/histograms emitted per stage and reflect outcomes.

### PNFR‑7 Multi‑tenancy
- Stages: All
- Tests:
  - TM‑14.1 Cross‑tenant isolation validated: group A operations do not affect group B state or metrics attribution.

### PNFR‑8 Time handling
- Stages: All time‑bearing
- Tests:
  - TM‑15.1 All times are ISO 8601 UTC with ms; invalid times rejected early.

### PNFR‑9 Versioning
- Stages: API/pipeline surface
- Tests:
  - TM‑16.1 No alternate/legacy paths; behaviors align with v1 only.

## 5. Example Scenario Set (Black‑Box)

1) Acceptance & Dedup
- Submit batch within S and M → ACCEPTED (count matches).
- Replay by uuid, idempotency_key, and identical content → no duplicate effects; metrics reflect dedup.

2) Extraction & Parking
- Valid item → deterministic nodes/edges with lineage.
- Invalid output → retries up to N → PARKED; no graph effects; parked counter increments.

3) Embedding Cache/Degrade
- Same content twice → first computes, second hits cache.
- Persistent failure → PARKED; affected items still complete via keyword‑only.

4) Upsert & Contradictions
- Insert node/edge; re‑send with same keys → idempotent merge.
- Conflicting fact → superseded expired_at set; retrieval shows only current fact by default.

5) Durability & Restart
- Crash during extraction or upsert → after restart, processing resumes; final state correct and no duplicates.

6) Backpressure
- Saturate capacity → chosen policy triggers (reject or throttle) and is observable in logs/metrics.

7) Limits & Validation
- Payload > S / items > M / unknown fields → rejected at acceptance; no enqueued work; metrics updated.

## 6. Phasing Alignment

- P1: ST‑ACCEPT, limits, dedup, basic observability → TM‑1.*, TM‑2.*, TM‑12.*, TM‑13.*
- P2: ST‑EXTRACT, parking, determinism → TM‑3.*
- P3: ST‑EMBED (optional), caching/degrade → TM‑4.*
- P4: ST‑UPSERT, contradictions, lineage → TM‑5.*, TM‑6.*
- P5: Completion/SLOs, durability/restart, backpressure → TM‑7.*, TM‑9.*, TM‑11.*

## 7. Open Parameters to Fix

- N, W, S, M, R, R_t, T_ingest_visibility, T_accept_p95, stage concurrency limits, backpressure policy (reject vs throttle)

