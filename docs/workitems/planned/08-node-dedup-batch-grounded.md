# Work Item: Node Deduplication (Batch)

Title: Deduplicate Multiple Extracted Entities Against Existing Set

Summary
- For a list of extracted entities, determine duplicates against an existing set using the reference batch prompt.

Functional Requirements
- FR1: One resolution per input entity with duplicate_idx or -1.
- FR2: Deterministic per-entity decisions; no cross-entity leakage.

Non‑Functional Requirements
- NFR1: Idempotent; strict schema.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/dedupe_nodes.py: nodes
  - Keys: previous_episodes, episode_content, extracted_nodes (with duplication_candidates), existing_nodes, ensure_ascii
  - Instructions (from file): For each entity, determine duplicate of any existing entities; return entity_resolutions list.
- Output model: NodeResolutions { entity_resolutions: [ { id, duplicate_idx, name } ] }

Evaluation & Retry
- Structural validation: one result per input entity; ids mapped correctly; duplicate_idx either -1 or valid; deterministic order.
- Retry behavior: LLMClient retry for transient errors; one guided re-prompt on structural failure; otherwise finalize.

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

