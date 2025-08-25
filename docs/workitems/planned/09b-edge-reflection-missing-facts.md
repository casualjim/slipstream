# Work Item: Edge Reflection (Missing Facts)

Title: Identify Potentially Missed Facts

Summary
- Given current message, prior context, entities, and extracted facts, identify missing facts candidates using the reference reflexion prompt.

Functional Requirements
- FR1: Input: previous_episodes, episode content, entities, extracted_facts.
- FR2: Output: missing_facts: [string].

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical inputs.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_edges.py: reflexion
  - Keys: previous_episodes, episode_content (CURRENT MESSAGE), nodes (EXTRACTED ENTITIES), extracted_facts, ensure_ascii
  - Instructions (from file): determine if any facts haven’t been extracted.
- Output model: MissingFacts { missing_facts: [string] }

Evaluation & Retry
- Structural validation: missing_facts is a list of non-empty strings; no duplicates; consistent with entities.
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
    "entities": [ { "id": 1, "name": "string" } ],
    "extracted_facts": [ { "relation_type": "string", "source_entity_id": 1, "target_entity_id": 2, "fact": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "missing_facts": ["string"] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Missing facts reflect plausible omissions in CURRENT MESSAGE; deterministic on replay.

