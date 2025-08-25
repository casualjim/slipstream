# Work Item: Node List Deduplication (Cluster & Summary)

Title: Group Duplicate Node UUIDs and Generate Group Summary

Summary
- Deduplicate a set of nodes by grouping duplicates; produce a short synthesized summary per group.

Functional Requirements
- FR1: Each input uuid appears exactly once in exactly one group.
- FR2: Provide summary per group; concise and factual.

Non‑Functional Requirements
- NFR1: Deterministic grouping; summary ≤ 250 words; idempotent replay.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": { "group_id": "string", "nodes": [ { "uuid": "string", "name": "string", "summary": "string" } ] }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "nodes": [ { "uuids": ["string"], "summary": "string" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: All uuids present exactly once across groups.
- AC2: Group summary present; ≤ 250 words; deterministic.

