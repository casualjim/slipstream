# Work Item: Node Deduplication (Single)

Title: Determine if a New Entity Duplicates an Existing One

Summary
- Decide if a single new entity is a duplicate of any existing entities given context.

Functional Requirements
- FR1: Output duplicate_idx (first match or -1) and duplicates (all matches), plus canonical name.
- FR2: Duplicates represent the same real-world object; similar-but-distinct is not duplicate.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; multilingual preserved.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "new_entity": { "id": 0, "name": "string", "entity_type": "string?" },
    "existing_entities": [ { "idx": 0, "name": "string", "entity_type": "string?" } ],
    "entity_type_description": "string"
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "duplicate_idx": 0, "duplicates": [0], "name": "string" },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: duplicate_idx is first match or -1; duplicates lists all matches.
- AC2: Deterministic on replay.

