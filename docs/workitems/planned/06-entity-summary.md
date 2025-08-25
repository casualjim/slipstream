# Work Item: Entity Summary

Title: Generate or Update Entity Summary (≤ 250 words)

Summary
- Produce a concise, factual entity summary using current and prior context.

Functional Requirements
- FR1: Summary ≤ 250 words; derived solely from provided inputs.
- FR2: No hallucination beyond inputs.
- FR3: Strict schema.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent behavior.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "episode": { "uuid": "string", "content": "string" },
    "previous_episodes": [ { "uuid": "string", "content": "string" } ],
    "entity": { "uuid": "string", "name": "string", "summary": "string?" }
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "summary": "string" },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: ≤ 250 words; consistent with inputs.
- AC2: Deterministic replay yields identical summary.

