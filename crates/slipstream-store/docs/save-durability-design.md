# Save Operation & Durability Design

## Overview

slipstream memory implements a dual-database architecture with lazy version reconciliation to ensure data consistency without coordination overhead. This design enables high availability and performance while maintaining eventual consistency.

## Architecture

### Dual Database System

1. **LanceDB (MetaDB)**: Source of truth
   - Stores complete data including embeddings
   - Maintains full history
   - Provides vector search capabilities

2. **KuzuDB (GraphDB)**: Index for graph operations
   - Stores only indexed fields needed for graph queries
   - Enables fast graph traversal
   - Contains version information for reconciliation

### Version Tracking

Each record includes a `version` field containing a UUID v7, which provides:

- **Natural time ordering**: UUIDs increase monotonically with time
- **Uniqueness**: No collision risk
- **Cache friendliness**: Recent versions cluster together
- **String comparison**: Simple lexicographic comparison works

## Save Operation Flow

```rust
// SaveEpisode command implementation
async fn execute(&self, engine: &Engine) -> Result<()> {
    // 1. Save to LanceDB first (source of truth)
    engine.execute_meta(self).await?;

    // 2. Then save to GraphDB with version from the episode
    let save_with_version = SaveEpisodeWithVersion {
        episode: self.0.clone(),
    };
    engine.execute_graph(&save_with_version).await?;

    Ok(())
}
```

### Key Design Decisions

1. **LanceDB First**: Since LanceDB is the source of truth, we save there first
2. **Version in Episode**: The version is generated when creating the episode (UUID v7)
3. **GraphDB Second**: If GraphDB save fails, data is still safe in LanceDB
4. **No Rollback**: If GraphDB fails, the inconsistency is resolved on next read

## Read Operation & Reconciliation

```rust
// GetEpisodeByUuid command implementation
async fn execute(&self, engine: &Engine) -> Result<Episode> {
    // 1. Get version from GraphDB
    let graph_index = engine.execute_graph(self).await?;

    // 2. Get full data from LanceDB
    let episode = engine.execute_meta(self).await?;

    // 3. Check for version mismatch
    if graph_index.kumos_version != episode.version {
        // Reindex in GraphDB with current version
        let save_with_version = SaveEpisodeWithVersion {
            episode: episode.clone(),
        };
        engine.execute_graph(&save_with_version).await?;
    }

    Ok(episode)
}
```

### Reconciliation Benefits

1. **Lazy**: Only pays cost when data is accessed
2. **Self-healing**: Inconsistencies fix themselves automatically
3. **No coordination**: No distributed locks or transactions needed
4. **Performance**: Most reads hit consistent data, reconciliation is rare

## Failure Scenarios

### Scenario 1: GraphDB Save Fails

```
1. Episode saved to LanceDB ✓
2. GraphDB save fails ✗
3. Next read detects missing/stale entry
4. Automatically reindexes from LanceDB
```

### Scenario 2: Process Crash Between Saves

```
1. Episode saved to LanceDB ✓
2. Process crashes before GraphDB save
3. GraphDB has stale/missing data
4. Next read detects and fixes
```

### Scenario 3: Concurrent Updates

```
1. Process A saves version 1
2. Process B saves version 2
3. GraphDB might have version 1 or 2
4. Next read will ensure latest version from LanceDB
```

## Performance Considerations

1. **Version Comparison**: UUID string comparison is fast
2. **Reindex Cost**: Only paid on version mismatch
3. **Cache Locality**: UUID v7 ensures recent versions are close in memory
4. **No Extra Queries**: Version check happens during normal read path

## Future Enhancements

1. **Background Reconciliation**: Periodic sweep to fix inconsistencies
2. **Metrics**: Track reconciliation frequency
3. **Batch Operations**: Optimize for bulk updates
4. **Version History**: Leverage LanceDB's versioning for time travel queries

## Example Test

```rust
#[tokio::test]
async fn test_version_reconciliation() {
    let engine = Engine::new_test().await;

    // Save episode normally
    let episode = Episode {
        uuid: test_uuid,
        version: Uuid::now_v7(),
        // ... other fields
    };
    engine.execute(&SaveEpisode(episode)).await?;

    // Simulate external update (only to LanceDB)
    let updated = Episode {
        version: Uuid::now_v7(), // New version
        content: "Updated content",
        // ... same uuid, other fields
    };
    engine.execute_meta(&SaveEpisode(updated)).await?;

    // Read triggers reconciliation
    let fetched = engine.execute(&GetEpisodeByUuid(test_uuid)).await?;

    // Verify we got updated content and GraphDB was reindexed
    assert_eq!(fetched.content, "Updated content");
    assert_eq!(fetched.version, updated.version);
}
```

## Conclusion

This design provides a robust, performant solution for maintaining consistency across dual databases without the complexity of distributed transactions. The lazy reconciliation approach ensures the system remains available and performant while providing eventual consistency guarantees.
