# Mori Lens Capability and Evidence Matrix

**Date:** 2026-09-22
**Main SHA:** 9bba29143aa549adca05c1d1364b1070485cb008
**Purpose:** Canonical capability inventory with evidence classification

## Evidence classification

- **NOT IMPLEMENTED**: Code does not exist
- **SOURCE VERIFIED**: Code exists, reviewed, but no automated test
- **AUTOMATED CONTRACT PASS**: Automated test exists and passes
- **PLATFORM BUILD VERIFIED**: Compiles on target platform
- **RUNTIME VERIFIED**: Tested on real hardware with real applications
- **HUMAN VERIFIED**: Human operator confirmed correct behavior
- **EXTERNAL BLOCKED**: Requires external account/credential/hardware

## Windows capabilities

### Core acquisition

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| UI Automation text acquisition | RUNTIME VERIFIED | windows-app-matrix.md, 8 apps measured | p50 10ms, 86-100% correct-word rate |
| Word/sentence/paragraph segmentation | AUTOMATED CONTRACT PASS | lens-core tests | Unit tested, no human confirmation of intent |
| Secure-field refusal | RUNTIME VERIFIED | protected_harness.rs, real ES_PASSWORD control | Fires as Protected(Declared) |
| Hover intent state machine | AUTOMATED CONTRACT PASS | lifecycle.rs, hover.rs tests | Dwell, radius, cooldown, generation tracking |
| Manual OCR fallback | RUNTIME VERIFIED | 2026-08-06 VS Code measurement | 336 words recovered, 93 lines |
| Automatic OCR fallback | SOURCE VERIFIED | OcrTextAcquirer behind session routing | Unit tested, no real-world measurement |
| Per-application compatibility matrix | RUNTIME VERIFIED | windows-app-matrix.md | All 8 target apps measured 2026-08-05 |

### Overlay and UI

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Native overlay (Direct2D/DirectWrite) | RUNTIME VERIFIED | Multi-monitor, fullscreen measurements | Never takes focus, clicks pass through |
| Card themes | RUNTIME VERIFIED | 4 presets, contrast checked | White/black page composite |
| Per-pixel alpha rendering | RUNTIME VERIFIED | Direct2D implementation | Text opaque, corners round/antialiased |
| Tray icon | RUNTIME VERIFIED | Explorer restart survival | Pause on click, quit from menu |
| Control Center (Tauri webview) | RUNTIME VERIFIED | Status/Library/History/General sections | First-run discovery pending human verification |
| One instance at a time | RUNTIME VERIFIED | Named per-session mutex | Second copy says so and exits |
| No console window | AUTOMATED CONTRACT PASS | PE header test | Subsystem binary |

### Translation and analysis

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Quick translation via Ollama | RUNTIME VERIFIED | Warm median ~1.6s, streamed, cached | Model resolved at startup |
| Quick translation via LM Studio | AUTOMATED CONTRACT PASS | OpenAI-compatible HTTP | Contract tests pass |
| Deep analysis (grammar, structure) | RUNTIME VERIFIED | 2026-08-06 measurement | Schema-constrained, grounded, cached |
| Translation cache | AUTOMATED CONTRACT PASS | In-memory, keyed on model+prompt | Repeat hover served from cache |
| Target language routing | AUTOMATED CONTRACT PASS | Script detection, safe fallback | Latin-script returns Undecidable |

### Settings and configuration

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Settings file (TOML) | RUNTIME VERIFIED | %APPDATA%\Mori Lens\settings.toml | Versioned, round-trip preservation |
| Per-application tiers | RUNTIME VERIFIED | Settings file, password-manager floor | Deny before file read |
| Configurable shortcuts | RUNTIME VERIFIED | Control Center UI | Restart required |
| Start with Windows | RUNTIME VERIFIED | Per-user run key | Off unless turned on |
| Settings reload while running | RUNTIME VERIFIED | File watcher | No restart needed |
| Interface language (en/ko) | AUTOMATED CONTRACT PASS | 321 keys, 0 duplicates, 100% parity, key-reference validation | Dynamic re-render, P15 automated contracts |

### Library and history

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Saved vocabulary and sentences | RUNTIME VERIFIED | Ctrl+Alt+S, library.db + saved.md | SQLite, search, filters, notes, tags |
| Session-recent tier | AUTOMATED CONTRACT PASS | Memory only, 50 cards/2 hours | Gone on exit |
| Persistent history | SOURCE VERIFIED | Opt-in, capped, app-exclusion | Off by default |
| Library export (MD + JSON + JSONL) | AUTOMATED CONTRACT PASS | Round-trip tests | Import preserves repeated entries |
| Review list (Known/Later/Archive) | RUNTIME VERIFIED | Manual categorization | Undo supported |

### Privacy and security

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Local-only inference | RUNTIME VERIFIED | Loopback-only endpoint validation | Remote address refused |
| Nothing captured written to disk | AUTOMATED CONTRACT PASS | No file writes in capture path | OCR crop in memory only |
| Password field refusal | RUNTIME VERIFIED | IsPassword check before read | Unreadable = unprovable = secure |
| Clipboard write on keypress only | RUNTIME VERIFIED | Ctrl+Alt+C, never read | Card content only |
| Pause stops pointer sampling | RUNTIME VERIFIED | Ctrl+Alt+L or tray | No accessibility call when paused |
| Mori integration | SOURCE VERIFIED | Capability discovery, version negotiation | Explicit user action, no hidden sync |

### Diagnostics and health

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Health check (Quick/Deep) | AUTOMATED CONTRACT PASS | Provider reachability, model state | Typed failure classification |
| Diagnostic event ring | AUTOMATED CONTRACT PASS | Privacy-safe buffer | Content-free |
| Diagnostics export | RUNTIME VERIFIED | JSON export from Control Center | No captured text |
| Self-test mode | RUNTIME VERIFIED | --selftest flag | End-to-end chain verification |

### Packaging and distribution

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| NSIS installer | PLATFORM BUILD VERIFIED | scripts/package.sh | Per-user, no elevation |
| MSIX packaging | PLATFORM BUILD VERIFIED | package-msix.ps1 | Store-ready manifest (placeholder publisher) |
| Code signing | EXTERNAL BLOCKED | ADR-0017 | Needs Store account or self-signed cert |
| Microsoft Store distribution | EXTERNAL BLOCKED | Manifest prepared | Needs Store developer account |

## macOS capabilities

### Core acquisition

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| AX text acquisition | PLATFORM BUILD VERIFIED | MacTextAcquirer, AXUIElement | Compiles, no runtime verification |
| Vision OCR | PLATFORM BUILD VERIFIED | per-word geometry, VNRectangleObservation | Compiles, no runtime verification |
| ScreenCaptureKit crop | PLATFORM BUILD VERIFIED | Per-display selection, overlay exclusion | Compiles, no runtime verification |
| Word/sentence/paragraph segmentation | AUTOMATED CONTRACT PASS | Shared lens-core | Same as Windows |
| Secure-field refusal | SOURCE VERIFIED | AX password detection | No runtime verification |
| Hover intent state machine | SOURCE VERIFIED | Shared lifecycle.rs | No macOS-specific runtime test |
| Manual OCR fallback | PLATFORM BUILD VERIFIED | Vision framework integration | Compiles, no runtime verification |
| Automatic OCR fallback | SOURCE VERIFIED | OcrTextAcquirer routing | No runtime verification |

### Overlay and UI

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Native overlay (NSPanel) | PLATFORM BUILD VERIFIED | Transparent, click-through | Compiles, no runtime verification |
| Menu bar icon | PLATFORM BUILD VERIFIED | NSStatusItem | Compiles, no runtime verification |
| Control Center (Tauri webview) | PLATFORM BUILD VERIFIED | Same UI as Windows | Compiles, no runtime verification |
| One instance at a time | SOURCE VERIFIED | Single-instance enforcement | No runtime verification |

### Translation and analysis

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Quick translation via Ollama | SOURCE VERIFIED | Same as Windows | No macOS-specific runtime test |
| Quick translation via LM Studio | SOURCE VERIFIED | Same as Windows | No macOS-specific runtime test |
| Deep analysis | SOURCE VERIFIED | AnalysisCache implemented | No runtime verification |
| Translation cache | AUTOMATED CONTRACT PASS | Shared lens-core | Same as Windows |

### Settings and configuration

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Settings file (TOML) | SOURCE VERIFIED | Shared lens-settings | No macOS-specific runtime test |
| Per-application tiers | SOURCE VERIFIED | Shared policy.rs | No runtime verification |
| Configurable shortcuts | PLATFORM BUILD VERIFIED | Global hotkey registration | Compiles, no runtime verification |
| Interface language (en/ko) | PLATFORM BUILD VERIFIED | Same UI as Windows | Compiles, no runtime verification |

### Library and history

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Saved vocabulary and sentences | SOURCE VERIFIED | Shared library_service.rs | No runtime verification |
| Session-recent tier | AUTOMATED CONTRACT PASS | Shared lens-core | Same as Windows |
| Persistent history | SOURCE VERIFIED | Shared lens-storage | No runtime verification |
| Library export | AUTOMATED CONTRACT PASS | Shared implementation | Same as Windows |
| Review list | SOURCE VERIFIED | Shared UI | No runtime verification |

### Privacy and security

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Local-only inference | AUTOMATED CONTRACT PASS | Shared endpoint validation | Same as Windows |
| Nothing captured written to disk | AUTOMATED CONTRACT PASS | Shared capture path | Same as Windows |
| Password field refusal | SOURCE VERIFIED | AX check | No runtime verification |
| Clipboard write on keypress only | PLATFORM BUILD VERIFIED | NSPasteboard integration | Compiles, no runtime verification |
| Pause stops pointer sampling | SOURCE VERIFIED | Shared pause.rs | No runtime verification |
| Mori integration | SOURCE VERIFIED | Shared mori_transfer.rs | No runtime verification |

### Permissions and lifecycle

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Accessibility permission check | PLATFORM BUILD VERIFIED | AXIsProcessTrustedWithOptions | Compiles, no runtime verification |
| Screen Recording permission check | PLATFORM BUILD VERIFIED | CGPreflightScreenCaptureAccess | Compiles, no runtime verification |
| Permission lifecycle (grant/revoke) | SOURCE VERIFIED | Permission state transitions | No runtime verification |
| Retina scaling | SOURCE VERIFIED | Coordinate conversion | No runtime verification |
| Multiple displays / Spaces | SOURCE VERIFIED | Per-display capture selection | No runtime verification |

### Packaging and distribution

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| .app bundle | PLATFORM BUILD VERIFIED | verify-app-bundle.sh | Compiles, structure verified |
| Developer ID signing | EXTERNAL BLOCKED | ADR-0017 | Needs Apple Developer credentials |
| Notarization | EXTERNAL BLOCKED | ADR-0017 | Needs Apple Developer credentials |
| Hardened runtime | SOURCE VERIFIED | Entitlements prepared | No runtime verification |

## Cross-platform capabilities

### Provider architecture

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Two-speed runtime (Quick/Deep) | AUTOMATED CONTRACT PASS | ADR-0005, separate translators | Independent models/endpoints |
| Provider capability model | NOT IMPLEMENTED | — | P20 deliverable |
| Model discovery | AUTOMATED CONTRACT PASS | Ollama/LM Studio runtime | Health checks, state tracking |
| Provider switching | SOURCE VERIFIED | Control Center UI | Restart required for some changes |

### Testing infrastructure

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| Rust unit tests | AUTOMATED CONTRACT PASS | cargo test, 216 tests | lens-core, lens-translate, lens-settings |
| Contract tests | AUTOMATED CONTRACT PASS | lens-core contract_tests.rs | Acquisition contract |
| Stress tests | AUTOMATED CONTRACT PASS | lens-core stress_tests.rs | Concurrent acquisition |
| HTTP contract tests | AUTOMATED CONTRACT PASS | lens-translate fake_server.rs | Provider API simulation |
| Live integration tests | AUTOMATED CONTRACT PASS | lens-translate live_tests.rs | Requires running provider |
| Hover probe | RUNTIME VERIFIED | apps/hover-probe | Measurement instrument |
| Frontend i18n tests | AUTOMATED CONTRACT PASS | i18n.test.js | 11 tests, custom framework |
| Frontend behavioral tests | NOT IMPLEMENTED | — | P19 deliverable |
| Tauri E2E tests | NOT IMPLEMENTED | — | P19 deliverable |

### Quality gates

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| cargo fmt | AUTOMATED CONTRACT PASS | scripts/check.sh | Enforced |
| cargo clippy -D warnings | AUTOMATED CONTRACT PASS | scripts/check.sh | Enforced |
| cargo test | AUTOMATED CONTRACT PASS | scripts/check.sh | 216 tests |
| Module graph reachability | AUTOMATED CONTRACT PASS | check_module_graph.py | No orphans, no cycles |
| i18n key integrity | AUTOMATED CONTRACT PASS | i18n-key-checker.js | 0 duplicates, 100% parity |
| Privacy static gates | AUTOMATED CONTRACT PASS | scripts/check.sh --full | No captured text in logs |
| Trailing backslash check | AUTOMATED CONTRACT PASS | scripts/check.sh | rustfmt string continuation |
| Commit message format | AUTOMATED CONTRACT PASS | validate-commit-message.sh | Enforced by hook |

### CI/CD

| Capability | Status | Evidence | Notes |
|------------|--------|----------|-------|
| CI workflow configured | SOURCE VERIFIED | .github/workflows/ci.yml | Push to main and pull requests, hosted runners |
| Package smoke workflow configured | SOURCE VERIFIED | .github/workflows/package-smoke.yml | Manual-only trigger |
| Release workflow configured | SOURCE VERIFIED | .github/workflows/release.yml | Tag-triggered |
| CI run on current main | NOT RUN ON CURRENT MAIN | — | Historical success on old SHA (0cf3c47, 2026-09-06) |
| Package smoke on current main | NOT RUN ON CURRENT MAIN | — | Historical failures on old SHAs (e6337d8, 2026-09-07) |

## Human-only evidence gates

| Gate | Status | Notes |
|------|--------|-------|
| P13 Quick/Deep quality review | NOT STARTED | Human-only, cannot be automated |
| #140 unzip-to-first-card measurement | NOT STARTED | Human operator required |
| Screen reader runtime verification | NOT STARTED | Requires assistive technology user |
| Windows OCR real-world validation | NOT STARTED | Requires human operator with target apps |
| macOS runtime validation campaign | NOT STARTED | Requires macOS hardware + operator |
| Clean-machine installation test | NOT STARTED | Requires human operator |

## External blockers

| Blocker | Affects | Notes |
|---------|---------|-------|
| GitHub Actions billing | CI/package-smoke on current main | Cannot run workflows |
| Microsoft Store developer account | MSIX signing, Store distribution | External credential |
| Apple Developer ID | macOS signing, notarization | External credential |
| Code signing certificates | Windows/macOS distribution | External credential |
| macOS hardware | macOS runtime validation | External hardware |
| KakaoTalk account | KakaoTalk compatibility testing | External account |
| Real screen reader user | Accessibility runtime verification | External human |

## Summary statistics

- **Total capabilities tracked:** 110
- **AUTOMATED CONTRACT PASS:** 34
- **RUNTIME VERIFIED:** 26
- **SOURCE VERIFIED:** 26
- **PLATFORM BUILD VERIFIED:** 15
- **NOT IMPLEMENTED:** 3
- **EXTERNAL BLOCKED:** 4
- **NOT RUN ON CURRENT MAIN:** 2
- **HUMAN VERIFIED:** 0
- **Human-only gates:** 6
- **External blockers:** 7

## Evidence gaps by priority

### Critical (blocks distribution)

1. Code signing (Windows/macOS) - EXTERNAL BLOCKED
2. Store/Developer accounts - EXTERNAL BLOCKED
3. CI/package-smoke on current main - NOT RUN ON CURRENT MAIN

### High (blocks product readiness)

1. macOS runtime validation - NOT STARTED
2. Windows OCR real-world validation - NOT STARTED
3. First-card user measurement (#140) - NOT STARTED

### Medium (blocks maintainability)

1. Frontend behavioral tests - NOT IMPLEMENTED
2. Tauri E2E tests - NOT IMPLEMENTED
3. Performance regression framework - NOT STARTED

### Low (nice to have)

1. Screen reader verification - NOT STARTED
2. Clean-machine installation test - NOT STARTED

## Next actions

1. **P18**: Complete this matrix, reconcile stale docs, correct CI terminology
2. **P19**: Implement frontend behavioral tests (Vitest + jsdom)
3. **P19**: Implement Tauri E2E tests (if supported)
4. **P20**: Build provider readiness model, first-card setup experience
5. **P21**: Compatibility model in Control Center, OCR UX improvements
6. **P22**: macOS testability (pure seam extraction), runtime validation
7. **P23**: Windows orchestration decomposition (win.rs main())
8. **P24**: Fault recovery tests, security audit
9. **P25**: Distribution readiness (when external blockers resolved)
10. **P26**: Performance budget, deterministic regression tests
11. **P27**: Library/Review audit, schema migration tests
12. **P28**: Documentation reconciliation, handoff.md update

## CI/CD evidence status

**Date:** 2026-09-22
**Audited against SHA:** 9bba29143aa549adca05c1d1364b1070485cb008

### Workflow configuration

| Workflow | Trigger | Status |
|----------|---------|--------|
| ci.yml | push to main, pull_request, workflow_dispatch | CONFIGURED, AUTOMATIC |
| package-smoke.yml | workflow_dispatch (manual) | CONFIGURED, MANUAL-ONLY |
| release.yml | tag push (v*.*.*) | CONFIGURED, TAG-TRIGGERED |

### Current main verification status

- **CI (current main):** NOT REMOTELY VERIFIED
- **Package smoke (current main):** NOT REMOTELY VERIFIED
- **Release (current main):** NOT REMOTELY VERIFIED (no tags yet)

### Historical evidence

- **CI success:** SHA 0cf3c470408009c69ce24399a0e113474c286fc5, 2026-09-06, conclusion: success
- **Package smoke failures:** SHA e6337d82fcb9326ae06dc56ed44818a6d7f091c4, 2026-09-07, Windows packages failure, macOS .app bundle failure

**Note:** Historical evidence is from older SHAs, not current main. Current main CI/package-smoke status is NOT REMOTELY VERIFIED.


## Open issues and PRs inventory

**Date:** 2026-09-22
**Total open issues:** 7
**Total open PRs:** 1

### Active roadmap

- **#622** Post-v1 product hardening and next-roadmap program (P18-P28)
- **#527** Post-v1 product evolution (P1-P17 tracker, all COMPLETE except P13)

### Active PRs

- **#623** P18: Product completeness audit and capability matrix

### Human evidence gates

- **#610** P13: Quick/Deep quality benchmark continuation (human review pending)
- **#140** docs: measure unzip to first card on a machine with neither Ollama nor a model

### Deferred by decision

- **#533** Defer axe-core integration until behavioral testing infrastructure exists

### Product limitations (compatibility)

- **#14** VS Code editor text containers are too coarse for point resolution
- **#34** KakaoTalk is one text region, so Lens cannot tell which message you are on

### Issue classification

All 7 open issues fall into one of:
- ACTIVE ROADMAP: #622, #527
- HUMAN EVIDENCE: #610, #140
- DEFERRED BY DECISION: #533
- PRODUCT LIMITATION: #14, #34

1 open PR: #623 (P18 product completeness audit)

No stale issues identified. No issues recommended for closure.

