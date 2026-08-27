<div align="center">

<img src="docs/assets/mark.png" alt="" width="96" height="96">

# Mori Lens

Mori Lens explains what you are reading, without interrupting it.

</div>

A local-first language layer for everything on your screen. Rest the
pointer on a word or a sentence in any application and Lens reads the
text the operating system already knows about, detects its language, and
shows the meaning in your target language next to it. No selecting, no
copying, no hotkey, no screenshot.

Lens is not a screen translator. It asks the accessibility tree for the
real characters under the pointer first, and only falls back to a small
local OCR crop when an application exposes nothing to ask. That single
decision is what makes it cheap enough to leave running, accurate about
word boundaries, and able to stay off the screen-recording permission
until the user actually needs it.

## Current state

**Milestone 4 · Windows manual OCR.** The whole chain runs on Windows:
hover, read, translate, card, and a hotkey that reads the pixels where
the accessibility path gives nothing. `cargo build --release -p lens`
produces something you can use today.

What ships:

- Hover to translation to card, end to end. Acquisition p50 3-7ms, a
  warm translation about 1.5s, a repeat of the same sentence from memory.
- All eight target applications measured, in
  [`docs/compatibility/windows-app-matrix.md`](docs/compatibility/windows-app-matrix.md).
- A card drawn with per-pixel alpha: text is opaque whatever the surface,
  corners are round and antialiased, sized to its contents, on the right
  monitor, never taking focus, passing every click through.
- A tray icon, a settings file that reloads while running, and
  start-with-Windows.
- Secure fields refused before they are read, checked against a real
  password control.
- OCR where accessibility gives nothing. `Ctrl+Alt+O` reads the band
  around the pointer.
- `Ctrl+Alt+D` for grammar, structure, and vocabulary explanation.
- `Ctrl+Alt+C` and `Ctrl+Alt+S` to copy or save the card.

What does not exist yet: macOS, saved vocabulary review, and a settings
window. Sequenced in [docs/roadmap.md](docs/roadmap.md).

The authoritative list, capability by capability, is in
[`docs/mori-lens-project-goal.md`](docs/mori-lens-project-goal.md).

## How it works

```
pointer rests
  → OS accessibility API  (UI Automation on Windows, AX on macOS)
  → word · sentence · paragraph ranges + screen bounds
  → language detection
  → quick translation from a local model
  → a native overlay card beside the text
```

OCR runs only when the accessibility path returns nothing, or when the
user explicitly asks for it. A deeper grammar and vocabulary explanation
runs only when the user opens it.

## Relationship to Mori

[Mori](https://github.com/swarea/mori) is a local-first AI work memory
app. Mori Lens is a separate product with a separate binary, and it
runs with Mori not installed.

- **Mori Lens** understands language on the screen, and keeps and
  reviews what a reader chose to keep of it.
- **Mori** is the broader work memory: projects, tasks, conversations,
  and knowledge across sources.

There is no shared package, no shared database, and no runtime
dependency in either direction.

## Tech stack

- Core: Rust, platform-neutral, no OS types in the domain crates
- Windows: `windows-rs`, UI Automation, Win32, Direct2D and DirectWrite
  for the card
- macOS (later): a small Swift bridge over Accessibility, AppKit,
  ScreenCaptureKit, Vision
- Local inference: Ollama over `127.0.0.1`, never bundled, spoken to
  over hand-written HTTP with no client dependency
- Storage (later): SQLite for the library

## Layout

```
crates/lens-core/          domain types, hover machine, segmentation, theme,
                           card layout, settings, model choice. No OS types.
crates/lens-translate/     the Ollama client, HTTP and JSON by hand
crates/platform-windows/   UI Automation, the Direct2D card, tray, autostart
apps/lens/                 the product
apps/hover-probe/          the measurement instrument
docs/decisions/            the decisions that constrain everything else
docs/research/             what was verified, and where the evidence is
docs/compatibility/        per-application results, filled in by running
docs/benchmarks/           latency, idle cost, and which model to use
```

## Use it

### 1. A local model

Lens does not bundle one. Install [Ollama](https://ollama.com), then
pull a small model:

```bash
ollama pull qwen2.5:7b
```

A 3B model is fast and visibly wrong; 7B costs 200ms more and is right
every time. Lens picks the best of what is installed, so pulling a
better one is the whole configuration step.

### 2. Build it

```bash
cargo build --release -p lens
```

`target/release/lens.exe` is the whole product: one file, no service, no
elevation. It opens no window and appears in the notification area.

To hand it to somebody else:

```bash
scripts/package.sh
```

That runs the gate, builds, and writes a zip named for the version and
the commit, with a SHA-256 and a short file saying what to do with it.

### What happens then

- **Hold Alt and rest the pointer** on a sentence. A card appears beside
  it with the translation, and the original above it.
- **Click the tray icon** to pause. Paused means no accessibility call
  is made at all, not that the card is hidden. `Ctrl+Alt+L` does the same.
- **Press Ctrl+Alt+O** where the accessibility path gives nothing, and
  Lens reads the band around the pointer with OCR instead.
- **Right-click the tray icon** for the settings file, start-with-Windows,
  and quit.

The first run writes `%APPDATA%\Mori Lens\settings.conf` with every
option in it, commented. That is where the target language, the theme,
the Ollama address, and the per-application rules live.

Password managers are denied before that file is read, and writing
`default = allow` does not switch that off.

`lens.exe --selftest` runs the whole chain once against text already on
screen, without touching the pointer, and reports each stage.

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

Before committing:

```bash
scripts/check.sh
```

## Privacy

Local-first is a constraint, not a marketing line. The short version:

- Password and protected fields are refused before their contents are read.
- Nothing captured is written to disk.
- No screen image is taken at all in the default configuration.
- No network call is made; translation runs against a local Ollama.

The full model, with what each promise depends on and how to verify it:
[docs/privacy-model.md](docs/privacy-model.md).

## License

**Not licensed for use.** The author holds the copyright and has granted no
permission to use, copy, modify or distribute this code.

That is a state rather than a decision. Nothing has shipped, the repository is
private, and opening it later stays possible: every commit is one author's, so
relicensing needs nobody's consent, and every dependency is permissive, so a
permissive licence remains reachable. Opening a repository is a commit. Closing
one is not — what has shipped stays shipped.
