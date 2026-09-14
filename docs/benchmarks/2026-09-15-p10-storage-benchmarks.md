# P10 Storage Benchmarks: Comprehensive Query Performance

**Date:** 2026-09-15  
**Commit:** 9ed439d (feat: comprehensive P10 storage benchmarks across 9 query patterns)  
**Purpose:** Measure storage query performance across all operation types at different scales to validate that FTS5 is not needed at current scale.

## Test Environment

- **Platform:** Windows 11
- **Build:** Release mode (`cargo test --release`)
- **Database:** SQLite with WAL mode
- **Schema:** Latest (includes tags, library_tags, normalized structure)

## Methodology

Benchmarks measure 9 distinct query patterns at 3 scale points (100, 1k, 10k items). Each measurement includes the full query execution with `Page.total` COUNT cost. All queries use the same dataset with realistic distribution of:
- 4 apps (Chrome, VSCode, Notepad, Slack)
- 4 tags (grammar, vocabulary, idiom, technical)
- Notes on 33% of items
- Tags on 50% of items

## Results

### 100 items

| Operation | Time (µs) | Results |
|-----------|-----------|---------|
| Plain list | 1,306 | 100 |
| Text query | 256 | 1 |
| Note query | 139 | 34 |
| Tag filter | 130 | 13 |
| App filter | 81 | 25 |
| Query + app | 138 | 25 |
| Query + tag | 188 | 13 |
| Query + tag + app | 132 | 13 |
| Multiple filters | 82 | 25 |

### 1,000 items

| Operation | Time (µs) | Results |
|-----------|-----------|---------|
| Plain list | 1,750 | 1,000 |
| Text query | 637 | 11 |
| Note query | 292 | 334 |
| Tag filter | 487 | 125 |
| App filter | 166 | 250 |
| Query + app | 311 | 250 |
| Query + tag | 633 | 125 |
| Query + tag + app | 364 | 125 |
| Multiple filters | 192 | 250 |

### 10,000 items

| Operation | Time (µs) | Results |
|-----------|-----------|---------|
| Plain list | 4,853 | 10,000 |
| Text query | 3,943 | 111 |
| Note query | 1,413 | 3,334 |
| Tag filter | 2,618 | 1,250 |
| App filter | 433 | 2,500 |
| Query + app | 592 | 2,500 |
| Query + tag | 3,353 | 1,250 |
| Query + tag + app | 1,462 | 1,250 |
| Multiple filters | 839 | 2,500 |

## Analysis

### Performance Characteristics

1. **All operations complete in under 5ms at 10k scale**
   - Worst case: plain list at 4.85ms
   - Typical case: 1-3ms for filtered queries
   - Best case: 82µs for simple filters at 100 items

2. **Text queries scale linearly**
   - 100 items: 256µs
   - 1k items: 637µs (2.5x slower)
   - 10k items: 3,943µs (6.2x slower)
   - This is expected for LIKE-based search without FTS5

3. **Tag filters are efficient**
   - Tag-only filter: 2.6ms at 10k
   - Combined query + tag: 3.4ms at 10k
   - EXISTS subquery avoids duplicate rows and performs well

4. **App filters are very fast**
   - Simple app filter: 433µs at 10k
   - Combined with query: 592µs at 10k
   - Indexed column lookup

5. **Composed filters remain performant**
   - Query + tag + app: 1.46ms at 10k
   - Multiple structured filters: 839µs at 10k
   - AND semantics with multiple conditions works well

### FTS5 Decision

**Conclusion: FTS5 is NOT justified at current scale.**

Rationale:
- All operations complete in under 5ms at 10k items
- Text search (LIKE-based) is the slowest operation but still acceptable at 3.9ms
- FTS5 would add complexity (schema migration, index maintenance, query syntax changes)
- Current performance is well within acceptable bounds for interactive use
- FTS5 would be reconsidered if:
  - Dataset grows beyond 100k items
  - Text query latency exceeds 20ms
  - User feedback indicates search is too slow

## Code

Benchmark implementation: `crates/lens-storage/src/bench.rs`

Run with:
```bash
cargo test --release --package lens-storage bench:: -- --nocapture
```

## Verification

All benchmarks include correctness assertions:
- Result counts match expected values
- Pagination works correctly
- Filters compose properly
- Page.total is consistent with actual result count

## Next Steps

1. Monitor performance as dataset grows in production use
2. If FTS5 becomes necessary, implement with:
   - Virtual table for full-text search
   - Separate index for original/answer/note fields
   - Query syntax that combines FTS5 with structured filters
   - Migration strategy for existing databases
3. Consider caching frequently-run queries if patterns emerge
