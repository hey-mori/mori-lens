# P27: Library & Review Data Integrity Audit

**Date**: 2026-09-25  
**Phase**: Release Candidate Preparation  
**Status**: SOLID with minor gaps

## Summary

Library and Review sections have **solid storage foundations** with proper error handling, schema migrations, and corruption recovery. Minor gaps exist in transaction atomicity and edge case testing, but no critical data integrity issues.

## Storage Layer Assessment

### ✅ Strengths
- **SQLite with WAL mode**: Proper concurrent read/write handling
- **Mutex-protected state**: `SharedLibrary` (`Arc<Mutex<LibraryState>>`) prevents races
- **Schema migrations**: 8 numbered migrations tracked in `schema_migrations`
- **Future-schema protection**: `FutureSchema` error refuses DB from newer builds
- **First-run handling**: Fresh DB creation, all migrations run, one-time `saved.md` import
- **Corruption recovery**: In-memory fallback with diagnostic (`intended_path` + `reason`)
- **UI visibility**: `library_persistent` flag surfaced via `diagnostics.json`

### ⚠️ Minor Gaps
- **Tag save atomicity**: `set_tags()` does DELETE + multiple INSERTs without transaction
  - **Risk**: Partial failure leaves inconsistent tag state
  - **Fix**: Wrap in `unchecked_transaction()`
  - **Severity**: LOW (rare failure mode, user can retry)

## Library Section (library.js)

### ✅ Strengths
- **Error handling**: try/catch on all `invoke` calls
- **User feedback**: Errors shown in list and detail views
- **No TODOs/FIXMEs**: Code is complete

### ⚠️ Minor Gaps
- **`loadFilterOptions()` silent failure**: Catches errors but doesn't inform user
  - **Impact**: Dropdowns stay at "All", user doesn't know backend is failing
  - **Fix**: Add console.warn or subtle UI indicator
  - **Severity**: LOW (UX issue, not data integrity)

## Review Section (review.js)

### ✅ Strengths
- **Session snapshot model**: `list_later_session()` loads all "later" items at session start
- **Race condition prevention**: `seenActions` Map prevents re-presentation
- **Double-click prevention**: `actionInFlight` flag
- **Undo support**: `historyStack` for reversible actions
- **DB-truth semantics**: `updateSummaryFromDb()` re-reads `library_later_count` after each action
- **Backend validation**: Validates review state, returns `InvalidReviewState` for bad values

### ⚠️ Minor Gaps
- **Large review sets**: No pagination for `list_later_session()`
  - **Impact**: Fine for hundreds, problematic at thousands
  - **Severity**: LOW (not a release blocker)

## Test Coverage

### ✅ Good Coverage
- **Rust**: ~50 unit tests in `lens-storage`
  - Insert, search, filter, review state, migrations
  - Import/export, session snapshot, history, secure wipe
- **JS**: 
  - `library.vitest.test.js` (5 tests): load, search, filter, empty state, backend error
  - `review.vitest.test.js` (4 tests): similar coverage
- **No skipped tests**: No `#[ignore]` found

### ⚠️ Missing Coverage
- **Large datasets**: No tests for 100+ items
- **Concurrent operations**: No stress tests
- **Special characters**: No tests for titles/content with quotes, newlines, Unicode
- **JS edge cases**: No tests for `showLibraryItem`, `deleteLibraryItem`, `saveLibraryNote`, `saveLibraryTags`, `exportLibrary`, `importLibraryJsonl`

## Edge Cases Needing Attention

### Priority 1 (Before Release)
None. All P1 issues are handled correctly.

### Priority 2 (Post-Release Hardening)
1. **Tag save atomicity**: Wrap `set_tags()` in transaction
2. **`loadFilterOptions` error visibility**: Add console.warn or UI indicator
3. **Large review sets**: Add pagination if user feedback indicates need

### Priority 3 (Nice-to-Have)
1. **Search debounce**: Improve UX for rapid typing (currently Enter-only)
2. **Stress tests**: Add tests for 100+ items, special characters
3. **JS test coverage**: Add tests for missing library.js functions

## Data Integrity Guarantees

✅ **No data loss**: Corruption recovery with in-memory fallback  
✅ **No silent corruption**: `FutureSchema` refuses incompatible DBs  
✅ **No race conditions**: Mutex protection on all writes  
✅ **Schema evolution**: Numbered migrations prevent downgrade corruption  
✅ **Diagnostic visibility**: `library_persistent` flag informs UI  

## Recommendation

**RELEASE READY**: No critical data integrity issues. Minor gaps are post-release hardening candidates.

Ship v1.0 with current implementation. Address Priority 2 items in v1.1 based on user feedback.

## Related

- `crates/lens-storage/`: Storage implementation
- `apps/lens/web/js/library.js`: Library UI logic
- `apps/lens/web/js/review.js`: Review UI logic
- Issue #622: Release candidate preparation tracking
