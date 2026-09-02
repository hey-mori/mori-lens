# M8 Storage Integrity Audit - Summary

**Date:** 2026-09-02  
**Status:** ✅ COMPLETE

## Verdict

```
M8 storage integrity: SAFE
```

## Summary

All five critical storage integrity areas verified through code tracing and automated tests. One CORRECTNESS issue found and fixed.

## Results

| Area | Status | Tests | Notes |
|------|--------|-------|-------|
| Secure wipe | ✅ PASS | 3 | Fixed: now includes history table |
| Migration | ✅ PASS | 2 | 5 migrations, all atomic |
| JSONL round-trip | ✅ PASS | 9 | Preserves content, resets review state |
| Import failure safety | ✅ PASS | 6 | No corruption, partial import documented |
| History/Library independence | ✅ PASS | 4 | Promotion creates independent items |

## Issues Found & Fixed

### CORRECTNESS: secure_wipe missing history deletion

**Problem:** `secure_wipe()` was not deleting from the `history` table, leaving persistent history data after "secure wipe".

**Fix:** Added `DELETE FROM history` to the secure_wipe method.

**Tests:** `secure_wipe_removes_history`, `secure_wipe_survives_reopen`

## Test Coverage

**New tests added:** 15
- Secure wipe: 2 tests (history removal, reopen survival)
- Migration: 2 tests (fresh schema, idempotent reopen)
- JSONL: 9 tests (round-trip, malformed input, edge cases)
- History/Library: 4 tests (independence, promotion semantics)

**Total lens-storage tests:** 64 passing

## Key Findings

### Secure Wipe
- **Guarantee:** Best-effort physical deletion (SQLite secure_delete + VACUUM)
- **Scope:** library, tags, library_tags, history, meta
- **Does NOT reach:** filesystem snapshots, backups, exports (documented)
- **Survives:** Database reopen ✅

### Migration
- **Versions:** v1 (initial), v2 (meta), v3 (tags), v4 (review), v5 (history)
- **Atomicity:** Each migration runs in single execute_batch (implicit transaction)
- **Idempotence:** Reopen does not re-apply migrations ✅

### JSONL Round-trip
- **Preserves:** original, answer, caveat, app, created_at, kind, languages, model, note, tags
- **Resets:** review_state, next_review_at, review_count (by design)
- **Excludes:** history (separate tier per ADR-0014)
- **Handles:** Unicode, quotes, newlines, blank lines, unknown fields ✅

### Import Failure
- **Semantics:** Partial import (not atomic) - lines before error are committed
- **Duplicates:** Silently skipped (by design)
- **Safety:** No database corruption on malformed input ✅

### History/Library Independence
- **Promotion:** Creates new library item, deletes history entry
- **Review state:** Initialized to 'new' on promotion
- **Independence:** Library delete doesn't affect history, vice versa ✅

## Files Modified

1. `crates/lens-storage/src/lib.rs`
   - Fixed secure_wipe to include history deletion
   - Added 15 new tests

## Documentation

- `docs/audits/2026-09-02-m8-storage-integrity-audit.md`: Full audit report

## Next Steps

No further action required. M8 storage integrity gate is satisfied.
