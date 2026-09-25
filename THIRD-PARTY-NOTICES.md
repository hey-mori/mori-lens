# Third-Party Notices

This document lists the third-party dependencies used by Mori Lens.

## Summary

- **Total Rust dependencies**: 493
- **Permissive licenses**: 457
- **Weak-copyleft (MPL)**: 5
- **Copyleft (GPL/AGPL)**: 0
- **JavaScript dependencies**: Development-only (not distributed)

### JavaScript Dependencies

The project uses JavaScript packages for testing only (`vitest`, `jsdom`, `axe-core`, `@axe-core/playwright`). These are listed as `devDependencies` in `apps/lens/package.json` and are **not included in the distributed application**. They do not create distribution obligations.

### Weak-Copyleft (MPL) Dependencies

The following dependencies use MPL or similar weak-copyleft licenses.
These are typically file-level copyleft and do not affect the larger work:

- `cssparser` 0.36.0: MPL-2.0
- `cssparser-macros` 0.6.1: MPL-2.0
- `dtoa-short` 0.3.5: MPL-2.0
- `option-ext` 0.2.0: MPL-2.0
- `selectors` 0.36.1: MPL-2.0

## Dependencies by License

### (Apache-2.0 OR MIT) AND Unicode-3.0

1 packages:

- `unicode-ident` 1.0.24

### 0BSD OR Apache-2.0 OR MIT

1 packages:

- `adler2` 2.0.1

### Apache-2.0

2 packages:

- `sync_wrapper` 1.0.2
- `tao` 0.35.3

### Apache-2.0 AND MIT

1 packages:

- `dpi` 0.1.2

### Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT

5 packages:

- `linux-raw-sys` 0.12.1
- `rustix` 1.1.5
- `wasi` 0.11.1+wasi-snapshot-preview1
- `wasip2` 1.0.4+wasi-0.2.12
- `wit-bindgen` 0.57.1

### Apache-2.0 OR BSD-2-Clause OR MIT

4 packages:

- `zerocopy` 0.8.56
- `zerocopy` 0.8.57
- `zerocopy-derive` 0.8.56
- `zerocopy-derive` 0.8.57

### Apache-2.0 OR BSD-3-Clause OR MIT

2 packages:

- `num_enum` 0.7.6
- `num_enum_derive` 0.7.6

### Apache-2.0 OR BSL-1.0

1 packages:

- `ryu` 1.0.23

### Apache-2.0 OR CC0-1.0 OR MIT-0

1 packages:

- `dunce` 1.0.5

### Apache-2.0 OR LGPL-2.1-or-later OR MIT

2 packages:

- `r-efi` 5.3.0
- `r-efi` 6.0.0

### Apache-2.0 OR MIT

284 packages:

- `ahash` 0.8.12
- `android_system_properties` 0.1.6
- `anyhow` 1.0.104
- `async-trait` 0.1.92
- `atomic-waker` 1.1.2
- `autocfg` 1.5.1
- `base64` 0.21.7
- `base64` 0.22.1
- `base64` 0.23.1
- `bit-set` 0.8.0
- `bit-vec` 0.8.0
- `bitflags` 2.13.1
- `bitflags` 1.3.2
- `block-buffer` 0.10.4
- `bs58` 0.5.1
- `bumpalo` 3.20.3
- `camino` 1.2.5
- `cargo-platform` 0.1.9
- `cargo_toml` 0.22.3
- `cc` 1.4.4
- `cc` 1.4.5
- `cesu8` 1.1.0
- `cfg-expr` 0.15.8
- `cfg-expr` 0.20.9
- `cfg-if` 1.0.4
- `chrono` 0.4.45
- `cookie` 0.18.2
- `core-foundation` 0.10.1
- `core-foundation-sys` 0.8.7
- `core-graphics` 0.25.0
- `core-graphics-types` 0.2.0
- `cpufeatures` 0.2.17
- `crc32fast` 1.5.1
- `crossbeam-channel` 0.5.17
- `crossbeam-utils` 0.8.23
- `crypto-common` 0.1.7
- `ctor` 0.8.0
- `ctor-proc-macro` 0.0.7
- `dbus` 0.9.12
- `defmt` 1.1.1
- `defmt-macros` 1.1.1
- `defmt-parser` 1.0.0
- `deranged` 0.5.8
- `digest` 0.10.7
- `dirs` 6.0.0
- `dirs-sys` 0.5.0
- `displaydoc` 0.2.7
- `dtoa` 1.0.11
- `dtor` 0.3.0
- `dtor-proc-macro` 0.0.6
- `dyn-clone` 1.0.20
- `embed_plist` 1.2.2
- `equivalent` 1.0.2
- `erased-serde` 0.4.10
- `errno` 0.3.14
- `fallible-iterator` 0.3.0
- `fallible-streaming-iterator` 0.1.9
- `fastrand` 2.5.0
- `fdeflate` 0.3.7
- `field-offset` 0.3.6
- `find-msvc-tools` 0.1.11
- `find-msvc-tools` 0.1.12
- `flate2` 1.1.10
- `fnv` 1.0.7
- `foreign-types` 0.5.0
- `foreign-types-macros` 0.2.4
- `foreign-types-shared` 0.3.1
- `form_urlencoded` 1.2.2
- `futures-channel` 0.3.34
- `futures-core` 0.3.34
- `futures-executor` 0.3.34
- `futures-io` 0.3.34
- `futures-macro` 0.3.34
- `futures-sink` 0.3.34
- `futures-task` 0.3.34
- `futures-util` 0.3.34
- `getrandom` 0.2.17
- `getrandom` 0.3.4
- `getrandom` 0.4.3
- `glob` 0.3.4
- `hashbrown` 0.14.5
- `hashbrown` 0.17.1
- `hashbrown` 0.12.3
- `hashlink` 0.9.1
- `heck` 0.4.1
- `heck` 0.5.0
- `hex` 0.4.3
- `html5ever` 0.38.0
- `http` 1.5.0
- `httparse` 1.10.1
- `httpdate` 1.0.3
- `iana-time-zone` 0.1.65
- `iana-time-zone-haiku` 0.1.2
- `ident_case` 1.0.1
- `idna` 1.1.0
- `idna_adapter` 1.2.2
- `indexmap` 2.14.0
- `indexmap` 1.9.3
- `indexmap` 2.14.2
- `ipnet` 2.12.2
- `itoa` 1.0.18
- `jni` 0.21.1
- `jni-sys` 0.3.1
- `jni-sys` 0.4.1
- `jni-sys-macros` 0.4.1
- `js-sys` 0.3.105
- `json-patch` 3.0.1
- `jsonptr` 0.6.3
- `keyboard-types` 0.7.0
- `libappindicator` 0.9.0
- `libappindicator-sys` 0.9.0
- `libc` 0.2.189
- `libdbus-sys` 0.2.7
- `lock_api` 0.4.14
- `log` 0.4.34
- `markup5ever` 0.38.0
- `mime` 0.3.17
- `muda` 0.19.3
- `ndk` 0.9.0
- `ndk-sys` 0.6.0+11769913
- `num-conv` 0.2.2
- `num-traits` 0.2.19
- `once_cell` 1.21.4
- `parking_lot` 0.12.5
- `parking_lot_core` 0.9.12
- `percent-encoding` 2.3.2
- `pin-project-lite` 0.2.17
- `pkg-config` 0.3.34
- `png` 0.17.16
- `png` 0.18.1
- `portable-atomic` 1.15.0
- `portable-atomic-util` 0.2.8
- `powerfmt` 0.2.0
- `proc-macro-crate` 1.3.1
- `proc-macro-crate` 2.0.0
- `proc-macro-crate` 3.5.0
- `proc-macro-error` 1.0.4
- `proc-macro-error-attr` 1.0.4
- `proc-macro2` 1.0.107
- `quote` 1.0.47
- `ref-cast` 1.0.27
- `ref-cast-impl` 1.0.27
- `regex` 1.13.1
- `regex-automata` 0.4.18
- `regex-syntax` 0.8.11
- `reqwest` 0.13.5
- `rustc-hash` 2.1.3
- `rustc_version` 0.4.1
- `rustversion` 1.0.23
- `scopeguard` 1.2.0
- `semver` 1.0.28
- `serde` 1.0.229
- `serde-untagged` 0.1.9
- `serde_core` 1.0.229
- `serde_derive` 1.0.229
- `serde_derive_internals` 0.29.1
- `serde_json` 1.0.151
- `serde_path_to_error` 0.1.20
- `serde_repr` 0.1.21
- `serde_spanned` 0.6.9
- `serde_spanned` 1.1.1
- `serde_urlencoded` 0.7.1
- `serde_with` 3.23.0
- `serde_with_macros` 3.23.0
- `serialize-to-javascript` 0.1.2
- `serialize-to-javascript-impl` 0.1.2
- `servo_arc` 0.4.3
- `sha2` 0.10.9
- `shlex` 2.0.1
- `signal-hook-registry` 1.4.8
- `siphasher` 1.0.3
- `smallvec` 1.15.2
- `smallvec` 1.16.0
- `socket2` 0.6.5
- `softbuffer` 0.4.8
- `stable_deref_trait` 1.2.1
- `string_cache` 0.9.0
- `string_cache_codegen` 0.6.1
- `swift-rs` 1.0.8
- `syn` 2.0.119
- `syn` 3.0.4
- `syn` 1.0.109
- `syn` 3.0.5
- `system-deps` 6.2.2
- `system-deps` 7.0.8
- `tao-macros` 0.1.4
- `tauri` 2.11.5
- `tauri-build` 2.6.3
- `tauri-codegen` 2.6.3
- `tauri-macros` 2.6.3
- `tauri-plugin` 2.6.3
- `tauri-runtime` 2.11.3
- `tauri-runtime-wry` 2.11.4
- `tauri-utils` 2.9.3
- `tempfile` 3.27.0
- `tendril` 0.5.1
- `thiserror` 1.0.69
- `thiserror` 2.0.20
- `thiserror-impl` 1.0.69
- `thiserror-impl` 2.0.20
- `time` 0.3.55
- `time-core` 0.1.9
- `time-macros` 0.2.32
- `toml` 0.8.23
- `toml` 0.9.12+spec-1.1.0
- `toml` 1.1.5+spec-1.1.0
- `toml_datetime` 0.6.11
- `toml_datetime` 0.7.5+spec-1.1.0
- `toml_datetime` 1.1.1+spec-1.1.0
- `toml_edit` 0.22.27
- `toml_edit` 0.19.15
- `toml_edit` 0.20.7
- `toml_edit` 0.25.13+spec-1.1.0
- `toml_parser` 1.1.3+spec-1.1.0
- `toml_write` 0.1.2
- `toml_writer` 1.1.2+spec-1.1.0
- `tray-icon` 0.24.2
- `typeid` 1.0.3
- `typenum` 1.20.1
- `unic-char-property` 0.9.0
- `unic-char-range` 0.9.0
- `unic-common` 0.9.0
- `unic-ucd-ident` 0.9.0
- `unic-ucd-version` 0.9.0
- `unicode-segmentation` 1.13.3
- `url` 2.5.8
- `utf8_iter` 1.0.4
- `uuid` 1.26.0
- `vcpkg` 0.2.15
- `version_check` 0.9.5
- `wasm-bindgen` 0.2.128
- `wasm-bindgen-futures` 0.4.78
- `wasm-bindgen-macro` 0.2.128
- `wasm-bindgen-macro-support` 0.2.128
- `wasm-bindgen-shared` 0.2.128
- `wasm-streams` 0.5.0
- `web-sys` 0.3.105
- `web_atoms` 0.2.6
- `winapi` 0.3.9
- `winapi-i686-pc-windows-gnu` 0.4.0
- `winapi-x86_64-pc-windows-gnu` 0.4.0
- `window-vibrancy` 0.6.0
- `windows` 0.62.2
- `windows` 0.61.3
- `windows-collections` 0.3.2
- `windows-collections` 0.2.0
- `windows-core` 0.62.2
- `windows-core` 0.61.2
- `windows-future` 0.3.2
- `windows-future` 0.2.1
- `windows-implement` 0.60.2
- `windows-interface` 0.59.3
- `windows-link` 0.2.1
- `windows-link` 0.1.3
- `windows-numerics` 0.3.1
- `windows-numerics` 0.2.0
- `windows-result` 0.4.1
- `windows-result` 0.3.4
- `windows-strings` 0.5.1
- `windows-strings` 0.4.2
- `windows-sys` 0.61.2
- `windows-sys` 0.45.0
- `windows-sys` 0.59.0
- `windows-targets` 0.42.2
- `windows-targets` 0.52.6
- `windows-threading` 0.2.1
- `windows-threading` 0.1.0
- `windows-version` 0.1.7
- `windows_aarch64_gnullvm` 0.42.2
- `windows_aarch64_gnullvm` 0.52.6
- `windows_aarch64_msvc` 0.42.2
- `windows_aarch64_msvc` 0.52.6
- `windows_i686_gnu` 0.42.2
- `windows_i686_gnu` 0.52.6
- `windows_i686_gnullvm` 0.52.6
- `windows_i686_msvc` 0.42.2
- `windows_i686_msvc` 0.52.6
- `windows_x86_64_gnu` 0.42.2
- `windows_x86_64_gnu` 0.52.6
- `windows_x86_64_gnullvm` 0.42.2
- `windows_x86_64_gnullvm` 0.52.6
- `windows_x86_64_msvc` 0.42.2
- `windows_x86_64_msvc` 0.52.6
- `wry` 0.55.1

### Apache-2.0 OR MIT OR Zlib

24 packages:

- `bytemuck` 1.25.2
- `dispatch2` 0.3.1
- `miniz_oxide` 0.8.9
- `miniz_oxide` 0.9.1
- `objc2-app-kit` 0.3.2
- `objc2-cloud-kit` 0.3.2
- `objc2-core-data` 0.3.2
- `objc2-core-foundation` 0.3.2
- `objc2-core-graphics` 0.3.2
- `objc2-core-image` 0.3.2
- `objc2-core-location` 0.3.2
- `objc2-core-text` 0.3.2
- `objc2-core-video` 0.3.2
- `objc2-exception-helper` 0.1.1
- `objc2-io-surface` 0.3.2
- `objc2-javascript-core` 0.3.2
- `objc2-quartz-core` 0.3.2
- `objc2-security` 0.3.2
- `objc2-ui-kit` 0.3.2
- `objc2-user-notifications` 0.3.2
- `objc2-web-kit` 0.3.2
- `raw-window-handle` 0.6.2
- `tinyvec` 1.13.2
- `tinyvec_macros` 0.1.1

### Apache-2.0 WITH LLVM-exception

2 packages:

- `target-lexicon` 0.12.16
- `target-lexicon` 0.13.5

### BSD-3-Clause

2 packages:

- `alloc-no-stdlib` 2.0.4
- `alloc-stdlib` 0.2.4

### BSD-3-Clause AND MIT

2 packages:

- `brotli` 8.0.4
- `matchit` 0.8.4

### BSD-3-Clause OR MIT

1 packages:

- `brotli-decompressor` 5.0.3

### ISC

1 packages:

- `libloading` 0.7.4

### MIT

111 packages:

- `atk` 0.18.2
- `atk-sys` 0.18.2
- `axum` 0.8.9
- `axum-core` 0.5.6
- `block2` 0.6.2
- `bytes` 1.12.1
- `cairo-rs` 0.18.5
- `cairo-sys-rs` 0.18.2
- `cargo_metadata` 0.19.2
- `cfb` 0.7.3
- `combine` 4.6.8
- `darling` 0.24.1
- `darling_core` 0.24.1
- `darling_macro` 0.24.1
- `derive_more` 2.1.1
- `derive_more-impl` 2.1.1
- `dlopen2` 0.8.2
- `dlopen2_derive` 0.4.3
- `dom_query` 0.27.0
- `embed-resource` 3.0.11
- `gdk` 0.18.2
- `gdk-pixbuf` 0.18.5
- `gdk-pixbuf-sys` 0.18.0
- `gdk-sys` 0.18.2
- `gdkwayland-sys` 0.18.2
- `gdkx11` 0.18.2
- `gdkx11-sys` 0.18.2
- `generic-array` 0.14.7
- `gio` 0.18.4
- `gio-sys` 0.18.1
- `gio-sys` 0.21.5
- `glib` 0.18.5
- `glib` 0.21.5
- `glib-macros` 0.18.5
- `glib-macros` 0.21.5
- `glib-sys` 0.18.1
- `glib-sys` 0.21.5
- `gobject-sys` 0.18.0
- `gobject-sys` 0.21.5
- `gtk` 0.18.2
- `gtk-sys` 0.18.2
- `gtk3-macros` 0.18.2
- `http-body` 1.1.0
- `http-body-util` 0.1.5
- `hyper` 1.11.1
- `hyper-util` 0.1.20
- `ico` 0.5.0
- `infer` 0.19.0
- `javascriptcore-rs` 1.1.2
- `javascriptcore-rs-sys` 1.1.1
- `libredox` 0.1.23
- `libsqlite3-sys` 0.30.1
- `memoffset` 0.9.1
- `mio` 1.2.3
- `new_debug_unreachable` 1.0.6
- `objc2` 0.6.4
- `objc2-encode` 4.1.0
- `objc2-foundation` 0.3.2
- `pango` 0.18.3
- `pango-sys` 0.18.0
- `phf` 0.13.1
- `phf_codegen` 0.13.1
- `phf_generator` 0.13.1
- `phf_macros` 0.13.1
- `phf_shared` 0.13.1
- `plist` 1.10.1
- `precomputed-hash` 0.1.1
- `quick-xml` 0.42.0
- `redox_syscall` 0.5.18
- `redox_users` 0.5.2
- `rusqlite` 0.32.1
- `schemars` 0.8.22
- `schemars` 0.9.0
- `schemars` 1.2.2
- `schemars_derive` 0.8.22
- `simd-adler32` 0.3.10
- `slab` 0.4.12
- `soup3` 0.5.0
- `soup3-sys` 0.5.0
- `strsim` 0.11.1
- `synstructure` 0.13.2
- `tauri-plugin-wdio-webdriver` 1.4.0
- `tauri-winres` 0.3.6
- `tokio` 1.53.1
- `tokio-macros` 2.7.2
- `tokio-util` 0.7.19
- `tower` 0.5.3
- `tower-http` 0.6.11
- `tower-layer` 0.3.3
- `tower-service` 0.3.3
- `tracing` 0.1.44
- `tracing-attributes` 0.1.31
- `tracing-core` 0.1.36
- `try-lock` 0.2.5
- `urlpattern` 0.3.0
- `version-compare` 0.2.1
- `vswhom` 0.1.0
- `vswhom-sys` 0.1.3
- `want` 0.3.1
- `webkit2gtk` 2.0.2
- `webkit2gtk-sys` 2.0.2
- `webview2-com` 0.38.2
- `webview2-com-macros` 0.8.1
- `webview2-com-sys` 0.38.2
- `winnow` 0.7.15
- `winnow` 0.5.40
- `winnow` 1.0.4
- `winreg` 0.55.0
- `x11` 2.21.0
- `x11-dl` 2.21.0
- `zmij` 1.0.23

### MIT OR Unlicense

11 packages:

- `aho-corasick` 1.1.5
- `byteorder` 1.5.0
- `jiff` 0.2.35
- `jiff-core` 0.1.0
- `jiff-static` 0.2.35
- `jiff-tzdb` 0.1.8
- `jiff-tzdb-platform` 0.1.3
- `memchr` 2.8.3
- `same-file` 1.0.6
- `walkdir` 2.5.0
- `winapi-util` 0.1.11

### MPL-2.0

5 packages:

- `cssparser` 0.36.0
- `cssparser-macros` 0.6.1
- `dtoa-short` 0.3.5
- `option-ext` 0.2.0
- `selectors` 0.36.1

### UNKNOWN

2 packages:

- `lens-build-stamp` 0.1.0
- `p13-live-baseline` 0.1.0

### UNLICENSED

8 packages:

- `hover-probe` 0.1.0
- `lens` 0.1.0
- `lens-core` 0.1.0
- `lens-platform-macos` 0.1.0
- `lens-platform-windows` 0.1.0
- `lens-settings` 0.1.0
- `lens-storage` 0.1.0
- `lens-translate` 0.1.0

### Unicode-3.0

18 packages:

- `icu_collections` 2.3.0
- `icu_locale_core` 2.3.0
- `icu_normalizer` 2.3.0
- `icu_normalizer_data` 2.3.0
- `icu_properties` 2.3.0
- `icu_properties_data` 2.3.0
- `icu_provider` 2.3.1
- `litemap` 0.8.3
- `potential_utf` 0.1.6
- `tinystr` 0.8.4
- `writeable` 0.6.4
- `yoke` 0.8.3
- `yoke-derive` 0.8.2
- `zerofrom` 0.1.8
- `zerofrom-derive` 0.1.7
- `zerotrie` 0.2.5
- `zerovec` 0.11.8
- `zerovec-derive` 0.11.6

### Zlib

2 packages:

- `foldhash` 0.2.0
- `zlib-rs` 0.6.7

## Full Dependency List

| Package | Version | License |
|---------|---------|----------|
| `adler2` | 2.0.1 | 0BSD OR Apache-2.0 OR MIT |
| `ahash` | 0.8.12 | Apache-2.0 OR MIT |
| `aho-corasick` | 1.1.5 | MIT OR Unlicense |
| `alloc-no-stdlib` | 2.0.4 | BSD-3-Clause |
| `alloc-stdlib` | 0.2.4 | BSD-3-Clause |
| `android_system_properties` | 0.1.6 | Apache-2.0 OR MIT |
| `anyhow` | 1.0.104 | Apache-2.0 OR MIT |
| `async-trait` | 0.1.92 | Apache-2.0 OR MIT |
| `atk` | 0.18.2 | MIT |
| `atk-sys` | 0.18.2 | MIT |
| `atomic-waker` | 1.1.2 | Apache-2.0 OR MIT |
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT |
| `axum` | 0.8.9 | MIT |
| `axum-core` | 0.5.6 | MIT |
| `base64` | 0.21.7 | Apache-2.0 OR MIT |
| `base64` | 0.22.1 | Apache-2.0 OR MIT |
| `base64` | 0.23.1 | Apache-2.0 OR MIT |
| `bit-set` | 0.8.0 | Apache-2.0 OR MIT |
| `bit-vec` | 0.8.0 | Apache-2.0 OR MIT |
| `bitflags` | 2.13.1 | Apache-2.0 OR MIT |
| `bitflags` | 1.3.2 | Apache-2.0 OR MIT |
| `block-buffer` | 0.10.4 | Apache-2.0 OR MIT |
| `block2` | 0.6.2 | MIT |
| `brotli` | 8.0.4 | BSD-3-Clause AND MIT |
| `brotli-decompressor` | 5.0.3 | BSD-3-Clause OR MIT |
| `bs58` | 0.5.1 | Apache-2.0 OR MIT |
| `bumpalo` | 3.20.3 | Apache-2.0 OR MIT |
| `bytemuck` | 1.25.2 | Apache-2.0 OR MIT OR Zlib |
| `byteorder` | 1.5.0 | MIT OR Unlicense |
| `bytes` | 1.12.1 | MIT |
| `cairo-rs` | 0.18.5 | MIT |
| `cairo-sys-rs` | 0.18.2 | MIT |
| `camino` | 1.2.5 | Apache-2.0 OR MIT |
| `cargo-platform` | 0.1.9 | Apache-2.0 OR MIT |
| `cargo_metadata` | 0.19.2 | MIT |
| `cargo_toml` | 0.22.3 | Apache-2.0 OR MIT |
| `cc` | 1.4.4 | Apache-2.0 OR MIT |
| `cc` | 1.4.5 | Apache-2.0 OR MIT |
| `cesu8` | 1.1.0 | Apache-2.0 OR MIT |
| `cfb` | 0.7.3 | MIT |
| `cfg-expr` | 0.15.8 | Apache-2.0 OR MIT |
| `cfg-expr` | 0.20.9 | Apache-2.0 OR MIT |
| `cfg-if` | 1.0.4 | Apache-2.0 OR MIT |
| `chrono` | 0.4.45 | Apache-2.0 OR MIT |
| `combine` | 4.6.8 | MIT |
| `cookie` | 0.18.2 | Apache-2.0 OR MIT |
| `core-foundation` | 0.10.1 | Apache-2.0 OR MIT |
| `core-foundation-sys` | 0.8.7 | Apache-2.0 OR MIT |
| `core-graphics` | 0.25.0 | Apache-2.0 OR MIT |
| `core-graphics-types` | 0.2.0 | Apache-2.0 OR MIT |
| `cpufeatures` | 0.2.17 | Apache-2.0 OR MIT |
| `crc32fast` | 1.5.1 | Apache-2.0 OR MIT |
| `crossbeam-channel` | 0.5.17 | Apache-2.0 OR MIT |
| `crossbeam-utils` | 0.8.23 | Apache-2.0 OR MIT |
| `crypto-common` | 0.1.7 | Apache-2.0 OR MIT |
| `cssparser` | 0.36.0 | MPL-2.0 |
| `cssparser-macros` | 0.6.1 | MPL-2.0 |
| `ctor` | 0.8.0 | Apache-2.0 OR MIT |
| `ctor-proc-macro` | 0.0.7 | Apache-2.0 OR MIT |
| `darling` | 0.24.1 | MIT |
| `darling_core` | 0.24.1 | MIT |
| `darling_macro` | 0.24.1 | MIT |
| `dbus` | 0.9.12 | Apache-2.0 OR MIT |
| `defmt` | 1.1.1 | Apache-2.0 OR MIT |
| `defmt-macros` | 1.1.1 | Apache-2.0 OR MIT |
| `defmt-parser` | 1.0.0 | Apache-2.0 OR MIT |
| `deranged` | 0.5.8 | Apache-2.0 OR MIT |
| `derive_more` | 2.1.1 | MIT |
| `derive_more-impl` | 2.1.1 | MIT |
| `digest` | 0.10.7 | Apache-2.0 OR MIT |
| `dirs` | 6.0.0 | Apache-2.0 OR MIT |
| `dirs-sys` | 0.5.0 | Apache-2.0 OR MIT |
| `dispatch2` | 0.3.1 | Apache-2.0 OR MIT OR Zlib |
| `displaydoc` | 0.2.7 | Apache-2.0 OR MIT |
| `dlopen2` | 0.8.2 | MIT |
| `dlopen2_derive` | 0.4.3 | MIT |
| `dom_query` | 0.27.0 | MIT |
| `dpi` | 0.1.2 | Apache-2.0 AND MIT |
| `dtoa` | 1.0.11 | Apache-2.0 OR MIT |
| `dtoa-short` | 0.3.5 | MPL-2.0 |
| `dtor` | 0.3.0 | Apache-2.0 OR MIT |
| `dtor-proc-macro` | 0.0.6 | Apache-2.0 OR MIT |
| `dunce` | 1.0.5 | Apache-2.0 OR CC0-1.0 OR MIT-0 |
| `dyn-clone` | 1.0.20 | Apache-2.0 OR MIT |
| `embed-resource` | 3.0.11 | MIT |
| `embed_plist` | 1.2.2 | Apache-2.0 OR MIT |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT |
| `erased-serde` | 0.4.10 | Apache-2.0 OR MIT |
| `errno` | 0.3.14 | Apache-2.0 OR MIT |
| `fallible-iterator` | 0.3.0 | Apache-2.0 OR MIT |
| `fallible-streaming-iterator` | 0.1.9 | Apache-2.0 OR MIT |
| `fastrand` | 2.5.0 | Apache-2.0 OR MIT |
| `fdeflate` | 0.3.7 | Apache-2.0 OR MIT |
| `field-offset` | 0.3.6 | Apache-2.0 OR MIT |
| `find-msvc-tools` | 0.1.11 | Apache-2.0 OR MIT |
| `find-msvc-tools` | 0.1.12 | Apache-2.0 OR MIT |
| `flate2` | 1.1.10 | Apache-2.0 OR MIT |
| `fnv` | 1.0.7 | Apache-2.0 OR MIT |
| `foldhash` | 0.2.0 | Zlib |
| `foreign-types` | 0.5.0 | Apache-2.0 OR MIT |
| `foreign-types-macros` | 0.2.4 | Apache-2.0 OR MIT |
| `foreign-types-shared` | 0.3.1 | Apache-2.0 OR MIT |
| `form_urlencoded` | 1.2.2 | Apache-2.0 OR MIT |
| `futures-channel` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-core` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-executor` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-io` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-macro` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-sink` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-task` | 0.3.34 | Apache-2.0 OR MIT |
| `futures-util` | 0.3.34 | Apache-2.0 OR MIT |
| `gdk` | 0.18.2 | MIT |
| `gdk-pixbuf` | 0.18.5 | MIT |
| `gdk-pixbuf-sys` | 0.18.0 | MIT |
| `gdk-sys` | 0.18.2 | MIT |
| `gdkwayland-sys` | 0.18.2 | MIT |
| `gdkx11` | 0.18.2 | MIT |
| `gdkx11-sys` | 0.18.2 | MIT |
| `generic-array` | 0.14.7 | MIT |
| `getrandom` | 0.2.17 | Apache-2.0 OR MIT |
| `getrandom` | 0.3.4 | Apache-2.0 OR MIT |
| `getrandom` | 0.4.3 | Apache-2.0 OR MIT |
| `gio` | 0.18.4 | MIT |
| `gio-sys` | 0.18.1 | MIT |
| `gio-sys` | 0.21.5 | MIT |
| `glib` | 0.18.5 | MIT |
| `glib` | 0.21.5 | MIT |
| `glib-macros` | 0.18.5 | MIT |
| `glib-macros` | 0.21.5 | MIT |
| `glib-sys` | 0.18.1 | MIT |
| `glib-sys` | 0.21.5 | MIT |
| `glob` | 0.3.4 | Apache-2.0 OR MIT |
| `gobject-sys` | 0.18.0 | MIT |
| `gobject-sys` | 0.21.5 | MIT |
| `gtk` | 0.18.2 | MIT |
| `gtk-sys` | 0.18.2 | MIT |
| `gtk3-macros` | 0.18.2 | MIT |
| `hashbrown` | 0.14.5 | Apache-2.0 OR MIT |
| `hashbrown` | 0.17.1 | Apache-2.0 OR MIT |
| `hashbrown` | 0.12.3 | Apache-2.0 OR MIT |
| `hashlink` | 0.9.1 | Apache-2.0 OR MIT |
| `heck` | 0.4.1 | Apache-2.0 OR MIT |
| `heck` | 0.5.0 | Apache-2.0 OR MIT |
| `hex` | 0.4.3 | Apache-2.0 OR MIT |
| `hover-probe` | 0.1.0 | UNLICENSED |
| `html5ever` | 0.38.0 | Apache-2.0 OR MIT |
| `http` | 1.5.0 | Apache-2.0 OR MIT |
| `http-body` | 1.1.0 | MIT |
| `http-body-util` | 0.1.5 | MIT |
| `httparse` | 1.10.1 | Apache-2.0 OR MIT |
| `httpdate` | 1.0.3 | Apache-2.0 OR MIT |
| `hyper` | 1.11.1 | MIT |
| `hyper-util` | 0.1.20 | MIT |
| `iana-time-zone` | 0.1.65 | Apache-2.0 OR MIT |
| `iana-time-zone-haiku` | 0.1.2 | Apache-2.0 OR MIT |
| `ico` | 0.5.0 | MIT |
| `icu_collections` | 2.3.0 | Unicode-3.0 |
| `icu_locale_core` | 2.3.0 | Unicode-3.0 |
| `icu_normalizer` | 2.3.0 | Unicode-3.0 |
| `icu_normalizer_data` | 2.3.0 | Unicode-3.0 |
| `icu_properties` | 2.3.0 | Unicode-3.0 |
| `icu_properties_data` | 2.3.0 | Unicode-3.0 |
| `icu_provider` | 2.3.1 | Unicode-3.0 |
| `ident_case` | 1.0.1 | Apache-2.0 OR MIT |
| `idna` | 1.1.0 | Apache-2.0 OR MIT |
| `idna_adapter` | 1.2.2 | Apache-2.0 OR MIT |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT |
| `indexmap` | 1.9.3 | Apache-2.0 OR MIT |
| `indexmap` | 2.14.2 | Apache-2.0 OR MIT |
| `infer` | 0.19.0 | MIT |
| `ipnet` | 2.12.2 | Apache-2.0 OR MIT |
| `itoa` | 1.0.18 | Apache-2.0 OR MIT |
| `javascriptcore-rs` | 1.1.2 | MIT |
| `javascriptcore-rs-sys` | 1.1.1 | MIT |
| `jiff` | 0.2.35 | MIT OR Unlicense |
| `jiff-core` | 0.1.0 | MIT OR Unlicense |
| `jiff-static` | 0.2.35 | MIT OR Unlicense |
| `jiff-tzdb` | 0.1.8 | MIT OR Unlicense |
| `jiff-tzdb-platform` | 0.1.3 | MIT OR Unlicense |
| `jni` | 0.21.1 | Apache-2.0 OR MIT |
| `jni-sys` | 0.3.1 | Apache-2.0 OR MIT |
| `jni-sys` | 0.4.1 | Apache-2.0 OR MIT |
| `jni-sys-macros` | 0.4.1 | Apache-2.0 OR MIT |
| `js-sys` | 0.3.105 | Apache-2.0 OR MIT |
| `json-patch` | 3.0.1 | Apache-2.0 OR MIT |
| `jsonptr` | 0.6.3 | Apache-2.0 OR MIT |
| `keyboard-types` | 0.7.0 | Apache-2.0 OR MIT |
| `lens` | 0.1.0 | UNLICENSED |
| `lens-build-stamp` | 0.1.0 | UNKNOWN |
| `lens-core` | 0.1.0 | UNLICENSED |
| `lens-platform-macos` | 0.1.0 | UNLICENSED |
| `lens-platform-windows` | 0.1.0 | UNLICENSED |
| `lens-settings` | 0.1.0 | UNLICENSED |
| `lens-storage` | 0.1.0 | UNLICENSED |
| `lens-translate` | 0.1.0 | UNLICENSED |
| `libappindicator` | 0.9.0 | Apache-2.0 OR MIT |
| `libappindicator-sys` | 0.9.0 | Apache-2.0 OR MIT |
| `libc` | 0.2.189 | Apache-2.0 OR MIT |
| `libdbus-sys` | 0.2.7 | Apache-2.0 OR MIT |
| `libloading` | 0.7.4 | ISC |
| `libredox` | 0.1.23 | MIT |
| `libsqlite3-sys` | 0.30.1 | MIT |
| `linux-raw-sys` | 0.12.1 | Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT |
| `litemap` | 0.8.3 | Unicode-3.0 |
| `lock_api` | 0.4.14 | Apache-2.0 OR MIT |
| `log` | 0.4.34 | Apache-2.0 OR MIT |
| `markup5ever` | 0.38.0 | Apache-2.0 OR MIT |
| `matchit` | 0.8.4 | BSD-3-Clause AND MIT |
| `memchr` | 2.8.3 | MIT OR Unlicense |
| `memoffset` | 0.9.1 | MIT |
| `mime` | 0.3.17 | Apache-2.0 OR MIT |
| `miniz_oxide` | 0.8.9 | Apache-2.0 OR MIT OR Zlib |
| `miniz_oxide` | 0.9.1 | Apache-2.0 OR MIT OR Zlib |
| `mio` | 1.2.3 | MIT |
| `muda` | 0.19.3 | Apache-2.0 OR MIT |
| `ndk` | 0.9.0 | Apache-2.0 OR MIT |
| `ndk-sys` | 0.6.0+11769913 | Apache-2.0 OR MIT |
| `new_debug_unreachable` | 1.0.6 | MIT |
| `num-conv` | 0.2.2 | Apache-2.0 OR MIT |
| `num-traits` | 0.2.19 | Apache-2.0 OR MIT |
| `num_enum` | 0.7.6 | Apache-2.0 OR BSD-3-Clause OR MIT |
| `num_enum_derive` | 0.7.6 | Apache-2.0 OR BSD-3-Clause OR MIT |
| `objc2` | 0.6.4 | MIT |
| `objc2-app-kit` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-cloud-kit` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-data` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-foundation` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-graphics` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-image` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-location` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-text` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-core-video` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-encode` | 4.1.0 | MIT |
| `objc2-exception-helper` | 0.1.1 | Apache-2.0 OR MIT OR Zlib |
| `objc2-foundation` | 0.3.2 | MIT |
| `objc2-io-surface` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-javascript-core` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-quartz-core` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-security` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-ui-kit` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-user-notifications` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `objc2-web-kit` | 0.3.2 | Apache-2.0 OR MIT OR Zlib |
| `once_cell` | 1.21.4 | Apache-2.0 OR MIT |
| `option-ext` | 0.2.0 | MPL-2.0 |
| `p13-live-baseline` | 0.1.0 | UNKNOWN |
| `pango` | 0.18.3 | MIT |
| `pango-sys` | 0.18.0 | MIT |
| `parking_lot` | 0.12.5 | Apache-2.0 OR MIT |
| `parking_lot_core` | 0.9.12 | Apache-2.0 OR MIT |
| `percent-encoding` | 2.3.2 | Apache-2.0 OR MIT |
| `phf` | 0.13.1 | MIT |
| `phf_codegen` | 0.13.1 | MIT |
| `phf_generator` | 0.13.1 | MIT |
| `phf_macros` | 0.13.1 | MIT |
| `phf_shared` | 0.13.1 | MIT |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT |
| `pkg-config` | 0.3.34 | Apache-2.0 OR MIT |
| `plist` | 1.10.1 | MIT |
| `png` | 0.17.16 | Apache-2.0 OR MIT |
| `png` | 0.18.1 | Apache-2.0 OR MIT |
| `portable-atomic` | 1.15.0 | Apache-2.0 OR MIT |
| `portable-atomic-util` | 0.2.8 | Apache-2.0 OR MIT |
| `potential_utf` | 0.1.6 | Unicode-3.0 |
| `powerfmt` | 0.2.0 | Apache-2.0 OR MIT |
| `precomputed-hash` | 0.1.1 | MIT |
| `proc-macro-crate` | 1.3.1 | Apache-2.0 OR MIT |
| `proc-macro-crate` | 2.0.0 | Apache-2.0 OR MIT |
| `proc-macro-crate` | 3.5.0 | Apache-2.0 OR MIT |
| `proc-macro-error` | 1.0.4 | Apache-2.0 OR MIT |
| `proc-macro-error-attr` | 1.0.4 | Apache-2.0 OR MIT |
| `proc-macro2` | 1.0.107 | Apache-2.0 OR MIT |
| `quick-xml` | 0.42.0 | MIT |
| `quote` | 1.0.47 | Apache-2.0 OR MIT |
| `r-efi` | 5.3.0 | Apache-2.0 OR LGPL-2.1-or-later OR MIT |
| `r-efi` | 6.0.0 | Apache-2.0 OR LGPL-2.1-or-later OR MIT |
| `raw-window-handle` | 0.6.2 | Apache-2.0 OR MIT OR Zlib |
| `redox_syscall` | 0.5.18 | MIT |
| `redox_users` | 0.5.2 | MIT |
| `ref-cast` | 1.0.27 | Apache-2.0 OR MIT |
| `ref-cast-impl` | 1.0.27 | Apache-2.0 OR MIT |
| `regex` | 1.13.1 | Apache-2.0 OR MIT |
| `regex-automata` | 0.4.18 | Apache-2.0 OR MIT |
| `regex-syntax` | 0.8.11 | Apache-2.0 OR MIT |
| `reqwest` | 0.13.5 | Apache-2.0 OR MIT |
| `rusqlite` | 0.32.1 | MIT |
| `rustc-hash` | 2.1.3 | Apache-2.0 OR MIT |
| `rustc_version` | 0.4.1 | Apache-2.0 OR MIT |
| `rustix` | 1.1.5 | Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT |
| `rustversion` | 1.0.23 | Apache-2.0 OR MIT |
| `ryu` | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| `same-file` | 1.0.6 | MIT OR Unlicense |
| `schemars` | 0.8.22 | MIT |
| `schemars` | 0.9.0 | MIT |
| `schemars` | 1.2.2 | MIT |
| `schemars_derive` | 0.8.22 | MIT |
| `scopeguard` | 1.2.0 | Apache-2.0 OR MIT |
| `selectors` | 0.36.1 | MPL-2.0 |
| `semver` | 1.0.28 | Apache-2.0 OR MIT |
| `serde` | 1.0.229 | Apache-2.0 OR MIT |
| `serde-untagged` | 0.1.9 | Apache-2.0 OR MIT |
| `serde_core` | 1.0.229 | Apache-2.0 OR MIT |
| `serde_derive` | 1.0.229 | Apache-2.0 OR MIT |
| `serde_derive_internals` | 0.29.1 | Apache-2.0 OR MIT |
| `serde_json` | 1.0.151 | Apache-2.0 OR MIT |
| `serde_path_to_error` | 0.1.20 | Apache-2.0 OR MIT |
| `serde_repr` | 0.1.21 | Apache-2.0 OR MIT |
| `serde_spanned` | 0.6.9 | Apache-2.0 OR MIT |
| `serde_spanned` | 1.1.1 | Apache-2.0 OR MIT |
| `serde_urlencoded` | 0.7.1 | Apache-2.0 OR MIT |
| `serde_with` | 3.23.0 | Apache-2.0 OR MIT |
| `serde_with_macros` | 3.23.0 | Apache-2.0 OR MIT |
| `serialize-to-javascript` | 0.1.2 | Apache-2.0 OR MIT |
| `serialize-to-javascript-impl` | 0.1.2 | Apache-2.0 OR MIT |
| `servo_arc` | 0.4.3 | Apache-2.0 OR MIT |
| `sha2` | 0.10.9 | Apache-2.0 OR MIT |
| `shlex` | 2.0.1 | Apache-2.0 OR MIT |
| `signal-hook-registry` | 1.4.8 | Apache-2.0 OR MIT |
| `simd-adler32` | 0.3.10 | MIT |
| `siphasher` | 1.0.3 | Apache-2.0 OR MIT |
| `slab` | 0.4.12 | MIT |
| `smallvec` | 1.15.2 | Apache-2.0 OR MIT |
| `smallvec` | 1.16.0 | Apache-2.0 OR MIT |
| `socket2` | 0.6.5 | Apache-2.0 OR MIT |
| `softbuffer` | 0.4.8 | Apache-2.0 OR MIT |
| `soup3` | 0.5.0 | MIT |
| `soup3-sys` | 0.5.0 | MIT |
| `stable_deref_trait` | 1.2.1 | Apache-2.0 OR MIT |
| `string_cache` | 0.9.0 | Apache-2.0 OR MIT |
| `string_cache_codegen` | 0.6.1 | Apache-2.0 OR MIT |
| `strsim` | 0.11.1 | MIT |
| `swift-rs` | 1.0.8 | Apache-2.0 OR MIT |
| `syn` | 2.0.119 | Apache-2.0 OR MIT |
| `syn` | 3.0.4 | Apache-2.0 OR MIT |
| `syn` | 1.0.109 | Apache-2.0 OR MIT |
| `syn` | 3.0.5 | Apache-2.0 OR MIT |
| `sync_wrapper` | 1.0.2 | Apache-2.0 |
| `synstructure` | 0.13.2 | MIT |
| `system-deps` | 6.2.2 | Apache-2.0 OR MIT |
| `system-deps` | 7.0.8 | Apache-2.0 OR MIT |
| `tao` | 0.35.3 | Apache-2.0 |
| `tao-macros` | 0.1.4 | Apache-2.0 OR MIT |
| `target-lexicon` | 0.12.16 | Apache-2.0 WITH LLVM-exception |
| `target-lexicon` | 0.13.5 | Apache-2.0 WITH LLVM-exception |
| `tauri` | 2.11.5 | Apache-2.0 OR MIT |
| `tauri-build` | 2.6.3 | Apache-2.0 OR MIT |
| `tauri-codegen` | 2.6.3 | Apache-2.0 OR MIT |
| `tauri-macros` | 2.6.3 | Apache-2.0 OR MIT |
| `tauri-plugin` | 2.6.3 | Apache-2.0 OR MIT |
| `tauri-plugin-wdio-webdriver` | 1.4.0 | MIT |
| `tauri-runtime` | 2.11.3 | Apache-2.0 OR MIT |
| `tauri-runtime-wry` | 2.11.4 | Apache-2.0 OR MIT |
| `tauri-utils` | 2.9.3 | Apache-2.0 OR MIT |
| `tauri-winres` | 0.3.6 | MIT |
| `tempfile` | 3.27.0 | Apache-2.0 OR MIT |
| `tendril` | 0.5.1 | Apache-2.0 OR MIT |
| `thiserror` | 1.0.69 | Apache-2.0 OR MIT |
| `thiserror` | 2.0.20 | Apache-2.0 OR MIT |
| `thiserror-impl` | 1.0.69 | Apache-2.0 OR MIT |
| `thiserror-impl` | 2.0.20 | Apache-2.0 OR MIT |
| `time` | 0.3.55 | Apache-2.0 OR MIT |
| `time-core` | 0.1.9 | Apache-2.0 OR MIT |
| `time-macros` | 0.2.32 | Apache-2.0 OR MIT |
| `tinystr` | 0.8.4 | Unicode-3.0 |
| `tinyvec` | 1.13.2 | Apache-2.0 OR MIT OR Zlib |
| `tinyvec_macros` | 0.1.1 | Apache-2.0 OR MIT OR Zlib |
| `tokio` | 1.53.1 | MIT |
| `tokio-macros` | 2.7.2 | MIT |
| `tokio-util` | 0.7.19 | MIT |
| `toml` | 0.8.23 | Apache-2.0 OR MIT |
| `toml` | 0.9.12+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml` | 1.1.5+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml_datetime` | 0.6.11 | Apache-2.0 OR MIT |
| `toml_datetime` | 0.7.5+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml_datetime` | 1.1.1+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml_edit` | 0.22.27 | Apache-2.0 OR MIT |
| `toml_edit` | 0.19.15 | Apache-2.0 OR MIT |
| `toml_edit` | 0.20.7 | Apache-2.0 OR MIT |
| `toml_edit` | 0.25.13+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml_parser` | 1.1.3+spec-1.1.0 | Apache-2.0 OR MIT |
| `toml_write` | 0.1.2 | Apache-2.0 OR MIT |
| `toml_writer` | 1.1.2+spec-1.1.0 | Apache-2.0 OR MIT |
| `tower` | 0.5.3 | MIT |
| `tower-http` | 0.6.11 | MIT |
| `tower-layer` | 0.3.3 | MIT |
| `tower-service` | 0.3.3 | MIT |
| `tracing` | 0.1.44 | MIT |
| `tracing-attributes` | 0.1.31 | MIT |
| `tracing-core` | 0.1.36 | MIT |
| `tray-icon` | 0.24.2 | Apache-2.0 OR MIT |
| `try-lock` | 0.2.5 | MIT |
| `typeid` | 1.0.3 | Apache-2.0 OR MIT |
| `typenum` | 1.20.1 | Apache-2.0 OR MIT |
| `unic-char-property` | 0.9.0 | Apache-2.0 OR MIT |
| `unic-char-range` | 0.9.0 | Apache-2.0 OR MIT |
| `unic-common` | 0.9.0 | Apache-2.0 OR MIT |
| `unic-ucd-ident` | 0.9.0 | Apache-2.0 OR MIT |
| `unic-ucd-version` | 0.9.0 | Apache-2.0 OR MIT |
| `unicode-ident` | 1.0.24 | (Apache-2.0 OR MIT) AND Unicode-3.0 |
| `unicode-segmentation` | 1.13.3 | Apache-2.0 OR MIT |
| `url` | 2.5.8 | Apache-2.0 OR MIT |
| `urlpattern` | 0.3.0 | MIT |
| `utf8_iter` | 1.0.4 | Apache-2.0 OR MIT |
| `uuid` | 1.26.0 | Apache-2.0 OR MIT |
| `vcpkg` | 0.2.15 | Apache-2.0 OR MIT |
| `version-compare` | 0.2.1 | MIT |
| `version_check` | 0.9.5 | Apache-2.0 OR MIT |
| `vswhom` | 0.1.0 | MIT |
| `vswhom-sys` | 0.1.3 | MIT |
| `walkdir` | 2.5.0 | MIT OR Unlicense |
| `want` | 0.3.1 | MIT |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT |
| `wasip2` | 1.0.4+wasi-0.2.12 | Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT |
| `wasm-bindgen` | 0.2.128 | Apache-2.0 OR MIT |
| `wasm-bindgen-futures` | 0.4.78 | Apache-2.0 OR MIT |
| `wasm-bindgen-macro` | 0.2.128 | Apache-2.0 OR MIT |
| `wasm-bindgen-macro-support` | 0.2.128 | Apache-2.0 OR MIT |
| `wasm-bindgen-shared` | 0.2.128 | Apache-2.0 OR MIT |
| `wasm-streams` | 0.5.0 | Apache-2.0 OR MIT |
| `web-sys` | 0.3.105 | Apache-2.0 OR MIT |
| `web_atoms` | 0.2.6 | Apache-2.0 OR MIT |
| `webkit2gtk` | 2.0.2 | MIT |
| `webkit2gtk-sys` | 2.0.2 | MIT |
| `webview2-com` | 0.38.2 | MIT |
| `webview2-com-macros` | 0.8.1 | MIT |
| `webview2-com-sys` | 0.38.2 | MIT |
| `winapi` | 0.3.9 | Apache-2.0 OR MIT |
| `winapi-i686-pc-windows-gnu` | 0.4.0 | Apache-2.0 OR MIT |
| `winapi-util` | 0.1.11 | MIT OR Unlicense |
| `winapi-x86_64-pc-windows-gnu` | 0.4.0 | Apache-2.0 OR MIT |
| `window-vibrancy` | 0.6.0 | Apache-2.0 OR MIT |
| `windows` | 0.62.2 | Apache-2.0 OR MIT |
| `windows` | 0.61.3 | Apache-2.0 OR MIT |
| `windows-collections` | 0.3.2 | Apache-2.0 OR MIT |
| `windows-collections` | 0.2.0 | Apache-2.0 OR MIT |
| `windows-core` | 0.62.2 | Apache-2.0 OR MIT |
| `windows-core` | 0.61.2 | Apache-2.0 OR MIT |
| `windows-future` | 0.3.2 | Apache-2.0 OR MIT |
| `windows-future` | 0.2.1 | Apache-2.0 OR MIT |
| `windows-implement` | 0.60.2 | Apache-2.0 OR MIT |
| `windows-interface` | 0.59.3 | Apache-2.0 OR MIT |
| `windows-link` | 0.2.1 | Apache-2.0 OR MIT |
| `windows-link` | 0.1.3 | Apache-2.0 OR MIT |
| `windows-numerics` | 0.3.1 | Apache-2.0 OR MIT |
| `windows-numerics` | 0.2.0 | Apache-2.0 OR MIT |
| `windows-result` | 0.4.1 | Apache-2.0 OR MIT |
| `windows-result` | 0.3.4 | Apache-2.0 OR MIT |
| `windows-strings` | 0.5.1 | Apache-2.0 OR MIT |
| `windows-strings` | 0.4.2 | Apache-2.0 OR MIT |
| `windows-sys` | 0.61.2 | Apache-2.0 OR MIT |
| `windows-sys` | 0.45.0 | Apache-2.0 OR MIT |
| `windows-sys` | 0.59.0 | Apache-2.0 OR MIT |
| `windows-targets` | 0.42.2 | Apache-2.0 OR MIT |
| `windows-targets` | 0.52.6 | Apache-2.0 OR MIT |
| `windows-threading` | 0.2.1 | Apache-2.0 OR MIT |
| `windows-threading` | 0.1.0 | Apache-2.0 OR MIT |
| `windows-version` | 0.1.7 | Apache-2.0 OR MIT |
| `windows_aarch64_gnullvm` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_aarch64_gnullvm` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_aarch64_msvc` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_aarch64_msvc` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_i686_gnu` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_i686_gnu` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_i686_gnullvm` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_i686_msvc` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_i686_msvc` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_x86_64_gnu` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_x86_64_gnu` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_x86_64_gnullvm` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_x86_64_gnullvm` | 0.52.6 | Apache-2.0 OR MIT |
| `windows_x86_64_msvc` | 0.42.2 | Apache-2.0 OR MIT |
| `windows_x86_64_msvc` | 0.52.6 | Apache-2.0 OR MIT |
| `winnow` | 0.7.15 | MIT |
| `winnow` | 0.5.40 | MIT |
| `winnow` | 1.0.4 | MIT |
| `winreg` | 0.55.0 | MIT |
| `wit-bindgen` | 0.57.1 | Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT |
| `writeable` | 0.6.4 | Unicode-3.0 |
| `wry` | 0.55.1 | Apache-2.0 OR MIT |
| `x11` | 2.21.0 | MIT |
| `x11-dl` | 2.21.0 | MIT |
| `yoke` | 0.8.3 | Unicode-3.0 |
| `yoke-derive` | 0.8.2 | Unicode-3.0 |
| `zerocopy` | 0.8.56 | Apache-2.0 OR BSD-2-Clause OR MIT |
| `zerocopy` | 0.8.57 | Apache-2.0 OR BSD-2-Clause OR MIT |
| `zerocopy-derive` | 0.8.56 | Apache-2.0 OR BSD-2-Clause OR MIT |
| `zerocopy-derive` | 0.8.57 | Apache-2.0 OR BSD-2-Clause OR MIT |
| `zerofrom` | 0.1.8 | Unicode-3.0 |
| `zerofrom-derive` | 0.1.7 | Unicode-3.0 |
| `zerotrie` | 0.2.5 | Unicode-3.0 |
| `zerovec` | 0.11.8 | Unicode-3.0 |
| `zerovec-derive` | 0.11.6 | Unicode-3.0 |
| `zlib-rs` | 0.6.7 | Zlib |
| `zmij` | 1.0.23 | MIT |
