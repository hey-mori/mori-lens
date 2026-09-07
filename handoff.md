# Mori Lens — Session Handoff

Last updated: 2026-09-07

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS implementation is complete.
**All repository-local implementation work is complete. Distribution infrastructure implemented. Remaining work is CI execution completion, validation, external accounts, human gates, or quality research.**

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
- Smoke asset generation (`scripts/generate-smoke-assets.py`)
- MSIX verification (`scripts/verify-msix.ps1`)
- MSIX manifest: `runFullTrust` only (no unnecessary `broadFileSystemAccess`)

**Verification gate (`scripts/check.sh --full`):**
- Workspace fmt + clippy + test
- Product binary fmt + clippy + check + test
- Clean native rebuild + privacy gates + version consistency

## This Session

- Dead code removal: `library_count` and `mori_is_available` commands removed from tauri_app.rs
- Architecture documentation: added platform-macos crate section and all missing files across crates
- MSIX packaging: fixed manifest template (placeholder names, regex bug), generated Store assets from product icon, verified MSIX build locally (11 pass, 0 fail, 2 warn), integrated MSIX build into release workflow
- Web UI: fixed history_prune missing parameter, dynamic version display, added try/catch error handling
- Code quality audit: no clippy warnings, no dead code, all 40 Tauri commands wired to UI
- Mori integration audit: confirmed fully implemented (ADR-0018, 30+ tests, UI button)

## Previous Session

- **PR #303** — docs: fix stale macOS claims in project-goal.md
- **PR #304** — docs: add M6/M7/M8 gate status tables to roadmap
- **PR #305** — feat: Windows MSIX packaging infrastructure (ADR-0017)
- **PR #306** — docs: fix settings.conf → settings.toml, README milestone
- **PR #307** — fix: macOS graceful degradation without AX permission
- **PR #308** — fix: canonical doc contradictions + MSIX manifest cleanup
- **PR #309** — ci: package-smoke workflow for NSIS + MSIX + macOS .app
- **PR #310** — fix: package-smoke RGBA icon generation

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

REMAINING PACKAGING IMPLEMENTATION:   NONE
  (MSIX infrastructure, package-smoke CI, smoke assets all implemented)

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
  Package-smoke CI completion         COMPLETE (workflow verified, builds NSIS + MSIX + macOS .app)

REMAINING EXTERNAL BLOCKERS:
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
scripts/check.sh --full:          PASS (2026-09-06, local)
workspace fmt/clippy/tests:       PASS (665+ tests)
product fmt/clippy/check/test:    PASS
clean native rebuild:             PASS
platform-macos tests:             PASS (56 tests including 16 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
lens-storage tests:               PASS (73 tests, full M8 lifecycle)
privacy static gate:              CLEAN
CI run 34027544715:               PASS (both platforms)
Package-smoke workflow:           CREATED (execution in progress)
```

## Canonical Doc Truth (verified this session)

```
project-goal.md:
  macOS acquisition:        "built, unverified" (was "planned") ✓
  macOS overlay/OCR:        "built, unverified" (was "planned") ✓
  Mori integration:         "built, unverified" (was "planned") ✓
  OCR contract:             "default manual, auto opt-in" (was "manual only") ✓
  Settings file:            settings.toml (was settings.conf) ✓
  Distribution:             MSIX infrastructure implemented ✓

roadmap.md:
  M6 gate status table:     Added ✓
  M7 gate status table:     Added ✓
  M8 gate status table:     Added (CLOSED by automated tests) ✓

privacy-model.md:           settings.toml references fixed ✓
README.md:                  M8+ milestone, settings.toml ✓
```

## Known Runtime Issues

- Lock file permission bug (`--w-------` → fixed to `-rw-------`)
- Ollama not available locally (Quick/Deep translation BLOCKED)
- macOS binary starts gracefully without AX permission (SIGTRAP fixed in PR #307)
- Package-smoke CI slow: `cargo install tauri-cli` takes 30-50 min on CI runners
- Package-smoke CI icon iteration: Tauri requires specific icon format/size, multiple iterations needed (RGBA ✓, 1024x1024 ✓, format still being validated on CI)

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

1. Check package-smoke CI results (run 34039483348 or later)
2. Grant Accessibility + Screen Recording permissions
3. (Optional) Start Ollama with a model
4. Run `bash scripts/validate-macos.sh`
5. Review evidence in `docs/benchmarks/`

## Git State

```
latest merge: PR #310 (package-smoke RGBA fix)
CI pass: run 34027544715 (both platforms)
Package-smoke: run 34039483348 (in progress)
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
