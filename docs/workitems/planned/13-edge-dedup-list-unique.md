# Work Item: Edge Deduplication (List → Unique)

Title: Reduce a List of Facts to a Unique Set

Summary
- Given a list of facts, identify duplicates and return a unique subset using a representative UUID per duplicate cluster.

Functional Requirements
- FR1: Input: edges list with uuids and fact texts.
- FR2: Output: unique_facts list containing exactly one representative per duplicate cluster.
- FR3: Duplicates consider semantically identical/near-identical expressions as duplicates; near-similar with key differences are not duplicates.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical input ordering.

Prompt Specification (LLM)
- Source: graphiti_core/prompts/dedupe_edges.py: edge_list
  - Keys: edges, ensure_ascii
  - Instructions (from file):
    - Find all duplicates in Facts and return a new fact list with representative UUIDs.
    - Identical/near-identical facts are duplicates; select exactly one representative.
- Output model: UniqueFacts { unique_facts: [ { uuid, fact } ] }

Evaluation & Retry
- Structural validation: output uuids must exist in input; no duplicates; total count ≤ input count; stable order.
- Retry behavior: LLMClient retry for transient errors; one guided re-prompt if structural validation fails; otherwise finalize.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": { "group_id": "string", "edges": [ { "uuid": "string", "fact": "string" } ] }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "unique_facts": [ { "uuid": "string", "fact": "string" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Exactly one representative per duplicate cluster; representative UUID must be from input.
- AC2: Deterministic result for identical inputs.

