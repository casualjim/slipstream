# Work Item: Edge Deduplication (New vs Related)

Title: Detect Duplicates Between New Facts and Related Facts

Summary
- Decide if a new extracted fact duplicates any in a provided related set, matching the reference prompts.

Functional Requirements
- FR1: Input includes related_edges (candidates) and extracted_edges (new).
- FR2: Output duplicate_facts: list of indices of related_edges that are duplicates; optionally contradicted_facts and fact_type where applicable to this prompt.
- FR3: Treat paraphrases that express the same information as duplicates; key differences (e.g., numeric values) are not duplicates.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; strict schema.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/dedupe_edges.py: edge
  - Keys: related_edges, extracted_edges, ensure_ascii
  - Instructions (from file):
    - Determine whether the NEW EDGE represents any edges in EXISTING EDGES (related_edges).
    - Return duplicate_facts as the ids/indices of duplicates; be robust to paraphrase but respect key differences.
- Output model from file: EdgeDuplicate { duplicate_facts: [int], contradicted_facts: [int], fact_type: string }

Evaluation & Retry
- Structural validation: ensure duplicate_facts contains indices within related_edges bounds; no duplicates in the list; stable order.
- Retry behavior:
  - Transport/rate-limit/server: LLMClient retry (up to 4 attempts).
  - One guided re-prompt if structural validation fails; else finalize.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "related_edges": [ { "uuid": "string", "fact": "string" } ],
    "extracted_edges": [ { "id": 0, "fact": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "duplicate_facts": [0], "contradicted_facts": [0], "fact_type": "string" },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: duplicate_facts indices are valid, deduplicated, and deterministic for identical inputs.
- AC2: Key differences in facts prevent duplicate marking.

