<div align="center">

<img src="docs/assets/mori-mark.svg" alt="" width="88" height="88">

# Mori Lens

**Mori Lens explains what you are reading, without interrupting it.**

Rest the pointer on a word or a sentence in any application, and Lens shows its
meaning in your language, on your own machine.

[Current state](#current-state) ·
[Installation](#installation) ·
[Develop](#develop) ·
[Privacy](#privacy)

![status: pre-release](https://img.shields.io/badge/status-pre--release-6246f5)
[![license: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-1c1e22)](LICENSE)

</div>

A local-first language layer for everything on your screen. Rest the
pointer on a word or a sentence in any application and Lens reads the
text the operating system already knows about, detects its language, and
shows the meaning in your target language next to it. No selection or
copy/paste is required.

Lens is not a screen translator. It asks the accessibility tree for the
real characters under the pointer first, and only falls back to a small
in-memory OCR crop when an application exposes nothing to ask. That
single decision is what makes it cheap enough to leave running, accurate
about word boundaries, and respectful of privacy by default.

## Current state

**Pre-release · Windows.** There is no published release or installer
yet. You build Lens from source and install it with the scripts under
[Installation](#installation). On Windows the whole product runs: hover,
read, translate, the card, OCR, deep analysis, saving to the Library, manual
review, and the Control Center for settings and the library. Version 0.1.0 is
being prepared as the first release; CI already builds unsigned NSIS and MSIX
packages to prove the packaging, but they are not published.

**macOS:** the macOS app compiles and its tests pass in CI, but it has not
been run and checked by real readers, which needs the Accessibility and
Screen Recording permissions. macOS is not part of the 0.1.0 scope.
Sequenced in [docs/roadmap.md](docs/roadmap.md).

What works on Windows:

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
- OCR where accessibility gives nothing. The Manual OCR shortcut
  (default `Ctrl+Alt+O` on Windows, `Cmd+Opt+O` on macOS) reads the band
  around the pointer.
- `Ctrl+Alt+D` (default) for grammar, structure, and vocabulary explanation.
- `Ctrl+Alt+C` and `Ctrl+Alt+S` (defaults) to copy or save the card.

Global shortcuts are configurable in Control Center.

The Control Center interface supports English and Korean, selectable in
Settings > General, without restarting the application.

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

[Mori](https://github.com/hey-mori/mori) is a local-first personal
context workspace. Mori Lens is a separate product with a separate
binary, and it runs with Mori not installed.

- **Mori Lens** understands language on the screen, and keeps
  what a reader chose to keep of it.
- **Mori** turns the traces of a person's work, such as notes, links,
  AI sessions and Git history, into a connected record across sources.

There is no shared package, no shared database, and no runtime
dependency in either direction.

## Tech stack

- Core: Rust, platform-neutral, no OS types in the domain crates
- Windows: `windows-rs`, UI Automation, Win32, Direct2D and DirectWrite
  for the card
- macOS: Accessibility, AppKit, ScreenCaptureKit, Vision
- Local inference: Ollama or LM Studio over `127.0.0.1`, never bundled,
  spoken to over hand-written HTTP with no client dependency
- Storage: `settings.toml` (versioned, round-trip TOML via `lens-settings`);
  `library.db` (SQLite, [ADR-0013](docs/decisions/ADR-0013-the-local-library.md),
  [ADR-0015](docs/decisions/ADR-0015-the-library-schema.md))

## Layout

```
crates/lens-core/          domain types, hover machine, segmentation, theme,
                           card layout, settings, model choice. No OS types.
crates/lens-settings/      versioned TOML settings, round-trip preservation
crates/lens-storage/       SQLite library for saved items
crates/lens-translate/     provider-aware translation and runtime management
crates/platform-windows/   UI Automation, the Direct2D card, tray, autostart
crates/platform-macos/     AX text acquisition, AppKit overlay, menu bar
apps/lens/                 the product
apps/hover-probe/          the measurement instrument
docs/decisions/            the decisions that constrain everything else
docs/research/             what was verified, and where the evidence is
docs/compatibility/        per-application results, filled in by running
docs/benchmarks/           latency, idle cost, and which model to use
```

## Installation

### Prerequisites

Lens does not bundle a model. Install a supported local provider and
make a model available:

**Ollama:**

```bash
ollama pull qwen2.5:7b
```

**LM Studio:**

Download a model through LM Studio and start its local server.

A 3B model is fast but may produce lower-quality translations; 7B models
generally produce better results at the cost of slightly higher latency.
Model resolution is provider-specific: Ollama's router picks the best
installed model when none is named; LM Studio uses the configured or
discovered model.

### From the repository (Windows)

Requires [Rust 1.85+](https://rustup.rs) and a local provider from the step
above.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-and-install.ps1
```

This builds a release binary and installs it to
`%LOCALAPPDATA%\Programs\Mori Lens`, adds a Start Menu entry, and
registers it in Settings > Apps. Per-user, no elevation.

Or build and install separately:

```bash
cargo build --release --manifest-path apps/lens/Cargo.toml
```

```powershell
powershell -ExecutionPolicy Bypass -File scripts/install.ps1
```

### Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File scripts/uninstall.ps1
```

This reverses the installation and keeps the settings file. Add `-Purge` to
also delete it.

## Use it

- **Hold Alt and rest the pointer** on a sentence. A card appears beside
  it with the translation, and the original above it.
- **Click the tray icon** to pause. Paused means no accessibility call
  is made at all, not that the card is hidden. The Pause shortcut
  (default `Ctrl+Alt+L` on Windows, `Cmd+Opt+L` on macOS) does the same.
- **Press the Manual OCR shortcut** (default `Ctrl+Alt+O` on Windows,
  `Cmd+Opt+O` on macOS) where the accessibility path gives nothing, and
  Lens reads the band around the pointer with OCR instead.
- **Right-click the tray icon** for the settings file, start-with-Windows,
  and quit.

The first run writes `%APPDATA%\Mori Lens\settings.toml` with every
option in it, commented. That is where the target language, the theme,
the provider endpoint, the model, and the per-application rules live.

Password managers are denied before that file is read, and writing
`default = allow` does not switch that off.

`lens.exe --selftest` runs the whole chain once against text already on
screen, without touching the pointer, and reports each stage.

## Global shortcuts

The global shortcuts are configurable in Control Center (Settings → Shortcuts).
Changes require a restart to take effect.

Windows defaults:

| Action       | Shortcut   |
|--------------|------------|
| Pause        | Ctrl+Alt+L |
| Manual OCR   | Ctrl+Alt+O |
| Deep         | Ctrl+Alt+D |
| Copy Card    | Ctrl+Alt+C |
| Save         | Ctrl+Alt+S |
| Pin Card     | Ctrl+Alt+P |

macOS defaults:

| Action       | Shortcut   |
|--------------|------------|
| Pause        | Cmd+Opt+L  |
| Manual OCR   | Cmd+Opt+O  |
| Deep         | Cmd+Opt+D  |
| Copy Card    | Cmd+Opt+C  |
| Save         | Cmd+Opt+S  |

Pause cannot be unbound. It is the privacy control that stops all
screen acquisition. Other actions can be disabled by clearing their
shortcut field.

## Develop

```bash
cargo test
```

```bash
cargo run -p hover-probe   # Windows only
```

The probe prints one line per resolved hover: the application, how the
text was acquired, the character counts, the latency, and the failure
reason when there is one. It never prints the text it read. See
[apps/hover-probe/README.md](apps/hover-probe/README.md) for the flags
and for how to record a session into the compatibility matrix.

Before committing:

```bash
scripts/check.sh          # the fast gate; CI runs scripts/check.sh --full
```

`--full` also builds the Tauri app and runs its tests. It is meant for
Windows and macOS: on Linux the app's platform code reads as unused and
clippy fails, so use the fast gate there.
[CONTRIBUTING.md](CONTRIBUTING.md) covers the full development contract.

## Privacy

Local-first is a constraint, not a marketing line. The short version:

- Password and protected fields are refused before their contents are read.
- Nothing captured is written to disk unless you save it, or turn on
  history, which is off by default.
- No screen image is taken at all in the default configuration.
- No network call leaves the machine; translation runs against a local
  provider (Ollama or LM Studio) over a loopback address only.

The full model, with what each promise depends on and how to verify it:
[docs/privacy-model.md](docs/privacy-model.md).

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Commits
follow [docs/commit-format.md](docs/commit-format.md), and contributors from
outside the organization sign off each commit (`git commit -s`).

Report a vulnerability privately as [SECURITY.md](SECURITY.md) describes,
never in an issue.

## License

The source code in this repository is licensed under the [Mozilla Public License 2.0](LICENSE)
(`MPL-2.0`). Copyright 2026 Isaac Lee.

- Third-party dependencies keep their own licences.
- The licence grants no rights to the Mori names or logos as trademarks.
- Contributions are accepted under the same licence.
