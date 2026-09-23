# Post-v1 product hardening and next-roadmap program

**Started:** 2026-09-22
**Previous roadmap:** P1–P17 (P1–P12 COMPLETE, P13 IN PROGRESS human-only, P14–P17 COMPLETE)
**Main at start:** de85537296d778d83ac9e645b5114d1b4c61fc79

## Objective

Turn Mori Lens from a highly developed implementation into a deeply verified, maintainable, resilient, distributable product.

Not a feature-count exercise. Make the core product sentence work better:
> Rest the pointer on a sentence. A card appears with the translation.

Preserve the eight product invariants (CLAUDE.md).

## P13 remains deferred

P13 human quality review is a human-only evidence gate. Do not fill human-review scores. Do not use an LLM as a replacement human reviewer. Continue all other independently actionable engineering work.

## Priority order

1. **P18** Product completeness audit + evidence reconciliation
2. **P19** Control Center behavioral/E2E test foundation
3. **P20** First-card/provider readiness and setup experience
4. **P21** Compatibility + OCR fallback productization
5. **P22** macOS testability, parity, and runtime readiness
6. **P23** Platform orchestration maintainability
7. **P24** Reliability / fault recovery / security hardening
8. **P25** Distribution / signing / update readiness
9. **P26** Performance and resource regression framework
10. **P27** Library / Review long-term usefulness
11. **P28** Final documentation and product-readiness reconciliation

## P18 — Product completeness audit + evidence reconciliation

**Status:** COMPLETE

### Goals

- Build canonical capability/evidence matrix
- Reconcile stale historical documents (gap-analysis-2026-09-09.md, handoff.md, what-finished-means.md)
- Correct CI evidence terminology (CONFIGURED MANUAL-ONLY, not "NO RUN")
- Verify current repository state against historical claims

### Deliverables

- [x] Capability matrix with evidence classification per capability (docs/capability-evidence-matrix.md)
- [x] Stale document reconciliation (mark HISTORICAL, link to current truth)
- [x] CI/package-smoke evidence status corrected
- [x] Open issues/PRs inventoried and reconciled

### Evidence classification

```
NOT IMPLEMENTED
SOURCE VERIFIED
AUTOMATED CONTRACT PASS
PLATFORM BUILD VERIFIED
RUNTIME VERIFIED
HUMAN VERIFIED
EXTERNAL BLOCKED
```

## P19 — Control Center behavioral/E2E test foundation

**Status:** COMPLETE (P19A behavioral tests, P19B Tauri E2E, P19C axe-core accessibility automation all merged)

### Goals

Build fast frontend behavioral layer and real Tauri E2E infrastructure.

### Behavioral tests (Vitest + jsdom or equivalent)

Test actual user workflows, not implementation trivia:
- Navigate every Control Center section
- Switch interface language, persist locale
- Load/edit/save settings
- Display restart-required truth
- Switch Quick provider, render models
- Provider unavailable state
- Library search, filters, detail, notes/tags
- Review Known/Later/Archive, undo
- History actions
- Health check rendering
- Diagnostics rendering
- Per-app policy editing
- Shortcut editing
- Danger-zone flow

### Real Tauri E2E (WebDriverIO/Tauri)

Start narrowly:
- Launch Control Center, window appears
- Navigate UI, change interface locale
- Save a setting, reload/reopen, verify persistence
- Open Library, exercise safe read-only commands
- Close/reopen main window

### Accessibility (axe-core)

Integrate automated accessibility rules engine in behavioral suite.
Label as AUTOMATED ACCESSIBILITY CHECK, not FULL WCAG CERTIFICATION.
Preserve SCREEN READER RUNTIME VERIFIED = NO until real screen-reader run.

### Deliverables

- [ ] Test infrastructure chosen and installed
- [ ] First behavioral test passing (navigate all sections)
- [ ] Locale persistence test
- [ ] Settings save/load test
- [ ] Library search test
- [ ] Tauri E2E infrastructure (if supported)
- [ ] axe-core integration (if appropriate)

## P20 — First-card/provider readiness and setup experience

**Status:** IN PROGRESS (P20A domain separation: health vs readiness, provider capability model, model management types)

### Goals

Build explicit local-AI readiness model. Control Center should understand:
- runtime not installed
- runtime installed but stopped
- endpoint unreachable
- provider reachable
- no compatible model
- model downloading
- model present
- model loading
- model ready
- Quick ready
- Deep not ready
- fully ready

Do not collapse into generic "not connected".

### Provider capability model

Small typed capability descriptor:
- can_quick
- can_deep
- can_list_models
- can_download_model
- can_load_model
- can_unload_model
- requires_auth
- supports_progress

Quick: Ollama, LM Studio. Deep: Ollama only.

### Explicit user-initiated model setup

Investigate official provider APIs:
- Ollama: local model-pull with streaming progress
- LM Studio: native model-management API

If stable enough, provide explicit Control Center action:
"Download recommended Quick model"

Hard rules:
- Never download automatically because hover failed
- Never initiate downloads silently
- User explicitly starts, show progress, show cancel/failure
- No captured screen text involved
- Do not hit cloud endpoints directly if provider owns download
- Explain disk cost where known
- Do not pretend provider/runtime is installed if it is not

### First-run lesson integration

Current staged lesson remains. But do not teach hover gesture before inference path is realistically ready.

Design coherent first-run state machine:
1. Control Center opens
2. Local AI readiness evaluated
3. Required setup explained
4. Provider/model becomes ready
5. Staged first-hover lesson
6. First real card

Do not add long wizard. Guarantee comprehension with minimum ceremony.

### Deliverables

- [ ] Readiness state model implemented
- [ ] Control Center surfaces readiness state
- [ ] Provider capability descriptor (typed, narrow)
- [ ] Explicit model download action (if provider APIs stable)
- [ ] First-run lesson gated on readiness
- [ ] #140 measurement hooks prepared (timestamp, setup-state visibility, reproducible instructions)

## P21 — Compatibility + OCR fallback productization

**Status:** NOT STARTED

### Goals

Compatibility should become product-visible. When Lens cannot point-resolve text, silence looks like broken product.

### Compatibility model in Control Center

For known application, expose:
- Accessibility: good
- Accessibility: coarse / unreliable
- Manual OCR recommended
- Automatic OCR available
- OCR unavailable
- Protected / denied
- Not yet measured

Do not promise support merely because fallback exists.

### Per-app OCR UX

For applications with repeated Unsupported/coarse behavior, explicit user-facing suggestion:
"This app does not expose text reliably. Use Manual OCR, or enable Automatic OCR for this app."

Never silently enable automatic OCR. Never downgrade Deny policy.

### Automatic OCR productization

Benchmarks show fresh automatic OCR can be expensive (~0.4s sparse, ~1.4s text-heavy).

Therefore:
- Keep default off/manual
- Preserve negative-memory/caching protections
- Measure actual moving-pointer usage
- Audit capture frequency
- Surface capability/permission/language problems clearly
- Avoid capture storms

Do not optimize benchmark numbers by reducing safety.

### Windows OCR runtime verification

Prepare and execute runtime validation:
- VS Code editor
- KakaoTalk if available
- Real PDF
- Browser content
- High-DPI monitor
- Multi-monitor where available

Measure:
- Whether fallback triggers
- Correct target
- Capture count
- OCR latency
- Unwanted card rate
- Repeated-region suppression
- Card self-capture prevention

### Deliverables

- [ ] Compatibility model in Control Center
- [ ] Per-app OCR suggestion UX
- [ ] Automatic OCR capture audit
- [ ] Windows OCR runtime validation (requires human operator)

## P22 — macOS testability, parity, and runtime readiness

**Status:** NOT STARTED

### Goals

macOS is the largest evidence debt. Substantial implementation exists but runtime evidence much weaker than Windows.

### Improve testability before more features

Do NOT force macOS to use Windows architecture just for symmetry.

Current macos.rs ~1900 lines, product_loop() ~1246 lines, very little direct unit-test coverage at orchestration level.

Extract platform-local pure seams:
- ReadingContext
- dispatch context
- Quick result acceptance
- Deep result acceptance
- generation/stale-result decision
- modifier/scope decision
- presentation context
- save/copy provenance context
- permission state transitions

Goal: testability and reasoning, not abstract beauty.

### macOS runtime campaign

When macOS hardware/runtime available, verify:
- Accessibility permission grant/revoke
- AX point acquisition
- Modifier tier, hover_scope
- Pause stops pointer sampling
- Stale-result discard
- Quick translation, Deep analysis, Deep cache
- Overlay placement, no-focus/click-through
- Retina scaling, multiple displays, Spaces
- Manual Vision OCR, automatic Vision OCR
- Screen Recording permission lifecycle
- Shortcut registration
- Control Center, history/recent
- Single-instance behavior

Record actual hardware and OS version. Do not call source parity runtime parity.

### Permission lifecycle

macOS permissions can change while app running. Audit:
- Accessibility granted after launch
- Revoked during session
- Screen Recording granted after launch
- Revoked during session

Avoid forcing restart where OS allows recovery. If restart truly necessary, explain accurately.

### Deliverables

- [ ] Pure seams extracted from macos.rs
- [ ] Unit tests for extracted seams
- [ ] macOS runtime validation campaign (requires macOS hardware)
- [ ] Permission lifecycle audit

## P23 — Platform orchestration maintainability

**Status:** NOT STARTED

### Goals

Windows orchestration also needs maintainability attention.

Current win.rs ~5223 lines, main() ~2661 lines. Much stronger test coverage than macOS, but still maintenance hotspot.

### Decomposition

Audit whether main() can be decomposed by responsibility without changing semantics:
- startup/configuration
- runtime/provider construction
- hover dispatch
- result processing
- hotkey actions
- presentation
- settings refresh
- diagnostics
- shutdown

Do not make one giant refactor. Each extraction must:
- Preserve behavior
- Have focused regression tests
- Reduce real reasoning burden

### Shared orchestration

Do not blindly expand shared_orchestration.rs. P16 deliberately extracted only one clearly shared business seam: record_presented_reading().

Before sharing more code between Windows/macOS, prove semantic identity. Platform-local decomposition preferable to premature cross-platform abstraction.

### Deliverables

- [ ] win.rs main() decomposition (incremental, tested)
- [ ] Regression tests for each extracted seam
- [ ] Shared orchestration audit (what truly is shared)

## P24 — Reliability / fault recovery / security hardening

**Status:** NOT STARTED

### Goals

Test adverse runtime states that mature desktop product must survive.

### Fault recovery

Audit:
- Ollama stops mid-session
- LM Studio stops mid-session
- Model unloads while Lens running
- Malformed provider response
- Provider hangs
- Settings file becomes invalid
- Settings file changes during read
- Library DB locked
- WAL recovery
- Disk full/write failure
- Export failure
- Mori connector disappears during transfer
- Display topology changes
- Explorer restarts
- Sleep/resume
- Monitor disconnect/reconnect
- Rapid pause/resume
- Rapid provider setting changes
- Application exits while job pending

Use deterministic fakes/contract tests wherever possible. Use runtime tests only for genuinely OS-dependent behavior.

### Privacy during fault handling

Failure diagnostics must remain content-free. Never add:
- Captured sentence to logs
- OCR pixels to disk
- Prompt body to diagnostics
- Model response body to telemetry
- Application path where hashed identity suffices

Keep existing event-ring privacy design.

### Security hardening

Fresh threat-model audit after Control Center expansion. Review:
- Tauri capabilities, command allowlists
- Detail vs main window permissions
- CSP, unsafe-inline necessity
- File paths, symlink handling
- Atomic writes, temporary files
- Library imports, JSONL imports
- Mori capability marker trust
- Mori transfer destination
- Model endpoints, localhost validation
- Diagnostics exports
- Clipboard boundaries

Do not weaken localhost/provider constraints merely for convenience.

### Endpoint classification

Add adversarial endpoint cases:
- 127.0.0.1.evil.example
- 127.0.0.999
- localhost.evil
- [::1]
- mixed-case localhost
- userinfo tricks
- whitespace
- scheme/path confusion

Goal: prove screen text cannot be redirected remotely through malformed configuration. Use one canonical endpoint validation path.

### Dependency/security automation

Evaluate lightweight supply-chain checks:
- cargo audit
- cargo deny
- Dependency license/vulnerability reporting
- npm/package lock audit if frontend dependencies introduced

Do not add tools solely for badges. If added, integrate into existing canonical check path where stable.

### Deliverables

- [ ] Fault recovery contract tests
- [ ] Security threat-model audit
- [ ] Adversarial endpoint tests
- [ ] Dependency audit integration (if justified)

## P25 — Distribution / signing / update readiness

**Status:** NOT STARTED

### Goals

Distribution infrastructure exists but current main does not have fresh remote package evidence.

### Current-main CI/package verification

If GitHub Actions billing/runners permit, run:
- ci.yml
- package-smoke.yml

If cannot start because of account/billing constraints, record:
```
CONFIGURED
NOT RUN ON CURRENT MAIN
EXTERNAL/ACCOUNT BLOCKER
```

Continue other engineering. Do not stop program.

### Windows distribution

Audit actual intended primary route. For Microsoft Store MSIX:
- Replace placeholder publisher identity when credentials/account exist
- Validate Store-ready manifest
- Preserve per-user behavior/data
- Test install, uninstall, upgrade, downgrade/refusal policy
- Test settings/library preservation

Do not require unnecessary standalone certificate if Store distribution path does not. Keep NSIS as appropriate secondary/development channel if desired.

### macOS distribution

Prepare for real external distribution:
- Developer ID signing
- Hardened runtime
- Required entitlements
- Notarization, stapling
- Gatekeeper verification

Do not fake signed/notarized status. Signing credentials remain external secrets. Build deterministic verification so credentials can be inserted later without redesigning release flow.

### Auto-update

Tauri update signatures are security boundary. Do not ship updater before:
- Signing key decision
- Secure key storage
- Release endpoint
- Update manifest format
- Signature verification
- Rollback/failure UX

Engineering that can be done now:
- Manifest generation
- Signature-verification test fixtures
- Version comparison tests
- Interrupted-update UX/state design

Do not create insecure unsigned updater.

### Clean-machine installation tests

Prepare reproducible smoke plans:
- Clean Windows machine
- Clean macOS user profile
- Upgrade from previous version
- Uninstall, reinstall
- Settings preservation, library preservation
- Autostart state
- First-run Control Center
- Provider missing, model missing

Human execution can happen later. Automate structural portions now.

### Deliverables

- [ ] CI/package-smoke run on current main (if possible)
- [ ] Windows Store manifest audit
- [ ] macOS signing/notarization preparation
- [ ] Auto-update infrastructure (manifest, signature verification tests)
- [ ] Clean-machine installation smoke plans

## P26 — Performance and resource regression framework

**Status:** NOT STARTED

### Goals

Repository has excellent individual benchmark history but lacks one coherent current performance view.

### Product performance budget

Create performance budget document with evidence:
- idle CPU
- idle memory
- startup to tray
- startup to Control Center
- settled pointer to acquisition
- acquisition to waiting card
- waiting card to Quick result
- Quick warm/cold
- Deep warm/cold
- manual OCR
- automatic OCR
- settings reload
- Library search small/large
- Control Center idle resource use

Separate:
- goal/budget
- current measurement
- hard regression threshold
- informational only

Avoid flaky machine-dependent CI failures.

### Deterministic performance regression tests

Good hard gates:
- Algorithmic complexity tests
- Query benchmark orders of magnitude
- Bounded memory structures
- No unbounded queues
- No accidental polling
- Cache size bounds

Poor hard gates:
- Exact wall-clock latency on shared CI runner

Use measurement rather than fake precision.

### Deliverables

- [ ] Performance budget document
- [ ] Deterministic regression tests for algorithmic properties
- [ ] Current measurement baseline

## P27 — Library / Review long-term usefulness

**Status:** NOT STARTED

### Goals

Re-evaluate Library/Review as the return loop. what-finished-means.md says "Nothing brings a reader back" but that predates substantial Library/Review work.

### Audit what now exists

- Saved items, notes, tags
- Search, filters
- Review: Known/Later/Archive, undo
- History, Recent

Ask what genuine user need remains. Do not add streaks, gamification, notifications, or engagement mechanics merely to improve retention.

### Possible maturity work

Evidence may justify:
- Backup/restore confidence
- Schema migration property tests
- Large-library behavior
- Duplicate handling
- Import conflict reporting
- Review queue semantics
- Review progress visibility
- Saved-item provenance clarity
- Export round-trip guarantees

Only implement what improves reading continuity.

### Tray remains intentionally quiet

Do not turn app into notification generator. If future review reminders considered, require explicit product decision and opt-in. Current invariant of quiet persistent tray remains default.

### Deliverables

- [ ] Library/Review audit (what exists, what's missing)
- [ ] Schema migration property tests
- [ ] Export round-trip guarantees
- [ ] Large-library behavior validation

## P28 — Final documentation and product-readiness reconciliation

**Status:** NOT STARTED

### Goals

Documentation must become current-state navigation. Mori Lens has large documentation corpus. Asset only if current and historical documents distinguishable.

### Convention

Introduce clear convention:
- CURRENT
- HISTORICAL MEASUREMENT
- SUPERSEDED
- OPEN RESEARCH

Do not rewrite benchmark history.

### mori-lens-project-goal.md

Remains canonical. It is only file allowed to claim capabilities. Honor that. After each runtime-validation milestone, update capability status only when evidence warrants:
- built, unverified
- runtime verified
- shipped

### what-finished-means.md

Update after auditing current Library/Review and first-run implementation. Preserve important product philosophy. Remove claims that became false after later implementation. Historical insight can remain explicitly dated.

### handoff.md

Make useful for new engineer today:
- Current main
- Current architecture
- True open product risks
- Human-only blockers
- External credential blockers
- Current test commands
- Current runtime evidence
- Next roadmap issue

Do not make another enormous history dump. Link to historical reports instead.

### Deliverables

- [ ] Documentation convention established
- [ ] mori-lens-project-goal.md updated with runtime evidence
- [ ] what-finished-means.md reconciled
- [ ] handoff.md made current

## Extensibility principles

Avoid premature frameworks. Current good extension seams:
- Translator
- Analyser
- provider runtime
- TextAcquirer
- settings schema
- per-app policy
- storage services

Audit for actual pressure. Do not create:
- Generic provider plugin SDK
- Generic OCR plugin ecosystem
- Scripting engine
- Remote plugin marketplace

unless concrete third implementation proves abstraction necessary.

Provider capability model is exception: small capability descriptor justified if setup UI needs to ask can_download/can_list_models/can_load/can_serve_deep. Keep narrow and typed.

Platform-local modularity before cross-platform abstraction. Share only proven business logic.

## Test architecture

Every extracted pure seam should gain tests for actual bugs/invariants:
- generation
- stale result discard
- current reading identity
- target language captured at dispatch
- provider/model provenance
- save/copy provenance
- pause
- modifier
- scope
- OCR policy
- history recording

Tests must exercise same seam production uses. Avoid tests that reconstruct expected result manually beside production code.

## Verification gates

Use:
- git diff --check
- scripts/check.sh
- scripts/check.sh --full

Plus focused phase-specific tests. After frontend behavioral infrastructure exists, include canonical test command in main gate if stable. Do not make expensive live-model benchmarks run on every commit.

## GitHub Actions evidence

Always distinguish:
- workflow configured
- workflow triggered
- job started
- job completed
- job passed
- SHA tested

Do not call old green run evidence for current main.

## Merge discipline

Follow repository rules. Before merge verify:
- Exact PR head reviewed
- Changed files
- Commit messages
- Tests
- Attribution
- Tracker truth

No prohibited AI attribution. Preserve intended merge history. Delete completed source branches.

## Documentation accompanies behavior

For every material product change, update only relevant docs. Do not write future functionality as current functionality. Use:
- IMPLEMENTED
- SOURCE VERIFIED
- AUTOMATED VERIFIED
- RUNTIME VERIFIED
- HUMAN VERIFIED
- EXTERNAL BLOCKED

consistently.

## External references

Verify before implementation using current official documentation:
- Tauri testing / WebDriver
- Tauri updater signing
- Tauri capability security
- Ollama model management
- LM Studio native model-management API
- Apple Developer ID / notarization / hardened runtime
- Microsoft Store MSIX requirements

Do not cargo-cult examples from unofficial blog posts.

## Competitive references

Current translation tools commonly use:
- select to shortcut
- region to OCR
- clipboard to translate

Mori Lens distinction:
- keep reading
- rest pointer
- understand sentence
- continue reading

Protect that. Any new UX should reduce friction around this interaction, not replace with more ordinary translation workflow.

## Things NOT to do

Do not:
- Start cloud inference
- Add telemetry
- Add accounts
- Add teams
- Create browser extension
- Listen to clipboard continuously
- OCR full screen continuously
- Poll pointer while paused
- Auto-download models because hover failed
- Make automatic OCR default globally
- Add generic plugins without evidence
- Chase arbitrary feature count
- Make UI visually complex merely to appear polished
- Close human/runtime issues with source evidence

## Success criteria

Ultimately answer:
- Can a new user install Mori Lens?
- Can they understand what to do?
- Can the app explain what is missing?
- Can they get a model ready without terminal archaeology?
- Does the core gesture work where the platform allows it?
- When it cannot, does the fallback make sense?
- Does it fail safely?
- Does it recover?
- Can it update?
- Can it preserve the user's library/settings?
- Can Windows and macOS behavior be explained and tested?
- Can a maintainer change it without reading a 2600-line main function?
- Can every product claim point to the right kind of evidence?

That is closer to "finished" than a green feature checklist.

## Running checkpoint report

After every meaningful PR, report:
- main before
- issue
- PR
- reviewed head
- merge commit
- main after
- what product gap changed
- evidence before
- evidence after
- tests
- runtime evidence
- remaining uncertainty
- next roadmap item

Then continue automatically.

## Stopping condition

Stop only when:
- All independently actionable roadmap items discovered by audit are implemented or explicitly rejected with evidence
- Remaining tasks require genuine human judgment, unavailable hardware/accounts, or signing credentials
- Repository docs accurately state those blockers
- Current engineering state is coherent
- No open implementation issue being ignored merely because old roadmap ended

At that point provide final product-readiness matrix.

Until then: keep developing.
