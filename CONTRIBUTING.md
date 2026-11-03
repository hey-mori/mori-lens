# Contributing to Mori Lens

This document covers environment setup, testing, and the development
contract for Mori Lens. For architectural decisions, see
[`docs/decisions/`](docs/decisions/). For the privacy model, see
[`docs/privacy-model.md`](docs/privacy-model.md).

## The rule this project exists to keep

**Accessibility first, OCR fallback, and neither on every mouse move.**

Every performance and privacy property of Lens follows from those two
clauses. A change that reads the screen on a timer, that runs OCR
before the accessibility path has failed, or that calls a language model
on pointer movement rather than on a settled hover, is wrong even if it
works. See [ADR-0002](docs/decisions/ADR-0002-accessibility-first-acquisition.md).

## Privacy invariants · non-negotiable

These are enforced by the code, the tests, and the check script:

- **No captured text in logs.** Log its length, its language, and how it
  was acquired. Never the string itself. A debug build may opt in behind
  an explicit flag; a release build may not.
- **No screen capture written to disk.** Ever, including for debugging.
  An OCR crop lives in memory for the duration of one recognition and is
  dropped.
- **Protected fields fail closed.** `IsPassword` is checked before any
  text is materialised. If the check itself fails, the field is treated
  as protected. Unreadable means unprovable, and unprovable is treated
  as secure.
- **Local-only inference.** Translation runs against a local provider on
  `127.0.0.1`. A remote address is refused, not warned about. The HTTP
  client has no TLS and no per-provider grant.
- **Pause stops all acquisition.** Pause means the pointer is not
  sampled at all, not that the card is hidden. A process monitor can
  confirm it.

## Evidence terminology

This repository is a measurement instrument before it is a product, so
the standard for "it works" is higher than a green test run.

- **Source verified:** the code is correct by inspection. No runtime
  test exists.
- **Automated:** a test in CI proves the property.
- **Runtime verified:** a live measurement against a real application
  produced evidence.

State which of these is the basis for each claim. A change to
`crates/lens-core/` is verified by `cargo test`. A change to
`crates/platform-windows/` is **not** verified by `cargo test` alone;
UI Automation cannot be asserted headlessly. It is verified by running
`apps/hover-probe` against named applications and pasting the result.
Never report an application as supported without a probe line for it.

## Node.js environment

The Control Center frontend uses Node.js for the test harness (Vitest + jsdom).
The production frontend runs in Tauri's WebView and does not require Node at
runtime.

### Supported version

Node 20.x LTS is the supported version for development and CI. This aligns
with the GitHub Actions runner environment and Vitest 1.6 / jsdom 24
requirements.

### Installation

```bash
npm ci --prefix apps/lens
```

Use `npm ci` (not `npm install`) to ensure the lockfile is respected and
dependencies are reproducible. The lockfile (`package-lock.json`) is committed
and must be kept up to date.

### Running tests

```bash
npm test --prefix apps/lens
```

This runs the Vitest suite in headless jsdom. The canonical gate
(`scripts/check.sh`) invokes this automatically.

### Watch mode

```bash
npm run test:watch --prefix apps/lens
```

## Frontend test architecture

### Production-seam behavioral tests

Frontend tests import actual production modules and mock only the Tauri IPC
boundary (`./tauri.js`). This ensures tests verify real behavior, not mock
responses.

Example pattern:

```javascript
import { describe, it, expect, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

import { loadSettings } from './settings.js';

describe('Settings module', () => {
  it('loads settings through production loadSettings function', async () => {
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_settings') {
        return { language: 'ko', dwell_ms: 320, /* ... */ };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadSettings();
    // Assert real DOM state, real state updates, real side effects
  });
});
```

### What to mock

- **Mock:** `./tauri.js` (IPC boundary), adjacent modules that are not the
  subject of the test (`./ui.js`, `./i18n.js`, `./state.js`, etc.)
- **Do not mock:** The production module under test, the DOM (jsdom provides
  the real DOM API), or business logic.

### Strict IPC command contract

The test infrastructure (`js/test-infrastructure.js`) validates IPC command
names. Unknown commands fail immediately to catch frontend/backend drift.

When adding a new IPC command:

1. Add it to `knownCommands` in `test-infrastructure.js`
2. Use the exact command name from the Rust backend
3. Write a test that exercises the command through production code

### Test file naming

Test files follow the pattern `<module>.vitest.test.js` and live alongside
the production modules in `apps/lens/web/js/`.

### DOM fixtures

Tests use `createControlCenterFixture()` from `test-infrastructure.js` to
create a minimal DOM structure matching the actual Control Center markup.
Fixtures include all form elements, buttons, and containers needed by the
production code.

## Canonical validation gate

`scripts/check.sh` is the single source of truth for "verified". It runs:

- `cargo fmt --check`
- `cargo clippy -D warnings`
- `cargo test`
- `node apps/lens/web/js/i18n.test.js` (legacy i18n tests)
- `node apps/lens/web/js/i18n-key-checker.js` (key reference integrity)
- `(cd apps/lens && npm test)` (Vitest behavioral tests)
- `python3 scripts/check_capability_matrix.py` (capability matrix integrity)

Add `--full` to also build the probe in release mode.

CI workflows (`.github/workflows/ci.yml`, `package-smoke.yml`, `release.yml`)
provision Node and run `npm ci` before invoking `scripts/check.sh`. Local
developers must do the same on a clean checkout.

## What the tests prove

Frontend behavioral tests provide **AUTOMATED BEHAVIORAL CONTRACT** evidence.
They verify:

- Production modules perform the expected actions
- IPC commands are invoked with correct payloads
- DOM state is updated correctly
- Error paths are handled gracefully
- UI reflects backend truth

They do **not** prove:

- Tauri WebView integration (requires P19B E2E)
- Screen reader compatibility (requires P19C axe-core)
- Real IPC transport (mocked at boundary)
- Backend correctness (backend has its own Rust tests)

## Adding new tests

1. Identify the production module to test
2. Create `<module>.vitest.test.js` in `apps/lens/web/js/`
3. Mock only `./tauri.js` and adjacent non-subject modules
4. Import the production module
5. Use `createControlCenterFixture()` for DOM setup
6. Test actual behavior, not mock responses
7. Cover success paths, error paths, and edge cases
8. Add new IPC commands to `knownCommands` in `test-infrastructure.js`
9. Run `npm test` to verify
10. Run `scripts/check.sh` to ensure canonical gate passes

## Questions?

See `docs/git-workflow.md` for branch/PR workflow, and `docs/decisions/`
for architectural decisions.

## Rust conventions

- The domain crates (`lens-core`) must not depend on `windows`,
  `objc`, `tauri`, or any OS crate. If a type needs an OS concept,
  the concept gets a plain Rust representation in `lens-core` and the
  adapter converts. This is what makes the macOS adapter a new file
  rather than a rewrite.
- Every `unsafe` block carries a `// SAFETY:` comment stating the
  invariant that makes it sound. A block without one does not merge.
- A COM call site initialises its own apartment and tolerates
  `RPC_E_CHANGED_MODE`. Do not assume a thread's apartment.
- Failure is a value, not a panic. The adapter returns a typed
  `AcquireError`; it never unwraps a COM result.
- **Never continue a string literal with a trailing `\`.** `rustfmt`
  rejoins such a literal onto one line and leaves the indentation
  *inside the string*. A sweep on 2026-08-18 found ten strings it had
  already done this to, carrying sixteen runs of spaces between them.
  Use one long line, or `concat!`, or build the string in pieces.
  `scripts/check.sh` fails on the trailing `\` itself, so this is
  enforced rather than remembered.

## Writing style · non-negotiable

- Never use the em dash `—` anywhere: not in UI strings, not in code
  comments, not in docs or commit messages. Use `,` or `·` instead, or
  split into two sentences. An en dash `–` inside a numeric range like
  `3–7` is fine; the banned character is U+2014.
- Repository artifacts are English. Product UI copy will be Korean-first
  when a UI exists. See [`docs/git-workflow.md`](docs/git-workflow.md) §7.

## Licence and sign-off

Mori Lens is licensed under the [Mozilla Public License 2.0](LICENSE)
(`MPL-2.0`), and a contribution is accepted under that same licence. There is
no separate contributor agreement and no transfer of copyright: you keep the
copyright in what you wrote.

Every commit in a pull request from outside the organization carries a
sign-off, as the last block of the message after a blank line:

```
Signed-off-by: Your Name <you@example.com>
```

`git commit -s` adds it from your `user.name` and `user.email`, which must
match the commit's author. By signing off you certify the
[Developer Certificate of Origin 1.1](https://developercertificate.org): that
you wrote the change, or otherwise have the right to submit it under this
repository's licence. The `dco` check on the pull request names any commit
without one, and `git rebase --signoff main` adds it to every commit on a
branch. Maintainers' own commits do not need it.

Code you did not write yourself, generated code included, is yours to account
for: do not submit it unless its origin allows you to license it under
MPL-2.0. Third-party code keeps its own licence and notice, recorded in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

The licence grants no rights to the Mori names or logos (MPL-2.0 §2.3). A
modified version you distribute may say it is based on Mori Lens, but must not
use the Mori name or logo as its own name or icon.

## Commit messages

Every commit MUST follow [`docs/commit-format.md`](docs/commit-format.md).
That file is the authoritative spec. The short form:

- Subject: `<type>: <imperative lower-case summary>`, no trailing
  period, under ~70 chars. Type is one of `feat`, `fix`, `refactor`,
  `test`, `docs`, `chore`.
- One blank line after the subject.
- Body is a **bullet list only**, each line starting with `- `. No prose
  paragraphs. No blank lines between bullets. 3 to 7 bullets is typical.
- Never include emoji, an AI footer, `Co-Authored-By:` lines,
  "Generated with" lines, validation logs, or filler.
- The one trailer allowed is a `Signed-off-by:` block at the end, after a
  blank line (see "Licence and sign-off" above).
