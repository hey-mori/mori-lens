# Contributing to Mori Lens

This document supplements the project-specific guidance in `CLAUDE.md` and
`docs/git-workflow.md`. It covers environment setup and the testing contract
for the Control Center frontend.

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
npm ci --prefix apps/lens/web
```

Use `npm ci` (not `npm install`) to ensure the lockfile is respected and
dependencies are reproducible. The lockfile (`package-lock.json`) is committed
and must be kept up to date.

### Running tests

```bash
npm test --prefix apps/lens/web
```

This runs the Vitest suite in headless jsdom. The canonical gate
(`scripts/check.sh`) invokes this automatically.

### Watch mode

```bash
npm run test:watch --prefix apps/lens/web
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
- `(cd apps/lens/web && npm test)` (Vitest behavioral tests)
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

See `CLAUDE.md` for project-wide conventions, `docs/git-workflow.md` for
branch/PR workflow, and `docs/decisions/` for architectural decisions.
