# Work Item: Edge Date Extraction

Title: Extract valid_at / invalid_at for a Fact (Edge)

Summary
- From current/previous context and a reference timestamp, extract datetimes that bound the fact (when it became true and, if applicable, when it ceased to be true), consistent with the reference implementation.

Functional Requirements
- FR1: Only set dates directly related to the relationship itself (establishment/change/termination of the fact).
- FR2: Resolve relative times using the provided reference timestamp.
- FR3: Defaults for partial dates: date-only → 00:00:00; year-only → Jan 1 00:00:00 (as guided by the prompt text).
- FR4: Leave values null when no explicit or resolvable time is stated.
- FR5: Strict schema and ISO 8601 UTC with Z suffix when present.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical inputs.
- NFR2: Language preserved; Unicode safe.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_edge_dates.py: v1
  - Keys expected: previous_episodes, current_episode, reference_timestamp, edge_fact
  - Instructions from file:
    - Focus only on dates directly related to the edge fact.
    - Resolve relative times using reference timestamp.
    - Produce valid_at/invalid_at (ISO 8601 UTC) or nulls.
- Output model from file: EdgeDates { valid_at: str|null, invalid_at: str|null }

Evaluation & Retry
- Structural/time rule validation:
  - If both valid_at and invalid_at are present, ensure valid_at <= invalid_at.
  - If provided, both must be strict ISO 8601 UTC with Z.
  - Ensure dates correspond to the fact itself, not unrelated temporal mentions.
- Retry behavior:
  - Transport/rate-limit/server errors: rely on LLMClient retries (graphiti_core/llm_client/client.py, up to 4 attempts).
  - One guided re-prompt allowed if structural/time rules are violated (embed reasons); otherwise finalize.
  - Terminal errors (schema invalid inputs) do not retry.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "previous_episodes": [ "string" ],
    "current_episode": "string",
    "reference_timestamp": "ISO 8601 UTC",
    "edge_fact": "string"
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "valid_at": "ISO|null", "invalid_at": "ISO|null" },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Dates are set only when clearly part of the fact; otherwise nulls.
- AC2: Relative time mentions are correctly resolved using reference_timestamp.
- AC3: Timestamps, when present, use strict ISO 8601 UTC with Z.

