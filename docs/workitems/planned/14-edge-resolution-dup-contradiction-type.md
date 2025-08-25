# Work Item: Edge Resolution (Duplicate + Contradiction + Type)

Title: Resolve New Fact vs Existing — Duplicates, Contradictions, and Fact Type

Summary
- Given a new fact and existing facts plus invalidation candidates, determine which facts are duplicates, which are contradicted, and whether the new fact matches a known fact_type.

Functional Requirements
- FR1: Input: new_edge, existing_edges, edge_invalidation_candidates, edge_types.
- FR2: Output: duplicate_facts (indices), contradicted_facts (indices), fact_type ∈ edge_types or DEFAULT.
- FR3: Near-identical semantics → duplicate; clear value differences → not duplicate.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent; strict schema.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/dedupe_edges.py: resolve_edge
  - Keys: new_edge, existing_edges, edge_invalidation_candidates, edge_types, ensure_ascii
  - Instructions (from file):
    - Return duplicate_facts (idx list), contradicted_facts (idx list), and fact_type or DEFAULT
    - Be robust to paraphrase but respect key differences
- Output model: { duplicate_facts: [int], contradicted_facts: [int], fact_type: string }

Evaluation & Retry
- Structural validation: indices within bounds; no duplicates; fact_type either present in edge_types or DEFAULT.
- Retry behavior: LLMClient retries for transient errors; one guided re-prompt if structural validation fails; else finalize.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "new_edge": { "uuid": "string?", "fact": "string" },
    "existing_edges": [ { "uuid": "string", "fact": "string" } ],
    "edge_invalidation_candidates": [ { "uuid": "string", "fact": "string" } ],
    "edge_types": [ { "name": "string", "source_type": "string", "target_type": "string" } ]
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
- AC1: duplicate_facts and contradicted_facts contain only valid indices; no duplicates.
- AC2: fact_type ∈ edge_types or DEFAULT; deterministic selection.

