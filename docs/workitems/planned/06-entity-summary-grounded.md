# Work Item: Entity Summary Extraction

Title: Generate/Update Entity Summary (≤ 250 words)

Summary
- Produce a concise summary for an entity using current and prior context, grounded in the reference prompt.

Functional Requirements
- FR1: Summary ≤ 250 words; based only on provided content.
- FR2: No hallucination beyond inputs; language preserved.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical inputs.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/extract_nodes.py: extract_summary
  - Keys: previous_episodes, episode_content (current), node (entity), ensure_ascii
  - Instructions (from file):
    - Update the entity summary combining relevant information from messages and existing summary; ≤ 250 words
- Output: { summary: string }

Evaluation & Retry
- Structural checks: word count ≤ 250; summary consistent with inputs.
- Retry behavior: LLMClient retry for transient errors; one guided re-prompt if constraints fail; else finalize.

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
- AC1: Summary ≤ 250 words and grounded in inputs.
- AC2: Deterministic replay produces identical summary.

