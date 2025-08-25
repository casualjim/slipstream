# Work Item: Entity Extraction (Message)

Title: Extract Entities from a Conversational Message

Summary
- Extract entities (including the speaker) explicitly or implicitly present in the current message, using prior messages only for disambiguation.

Functional Requirements
- FR1: Identify the speaker (before the colon if present) as the first entity; merge repeated mentions.
- FR2: Extract significant entities present in the current message; use previous messages only to resolve references.
- FR3: Exclude pronouns and purely temporal tokens.
- FR4: If an ontology is provided, attach entity_type_id from the supplied list; otherwise leave null.
- FR5: Strict schema; unknown fields rejected.

Non‑Functional Requirements
- NFR1: Deterministic and idempotent given identical input and idempotency_key.
- NFR2: Language-preserving (no translation); Unicode intact.
- NFR3: Input size limits enforced (reject on violation).

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": {
      "uuid": "string?",
      "content": "string",
      "reference_time": "ISO 8601 UTC",
      "source_description": "string?"
    },
    "previous_episodes": [ { "uuid": "string", "content": "string", "reference_time": "ISO" } ],
    "entity_types": [ { "entity_type_id": 1, "name": "string", "description": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": {
    "extracted_entities": [ { "name": "string", "entity_type_id": 1 } ],
    "missed_entities": ["string"]
  },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Speaker is present as first entity when a speaker marker exists.
- AC2: Only entities from the current message are returned (context only used for disambiguation).
- AC3: Pronouns and purely temporal tokens are excluded.
- AC4: Replays with the same idempotency_key produce identical output.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: extract_message
  - Uses keys: entity_types, previous_episodes (rendered via to_prompt_json), episode_content (CURRENT MESSAGE), ensure_ascii, custom_prompt
  - Key instructions (from file):
    - Always extract the speaker as the first entity node
    - Use ENTITY TYPES to classify each extracted entity (entity_type_id)
    - Exclude relationships/actions and dates/times (temporal info handled elsewhere)
    - Be explicit and unambiguous in naming entities
- Output models (from file):
  - ExtractedEntities { extracted_entities: [{ name, entity_type_id }] }
  - MissedEntities { missed_entities: [string] }

Evaluation & Retry
- Eval prompt: graphiti_core/prompts/extract_nodes.py: reflexion
  - Purpose: determine if any entities from CURRENT MESSAGE were missed
  - Output: MissedEntities
- Retry behavior (aligned to implementation):
  - Transport/server/rate-limit: LLMClient retry policy applies (graphiti_core/llm_client/client.py:_generate_response_with_retry, up to 4 attempts)
  - Content/eval retry: If reflexion.missed_entities is non-empty, perform one guided re-prompt including the reflexion feedback; otherwise finalize
  - Terminal errors: input schema violations or over-limit payloads do not retry; return ERROR

