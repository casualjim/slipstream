# Work Item: Entity Classification (Ontology)

Title: Classify Extracted Entities Against Provided Ontology

Summary
- Assign entity_type (or null) to each candidate using the prompt and models in the reference implementation.

Functional Requirements
- FR1: One classification per input entity.
- FR2: entity_type must be one of provided ontology names or null when no match.
- FR3: Deterministic mapping for identical inputs.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; strict schema; language preserved.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: classify_nodes
  - Keys: previous_episodes, episode_content, extracted_entities, entity_types, ensure_ascii
  - Instructions (from file):
    - Each entity must have exactly one type
    - Only use the provided ENTITY TYPES; otherwise set to None
- Output model: EntityClassification { entity_classifications: [ { uuid, name, entity_type|null } ] }

Evaluation & Retry
- Structural validation: one output triple per input entity; entity_type ∈ ontology or null.
- Retry behavior: LLMClient retry for transient errors; one guided re-prompt if structural validation fails; otherwise finalize.

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
- AC1: One classification per input entity; entity_type is in ontology or null.
- AC2: Deterministic results on identical inputs.

