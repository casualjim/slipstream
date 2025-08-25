# Work Item: Entity Extraction (JSON)

Title: Extract Entities from JSON

Summary
- Extract relevant entities from a JSON document using the provided ontology.

Functional Requirements
- FR1: Parse the JSON string as input content; extract entities represented by the JSON.
- FR2: Map extracted entities to provided entity_type_id values; no invented types.
- FR3: Exclude date/time-only properties; relationships/actions are not entities.
- FR4: Strict schema; unknown fields rejected.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent outputs for identical inputs.
- NFR2: Unicode preserved.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string?", "content": "string", "source_description": "string?" },
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
- AC1: Entity types correspond only to provided ontology IDs.
- AC2: No date/time-only fields are emitted as entities.
- AC3: Deterministic/idempotent across replays.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: extract_json
  - Keys: entity_types, source_description, episode_content (JSON), ensure_ascii, custom_prompt
  - Instructions (from file):
    - Extract relevant entities from JSON and classify using provided entity types via entity_type_id
    - Exclude properties that contain dates
- Output model: ExtractedEntities

Evaluation & Retry
- Reflexion: Not defined specifically for JSON in code; use structural validation only
  - Validate each entity_type_id exists in provided entity_types
- LLMClient retry policy: as implemented (up to 4 attempts)
- No additional invented eval prompts; one re-prompt only if the structural validation fails; else finalize

