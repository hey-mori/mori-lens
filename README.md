# Mori Lens

Mori Lens explains what you are reading, without interrupting it.

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

## Use it

Two things, and one of them is not Lens.

**1. A local model.** Lens does not bundle one
([ADR-0007](docs/decisions/ADR-0007-local-first-inference.md)). Install
[Ollama](https://ollama.com), then pull a small model:

```bash
ollama pull qwen2.5:7b
```

A 3B model is fast and visibly wrong: measured on 2026-08-05 it rendered
"pointer" as 점퍼, a jumper. 7B cost 200ms more and was right every time.
See [the comparison](docs/benchmarks/2026-08-05-model-comparison.md).
Lens picks the best of what is installed, so pulling a better one is the
whole configuration step.

If you skip this, Lens still starts and tells you exactly this, on the
card and in the tray tooltip. It does not fail silently.

**2. Build it.**

```bash
cargo build --release -p lens
```

`target/release/lens.exe` is the whole product: one file, no installer,
no service, no elevation. Put it wherever you keep such things and run
it. It opens no window and appears in the notification area.

### What happens then

- **Hold Alt and rest the pointer** on a sentence. A card appears beside
  it with the translation, and the original above it so you can always
  check what was read.
- **Click the tray icon** to pause. Paused means no accessibility call is
  made at all, not that the card is hidden. `Ctrl+Alt+L` does the same.
- **Right-click it** for the settings file, start-with-Windows, and quit.
  That is the way out: Lens opens no console, so a shell does not wait for
  it and `Ctrl+C` at the prompt goes to the shell rather than to Lens.

The first run writes `%APPDATA%\Mori Lens\settings.conf` with every
option in it, commented. That is where the target language, the theme,
and the per-application rules live:

```
default = modifier          # allow, modifier, or deny

[applications]
chrome = allow              # no Alt needed here
code = deny                 # never, not even with Alt
```

Password managers are denied before that file is read, and writing
`default = allow` does not switch that off.

Command line flags win over the file, for a one-off run:

```bash
lens.exe --auto --theme forest --text large
```

`lens.exe --selftest` runs the whole chain once against text already on
screen, without touching the pointer, and reports each stage. It is the
fastest way to tell a broken install from an empty screen.

## Develop

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
