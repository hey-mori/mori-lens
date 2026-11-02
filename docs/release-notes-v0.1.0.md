# Mori Lens v0.1.0 Release Notes

**Release Date**: 2026-09-25  
**Version**: 0.1.0  
**Platform**: Windows (NSIS installer, MSIX package)  
**Status**: Release Candidate

## What is Mori Lens?

Mori Lens is an accessibility-first text capture and translation tool for Windows. It reads text from applications using the accessibility API first, falling back to OCR only when necessary. All processing happens locally, on your machine, with no remote transmission of captured text.

## Key Features

### Accessibility-First Acquisition
- Reads text from applications using UI Automation (Windows)
- Falls back to OCR only when accessibility fails
- Privacy-first: text never leaves your machine
- Works with Ollama and LM Studio for local inference

### Provider Readiness Management (NEW in v0.1.0)
- **Real-time readiness display**: See which providers are ready, what models are available
- **One-click actions**: Download, Load, Refresh, Configure models directly from the Status section
- **Operation tracking**: Watch download progress and status
- **Accurate state reporting**: No more stale or misleading readiness information
- **Event-driven updates**: UI updates only when needed, not on every poll

### Provider Management
- **Ollama support**: Automatic detection, model download, and inference
- **LM Studio support**: JIT/inference-plane truth model (physical load vs inference visibility)
- **Multi-provider**: Switch between providers, configure endpoints
- **Model inventory**: See all available models, download status, and readiness

### Library & Review
- **Capture history**: All captured text saved to local SQLite database
- **Search & filter**: Find captures by text, tag, date, or application
- **Review workflow**: Mark items for later review, with undo support
- **Export**: Export library to JSONL for backup or migration

### Translation & OCR
- **Multiple languages**: Supports translation between various languages via local models
- **OCR fallback**: Windows OCR implementation for when accessibility fails (manual by default, automatic opt-in)
- **Model selection**: Choose from various local models (qwen2.5:7b recommended)

### User Experience
- **Native overlay**: Cards appear near hovered text, draggable and dismissable
- **Copy to clipboard**: One-click copy from lens cards
- **Welcome lesson**: First-run guide explains OCR fallback, privacy, and capture trigger
- **Korean localization**: Korean-first UI when system locale is Korean

## Installation

### Windows NSIS Installer (Recommended)

1. Download `Mori Lens_0.1.0_x64-setup.exe` from the release assets
2. Run the installer (no admin rights required)
3. Launch Mori Lens from Start Menu or desktop shortcut
4. Follow the welcome lesson
5. Install Ollama or LM Studio (see Engine Installation below)
6. Download a model (qwen2.5:7b recommended for Ollama)
7. Start capturing text by hovering over it

### Windows MSIX Package (Store-Preparation Artifact)

1. Download `MoriLens_0.1.0.0_x64.msix` from the release assets
2. This is a structurally verified Store-preparation artifact with placeholder publisher identity
3. For testing: double-click to install (may require Developer Mode or sideloading)
4. For production: requires code signing and publisher identity replacement before Store submission

### Unsigned Build Warning

This release is **not code-signed**. Windows SmartScreen will warn on first run. This warning is accurate, not a false alarm. To proceed:
- Click "More info" → "Run anyway"
- Or verify the SHA-256 checksum matches `SHA256SUMS.txt`

## Engine Installation

### Ollama (Recommended)

1. Download from https://ollama.com/download
2. Install and launch Ollama
3. Download a model: `ollama pull qwen2.5:7b`
4. Mori Lens will automatically detect Ollama at `127.0.0.1:11434`

### LM Studio

1. Download from https://lmstudio.ai/
2. Install and launch LM Studio
3. Download a model (qwen2.5-7b-instruct recommended)
4. Start the local server
5. Mori Lens will detect LM Studio at `127.0.0.1:1234`

## What's New in v0.1.0

### Major Changes
- **Provider readiness UI**: Complete rewrite of Status section with real-time readiness display
- **Event delegation**: Single listener pattern eliminates handler accumulation
- **Stable DOM identity**: UI regions maintain identity across updates
- **Operation controller**: Centralized polling lifecycle for downloads and loads
- **Exact wire format**: Download status serialization matches Rust serde contract

### Fixes
- Activation ordering: readiness refresh before operation observation
- Paused download behavior: correct state transitions
- ProviderAccessState: preserve reason field in wire format
- LM Studio: separate physical load from inference visibility
- NextAction: consolidate around explicit model truth
- Stale failed downloads: no longer override successful lifecycle

### Tests
- 11 production-markup accessibility tests (axe-core)
- Stable DOM identity proof across passive loads
- Exact DownloadStatus wire-format tests
- Adversarial endpoint security tests (userinfo tricks, scheme confusion)
- Integration tests for readiness action handlers

## Known Limitations

### Windows
- **No code signing**: SmartScreen warning on first run (accurate, not false alarm)
- **Ollama/LM Studio required**: Not bundled, must be installed separately
- **No auto-update**: Download new versions from GitHub Releases
- **Start with Windows**: Off by default, can be enabled from tray menu

### macOS
- **Not verified**: Code exists but has not been tested on actual macOS hardware
- **No .app bundle in this release**: Windows-only for v0.1.0
- **Future milestone**: macOS support planned for post-v1.0

### General
- **No telemetry**: Intentional privacy feature, but means no usage analytics
- **No cloud sync**: All data stored locally
- **No multi-language UI**: Korean and English only (more planned)

## Privacy & Security

### What Mori Lens Does
- Reads text from applications using accessibility APIs
- Sends text to local inference engines (Ollama/LM Studio)
- Stores captured text in local SQLite database
- All processing happens on your machine

### What Mori Lens Does NOT Do
- Send text to remote servers
- Collect telemetry or usage analytics
- Start automatically with Windows unless explicitly enabled
- Bundle or download models without your consent

### Endpoint Security
- All engine endpoints validated as loopback (127.0.0.1 or localhost)
- Adversarial URL forms rejected (userinfo tricks, scheme confusion)
- Defense-in-depth: multiple validation layers prevent remote redirection

## Files Modified

### New Features
- `crates/lens-core/src/readiness.rs`: Provider readiness domain
- `crates/lens-core/src/operation.rs`: Operation controller
- `crates/lens-translate/src/lmstudio.rs`: LM Studio JIT model
- `apps/lens/web/js/readiness.js`: Readiness UI
- `apps/lens/web/js/status.js`: Status section with operation tracking

### Fixes
- `apps/lens/web/js/app.js`: Activation ordering
- `apps/lens/web/js/navigation.js`: Async readiness refresh
- `crates/lens-core/src/translate.rs`: Endpoint security tests

### Tests
- `apps/lens/web/js/readiness-integration.vitest.test.js`: Integration tests
- `apps/lens/web/js/accessibility-production.vitest.test.js`: Production accessibility
- `crates/lens-core/src/translate.rs`: Adversarial security tests

## Checksums

See `SHA256SUMS.txt` attached to this release for artifact verification.

## Verification

All checks passed:
- ✅ `scripts/check.sh --full`: All gates pass
- ✅ `scripts/release-preflight.sh`: Artifacts built and verified
- ✅ Version consistency: All three files show 0.1.0
- ✅ 657 Rust tests pass
- ✅ 105 JavaScript tests pass
- ✅ Automated accessibility tests: 11 production readiness states checked with axe-core, zero violations
- ✅ Security audit: Endpoint validation strong

## Documentation

- [Project Goal](mori-lens-project-goal.md)
- [Architecture](architecture.md)
- [Privacy Model](privacy-model.md)
- [Roadmap](roadmap.md)
- [Release Checklist](release-checklist.md)
- [Windows App Matrix](compatibility/windows-app-matrix.md)

## Support & Feedback

- **Issues**: https://github.com/hey-mori/mori-lens/issues
- **Discussions**: https://github.com/hey-mori/mori-lens/discussions
- **Security**: See [SECURITY.md](../SECURITY.md) for reporting vulnerabilities

## What's Next

### v0.2.0 (Planned)
- Auto-update support
- macOS runtime verification
- Performance regression baseline
- Library/Review hardening (tag atomicity, pagination)

### v1.0 (Planned)
- Code signing certificate
- Microsoft Store publication
- Expanded language support
- User-configurable OCR fallback threshold

## Acknowledgments

Built with:
- [Tauri](https://tauri.app/) - Cross-platform desktop framework
- [Ollama](https://ollama.com/) - Local LLM runtime
- [LM Studio](https://lmstudio.ai/) - Local LLM runtime
- [axe-core](https://github.com/dequelabs/axe-core) - Accessibility testing
- [qwen2.5](https://huggingface.co/Qwen) - Translation model

---

**Full Changelog**: See commit history on GitHub  
**Release Candidate**: This is a release candidate. Report issues before v1.0 final.
