# Work Item: Edge/Fact Extraction

Title: Extract Edges (Facts) Between Entities

Summary
- Extract factual relationships between provided entities using the current message and prior context for disambiguation only.

Functional Requirements
- FR1: Only edges where both endpoints are among provided entities; endpoints must be distinct.
- FR2: relation_type is SCREAMING_SNAKE_CASE; include fact text; optional valid_at/invalid_at.
- FR3: Use reference_time to resolve relative time expressions.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; strict schema.
- NFR2: ISO 8601 UTC timestamps.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string", "reference_time": "ISO" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "entities": [ { "id": 1, "name": "string" } ],
    "edge_types": [ { "name": "FOUNDED", "source_type": "string", "target_type": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "edges": [ { "relation_type": "string", "source_entity_id": 1, "target_entity_id": 2, "fact": "string", "valid_at": "ISO?", "invalid_at": "ISO?" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: All edges reference two distinct entities from the input set.
- AC2: relation_type adheres to SCREAMING_SNAKE_CASE.
- AC3: valid_at/invalid_at only when derivable; ISO format.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_edges.py: edge
  - Keys: edge_types (FACT TYPES), previous_episodes, episode_content (CURRENT MESSAGE), nodes (ENTITIES), reference_time, ensure_ascii, custom_prompt
  - Instructions (from file):
    - Extract facts between distinct entities from ENTITIES set only
    - Use SCREAMING_SNAKE_CASE for relation_type
    - Resolve relative times using REFERENCE_TIME; ISO 8601 with Z
    - Only facts clearly stated or unambiguously implied in CURRENT MESSAGE
- Output models: Edge, ExtractedEdges

Evaluation & Retry
- Eval prompt: graphiti_core/prompts/extract_edges.py: reflexion
  - Purpose: determine if any facts were missed
- LLMClient retry policy: as implemented (up to 4 attempts on server/rate-limit)
- One guided re-prompt if reflexion indicates missing facts or structural rule violations; otherwise finalize
- Terminal schema/limit violations do not retry

