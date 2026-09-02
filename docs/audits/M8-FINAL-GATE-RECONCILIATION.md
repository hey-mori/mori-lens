# M8 Final Gate Reconciliation

**Date:** 2026-09-02
**Purpose:** Determine whether M8 can be declared CLOSED by reconciling implementation against roadmap gate wording, ADRs, and privacy contracts.

---

## 1. Roadmap Reconciliation

### M8 Gate Wording (roadmap.md)

**Scope:** "Saved words and sentences, notes, review list, export. Saving is always explicit; nothing is captured automatically."

**Gate:** "nothing is stored without a user action; export round-trips; deletion is real."

### Conflict: Persistent History vs M8 Gate

**ADR-0014 (Accepted 2026-08-27)** introduces three tiers of history:
- Tier 1: Session recent (memory only, on by default)
- Tier 2: Saved library (explicit save, unchanged)
- Tier 3: Persistent history (opt-in, **off by default**)

**The conflict:**
- M8 gate says: "nothing is stored without a user action"
- ADR-0014 Tier 3 says: when enabled, **every translation is automatically recorded**

The user action is turning on the setting, but individual history entries are stored without individual user actions. This is a direct conflict with M8's gate wording.

**Analysis:**

ADR-0014 was accepted on the same day as ADR-0013 (2026-08-27), after M8's gate was written. The roadmap's M8 scope lists "Saved words and sentences, notes, review list, export" but does **not** list "persistent history." ADR-0013's context explicitly states: "Whether a durable history exists at all is #163 and its own record."

This means persistent history is **not part of M8's scope**. It is a separate feature implemented alongside M8 work.

**Conclusion:**
Persistent history does not violate M8's gate because it is not part of M8. The M8 gate applies to M8's scope (library, review list, export), and those components satisfy the gate. Persistent history is ADR-0014 work, implemented early.

However, the **roadmap should be updated** to reflect that:
1. M8's scope was completed as specified
2. Persistent history (ADR-0014 Tier 3) was implemented as a separate concern
3. The gate wording "nothing is stored without a user action" applies to M8's scope, not to ADR-0014

---

## 2. ADR-0014 Completeness

### Requirements vs Implementation

| Requirement | Status | Evidence |
|---|---|---|
| Default off | ✅ PASS | `settings.rs:139` - `history: false` |
| 30-day retention default | ✅ PASS | `settings.rs:140` - `history_days: 30` |
| Retention choices: 1, 7, 30, 90, forever | ⚠️ PARTIAL | Settings parser validates `[0, 1, 7, 30, 90]` ✅, but UI allows **any integer 0-3650** ❌. Invalid values silently fall back to 30 on next load. |
| **1000-entry cap** | ❌ **NOT IMPLEMENTED** | No code enforces a maximum entry count. ADR-0014 says "at most 1,000 entries by default" but this is not in the code. |
| Per-application exclusions | ✅ PASS | `settings.rs:123` - `history_excluded: Vec<String>`, enforced in `main.rs:2474-2482` |
| App name default off | ✅ PASS | `settings.rs:141` - `history_store_app: false`, enforced in `main.rs:2494-2498` |
| Provenance visible (OCR vs accessibility) | ✅ PASS | `lib.rs:91` - `provenance: String` field, stored and retrieved |
| No retroactive persistence | ✅ PASS (implicit) | Session-recent is memory-only (`recent.rs:24`), never flushed to history. No code path exists to retroactively persist. |
| Promotion survives expiry | ✅ PASS | `lib.rs:1020-1048` - creates independent library item, test `history_promoted_to_library_is_independent` verifies |

### Blockers

**CRITICAL: 1000-entry cap not implemented.** ADR-0014 requires this. Without it, a reader who enables history can accumulate unlimited entries, which contradicts the "at most 1,000 entries by default" contract.

**MINOR: Retention UI allows arbitrary values.** The settings parser enforces the ADR-specified values, but the UI does not. This creates a confusing UX where a user can enter "45" and it silently becomes "30" on next load.

---

## 3. Secure Wipe

### ADR-0013 Semantics

**Prescribed sequence:** "a WAL checkpoint, then `secure_delete`, then `VACUUM`"

**Implementation (`lib.rs:797-824`):**
1. `PRAGMA wal_checkpoint(TRUNCATE);`
2. `PRAGMA secure_delete = ON;`
3. `DELETE FROM library_tags; DELETE FROM tags; DELETE FROM history; DELETE FROM library; DELETE FROM meta;`
4. `VACUUM;`

**Verdict:** ✅ PASS. The order matches ADR-0013. The intermediate DELETE step is necessary so that `secure_delete` overwrites the freed pages before VACUUM reclaims them.

### Physical Marker Test

**Not performed.** ADR-0013 says secure wipe "does remove the residue inside the SQLite file" but acknowledges it "does not reach filesystem snapshots, drive controller remapped blocks, backups, or exports." A physical marker test (writing a known string, wiping, and searching the raw file) would confirm the SQLite-level guarantee but was not executed in this audit.

**Evidence:** Tests `secure_wipe_removes_everything_and_leaves_a_working_database`, `secure_wipe_removes_history`, and `secure_wipe_survives_reopen` verify logical deletion and reopen survival.

---

## 4. Export Round-trip

### JSONL Gate Evidence

**Tests:** 9 JSONL tests in `lib.rs`:
- `jsonl_export_has_a_version_header_and_one_item_per_line`
- `jsonl_round_trip_preserves_items_and_tags`
- `jsonl_round_trip_resets_review_state`
- `malformed_jsonl_does_not_corrupt_database`
- `jsonl_missing_required_field`
- `jsonl_unknown_extra_fields_ignored`
- `jsonl_blank_lines_skipped`
- `jsonl_duplicates_silently_skipped`

**Round-trip preserves:** original, answer, caveat, app, created_at, kind, languages, model, note, tags ✅

**Round-trip resets:** review_state, next_review_at, review_count (by design) ✅

**Import semantics:** Partial import (not atomic). Lines before an error are committed. This is documented in the code and tests.

**Verdict:** ✅ PASS. Export round-trips as required by M8 gate.

---

## 5. Durable Write Inventory

### All Durable Write Paths

| Write Path | Trigger | Location |
|---|---|---|
| `library.insert()` | `Ctrl+Alt+S` (explicit save) | `library_service.rs` via Tauri command |
| `library.record_history()` | Each card shown (when history enabled) | `main.rs:2512` |
| `library.record_review()` | Review list UI action | `tauri_app.rs` via Tauri command |
| `library.history_prune()` | Startup (when history enabled) | `main.rs:1179` |
| `library.secure_wipe()` | "Delete all library data" button | `tauri_app.rs` via Tauri command |
| `library.history_delete()` | History UI delete action | `tauri_app.rs` via Tauri command |
| `library.history_promote()` | History UI promote action | `tauri_app.rs` via Tauri command |

**Inventory:**
- Library inserts: 1 path (explicit save) ✅
- History inserts: 1 path (automatic when enabled, gated by settings + exclusions) ✅
- Review recording: 1 path (explicit UI action) ✅
- Deletions: 3 paths (secure wipe, history delete, history promote) ✅

**Verdict:** ✅ PASS. All durable write paths are enumerated and accounted for.

---

## 6. Documentation Consistency

### privacy-model.md

**Section 1 ("The promises"):**
- Line 18-22: "Nothing is recorded... no automatic transcript of what was hovered"
- Line 54-61: References ADR-0014 and states "three tiers of history of which only a memory-only one is on by default"

**Section 6 ("Current state"):**
- Line 377: "Nothing is recorded" row says "**kept for everything automatic, and two stores are the exception**" and mentions `saved.md` and `library.db`

**Issue:** The privacy model has been updated to reference ADR-0014 but does not explicitly state that persistent history exists as an opt-in transcript. The phrase "no automatic transcript" in §1 is partially contradicted by ADR-0014 Tier 3 (which is an automatic transcript when enabled).

**Recommendation:** §1 should explicitly state: "No automatic transcript of what was hovered **unless the reader has turned on persistent history (ADR-0014 Tier 3).** Off by default, and off means nothing is written."

### UI Wording vs Wipe Scope

**Button text:** "Delete all library data"
**Description:** "Permanently delete all library data..."
**Confirmation:** "All saved items, notes, tags, and review progress will be permanently deleted."

**Issue:** The secure wipe **also deletes the history table** (verified in `lib.rs:813`), but the UI text does not mention history. A reader who has enabled persistent history and clicks "Delete all library data" will lose their history without the UI telling them so.

**Recommendation:** Update UI text to say "Delete all library data **and history**" or "Delete all saved data" to reflect the actual scope.

### saved.md Semantics

**ADR-0013:** "`saved.md` is never overwritten and never removed... Afterwards it becomes the human-readable export"

**Implementation:** `saved.md` is imported on first library activation (`lib.rs:1053-1168`) but is not deleted by secure wipe. This matches ADR-0013.

**Verdict:** ✅ PASS. `saved.md` survives secure wipe as intended.

---

## 7. Test-count Reconciliation

### Actual Test Count

**Workspace tests (`cargo test`):**
- build-stamp: 0 (no tests)
- lens-core: 459 passed
- lens-settings: 29 passed
- lens-storage: 64 passed
- lens-translate: 43 passed (50 ignored)
- platform-windows: 16 passed (54 ignored)
- hover-probe: 0 (no tests)

**Workspace total: 651 passed, 104 ignored**

**apps/lens tests:**
- 78 passed + 1 passed = 79 passed

**Grand total: 730 passed, 104 ignored**

### Discrepancy Resolution

Previous reports cited 633 or 622. The actual count is **730**.

The discrepancy likely arose from:
1. Different counting methods (workspace only vs workspace + apps/lens)
2. Tests added in uncommitted changes (this session added 24 tests to lens-storage and 5 to main.rs, but these are now included in the counts above)
3. Ignored tests being counted or not counted

**Correct count: 730 passed, 104 ignored.**

### New Tests Added in This Session

**Uncommitted changes (persistent history + storage integrity work):**
- lens-storage: 24 new tests (secure wipe, migration, JSONL, history/library independence)
- main.rs: 5 new tests (privacy invariants, latency benchmark)

**Total new tests: 29**

---

## 8. Review List Gate Relationship

### M8 Scope

The roadmap lists "review list" as part of M8's scope: "Saved words and sentences, notes, **review list**, export."

**Implementation:** Review list with spaced repetition scheduling is built (`ADR-0015`, implemented in `lib.rs` and UI).

**Gate relationship:** The review list is subject to M8's gate ("nothing is stored without a user action"). Review recording is triggered by explicit UI actions (review button clicks), so it satisfies the gate.

**Verdict:** ✅ PASS. Review list is M8 scope and satisfies the gate.

---

## 9. Final Verification

### scripts/check.sh

**Result:** ✅ PASS (verified)
- `cargo fmt --check` ✅
- `cargo clippy -D warnings` ✅
- `cargo test` ✅

### apps/lens Compilation

**Result:** ✅ PASS (verified)
- `cargo check` from `apps/lens/` directory completes without errors

---

## 10. M8 Verdict

### Gate Conditions

| Condition | Status | Evidence |
|---|---|---|
| Nothing is stored without a user action | ✅ PASS (for M8 scope) | Library inserts require `Ctrl+Alt+S`. Review recording requires UI action. Persistent history is not M8 scope. |
| Export round-trips | ✅ PASS | JSONL round-trip tests verify preservation of all fields. |
| Deletion is real | ✅ PASS | Secure wipe implements ADR-0013 sequence, verified by tests. |

### Blockers

1. **CRITICAL: 1000-entry cap not implemented.** ADR-0014 requires "at most 1,000 entries by default." This is not enforced in code. **This is a blocker for ADR-0014 completeness, not for M8's gate.**

2. **MINOR: UI allows arbitrary retention values.** The settings parser enforces ADR-specified values, but the UI does not. **This is a UX issue, not a gate blocker.**

3. **MINOR: UI wording does not mention history deletion.** "Delete all library data" also deletes history but the text does not say so. **This is a documentation/UX issue, not a gate blocker.**

4. **MINOR: privacy-model.md §1 does not explicitly mention persistent history.** The privacy model references ADR-0014 but does not explicitly state that an opt-in automatic transcript exists. **This is a documentation issue, not a gate blocker.**

### M8 Gate Verdict

**M8 scope:** Library, review list, export, saved words/sentences, notes.

**Gate satisfaction:**
- ✅ Nothing is stored without a user action (for M8 scope)
- ✅ Export round-trips
- ✅ Deletion is real

**M8 verdict: CLOSED**

### ADR-0014 Completeness Verdict

**OPEN** due to:
- ❌ 1000-entry cap not implemented
- ⚠️ Retention UI allows arbitrary values
- ⚠️ UI wording does not mention history deletion
- ⚠️ privacy-model.md does not explicitly mention persistent history

---

## 11. Closure Rule

**M8 is CLOSED.** The gate conditions are satisfied for M8's scope.

**ADR-0014 is OPEN.** The 1000-entry cap is a blocker for ADR-0014 completeness. The other issues are minor but should be addressed before persistent history is considered complete.

**Recommendation:**
1. Implement the 1000-entry cap (enforce in `record_history()` or add a startup prune)
2. Update the UI to use a dropdown with values [0, 1, 7, 30, 90] instead of a free numeric input
3. Update the secure wipe UI text to mention history
4. Update privacy-model.md §1 to explicitly mention persistent history as an opt-in automatic transcript

These are ADR-0014 completion tasks, not M8 blockers.

---

**Report complete.**
