# ADR-0003 · A Rust core with thin platform adapters

**Status** · Accepted, 2026-08-04

## Context

Lens has to run on Windows and macOS, and the two operating systems
disagree about almost everything it touches: the accessibility API, the
window model, the coordinate space, the capture API, the OCR engine, and
the permission flow.

The naive structure is one application per platform. It duplicates the
hover state machine, the segmentation, the language routing, the cache,
and the request lifecycle, which is where the product's actual logic
lives and where its bugs will be.

The opposite naive structure is one cross-platform toolkit doing
everything. That fails at the overlay ([ADR-0004](ADR-0004-native-overlay.md))
and at the accessibility layer, both of which need precise native
behaviour that a portable abstraction erases.

Language choice is nearly forced. The core has to call Win32 COM and,
later, Objective-C interfaces; it has to be cheap enough to run in a
tray process at idle; and it must not introduce a garbage collector into
a 300ms interaction loop. Rust reaches all of it, `windows-rs` is a
first-party binding maintained by Microsoft, and Mori's `src-tauri`
already proves the UI Automation path from Rust on this machine.

## Decision

**One Rust workspace: a platform-neutral core, plus one thin adapter
crate per operating system that implements the core's traits.**

```
crates/lens-core/           domain types, hover state machine,
                            segmentation, policy. No OS crates.
crates/platform-windows/    UI Automation, Win32. Implements the traits.
crates/platform-macos/      (later) AX, AppKit, via a small Swift bridge.
apps/*                      binaries that wire a core to an adapter.
```

The dependency arrow points one way. `lens-core` must not depend on
`windows`, `objc`, `tauri`, or any OS crate. When the core needs an OS
concept, the concept gets a plain Rust representation in the core and
the adapter converts at the boundary.

The seam is a set of traits, not a set of `#[cfg]` blocks inside shared
functions. A `cfg` seam looks cheap and then leaks: platform assumptions
end up inside domain logic, where they cannot be tested and cannot be
found.

## Consequences

- The macOS work is *adding a crate*, not rewriting a product. Whether
  that promise holds is testable: if `lens-core` ever fails to compile
  without an adapter, the seam has leaked.
- The core is fully unit testable against fake adapters, so the state
  machine, the stale-result rule, the cooldown, the secure-field refusal,
  and the segmentation all have real coverage without a desktop.
- The adapters have almost no coverage, because they cannot have any.
  This is accepted and compensated for by the probe
  ([ADR-0008](ADR-0008-probe-before-product.md)) and by the rule in
  `CLAUDE.md` that an adapter change is not verified by `cargo test`.
- macOS will need a small Swift bridge for AppKit and ScreenCaptureKit
  rather than raw Rust FFI. Wrapping AppKit lifetimes by hand at an
  `unsafe` boundary costs more in debugging than the bridge costs in
  build complexity. This does not change the seam: the bridge sits
  *inside* `platform-macos`, and the core never learns it exists.
