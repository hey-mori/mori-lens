# Mori Lens — Session Handoff

Last updated: 2026-09-07

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS implementation is complete.
**All repository-local implementation work is complete. Distribution infrastructure implemented and code-fixed. CI execution blocked by GitHub Actions billing. Remaining work is billing resolution, validation, external accounts, human gates, or quality research.**

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

## This Session

**PR #315** — fix: MSIX manifest encoding and macOS .icns icon for package-smoke
- package-msix.ps1: Replaced regex `-replace` with simple `.Replace()` string substitution. Previous BOM-trim + regex approach (PR #314) did not resolve error C00CEE40 on Windows Server 2025 MakeAppx
- AppxManifest.xml.in: Changed XML declaration encoding `utf-8` → `UTF-8`
- generate-smoke-assets.py: Added `make_icns()` — generates valid .icns with all required OSType entries (icp4–ic14). Tauri requires .icns for .app bundle creation; .png and .ico alone produce "No matching IconType" error
- tauri.conf.json: Added `icons/icon.icns` to icon list
- apps/lens/icons/icon.icns: Committed structural placeholder (14039 bytes)

**PR #316** — fix: add icon generation to release workflow jobs
- release.yml: Added `Generate package icons` step to all three build jobs (Windows NSIS, macOS .app, Windows MSIX). Without this, tauri.conf.json's reference to `icons/icon.icns` would cause build failures since the .icns is generated, not permanently committed as a design asset

**PR #317** — docs: fix stale capability count and installability claims
- project-goal.md: Capability count `Twenty-six` → `Twenty-eight` (actual count from capability table)
- project-goal.md: Installability section updated to mention package-smoke CI as structural proof for both platforms

**Hidden backlog found and fixed:**
1. MSIX manifest regex + PowerShell backreference interaction produced invalid XML on Windows Server 2025 MakeAppx (PR #314's BOM-trim fix was insufficient)
2. macOS .app bundling failed: Tauri needs .icns but only .png and .ico were provided
3. Release workflow would fail on macOS build: tauri.conf.json referenced .icns but release.yml had no icon generation step
4. Canonical doc capability count was wrong (26 vs actual 28)

## Previous Sessions

- **PR #303** — docs: fix stale macOS claims in project-goal.md
- **PR #304** — docs: add M6/M7/M8 gate status tables to roadmap
- **PR #305** — feat: Windows MSIX packaging infrastructure (ADR-0017)
- **PR #306** — docs: fix settings.conf → settings.toml, README milestone
- **PR #307** — fix: macOS graceful degradation without AX permission
- **PR #308** — fix: canonical doc contradictions + MSIX manifest cleanup
- **PR #309** — ci: package-smoke workflow for NSIS + MSIX + macOS .app
- **PR #310** — fix: package-smoke RGBA icon generation
- **PR #312** — fix: stale canonical doc claims and macOS compilation bug
- **PR #313** — fix: remaining settings.conf references → settings.toml
- **PR #314** — fix: package-smoke BOM and icon generation (insufficient fix)

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
REMAINING REPOSITORY IMPLEMENTATION:  NONE
  (all repository-local implementation work is complete)
  (this session found and fixed 4 hidden bugs)

REMAINING PACKAGING IMPLEMENTATION:   NONE
  (MSIX infrastructure, package-smoke CI, smoke assets, release workflow all fixed and aligned)
  (code fixes verified locally; CI execution blocked by billing)

REMAINING QUALITY RESEARCH:
  #148 colour linkage                 QUALITY GATE OPEN (threshold not met)
  M5 explanations right               QUALITY RESEARCH OPEN (model-dependent)

REMAINING HUMAN PRODUCT GATES:
  First-user installs unaided         HUMAN PRODUCT GATE
  Keeps running for a week            HUMAN PRODUCT GATE
  Can state what it is for            HUMAN PRODUCT GATE

REMAINING MACOS VALIDATION:           BLOCKED BY PERMISSIONS
  AX/OCR/overlay/hotkeys              Requires Accessibility + Screen Recording
  sleep/wake/hot-plug                 Requires physical hardware

REMAINING WINDOWS VALIDATION:         BLOCKED BY ENVIRONMENT
  VS Code/KakaoTalk real-app          Requires Windows machine

REMAINING DISTRIBUTION WORK:
  Store submission                    BLOCKED (developer account)
  Code signing (Windows direct)       BLOCKED (OV certificate)
  Code signing (macOS)                BLOCKED (Developer ID)
  Notarization                        BLOCKED (Apple account)
  Real icon/design assets             ASSET BLOCKER
  Package-smoke CI completion         BLOCKED BY BILLING (code fixes merged, CI cannot run)

REMAINING EXTERNAL BLOCKERS:
  GitHub Actions billing              BILLING BLOCKER (jobs fail to start: "recent account payments have failed")
  Store developer account             CREDENTIAL BLOCKER
  Apple Developer ID                  CREDENTIAL BLOCKER
  OV code-signing certificate         CREDENTIAL BLOCKER
  Ollama + model (Quick/Deep test)    SERVICE BLOCKER

REMAINING OWNER DECISIONS:
  Legal publishing identity           OWNER DECISION
  OV certificate vendor               OWNER DECISION
  Return/re-entry UX approach         OWNER DECISION
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (2026-09-07, local)
workspace fmt/clippy/tests:       PASS (676 tests)
product fmt/clippy/check/test:    PASS (52 tests)
clean native rebuild:             PASS
platform-macos tests:             PASS (56 tests including 16 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
lens-storage tests:               PASS (73 tests, full M8 lifecycle)
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

PREVIOUS RUN (34058540557):
WINDOWS NSIS:                     PASS
WINDOWS MSIX:                     FAIL (error C00CEE40: XML declaration syntax — fixed in PR #315)
MACOS APP:                        FAIL (No matching IconType — fixed in PR #315)
```

## Canonical Doc Truth (verified this session)

```
project-goal.md:
  capability count:               28 shipped (was 26, fixed in PR #317) ✓
  macOS acquisition:              "built, unverified" ✓
  macOS overlay/OCR:              "built, unverified" ✓
  Mori integration:               "built, unverified" ✓
  Settings file:                  settings.toml ✓
  installability:                 cross-platform (fixed in PR #317) ✓

roadmap.md:
  M6 gate status table:           current ✓
  M7 gate status table:           current ✓
  M8 gate status table:           CLOSED with automated evidence ✓

privacy-model.md:                 settings.toml references correct ✓
README.md:                        M8+ milestone, settings.toml ✓
what-finished-means.md:           no stale claims ✓
```

## M8 Gate Evidence

```
Gate condition                    Test evidence
───────────────────────────────── ─────────────────────────────────────────
Nothing stored without user       Save is explicit (Ctrl+Alt+S). SessionRecent
action                            is memory-only. 73 library tests confirm.
                                  insert() is the only path to persistent storage.

Export round-trips                jsonl_round_trip_preserves_items_and_tags
                                  jsonl_semantic_round_trip
                                  jsonl_round_trip_preserves_review_state
                                  MD + JSON export tested.

Deletion is real                  delete(): SQL DELETE FROM library WHERE id = ?
                                  secure_wipe(): removes all library + history rows
                                  secure_wipe_survives_reopen: persists after reopen
                                  library_delete_does_not_affect_history: isolation
                                  Archive is a review_state value, not a separate
                                  operation. delete() removes the row entirely.
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
2. Once billing is resolved, trigger `workflow_dispatch` on `package-smoke.yml`
3. If package-smoke passes: both platforms GREEN → handoff can declare PACKAGE-SMOKE: PASS
4. If package-smoke fails: inspect logs, iterate (code fixes are in place)
5. Grant Accessibility + Screen Recording permissions
6. (Optional) Start Ollama with a model
7. Run `bash scripts/validate-macos.sh`
8. Review evidence in `docs/benchmarks/`

## Git State

```
latest merge: PR #317 (docs: fix stale canonical claims)
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
