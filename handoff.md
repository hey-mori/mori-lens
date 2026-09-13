# Mori Lens — Session Handoff

Last updated: 2026-09-13

Repository HEAD:
query at session start

This document describes current implementation semantics;
git history is the provenance of the document itself.

## Current Goal

Windows-first product validation.
macOS implementation substantially complete, real runtime validation pending.
Repository-local bounded diagnostics follow-up remains.

## Current Product State

### Windows (automated PASS, human GUI smoke pending)

- Full product loop: hover -> UIA acquisition -> translation -> native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer + MSIX packaging infrastructure (ADR-0017)
- ADR-0005 true two-speed runtime: separate Quick/Deep translators (PR #353)
- Settings: single canonical TOML format via lens-settings crate (PR #352)
- Endpoint locality: canonical engine_for() classifier, adversarial tests (PR #354)
- Settings validation: round-trip consistency proven (PR #355)
- Card rendering separated from reading persistence (PR #374)
- Pending reading context atomic (PR #378)
- Save uses shared Library handle, split-brain removed (PR #401)
- Library degraded mode: explicit backing state (PR #382)
- saved.md migration preserves repeated entries (PR #376)
- Save provenance bound to acquisition-time app and model (PR #428)
- StructuredSave truth propagated to detail/hotkey callers (PR #429)
- History entries persist reading-bound target_language from dispatch time (PR #430, provenance binding corrected by #432/#437)
- StructuredSave enum distinguishes Inserted/Duplicate/Failed (PR #426)
- PresentedReading struct binds language/model at card creation (PR #432)
- Duplicate id carried through StructuredSave and Detail frontend (PR #433)
- Dual-store feedback reflects file and library truth (PR #434)
- Windows Copy preserves source_app through feedback card (PR #437)
- Detail frontend surfaces StructuredSave::Failed with degraded status (PR #437)
- Detail degraded-save warning survives 2-second polling refresh (saveStatus variable persists across render())
- Detail save status DOM mutation guarded by currentDetailRevision: stale async
  responses for a previous detail never write to or clear the current detail's UI
- First-run Control Center discovery: opens Status section automatically (PR #462)
- Second-launch activation: existing instance's Control Center comes to foreground (PR #459)
- Code signing: NOT CONFIGURED (Store account or self-signed certificate needed)
- Ollama + both models available locally (translategemma:latest, qwen2.5:7b)

### macOS (substantially implemented, runtime validation pending)

All items below are IMPLEMENTED at code level with source-level evidence.
Real macOS product build/run verification: PENDING.

**Two-speed runtime (PR #391):**
- Independent Quick/Deep config via quick_config_rx / deep_config_rx
- Independent translators with separate model/endpoint configuration
- Independent config reload on settings change

**Deep analysis cache (AnalysisCache):**
- IMPLEMENTED with model-aware cache key
- AnalysisCache instantiated alongside TranslationCache
- Deep model-switch cache isolation test proves cache correctness (PR #420)
- Model A answer never served when model B is active

**Quick focused word (PR #393):**
- IMPLEMENTED: focused word passed to Quick translator
- Quick cache key intentionally excludes focus (product policy)
- Deep cache key includes focused word
- Quick worker-owned cache key with resolved model identity (PR #400)
- Cache model identity race fixed (PR #398)

**Accessibility acquisition:**
- Production-wired MacTextAcquirer
- Vision OCR with per-word geometry (boundingBoxForRange -> VNRectangleObservation)
- Typed FFI ABI (LensOcrWord struct: float geometry + uint32_t offsets)

**Screen Recording permission (PR #384, #395):**
- Preflight uses actual Screen Recording permission check
- No startup permission prompt
- First OCR use owns request flow
- Permission lifecycle implemented at code level
- Real macOS dialog/restart behavior: RUNTIME PENDING

**Session history (PR #386, #397, #399):**
- SessionRecent: IMPLEMENTED
- PersistentHistory: IMPLEMENTED
- Startup history_prune: IMPLEMENTED
- record_history errors: logged (PR #399)
- Typed history metadata (PR #397): provenance, provider_scope, producer model,
  source/target language, source app delivered as typed fields, not footer parsing.
  Note: source_language detection is not implemented; value remains None for all entries.

**Native capture safety (PR #402, #403, #410, #414):**
- Integer overflow protection: checked arithmetic for stride * height
- Native capture zero guards: bytesPerRow == 0, width/height/stride validation
- Native narrowing guards: bytesPerRow/width/height vs INT32_MAX
- Stop timeout fail-safe: dispatch_semaphore_wait return checked
- Atomic delegate properties (PR #403)
- Rejection-path buffer free on all error paths
- Overlay exclusion via CGWindowID
- No image written to disk (privacy invariant)

**Other macOS fixes (PR #365-#397):**
- modifier/InputState: Ctrl + Option polling
- --auto flag: FIXED (applies policy.set_default(Allow))
- --settings flag: FIXED (data_dir derived from settings parent)
- ReadingContext: FIXED (job carries app/focus through async boundary)
- hover_scope: FIXED (Ctrl toggle implemented)
- Stale result safety: FIXED (Hover Left increments generation)
- Real modifier state: FIXED (overlay::option_held() polls Option/Alt)
- OCR provenance: CORRECT (source-driven from_pixels)
- Manual OCR app/focus: FIXED (uses target.app, not pending_app)
- CLI missing-value validation (PR #388)
- Selftest focused word privacy: length logged, not content (PR #389)
- History metadata typed fields replace footer parsing (PR #397)
- Async result target_language bound at dispatch time (PR #437)
- Production provenance seam: QuickJob/DeepJob into_result() called by macOS
  workers; unit regression tests are cross-platform (dispatch_carrier module)
  and pass on Windows. Real macOS app runtime: PENDING.

**macOS deferred:**
- HoverSession/HoverWorker: DEFERRED (macOS uses inline acquisition, follows core contract)

### Privacy hardening (PR #410, #414, #416, #418, #446)

- saved.md new files: created with 0600 from byte zero (OpenOptions + mode(0o600))
- saved.md existing files: tightened to 0600 BEFORE opening for write; chmod failure returns NotKept::NotWritten
- saved.md create_dir_all failure: propagates as NotKept::NotWritten
- Mori temp payload: created with 0600 from byte zero
- Library DB/WAL/SHM: permission tightening AFTER migrate completes; chmod failure returns Error
- Library hardening failure: Library::open returns error; library_service falls back to InMemoryFallback
- Absent WAL/SHM: normal, skipped (not a failure)
- Windows: inherits filesystem ACLs; no Unix-mode guarantee

- PRIVATE MODE HARDENING: IMPLEMENTED
- PERMISSION FAILURE HANDLING: CLOSED (#446)
  - Unix saved.md: owner-only hardening required before new reader-content append
  - Unix SQLite persistent backing: permission-hardening failure rejects persistent backing, falls back to in-memory
  - Windows: inherits filesystem ACL; no Unix-mode guarantee

### Mori integration (PR #380, #409, #416)

- Temp -> rename atomic visibility publication
- Real filesystem regression test: temp disappears after publish, final JSON parses
- Unix temp created private (0600)
- Explicit user send only
- Capability/version boundary
- No shared DB, no shared package, no hidden sync (ADR-0018)

CODE/AUTOMATED: implemented
REAL Mori watcher end-to-end: not human/runtime verified

### Git workflow safety (PR #372, #412)

- All main changes via PR (enforced by local pre-push hook, PR #372)
- Non-fast-forward published branch push rejected (local pre-push hook, PR #412)
- Commit attribution guard (PR #370)
- Server branch protection: OFF (no GitHub ruleset; enforcement is local only)
- Historical fact: published history was rewritten twice in old sessions
  (Co-authored-by removal). Current main is canonical baseline.
- CURRENT POLICY: direct-main and non-fast-forward autonomous pushes forbidden
- ENFORCEMENT: local pre-push hook when installed (core.hooksPath = .githooks)
- Historical violations: published main rewritten twice; one feature branch
  force-pushed before PR #412

### Distribution infrastructure

- Package-smoke CI workflow (.github/workflows/package-smoke.yml)
- Windows: NSIS (via cargo tauri build) + MSIX (via scripts/package-msix.ps1)
- macOS: .app bundle (via cargo tauri build)
- Smoke asset generation (scripts/generate-smoke-assets.py): PNG, ICO, ICNS
- NSIS verification (scripts/verify-nsis.ps1)
- MSIX verification (scripts/verify-msix.ps1)
- macOS .app bundle verification (scripts/verify-app-bundle.sh)
- Portable macOS verifier regression test (scripts/test_verify_app_bundle.sh)
- MSIX manifest: runFullTrust only (no unnecessary broadFileSystemAccess)
- Release workflow (.github/workflows/release.yml): aligned with package-smoke
- Command/frontend wiring three-set checker (scripts/check_command_wiring.py)
- Command wiring regression tests (scripts/test_check_command_wiring.py)
- Diagnostics export: privacy-safe metadata-only (PR #475, #478)
- Diagnostics privacy regression test: canary strings (PR #478)
- Diagnostics UX: button label matches action, copy success verified (PR #480)

## Additional Gate Checks

```
scripts/check.sh --full also runs:
  smoke asset verification:        PASS
  documentation integrity:         PASS (scripts/check_doc_integrity.py)
  doc integrity regression tests:  PASS (scripts/test_check_doc_integrity.py)
  macOS app verifier portable test: PASS (scripts/test_verify_app_bundle.sh)
  command/frontend wiring:         PASS (scripts/check_command_wiring.py)
  command wiring regression tests: PASS (scripts/test_check_command_wiring.py)
  version consistency:             PASS
  commit message policy:           PASS
```

## Automated Evidence

```
scripts/check.sh --full:     PASS (2026-09-14, local Windows host)
  hover-probe:               29 passed
  lens-core:                 493 passed
  lens-platform-windows:     46 passed, 54 ignored
  lens-settings:             23 passed
  lens-storage:              75 passed
  lens-translate:            51 passed, 51 ignored
  lens-platform-macos:       0 tests (cfg macOS, not compiled on Windows)
  apps/lens:                 185 passed
  no_console_window:         1 passed
Total:                       902 passed (717 workspace + 185 product), 105 ignored
```

### Full-gate scope truth

scripts/check.sh --full on Windows proves:
- workspace fmt/clippy/tests
- apps/lens current-host/Windows build and tests
- platform-macos crate native build/check where supported by gate
- privacy/static checks
- probe build
- commit policy

This does NOT prove:
- real macOS apps/lens product binary
- real ScreenCaptureKit behavior
- real CGEventTap
- real permission dialog
- Spaces/Retina behavior
- global hotkey function on real macOS

MACOS REAL RUNTIME: PENDING

## Windows Current Validation

```
WINDOWS AUTOMATED/PROBE:     AVAILABLE (see test counts above)

WINDOWS HUMAN GUI SMOKE:     PENDING
  Quick visible hover
  Deep hotkey (Ctrl+Alt+D)
  Copy feedback (Ctrl+Alt+C)
  Save + provenance (Ctrl+Alt+S)
  Manual OCR (Ctrl+Alt+O)
  Automatic OCR opt-in
  Settings UI / restart
  Library UI
  Manual Review
  hover_scope / Ctrl inversion
  Fullscreen / borderless
```

Unit/automated evidence does not substitute for GUI smoke evidence.

## Windows Sweep Observations (build 074844b)

```
Chrome (1366x1079):          92% on-target, 11 distinct targets
Claude (1697x1111):          31% on-target, high distances
WindowsTerminal:             0% on-target, single-blob shape
Chrome (1920x1078):          0% on-target, single-blob shape
SourceTree:                  unsupported
ChatGPT/Explorer/etc:        occluded
```

These are compatibility observations, not bug classifications.

## VS Code #14

```
Accessibility text:          EXISTS
RangeFromPoint targeting:    POOR
GetVisibleRanges geometry:   USEFUL LINE GEOMETRY EXISTS
Production synchronous GVR:  REJECTED (p50 ~950ms >> 400ms budget)
Future accessibility research: OPEN
Current practical fallback:  OCR per product OCR policy
Issue:                       OPEN (#14)
```

Do not say "VS Code exposes no text" or "no accessibility solution is possible."

## KakaoTalk #34

```
Real conversation UIA:       0% on-target across measured sweeps
GetVisibleRanges on KakaoTalk: NOT DIRECTLY MEASURED (window unavailable)
VS Code proxy measurement:   NOT KakaoTalk direct evidence
Current practical fallback:  OCR
Issue:                       OPEN (#34)
```

## Windows MSIX

```
LOCAL MSIX STRUCTURAL BUILD: PASS (11/11 checks, 2 expected publisher warnings)
NORMAL INSTALL:              NOT VERIFIED
Current artifact:            unsigned / placeholder publisher
Install error:               0x800B0100 (signature/certificate validation failure)
Distribution blocker:        signing / publisher trust
```

Developer Mode is NOT established as a canonical requirement.
Modern Windows trusted-signed package sideload may not require it.
Future research candidate: Windows 11 dev-only unsigned package path.

## NSIS

```
NSIS INFRASTRUCTURE:         IMPLEMENTED (cargo tauri build --bundles nsis)
NSIS VERIFIER:               IMPLEMENTED (scripts/verify-nsis.ps1)
LOCAL BUILD:                 PASS (2026-09-14, artifact confirmed)
LOCAL VERIFIER EXECUTION:    PASS (15/15 checks, 1 expected unsigned warning)
CI WIRED:                    YES (package-smoke workflow)
CI EXECUTED:                 NOT EXECUTED (billing blocker)
SIGNATURE:                   UNSIGNED (external blocker)
```

## Package CI

```
PACKAGE INFRASTRUCTURE:      IMPLEMENTED
WINDOWS LOCAL MSIX:          PASS
CURRENT-MAIN GITHUB CI:      NOT EXECUTED (billing blocker)
OLD CI:                      Historical pre-rewrite evidence only
```

## Completion Matrix

```
Area                    Implementation  Automated  Runtime   Human     Distribution
M1 Windows acquire      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M2 Windows overlay      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M3 Quick translation    SHIPPED         CLOSED     CLOSED    N/A       N/A
M4 Windows OCR          SHIPPED         CLOSED     CLOSED    PENDING   N/A
M5 Deep analysis        SHIPPED         CLOSED     CLOSED    OPEN*     N/A
M6 macOS acquire        SUBSTANTIAL     CLOSED     PENDING   PENDING   N/A
M7 macOS overlay/OCR    SUBSTANTIAL     CLOSED     PENDING   PENDING   N/A
M8 Library/Review       COMPLETE        CLOSED     N/A       N/A       N/A
Mori boundary           BUILT           CLOSED     PENDING   PENDING   N/A
Windows NSIS release    SHIPPED         CLOSED     N/A       N/A       UNSIGNED
Windows MSIX release    IMPLEMENTED     CLOSED     N/A       N/A       BLOCKED+
macOS .app bundle       IMPLEMENTED     CLOSED     PENDING   N/A       UNSIGNED
macOS notarization      BLOCKED%       N/A        N/A       N/A       BLOCKED%
First-run discovery     IMPLEMENTED     CLOSED     N/A       PENDING   N/A
First-user onboarding   NOT MEASURED    N/A        N/A       HUMAN     N/A
Return/re-entry         IMPLEMENTED     CLOSED     N/A       PENDING   N/A

* M5 explanation quality: linkage quality gate MET (mean 1.77, 5/5 runs >= 1.4)
+ Signing/publisher trust required; current-main CI execution blocked by billing
% Apple Developer ID credentials required; CI blocked by billing
```

## Remaining Work

```
REMAINING REPOSITORY AUDIT:
  Capture late-callback lifetime:   code hardened, real macOS/TSan pending

REMAINING WINDOWS WORK:
  Human GUI product smoke:          HUMAN GATE (agent cannot perform)
  MSIX signing/publisher:           OWNER DECISION
  NSIS local installation:          OWNER DECISION

REMAINING MACOS WORK:
  Real macOS product build/run:     PENDING
  Screen Recording dialog:          PENDING
  AX permission dialog:             PENDING
  OCR overlay/Spaces/Retina:        PENDING
  Global hotkeys real behavior:     PENDING

REMAINING EXTERNAL BLOCKERS:
  GitHub Actions billing:           BILLING BLOCKER
  Store developer account:          CREDENTIAL BLOCKER
  Apple Developer ID:               CREDENTIAL BLOCKER
  OV code-signing certificate:      CREDENTIAL BLOCKER

REMAINING HUMAN PRODUCT GATES:
  First-user installs unaided
  Keeps running for a week
  Can state what it is for

REMAINING OWNER DECISIONS:
  Legal publishing identity
  OV certificate vendor
```

## Open Issues

```
#140  first-user onboarding           HUMAN VALIDATION
#34   KakaoTalk                       OPEN (GVR not directly measured)
#14   VS Code                         OPEN (GVR fallback rejected on performance)
```

## Settings Current Truth

```
SETTINGS FORMAT OWNER:       lens-settings crate (canonical TOML loader/writer)
LEGACY MIGRATION:            Fail-safe + backup, format-based detection
WINDOWS STARTUP:             Same canonical schema via load_or_initialize()
SETTINGS UI:                 Same canonical schema via SettingsState
QUICK / DEEP CONFIG:         Separate [quick] and [deep] sections
                             Separate endpoint/model/runtime per ADR-0005
ENDPOINT LOCALITY:           lens_core::translate::engine_for() as single source
                             Adversarial tests prove locality cannot be bypassed
```

Historical split-brain (legacy parser vs new parser) is FIXED (PR #352).

## ADR-0005 Current Truth

```
ADR-0005 IMPLEMENTATION:     CLOSED (PR #353)
AUTOMATED EVIDENCE:          CLOSED (separate translator tests)
WINDOWS STARTUP DIAGNOSTICS: PASS (Quick=translategemma, Deep=qwen2.5)
BOTH MODELS LOADED:          PASS (engine warm after ~20s)
GUI HOVER -> QUICK:          NOT YET HUMAN-VERIFIED
GUI CTRL+ALT+D -> DEEP:      NOT YET HUMAN-VERIFIED
SAVE PRODUCER-MODEL:         NOT YET HUMAN-VERIFIED
```

Unit test evidence does not substitute for GUI smoke evidence.

## Control Center (ADR-0012)

```
CONTROL CENTER:            tray-accessible and runtime-synchronized (PR #451, #452)
PAUSE:                     single runtime state across hotkey/tray/control center
RECENT:                    session only
HISTORY:                   opt-in persistent
LIBRARY:                   explicit saved items
CLOSE:                     hide (window X does not quit)
QUIT:                      explicit exit via Control Center or tray menu
TRAY ROUTING:              Status / Library / History / Settings (deep-linked)
STATUS REFRESH:            1-second polling while Status page visible
STATUS ACTIVE RUNTIME:     language/dwell/policy show active values, not startup
RUNTIME STATUS:            history/library/autostart displayed in Status page
AUTOSTART TOGGLE:          Windows only; macOS shows "지원 안 함" (unsupported)
LIBRARY BACKING TRUTH:     uses LibraryBacking enum, not Library handle existence
PLATFORM BOUNDARY:         autostart cfg-gated; platform-neutral tauri_app.rs clean
FIRST-RUN DISCOVERY:       Control Center opens on first run (PR #462)
SECOND-LAUNCH ACTIVATION:  existing instance's Control Center comes to foreground (PR #459)
```

Tray left-click opens Control Center. Right-click menu is product-oriented with
Korean labels. Library/History/Settings menu items open the corresponding sections.

## Local AI Readiness (PR #510, #518)

```
INITIAL LOAD:              loadAll() triggers refreshLocalAiStatus() after initial render
1-SECOND POLLING:          loadStatus() uses cached value, no get_local_ai_status invocation
ENDPOINT VS MODEL-LIST:    ModelListState enum distinguishes Unreachable/Error/Models
AUTO MODEL:                None is canonical Auto, Some("") accepted for compatibility
FRONTEND STATES:           NOT_LOADED / LOADED / ERROR with distinct rendering
ERROR DISPLAY:             "상태 확인 실패" on error, not indefinite "확인 중..."
REGRESSION TESTS:          scripts/check.sh verifies initial load and no-polling invariants
TAURI COMMANDS:            39 total (get_local_ai_status, list_models, etc.)
```

## Restart Status (PR #511, #520)

```
BACKEND-DERIVED:           SettingsService.restart_status() compares saved vs active baseline
REASON LABELS:             raw keys mapped to Korean labels (Quick 모델, Quick 엔드포인트, etc.)
CANONICAL FORM:            applySettingsToForm() synchronizes all fields after save
QUICK MODEL:               f-quick-model-select updated in applySettingsToForm()
```

## Next Session — Start Here

1. **Human Windows GUI product smoke** — Quick/Deep/Copy/Save lifecycle
2. **macOS work when environment returns**
3. **First-run discovery verification** — confirm Control Center opens on first run (PR #462, human verification pending)

Do not reopen UIA GetVisibleRanges research without new independent evidence.

## Git State

```
Content baseline:    query `git log -1` at session start
Branches:            clean
History:             rewritten twice (Co-authored-by removal). Current main is
                     canonical. No further rewrites.
Local pre-push hook: rejects direct main push, rejects non-fast-forward push
                     (active when core.hooksPath = .githooks)
Server branch protection: OFF (no GitHub ruleset)
```

## Constraints

- Agent investigation stays inside `PROJECT_ROOT/**`.
- `lens-core` remains platform-neutral (ADR-0003).
- Do not install external tools without explicit permission.
- Work in coherent branch -> PR -> merge units. No direct main pushes.
- macOS native code uses Objective-C bridges compiled by `cc` crate with `-fobjc-arc`.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
- `apps/lens` is not a workspace member; verified by `scripts/check.sh --full`.
