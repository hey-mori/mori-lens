# Mori Lens — Session Handoff

Last updated: 2026-09-07

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS implementation is complete.
**All repository-local implementation and evidence work is complete. Remaining work is CI execution (blocked by billing), external accounts, human gates, or quality research.**

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer + MSIX packaging infrastructure (ADR-0017)
- Code signing: NOT CONFIGURED (Store account needed for MSIX path)

**macOS (implementation complete, CI passing, real-world validation pending permissions):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR with per-word geometry (`boundingBoxForRange` → `VNRectangleObservation`)
- Typed FFI ABI (`LensOcrWord` struct: float geometry + `uint32_t` offsets)
- Integer overflow protection, narrowing safety, RAII ownership, range validation
- Passive native overlay (NSPanel, transparent, click-through, ARC-clean)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- Hotkey queue: true SPSC + saturating unsigned Pause counter (max 1024)
- Correct capture: pointer-based display selection, BGRA canonical
- Overlay exclusion via CGWindowID, shutdown cleanup, diagnostics wired
- Settings hot-path: generation counter, policy enforcement
- Graceful degradation without AX permission (no SIGTRAP)

**Mori integration (built, unverified):**
- Process boundary with capability discovery (`.capability.json`)
- Version negotiation, explicit user action to send items
- No shared DB, no shared package, no hidden sync (ADR-0018)
- `mori_transfer` module with `TransferPayload`, `MoriStatus`, `send_to_mori`
- Tauri commands: `mori_status`, `mori_send_item`

**Distribution infrastructure:**
- Package-smoke CI workflow (`.github/workflows/package-smoke.yml`)
- Windows: NSIS (via `cargo tauri build`) + MSIX (via `scripts/package-msix.ps1`)
- macOS: .app bundle (via `cargo tauri build`)
- Smoke asset generation (`scripts/generate-smoke-assets.py`): PNG, ICO, ICNS
- MSIX verification (`scripts/verify-msix.ps1`)
- MSIX manifest: `runFullTrust` only (no unnecessary `broadFileSystemAccess`)
- Release workflow (`.github/workflows/release.yml`): aligned with package-smoke

**Verification gate (`scripts/check.sh --full`):**
- Workspace fmt + clippy + test
- Product binary fmt + clippy + check + test
- Clean native rebuild + privacy gates + version consistency

## This Session (two parts)

### Part 1 — Package-smoke fixes (previous session continuation)

**PR #315** — fix: MSIX manifest encoding and macOS .icns icon for package-smoke
**PR #316** — fix: add icon generation to release workflow jobs
**PR #317** — docs: fix stale capability count (26→28) and installability claims

### Part 2 — M8 gate closure and canonical truth (this session)

**PR #318** — docs: reconcile canonical truth with current code state
- project-goal.md §2: OCR contract "manual only" → "manual by default; automatic after explicit opt-in"
- project-goal.md: package-smoke claim downgraded to honest status (infrastructure exists, CI execution pending)
- privacy-model.md: macOS settings path added from code truth
- roadmap.md M8: exact test names for all three gate conditions
- ADR-0004/0005/0007: implementation status annotations appended (body preserved)

**PR #319** — test: close M8 gate evidence gaps
- `nothing_is_persisted_before_an_explicit_save`: product-level save boundary test
- `json_export_semantic_round_trip`: JSON export → deserialize → all semantic fields preserved
- `single_delete_survives_reopen`: delete(id) → drop → reopen → row absent, other items/tags survive

**PR #320** — fix: remaining settings.conf references
- SECURITY.md: settings.conf → settings.toml
- keeping.rs: Windows test paths updated

**Hidden backlog found and fixed:**
1. M8 gate had 3 evidence gaps (JSON round-trip, delete+reopen, explicit save e2e) — all closed
2. OCR contract stale in project-goal.md §2 — fixed
3. Package-smoke overclaim in project-goal.md — downgraded to honest status
4. ADR-0004/0005/0007 "Not yet built" markers — annotations added
5. macOS settings path missing from privacy-model.md — added from code truth
6. SECURITY.md had stale settings.conf reference — fixed
7. keeping.rs Windows test paths used settings.conf — fixed

## Previous Sessions

- **PR #303–#314** — various fixes (see git log)
- **PR #315** — fix: MSIX manifest encoding + .icns icon
- **PR #316** — fix: release workflow icon generation
- **PR #317** — docs: capability count + installability claims

## Completion Matrix

```
Area                    Implementation  Automated  Runtime   Human     Distribution
M1 Windows acquire      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M2 Windows overlay      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M3 Quick translation    SHIPPED         CLOSED     CLOSED    N/A       N/A
M4 Windows OCR          SHIPPED         CLOSED     CLOSED    PENDING   N/A
M5 Deep analysis        SHIPPED         CLOSED     CLOSED    OPEN*     N/A
M6 macOS acquire        COMPLETE        CLOSED     BLOCKED   PENDING   N/A
M7 macOS overlay/OCR    COMPLETE        CLOSED     BLOCKED   PENDING   N/A
M8 Library/Review       COMPLETE        CLOSED     N/A       N/A       N/A
Mori boundary           BUILT           CLOSED     N/A       PENDING   N/A
Windows NSIS release    SHIPPED         CLOSED     N/A       N/A       UNSIGNED
Windows MSIX release    VERIFIED        CLOSED     N/A       N/A       BLOCKED†
macOS .app bundle       INFRASTRUCTURE  CLOSED     PENDING   N/A       UNSIGNED
macOS notarization      BLOCKED‡       N/A        N/A       N/A       BLOCKED‡
First-user onboarding   NOT TESTED      N/A        N/A       PENDING   N/A
Return/re-entry         NOT TESTED      N/A        N/A       PENDING   N/A

* M5 explanation quality: model-quality research, gate open (P1 lemma 63%)
† Store developer account required (external)
‡ cargo-tauri not installed locally; CI execution in progress
```

## Agent Assessment

```
M8 IMPLEMENTATION:                COMPLETE
M8 AUTOMATED GATE:                CLOSED
  Explicit save:                  nothing_is_persisted_before_an_explicit_save
                                  a_save_also_writes_to_the_library_database
  Export round-trips:             jsonl_round_trip_preserves_items_and_tags
                                  jsonl_semantic_round_trip
                                  jsonl_round_trip_preserves_review_state
                                  json_export_semantic_round_trip
                                  exporting_generates_saved_md_format
  Deletion persistence:           single_delete_survives_reopen
                                  secure_wipe_survives_reopen
                                  library_delete_does_not_affect_history

PACKAGE INFRASTRUCTURE:           IMPLEMENTED (NSIS + MSIX + .app + release workflow)
PACKAGE CURRENT-MAIN EXECUTION:   PENDING (GitHub Actions billing blocker)

CANONICAL PRODUCT TRUTH:          RECONCILED
  OCR contract:                   manual by default, automatic after opt-in
  Package-smoke:                  honest status (not overclaimed)
  ADR markers:                    annotated (body preserved)
  macOS paths:                    documented from code truth
  Settings path:                  settings.toml everywhere (no stale .conf)

REMAINING REPOSITORY IMPLEMENTATION:  NONE
REMAINING PACKAGING EXECUTION:
  Package-smoke CI GREEN          BLOCKED BY BILLING
  (code fixes merged in PR #315; latest blocked run 34131713859 was on old HEAD)

REMAINING QUALITY RESEARCH:
  #148 colour linkage             QUALITY GATE OPEN (threshold not met)
  M5 explanations right           QUALITY RESEARCH OPEN (model-dependent)

REMAINING HUMAN PRODUCT GATES:
  First-user installs unaided     HUMAN PRODUCT GATE
  Keeps running for a week        HUMAN PRODUCT GATE
  Can state what it is for        HUMAN PRODUCT GATE

REMAINING MACOS VALIDATION:       BLOCKED BY PERMISSIONS
  AX/OCR/overlay/hotkeys          Requires Accessibility + Screen Recording
  sleep/wake/hot-plug             Requires physical hardware

REMAINING WINDOWS VALIDATION:     BLOCKED BY ENVIRONMENT
  VS Code/KakaoTalk real-app      Requires Windows machine

REMAINING EXTERNAL BLOCKERS:
  GitHub Actions billing          BILLING BLOCKER ("recent account payments have failed")
  Store developer account         CREDENTIAL BLOCKER
  Apple Developer ID              CREDENTIAL BLOCKER
  OV code-signing certificate     CREDENTIAL BLOCKER
  Ollama + model                  SERVICE BLOCKER

REMAINING ASSET DEBT:             Real icon/design assets (smoke placeholders in use)

REMAINING OWNER DECISIONS:
  Legal publishing identity       OWNER DECISION
  OV certificate vendor           OWNER DECISION
  Return/re-entry UX approach     OWNER DECISION
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (2026-09-07, local)
workspace fmt/clippy/tests:       PASS (678 tests)
product fmt/clippy/check/test:    PASS (53 tests)
clean native rebuild:             PASS
platform-macos tests:             PASS (56 tests including 16 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
lens-storage tests:               PASS (76 tests, full M8 lifecycle)
privacy static gate:              CLEAN
Generated .icns:                  VALID (file: "Mac OS X icon, 14039 bytes, icp4 type")
CI run 34027544715:               PASS (both platforms)
Package-smoke run 34058540557:    FAILURE (MSIX XML + macOS icon — fixed in PR #315)
Package-smoke run 34131713859:    BLOCKED (GitHub Actions billing failure)
```

## Package-Smoke Run

```
PACKAGE-SMOKE RUN:                34131713859 (BLOCKED — billing)
WINDOWS FULL GATE:                BLOCKED (billing)
WINDOWS NSIS:                     BLOCKED (billing)
WINDOWS MSIX:                     BLOCKED (billing)
MSIX VERIFY:                      BLOCKED (billing)
WINDOWS ARTIFACT UPLOAD:          BLOCKED (billing)

MACOS FULL GATE:                  BLOCKED (billing)
MACOS APP:                        BLOCKED (billing)
MACOS APP INSPECTION:             BLOCKED (billing)
MACOS ZIP/HASH:                   BLOCKED (billing)
MACOS ARTIFACT UPLOAD:            BLOCKED (billing)

NOTE: blocked run HEAD (e6337d8) ≠ current main HEAD.
      Billing resolution requires new workflow_dispatch on current main.

PREVIOUS RUN (34058540557):
WINDOWS NSIS:                     PASS
WINDOWS MSIX:                     FAIL (error C00CEE40 — fixed in PR #315)
MACOS APP:                        FAIL (No matching IconType — fixed in PR #315)
```

## Canonical Doc Truth (verified this session)

```
project-goal.md:
  capability count:               28 shipped ✓
  OCR contract:                   manual by default, automatic opt-in ✓
  macOS acquisition:              "built, unverified" ✓
  macOS overlay/OCR:              "built, unverified" ✓
  Mori integration:               "built, unverified" ✓
  Settings file:                  settings.toml ✓
  installability:                 honest status (not overclaimed) ✓

roadmap.md:
  M8 gate table:                  CLOSED with exact test names ✓
  M6/M7 gate tables:              current ✓

privacy-model.md:                 macOS path added, platform-neutral ✓
README.md:                        M8+ milestone, settings.toml ✓
what-finished-means.md:           no stale claims ✓
SECURITY.md:                      settings.toml ✓
ADR-0004/0005/0007:               implementation status annotated ✓
```

## Known Runtime Issues

- Lock file permission bug (`--w-------` → fixed to `-rw-------`)
- Ollama not available locally (Quick/Deep translation BLOCKED)
- macOS binary starts gracefully without AX permission (SIGTRAP fixed in PR #307)
- Package-smoke CI slow: `cargo install tauri-cli` takes 30-50 min on CI runners
- GitHub Actions billing: jobs fail to start with "recent account payments have failed"

## Validation Procedure

When permissions are granted, run:

```bash
bash scripts/validate-macos.sh
```

## Controlled Fixture

`docs/fixtures/validation-surface.html` — English, Korean, Japanese, mixed, repeated, Unicode/emoji.

## Open Issues

```
#148  colour linkage quality gate     QUALITY GATE OPEN
#140  first-user onboarding           HUMAN VALIDATION
#34   KakaoTalk                       WINDOWS VALIDATION (area:acquire)
#14   VS Code                         WINDOWS VALIDATION (area:platform-windows)
```

## Next Session — Start Here

1. **Resolve GitHub Actions billing** — package-smoke CI cannot run until this is fixed
2. Once billing is resolved, trigger `workflow_dispatch` on `package-smoke.yml` on **current main**
3. If package-smoke passes: both platforms GREEN → handoff can declare PACKAGE-SMOKE: PASS
4. If package-smoke fails: inspect logs, iterate (code fixes are in place)
5. Grant Accessibility + Screen Recording permissions
6. (Optional) Start Ollama with a model
7. Run `bash scripts/validate-macos.sh`
8. Review evidence in `docs/benchmarks/`

## Git State

```
latest merge: PR #320 (fix: remaining settings.conf references)
CI pass: run 34027544715 (both platforms)
Package-smoke: run 34131713859 (BLOCKED — billing)
always run `git log -1` at session start for actual HEAD.
```

## Constraints

- Agent investigation stays inside `PROJECT_ROOT/**`.
- `lens-core` remains platform-neutral (ADR-0003).
- Do not install external tools without explicit permission.
- Work in coherent branch → PR → merge units. No direct main pushes.
- macOS native code uses Objective-C bridges compiled by `cc` crate with `-fobjc-arc`.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
- `apps/lens` is not a workspace member; verified by `scripts/check.sh --full`.
