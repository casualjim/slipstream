# Work Item: Entity Classification

Title: Classify Extracted Entities Against Ontology

Summary
- Assign an entity_type (or null) to each candidate using a provided ontology.

Functional Requirements
- FR1: One classification per input entity.
- FR2: entity_type ∈ provided ontology names; else null.
- FR3: Deterministic mapping given same inputs.

Non‑Functional Requirements
- NFR1: Idempotent with idempotency_key.
- NFR2: Strict schema; multilingual preserved.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "entities": [ { "uuid": "string?", "name": "string" } ],
    "entity_types": [ { "name": "string", "description": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "entity_classifications": [ { "uuid": "string?", "name": "string", "entity_type": "string|null" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Every input entity has a classification record.
- AC2: entity_type is either in ontology or null.
- AC3: Deterministic results on replay.

