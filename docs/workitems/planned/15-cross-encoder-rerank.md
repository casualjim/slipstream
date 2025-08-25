# Work Item: Cross-Encoder Reranking

Title: Rank Passages by Relevance to a Query (Cross-Encoder)

Summary
- Given a query and a set of passages, return passages ranked by relevance using the cross-encoder interface defined in the reference implementation.

Functional Requirements
- FR1: Input: query string and passages [].
- FR2: Output: ranked list of (passage, score) sorted descending by score.
- FR3: Stable tie-breaking for equal scores (lexicographic by passage).

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical inputs.
- NFR2: Strict schema; no provider specifics.

Specification Reference
- Interface: graphiti_core/cross_encoder/client.py: CrossEncoderClient.rank(query, passages) -> list[(passage, score)]
- Implementations exist but are not part of this work item; only contract matters.

Evaluation & Retry
- Structural checks: ranked length equals input passages length (if scoring all), or documented subset; scores are numeric; order is consistent.
- Retry behavior: If ranking call is delegated to a side-effectful operation, adopt the same idempotency key as (query, passages hash) and allow one retry on transient errors; otherwise return ERROR.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": { "query": "string", "passages": ["string"] }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "ranked": [ ["string", 0.0] ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Sorted by score desc; ties by lexicographic passage.
- AC2: Deterministic results for identical inputs.

