# Work Item: Entity Attribute Extraction

Title: Update Entity Attributes from Context

Summary
- Update attributes for entities based on episode + prior context without hallucination, grounded in the reference prompt implementation.

Functional Requirements
- FR1: Only attributes supported by the given context are set/updated; others left unchanged.
- FR2: Strict schema; unknown fields rejected.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent behavior on identical inputs.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: extract_attributes
  - Keys: previous_episodes, episode_content (current), node (entity), ensure_ascii
  - Instructions (from file):
    - Update entity attributes strictly based on provided messages and entity; do not hallucinate
- Output: updated attributes object (key/value pairs)

Evaluation & Retry
- Structural validation: output keys match attribute_specs where provided; values are consistent with context.
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
- AC1: Attributes only updated when directly supported by content.
- AC2: Deterministic replay.

