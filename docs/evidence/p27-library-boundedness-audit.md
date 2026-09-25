# P27 Library Boundedness Audit

**Date**: 2026-09-25
**Status**: SOURCE VERIFIED
**Scope**: Library query pagination, Review session loading, import/export bounds

## Executive Summary

All Library query APIs implement **offset-based pagination** with `PAGE_SIZE = 50`. Review session loading is intentionally unbounded for session coherence. Import/export use paginated reads but may accumulate full results in memory.

**No release-blocking defects found.** Key tradeoffs are documented.

## Library Query APIs - All Paginated

### 1. `list(offset)` - Main Library View

**Location**: `crates/lens-storage/src/lib.rs:300`

```rust
pub fn list(&self, offset: i64) -> Result<Page, Error> {
    let limit = PAGE_SIZE + 1;
    let items: Vec<Item> = self
        .conn
        .prepare(LIST_QUERY)?
        .query_map(params![limit, offset], row_to_item)?
        .collect::<Result<Vec<_>, _>>()?;

    let has_more = items.len() as i64 > PAGE_SIZE;
    // ...
}
```

**Proof**:
- Fetches `PAGE_SIZE + 1` items to detect `has_more`
- Returns `Page { items, total, has_more }`
- Client uses `has_more` to decide whether to fetch next page
- **Status**: ✅ PAGINATED

### 2. `search(query, offset)` - Search Results

**Location**: `crates/lens-storage/src/lib.rs:474`

```rust
pub fn search(&self, query: &str, offset: i64) -> Result<Page, Error> {
    if query.trim().is_empty() {
        return self.list(offset);
    }
    let limit = PAGE_SIZE + 1;
    // ...
    let has_more = items.len() as i64 > PAGE_SIZE;
    // ...
}
```

**Proof**:
- Same pagination pattern as `list()`
- Empty query delegates to `list(offset)`
- **Status**: ✅ PAGINATED

### 3. `list_filtered(filter, offset)` - Filtered View

**Location**: `crates/lens-storage/src/lib.rs:507`

```rust
pub fn list_filtered(&self, filter: &Filter, offset: i64) -> Result<Page, Error> {
    // ...
    let limit = PAGE_SIZE + 1;
    // ...
    let has_more = items.len() as i64 > PAGE_SIZE;
    // ...
}
```

**Proof**:
- Same pagination pattern
- Dynamic WHERE clause construction
- **Status**: ✅ PAGINATED

### 4. `list_later(offset)` - Review Later Queue

**Location**: `crates/lens-storage/src/lib.rs:1014`

```rust
pub fn list_later(&self, offset: i64) -> Result<Page, Error> {
    let limit = PAGE_SIZE + 1;
    let items: Vec<Item> = self
        .conn
        .prepare(LIST_LATER_QUERY)?
        .query_map(params![REVIEW_LATER, limit, offset], row_to_item)?
        .collect::<Result<Vec<_>, _>>()?;

    let has_more = items.len() as i64 > PAGE_SIZE;
    // ...
}
```

**Proof**:
- Same pagination pattern
- Filters by `review_state = 'later'`
- **Status**: ✅ PAGINATED

### 5. `history_list(offset)` - Persistent History

**Location**: `crates/lens-storage/src/lib.rs:1326`

```rust
pub fn history_list(&self, offset: i64) -> Result<HistoryPage, Error> {
    let limit = PAGE_SIZE + 1;
    // ...
    let has_more = entries.len() as i64 > PAGE_SIZE;
    // ...
}
```

**Proof**:
- Same pagination pattern
- Returns `HistoryPage` instead of `Page`
- **Status**: ✅ PAGINATED

## Review Session Loading - Intentionally Unbounded

### `list_later_session()` - Full Session Coherence

**Location**: `crates/lens-storage/src/lib.rs:1050`

```rust
/// advances through them without relying on offset pagination, which would
/// be wrong here: a review session is a single coherent action, and a page
/// boundary in the middle of it would be a worse bug than loading a few
/// hundred rows.
pub fn list_later_session(&self) -> Result<Vec<Item>, Error> {
    // ...
}
```

**Rationale** (from code comment):
- Review session is a **single coherent action**
- Page boundary in middle of session would break user flow
- Design choice: coherence over memory bounds

**Risk Assessment**:
- **UNBOUNDED BY ITEM COUNT**: Loads all items marked "Later" into memory
- No hard cap exists
- Typical usage: tens to low hundreds of items
- Worst case: user has 10,000+ items in Later queue → all loaded into memory
- Memory impact: scales linearly with Later queue size

**Status**: ⚠️ INTENTIONALLY UNBOUNDED (v0.1.0 tradeoff)

**Mitigation for v0.1.0**:
- Acceptable for typical usage patterns
- If Later queue grows beyond ~1000 items, memory may become noticeable
- Future work: consider batched review sessions or pagination with session coherence preserved

## Import/Export - Paged Reads, Unbounded Accumulation

### Export

**Location**: `crates/lens-storage/src/lib.rs:775-787`

```rust
pub fn export_json(&self) -> Result<String, Error> {
    let mut offset = 0i64;
    let mut all = Vec::new();
    loop {
        let page = self.list(offset)?;
        all.extend(page.items);
        if !page.has_more {
            break;
        }
        offset += PAGE_SIZE;
    }
    // ...
}
```

**Behavior**:
- **DB retrieval is paged**: Uses pagination internally (PAGE_SIZE=50)
- **Overall operation memory scales with total items**: Accumulates all items into `all` Vec
- Final JSON string also scales with total library size

**Status**: ⚠️ DB reads are paged, but memory usage scales with total library size

### Import

**Location**: `crates/lens-storage/src/lib.rs:832-849`

```rust
pub fn import_json(&self, json: &str) -> Result<usize, Error> {
    // ...
    let mut offset = 0i64;
    loop {
        let page = self.list(offset)?;
        // ...
        if !page.has_more {
            break;
        }
        offset += PAGE_SIZE;
    }
    // ...
}
```

**Behavior**:
- **Input parsing**: Accepts complete JSON payload (caller provides full string)
- **DB reads are paged**: Iterates through existing items for deduplication using pagination
- **Memory**: Input JSON string held in memory, plus accumulated items during deduplication

**Status**: ⚠️ Input size unbounded, DB reads paged for deduplication

### Secure Wipe

**Location**: `crates/lens-storage/src/lib.rs:864-880`

```rust
pub fn secure_wipe(&self) -> Result<(), Error> {
    // ...
    let mut offset = 0i64;
    loop {
        let page = self.list(offset)?;
        // ...
        if !page.has_more {
            break;
        }
        offset += PAGE_SIZE;
    }
    // ...
}
```

**Proof**:
- Uses pagination
- **Status**: ✅ BOUNDED

## Test Coverage

### Pagination Tests

1. `listing_returns_newest_first` (line 1850)
2. `listing_pages_past_the_first_batch` (line 1871)
3. `page_total_reflects_unfiltered_count` (line 2068)
4. `page_total_reflects_search_result_count` (line 2084)
5. `page_total_reflects_filtered_count` (line 2108)
6. `page_total_with_combined_filters` (line 2136)
7. `page_total_with_pagination` (line 2167)
8. `page_total_zero_for_empty_results` (line 2197)
9. `list_later_returns_only_later_items` (line 4229)
10. `list_later_session_returns_all_items` (line 4591)
11. `list_later_session_exceeds_page_size` (line 4613)

**Status**: ✅ COMPREHENSIVE COVERAGE

## Performance Characteristics

### Query Scaling

- **Offset pagination**: SQLite `LIMIT ... OFFSET ...` is used
- **Not O(1) per page**: Offset pagination requires scanning OFFSET rows before returning LIMIT rows. Performance degrades linearly with offset value.
- **Acceptable for v0.1.0**: Typical library sizes (<10k items) keep offsets small
- **Future concern**: Very large libraries (>100k items) with high offsets may show latency

### Frontend Rendering

**Current implementation**: Incremental paged append
- "Load more" button appends next page to existing rows
- No viewport virtualization implemented
- All loaded rows remain in DOM

**Status**: ⚠️ Not virtualized (acceptable for v0.1.0 typical usage)

### Memory Usage

**Not measured with controlled fixtures.** Qualitative behavior:
- Per page: 50 items loaded at a time
- Accumulated pages: each "Load more" adds 50 items to DOM
- Review session: all Later items loaded (unbounded)
- Export: all items accumulated in memory before serialization

**Status**: UNKNOWN exact memory footprint (no benchmarks)

### Query Performance

**Indexed columns**: `created_at`, `kind`, `review_state`
- Pagination: requires scanning OFFSET rows (not constant time)
- Search: FTS if enabled (O(log n)), otherwise LIKE (O(n))
- Filters: dynamic WHERE clause construction

**Status**: Source-verified query patterns, no performance benchmarks

## Regression Guards

### Deterministic Tests

- Pagination correctness: ✅ tested
- has_more accuracy: ✅ tested
- total count accuracy: ✅ tested
- Filter + pagination interaction: ✅ tested

### Informational / Manual

- Large library performance: monitor in production
- Review session size: monitor via diagnostics
- **Status**: ✅ ADEQUATE FOR v0.1.0

## Conclusion

Library boundedness for v0.1.0:

**Strong bounds**:
- All query APIs paginated with PAGE_SIZE = 50
- Deterministic pagination tests (11 tests)

**Known unbounded paths** (intentional v0.1.0 tradeoffs):
- `list_later_session()`: loads all Later items for session coherence
- Export: accumulates all items in memory
- Import: accepts unbounded input JSON

**Not virtualized**:
- Frontend uses incremental paged append (not viewport virtualization)
- All loaded rows remain in DOM

**Performance**:
- Offset pagination is not O(1) (degrades with offset)
- No performance benchmarks exist
- Acceptable for typical v0.1.0 library sizes

**Recommendation**: Ship as-is for v0.1.0. Monitor large-library and large-Later-queue usage in production.

## Related

- `crates/lens-storage/src/lib.rs`
- `docs/evidence/p27-library-review-data-integrity-audit.md`
- `apps/lens/web/js/library.js` (client-side pagination)
