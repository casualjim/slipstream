# Work Item: Embedding Generation

Title: Generate Embeddings for Entities and Facts

Summary
- Create embeddings for entity names/summaries and fact texts using the embedder interface in the reference implementation.

Functional Requirements
- FR1: Input: items to embed with type (entity|fact), uuid, and text.
- FR2: Output: embedding_reference (opaque) or vector per item; order preserved.
- FR3: Optionally support batch embedding via create_batch; fall back to per-item create.

Non‑Functional Requirements
- NFR1: Deterministic/idempotent for identical (group_id, uuid, text) with idempotency_key.
- NFR2: Privacy: embeddings are logically scoped by group_id.

Specification Reference
- Interface: graphiti_core/embedder/client.py
  - EmbedderClient.create(input_data) -> list[float]
  - EmbedderClient.create_batch(input_data_list) -> list[list[float]] (optional)
- Call sites include EntityNode.generate_name_embedding, EntityEdge.generate_embedding, create_entity_node_embeddings, create_entity_edge_embeddings.

Evaluation & Retry
- Structural checks: one result per input item; ordering stable.
- Retry behavior: Adopt idempotency keys for (group_id, uuid, content hash); retry once on transient errors; otherwise return ERROR.

Input Schema
```json
{
  "request_id": "string?",
  "idempotency_key": "string?",
  "input": {
    "group_id": "string",
    "items": [ { "type": "entity|fact", "uuid": "string", "text": "string" } ]
  }
}
```

Output Schema
```json
{
  "request_id": "string",
  "status": "OK" | "ERROR",
  "output": { "embeddings": [ { "uuid": "string", "embedding_reference": "string" } ] },
  "error": { "error_code": "string", "message": "string", "details": {} }
}
```

Acceptance Criteria
- AC1: Exactly one embedding result per input item; order preserved.
- AC2: Idempotent replay yields identical embedding_reference per (group_id, uuid, text).

