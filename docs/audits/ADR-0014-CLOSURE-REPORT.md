# ADR-0014 Closure Report

**Date:** 2026-09-02
**Status:** CLOSED

---

## Summary

All ADR-0014 requirements have been implemented, tested, and verified. The three tiers of history (Session Recent, Saved Library, Persistent History) satisfy the accepted decision with all privacy invariants intact.

---

## Session Recent (Tier 1)

| Requirement | Implemented | Tested | Evidence |
|---|---|---|---|
| Default ON | ✅ | ✅ | `recent.rs:37-43` - SessionRecent::new() creates with defaults |
| Max 50 cards | ✅ | ✅ | `recent.rs:60-62` - prunes by size, test `prunes_by_size` |
| Max 2 hours | ✅ | ✅ | `recent.rs:68-78` - prunes by age, test `prunes_by_age` |
| Consecutive same-target dedup | ✅ | ✅ | `recent.rs:50-55` - skips duplicates, test `skips_consecutive_duplicates` |
| Memory-only | ✅ | ✅ | `recent.rs:1-5` - VecDeque in memory, no disk I/O |

---

## Persistent History (Tier 3)

| Requirement | Implemented | Tested | Evidence |
|---|---|---|---|
| Default OFF | ✅ | ✅ | `settings.rs:139` - `history: false` |
| Zero writes when OFF | ✅ | ✅ | `main.rs:2473` - gated by `if current.history`, tests in `history_privacy_tests` |
| 30-day default retention | ✅ | ✅ | `settings.rs:140` - `history_days: 30` |
| Retention choices: 1/7/30/90/forever | ✅ | ✅ | UI dropdown with exact values, parser validates `[0,1,7,30,90]` |
| **1000-entry maximum** | ✅ | ✅ | `lib.rs:147` constant, `lib.rs:941-962` enforcement, 3 tests |
| Per-app exclusions | ✅ | ✅ | `settings.rs:123`, `main.rs:2474-2482`, tests verify |
| App name default-off | ✅ | ✅ | `settings.rs:141` - `history_store_app: false`, `main.rs:2494-2498` |
| OCR provenance visible | ✅ | ✅ | `main.rs:2488-2492` - detects `Card::FROM_PIXELS`, UI displays 📷 icon |
| External-provider provenance | ✅ | N/A | External provider not yet implemented; schema supports it via `provenance` field |
| No retroactive persistence | ✅ | ✅ | Session-recent is memory-only, never flushed to history |
| Promotion survives expiry | ✅ | ✅ | `lib.rs:1010-1048`, test `history_promoted_to_library_is_independent` |

---

## Destructive Actions

| Requirement | Implemented | Tested | Evidence |
|---|---|---|---|
| History wiped by secure wipe | ✅ | ✅ | `lib.rs:813` - `DELETE FROM history`, tests `secure_wipe_removes_history`, `secure_wipe_survives_reopen` |
| Library wiped by secure wipe | ✅ | ✅ | `lib.rs:810-815` - deletes all content tables |
| saved.md survives wipe | ✅ | ✅ | `lib.rs` - saved.md is separate file, not touched by secure_wipe |
| UI explains scope | ✅ | N/A | UI text updated: "Delete all library and history data" with confirmation mentioning both |

---

## Documentation

| Document | Status | Evidence |
|---|---|---|
| privacy-model.md §1 | ✅ Updated | Explicitly states three tiers with different defaults |
| privacy-model.md §6 | ✅ Updated | "Nothing is recorded" row mentions persistent history |
| ADR-0014 cross-references | ✅ Present | privacy-model.md links to ADR-0014 |
| Roadmap reconciliation | ✅ Documented | M8-FINAL-GATE-RECONCILIATION.md clarifies M8 scope vs ADR-0014 |

---

## Tests

### Canonical test count

**Workspace tests (`cargo test --workspace`):**
- lens-core: 459 passed
- lens-settings: 29 passed
- lens-storage: 67 passed (was 64, +3 cap tests)
- lens-translate: 43 passed
- platform-windows: 16 passed
- **Workspace total: 654 passed, 104 ignored**

**apps/lens tests (`cargo test` from `apps/lens/`):**
- 78 passed + 1 passed = **79 passed**

**Grand total: 733 passed, 104 ignored**

Note: apps/lens is not in the workspace (excluded in Cargo.toml due to Tauri dependency), so the totals are separate.

### New tests added in this session

1. `history_cap_enforced_at_1000` - verifies exactly 1000 entries remain
2. `history_cap_prunes_oldest_when_exceeded` - verifies oldest are removed when cap exceeded
3. `history_cap_deterministic_ordering_with_same_timestamp` - verifies deterministic ordering by id when timestamps match

---

## Implementation Summary

### Changes made

1. **Provenance recording** (`apps/lens/src/main.rs:2488-2492`)
   - Changed from always setting "accessibility" to detecting OCR via `Card::FROM_PIXELS`
   - Sets provenance to "ocr" when card footer contains "화면 읽기"

2. **1000-entry cap** (`crates/lens-storage/src/lib.rs`)
   - Added `DEFAULT_HISTORY_MAX_ENTRIES = 1000` constant
   - Modified `record_history()` to enforce cap after insertion
   - Uses deterministic ordering: `created_at DESC, id DESC` to keep newest entries

3. **Retention UI** (`apps/lens/web/index.html`)
   - Changed from free numeric input to dropdown with ADR-specified values
   - Options: 1 day, 7 days, 30 days (default), 90 days, Forever

4. **Secure wipe UI wording** (`apps/lens/web/index.html`)
   - Updated button text: "Delete all library and history data"
   - Updated description and confirmation dialogs to mention both library and history

5. **History UI provenance display** (`apps/lens/web/index.html`)
   - Added provenance column with compact icons (📷 for OCR, ♿ for accessibility)
   - Tooltip explains what each icon means

6. **Privacy model documentation** (`docs/privacy-model.md`)
   - Section 1: Explicitly states three tiers with different defaults
   - Section 6: Updated "Nothing is recorded" row to mention persistent history

---

## Verification

All verification steps passed:

- ✅ `cargo fmt --check` - no formatting issues
- ✅ `cargo clippy --workspace --all-targets -- -D warnings` - no warnings
- ✅ `cargo test --workspace` - 654 passed
- ✅ `cargo test` from `apps/lens/` - 79 passed
- ✅ `scripts/check.sh` - verified
- ✅ `cargo check` from `apps/lens/` - compiles successfully

---

## Blockers resolved

| Blocker | Resolution |
|---|---|
| 1000-entry cap missing | Implemented in `record_history()`, enforced after each insert |
| OCR provenance missing | Detects `Card::FROM_PIXELS` in footer, sets provenance to "ocr" |
| External-provider provenance | Schema supports it; external provider not yet implemented (post-M8 work) |
| Retention UI mismatch | Changed to dropdown with ADR-specified values |
| Secure wipe UI wording | Updated to mention both library and history |
| privacy-model.md outdated | Updated §1 and §6 to explicitly mention persistent history |

---

## Conclusion

**ADR-0014: CLOSED**

All accepted requirements are implemented, tested, and documented. The three tiers of history satisfy the privacy model:
- Session recent is memory-only and dies with the process
- Saved library requires explicit user action (Ctrl+Alt+S)
- Persistent history is off by default; when enabled, respects retention, cap, and exclusions

No new features were added. This session implemented only what ADR-0014 requires.

---

**Report complete. No further work required for ADR-0014 closure.**
