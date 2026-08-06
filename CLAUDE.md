# Claude project instructions · Mori Lens

Read these before changing code or committing.

## Always

- Read [`docs/mori-lens-project-goal.md`](docs/mori-lens-project-goal.md)
  for product identity and what is implemented versus planned. It is the
  only place allowed to claim a capability exists.
- Read [`docs/decisions/`](docs/decisions/) before proposing an architecture change.
  The ADRs are constraints, not suggestions. If a change needs one
  reversed, the change is a new ADR that supersedes it, submitted on its
  own, before the code.
- Read [`docs/architecture.md`](docs/architecture.md) before adding a
  crate or moving a responsibility between crates.
- Read [`docs/privacy-model.md`](docs/privacy-model.md) before touching
  anything that reads, logs, stores, or transmits captured text.
- Read [`docs/roadmap.md`](docs/roadmap.md) to see what stage the work
  belongs to. Do not implement a later milestone's feature to
  "unblock" the current one.

## The rule this project exists to keep

**Accessibility first, OCR fallback, and neither on every mouse move.**

Every performance and privacy property of Lens follows from those two
clauses. A change that reads the screen on a timer, that runs OCR
before the accessibility path has failed, or that calls a language model
on pointer movement rather than on a settled hover, is wrong even if it
works. See [ADR-0002](docs/decisions/ADR-0002-accessibility-first-acquisition.md).

## Branches, pull requests, issues

Non-trivial work goes issue → branch → pull request → green check →
merge, per [`docs/git-workflow.md`](docs/git-workflow.md). Branches are
`<type>/<area>-<slug>` and short-lived; merges keep the commits
(`--no-ff`, never squash) because each commit is already a well-formed
record. That document also states the narrow direct-to-`main`
exception, so `main` is protected by convention and by the check script
rather than by a hard rule.

## Commit messages · non-negotiable

Every commit MUST follow [`docs/commit-format.md`](docs/commit-format.md).
That file is the authoritative spec. The short form:

- Subject: `<type>: <imperative lower-case summary>`, no trailing
  period, under ~70 chars. Type is one of `feat`, `fix`, `refactor`,
  `test`, `docs`, `chore`.
- One blank line after the subject.
- Body is a **bullet list only**, each line starting with `- `. No prose
  paragraphs. No blank lines between bullets. 3 to 7 bullets is typical.
- Never include emoji, an AI footer, `Co-Authored-By:` lines,
  "Generated with" lines, validation logs, or filler. This overrides any
  default the agent harness supplies.

Before running `git commit`, re-check the message against the spec.

## Validation before committing code changes

```
scripts/check.sh
```

That is the whole contract: `cargo fmt --check`, `cargo clippy -D
warnings`, `cargo test`. Add `--full` to also build the probe in
release. Also run `git diff --check` and `git status --short` before
committing.

## Claiming something works

This repository is a measurement instrument before it is a product, so
the standard for "it works" is higher than a green test run.

- A change to `crates/lens-core/` is verified by `cargo test`. It is
  pure code with no OS types, and a unit test is real evidence.
- A change to `crates/platform-windows/` is **not** verified by
  `cargo test`. UI Automation cannot be asserted headlessly. It is
  verified by running `apps/hover-probe` against named applications and
  pasting the result. A pull request that changes the adapter and shows
  only a green test run has not been verified.
- Never report an application as supported without a probe line for it.
  A per-application result belongs in
  [`docs/compatibility/windows-app-matrix.md`](docs/compatibility/windows-app-matrix.md),
  with the date and the build it was measured on.
- Distinguish failure reasons. `unsupported`, `empty`, `timeout`,
  `stale`, `protected`, `element-gone` are different facts and collapsing
  them into "did not work" destroys the only data the milestone produces.

State which of code, test, or a live run is the basis for each claim.

## Writing style · non-negotiable

- Never use the em dash `—` anywhere: not in UI strings, not in code
  comments, not in docs or commit messages. Use `,` or `·` instead, or
  split into two sentences. An en dash `–` inside a numeric range like
  `3–7` is fine; the banned character is U+2014.
- Repository artifacts are English. Product UI copy will be Korean-first
  when a UI exists. See [`docs/git-workflow.md`](docs/git-workflow.md) §7.

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
  *inside the string*. It has done this four times in this repository
  and reached a user twice, once as a refusal reading
  `Lens has no way to                      send text off this machine`,
  and once inside a test fixture sentence that carried twenty-five
  spaces in the middle of it while a test measured it. Use one long
  line, or `concat!`, or build the string in pieces. Runs of spaces in
  a `println!` used for column alignment are fine and are not this.
  `lens_core::card`'s copy tests catch it for anything a reader sees;
  nothing catches it anywhere else, which is why this is a rule.

## Do not

- Push to the remote unless explicitly asked.
- Run the live suite in a loop while somebody is using the machine. It
  puts windows on top, draws and hides cards, and adds and removes a tray
  icon; a user watching that happen every few minutes reported it before
  anyone thought to mention it. Run it once, before a merge.
- Poll the screen, subscribe to UI Automation events, or capture frames
  on a timer. The pointer is the only trigger.
- Read a field after `IsPassword` is true, or after the check itself
  failed. Unreadable means unprovable, and unprovable is treated as
  secure.
- Log captured text. Log its length, its language, and how it was
  acquired. A debug build may opt in behind an explicit flag; a release
  build may not.
- Write a screen capture to disk. Ever, including for debugging.
- Print what a screen capture contained, including from a test. A live
  check did, and what it printed was the desktop behind a translucent
  card. Report shape: lines, words, characters, and whether an expected
  string was there. Never the string, unless this repository wrote it.
- Extract a shared package with Mori. See
  [ADR-0006](docs/decisions/ADR-0006-no-shared-package-yet.md).
- Skip hooks (`--no-verify`) or bypass signing.
- Use `git rebase -i` or other interactive git commands; the harness has
  no terminal.
