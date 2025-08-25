# Work Item: Entity Attribute Extraction

Title: Update Entity Attributes from Context

Summary
- Determine attribute values for an entity using current and prior context without hallucination.

Functional Requirements
- FR1: Only attributes supported by provided context are set/updated.
- FR2: Unchanged/unsupported attributes are not modified.
- FR3: Strict schema; unknown fields rejected.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent on replay.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "entity": { "uuid": "string", "name": "string", "attributes": { } },
    "attribute_specs": [ { "name": "string", "description": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "updated_attributes": { "k": "v" } },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Attributes are only changed when directly supported by content.
- AC2: Deterministic/idempotent behavior verified via replay.

