# Third-Party Notices

Mori Lens includes third-party software under the following licenses.

## Rust Dependencies

### Core Crates

**serde** (1.0)
- License: MIT OR Apache-2.0
- Repository: https://github.com/serde-rs/serde
- Used in: lens-core, lens-settings, lens-storage, lens-translate

**unicode-segmentation** (1.13)
- License: MIT OR Apache-2.0
- Repository: https://github.com/unicode-rs/unicode-segmentation
- Used in: lens-core (text segmentation)

### Platform-Specific

**windows** (0.58)
- License: MIT OR Apache-2.0
- Repository: https://github.com/microsoft/windows-rs
- Used in: platform-windows (UI Automation, Win32, Direct2D, DirectWrite)

**objc2** (0.5)
- License: MIT
- Repository: https://github.com/madsmtm/objc2
- Used in: platform-macos (Objective-C runtime bindings)

**core-graphics** (0.24)
- License: MIT OR Apache-2.0
- Repository: https://github.com/servo/core-foundation-rs
- Used in: platform-macos (ScreenCaptureKit, coordinate conversion)

**vision** (0.3)
- License: MIT OR Apache-2.0
- Used in: platform-macos (Vision framework OCR)

### Storage

**rusqlite** (0.32)
- License: MIT
- Repository: https://github.com/rusqlite/rusqlite
- Used in: lens-storage (SQLite library)

### Application Shell

**tauri** (2.0)
- License: MIT OR Apache-2.0
- Repository: https://github.com/tauri-apps/tauri
- Used in: apps/lens (Control Center webview)

**serde_json** (1.0)
- License: MIT OR Apache-2.0
- Repository: https://github.com/serde-rs/json
- Used in: apps/lens (JSON serialization)

**toml** (0.8)
- License: MIT OR Apache-2.0
- Repository: https://github.com/toml-rs/toml
- Used in: lens-settings (TOML parsing)

### Build Dependencies

**cc** (1.1)
- License: MIT OR Apache-2.0
- Repository: https://github.com/rust-lang/cc-rs
- Used in: build-stamp (compile-time stamp generation)

## JavaScript Dependencies

**vitest** (1.6)
- License: MIT
- Repository: https://github.com/vitest-dev/vitest
- Used in: apps/lens/web (testing framework, dev only)

**jsdom** (24)
- License: MIT
- Repository: https://github.com/jsdom/jsdom
- Used in: apps/lens/web (DOM simulation for tests, dev only)

**axe-core** (4.10)
- License: MPL-2.0
- Repository: https://github.com/dequelabs/axe-core
- Used in: apps/lens/web (accessibility testing, dev only)

## External Providers

Mori Lens does not bundle inference models. It communicates with:

**Ollama**
- Website: https://ollama.com
- License: MIT
- Note: User-installed, not bundled. Lens speaks HTTP to localhost only.

**LM Studio**
- Website: https://lmstudio.ai
- Note: User-installed, not bundled. Lens speaks HTTP to localhost only.

## Asset Provenance

Application icon and mark: see [`docs/assets/README.md`](docs/assets/README.md).

The mark follows [the organization's mark rule](https://github.com/swarea/.github/blob/main/docs/repository-mark.md).

## License Compatibility

All dependencies use permissive licenses (MIT, Apache-2.0, MPL-2.0) compatible with the project's current all-rights-reserved state and future relicensing to a permissive license if chosen.

No copyleft (GPL, AGPL) dependencies are included.
