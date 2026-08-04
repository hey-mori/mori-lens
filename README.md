# Mori Lens

Understand without stopping to read.

Mori Lens is a local-first language layer for everything on your
screen. Rest the pointer on a word or a sentence in any application and
Lens reads the text the operating system already knows about, detects
its language, and shows the meaning in your target language next to it.
No selecting, no copying, no hotkey, no screenshot.

Mori Lens is not a screen translator. A screen translator captures
pixels and runs OCR over them. Lens asks the accessibility tree for the
real characters under the pointer first, and only falls back to a small
local OCR crop when an application exposes nothing to ask. That single
decision is what makes it cheap enough to leave running, accurate about
word boundaries, and able to stay off the screen-recording permission
until the user actually needs it.

## Relationship to Mori

[Mori](https://github.com/swarea/mori) is a local-first AI work memory
app. Mori Lens is a separate product with a separate binary, and it
runs with Mori not installed. The split is deliberate and recorded in
[ADR-0001](docs/decisions/ADR-0001-independent-repository.md).

- **Mori Lens** discovers and explains language on the screen.
- **Mori** accumulates and reviews what was learned.

There is no shared package, no shared database, and no runtime
dependency in either direction. If the two are ever connected it will
be through a small documented process boundary, not by merging the code
([ADR-0006](docs/decisions/ADR-0006-no-shared-package-yet.md)).

## Current state

**Milestone 1 · Windows Hover Probe.** The repository holds the
verification instrument, not the product. Nothing translates yet, and
no overlay is drawn yet.

What runs today:

- A console probe that follows the pointer, resolves the UI Automation
  element under it, extracts the word and its surrounding context, and
  records the screen rectangles, the latency, and the failure reason.
- A pure hover state machine (dwell, movement radius, stale generation
  discard, same-target cooldown) with unit tests and no platform types.
- Word / sentence / paragraph segmentation over the extracted context.
- A compatibility log format for recording what each application
  actually returns.

What does not exist yet: translation, Ollama, OCR, the overlay,
macOS, saved vocabulary, settings UI. Those are sequenced in
[docs/roadmap.md](docs/roadmap.md) and none of them start before the
probe has measured a real success rate per application
([docs/compatibility/windows-app-matrix.md](docs/compatibility/windows-app-matrix.md)).

## How it will work

```
pointer rests
  → OS accessibility API  (UI Automation on Windows, AX on macOS)
  → word · sentence · paragraph ranges + screen bounds
  → language detection
  → quick translation from a local model
  → a native overlay card beside the text
```

OCR runs only when the accessibility path returns nothing, or when the
user explicitly asks for it. A deeper grammar and vocabulary
explanation runs only when the user opens it. Both rules exist so the
common case stays fast: see
[ADR-0002](docs/decisions/ADR-0002-accessibility-first-acquisition.md) and
[ADR-0005](docs/decisions/ADR-0005-two-speed-ai.md).

## Tech stack

- Core: Rust, platform-neutral, no OS types in the domain crates
- Windows: `windows-rs`, UI Automation, Win32
- macOS (later): a small Swift bridge over Accessibility, AppKit,
  ScreenCaptureKit, Vision
- Local inference (later): Ollama over `127.0.0.1`, never bundled
- Storage (later): SQLite, opt-in history

## Layout

```
crates/lens-core/          domain types, hover state machine, segmentation
crates/platform-windows/   UI Automation text acquisition
apps/hover-probe/          the Milestone 1 measurement instrument
docs/decisions/                  the decisions that constrain everything else
docs/research/             what was verified, and where the evidence is
docs/compatibility/        per-application results, filled in by running
```

## Run

```bash
cargo test
```

```bash
cargo run -p hover-probe
```

The probe prints one line per resolved hover: the application, how the
text was acquired, the character counts, the latency, and the failure
reason when there is one. It never prints the text it read. See
[apps/hover-probe/README.md](apps/hover-probe/README.md) for the flags
and for how to record a session into the compatibility matrix.

## Privacy

Local-first is a constraint, not a marketing line, and it is written
down before the code that has to keep it:
[docs/privacy-model.md](docs/privacy-model.md).

The short version, true of the probe today: password and protected
fields are refused before their contents are read, nothing captured is
written to disk, no screen image is taken at all, and no network call
is made.

## Contributing

Read [CLAUDE.md](CLAUDE.md) first. It points at the commit format, the
Git workflow, and the rules that changes are held to.
