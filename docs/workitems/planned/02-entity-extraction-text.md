# Work Item: Entity Extraction (Text)

Title: Extract Entities from Text

Summary
- Extract entities present in a body of text without assuming conversational structure.

Functional Requirements
- FR1: Extract significant entities explicitly/implicitly present in the text.
- FR2: Exclude relationships/actions and purely temporal tokens.
- FR3: If ontology provided, return entity_type_id; else null.
- FR4: Strict schema; unknown fields rejected.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical inputs.
- NFR2: Language-preserving; Unicode intact.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string?", "content": "string" },
    "entity_types": [ { "entity_type_id": 1, "name": "string", "description": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "extracted_entities": [ { "name": "string", "entity_type_id": 1 } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Only entities derived from the text are returned.
- AC2: No relationship/action placeholders or temporal-only items appear as entities.
- AC3: Deterministic/idempotent behavior on replay.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: extract_text
  - Keys: entity_types, episode_content (TEXT), ensure_ascii, custom_prompt
  - Instructions (from file):
    - Extract entities explicitly/implicitly mentioned in TEXT
    - Use provided ENTITY TYPES to classify (entity_type_id)
    - Exclude relationships/actions and temporal information
- Output model: ExtractedEntities

Evaluation & Retry
- Eval prompt: graphiti_core/prompts/extract_nodes.py: reflexion (with PREVIOUS MESSAGES empty, focus on TEXT)
- LLMClient retry policy: as implemented (up to 4 attempts on server/rate-limit)
- One guided re-prompt if reflexion indicates missed_entities; otherwise finalize
- Terminal schema/limit violations do not retry

