# Changelog

All notable changes to Mori Lens are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-25

### Added

- Hover-to-translation overlay with per-pixel alpha rendering
- Windows UI Automation text acquisition under pointer
- Word/sentence/paragraph segmentation around pointer
- Secure-field refusal (password controls denied before text is read)
- Manual OCR fallback (Ctrl+Alt+O) for surfaces exposing no accessibility text
- Deep analysis (Ctrl+Alt+D) with grammar, structure, and vocabulary explanation
- Copy card to clipboard (Ctrl+Alt+C)
- Save to Library (Ctrl+Alt+S) with SQLite backing and Markdown export
- Control Center (Tauri webview) with Status, Library, History, and Settings
- Tray icon with pause control and settings access
- Per-application access tiers (Allow/Modifier/Deny)
- Start-with-Windows option (per-user, no elevation)
- Settings file with live reload (%APPDATA%\Mori Lens\settings.toml)
- Translation cache (in-memory, dies with process)
- Local-only inference via Ollama or LM Studio on 127.0.0.1
- Protected-field refusal verified against real ES_PASSWORD control
- Pause control stops all acquisition (not just hides card)
- First-run welcome lesson demonstrating hover-to-card path
- Library with search, filters, tags, notes, and review workflow
- Manual Review (Later/Known/Archive categorisation)
- Export to Markdown, JSON, and JSONL with round-trip support
- Session-recent tier (50 cards or 2 hours, memory only, off by default for persistent)
- Diagnostic event ring and health checks (privacy-safe, no captured text)
- Diagnostics export (metadata only, no screen content)

### Security

- No captured text logged (shape only: length, language, acquisition method)
- No screen capture written to disk, ever
- Protected fields fail closed (unreadable = unprovable = secure)
- Remote endpoints refused (not warned about) - local-only by construction
- Model output untrusted (never executed, opened, or followed)
- Password managers denied before settings file is read
- Application identity in logs is stable hash, not process name

### Platform

- Windows: full product (hover, read, translate, card, OCR, deep analysis, library)
- macOS: source implementation complete, runtime validation pending (not in v0.1.0 scope)

### Known Limitations

- Code signing: unsigned (SmartScreen will warn on first run)
- Auto-update: not shipped (requires publishing endpoint and private key)
- Microsoft Store: MSIX package structurally verified but not distributed (requires Store account)
- VS Code: accessibility text exists but targeting is poor (OCR fallback required)
- KakaoTalk: 0% on-target with accessibility (OCR fallback required)
- Exclusive full screen: not measured with real game application

### Documentation

- Privacy model: docs/privacy-model.md
- Architecture: docs/architecture.md
- Decisions: docs/decisions/ (19 ADRs)
- Compatibility: docs/compatibility/windows-app-matrix.md
- Benchmarks: docs/benchmarks/ (50+ measurement sessions)

[0.1.0]: https://github.com/swarea/mori-lens/releases/tag/v0.1.0
