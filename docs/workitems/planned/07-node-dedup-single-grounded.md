# Work Item: Node Deduplication (Single)

Title: Determine if a New Entity Duplicates an Existing One

Summary
- Decide if a single new entity is a duplicate of any existing entities given context, grounded in the reference prompt.

Functional Requirements
- FR1: Output duplicate_idx (first match or -1) and duplicates (all matches), plus canonical name.
- FR2: Duplicates represent the same real-world object; similar-but-distinct is not duplicate.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; multilingual preserved.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/dedupe_nodes.py: node
  - Keys: previous_episodes, episode_content, extracted_node (NEW ENTITY), existing_nodes, entity_type_description, ensure_ascii
  - Instructions (from file): compare new entity to existing; return duplicate_idx and duplicates; choose explicit name.
- Output model: NodeResolutions-like triple for the single entity { id, duplicate_idx, name, duplicates }

Evaluation & Retry
- Structural checks: duplicate_idx either -1 or valid candidate idx; duplicates list consistent and deduped.
- Retry behavior: LLMClient retry for transient errors; one guided re-prompt if structural validation fails; else finalize.

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

