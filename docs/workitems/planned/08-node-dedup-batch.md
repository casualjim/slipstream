# Work Item: Node Deduplication (Batch)

Title: Deduplicate Multiple Extracted Entities Against Existing Set

Summary
- For each extracted entity, determine if it duplicates an existing one.

Functional Requirements
- FR1: One resolution per input entity with duplicate_idx or -1.
- FR2: Deterministic per-entity decisions.

Non‑Functional Requirements
- NFR1: Idempotent; strict schema.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "extracted_entities": [ { "id": 0, "name": "string", "entity_type": "string?", "duplication_candidates": [ { "idx": 0, "name": "string" } ] } ],
    "existing_entities": [ { "idx": 0, "name": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "entity_resolutions": [ { "id": 0, "duplicate_idx": -1, "name": "string" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: One resolution per input entity; deterministic outcomes.
- AC2: Idempotent replays.

