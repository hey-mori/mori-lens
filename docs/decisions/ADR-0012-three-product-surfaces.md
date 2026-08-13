# ADR-0012 · Three product surfaces

**Status** · Proposed, 2026-08-13
**Amends** · [ADR-0004](ADR-0004-native-overlay.md)

## Context

[ADR-0004](ADR-0004-native-overlay.md) decided that the overlay is a native
window rather than a webview, and in doing so it named two surfaces and
anticipated a third:

- **Passive tooltip**, which never activates and ignores mouse events. Built.
- **Interactive card**, "a real window the user pins or opens". Decided and
  **never built**, because a button inside a click-through window cannot be
  clicked and nobody made the second window.
- And directly: *"A webview is still allowed, and expected, for settings,
  history, and onboarding, which are ordinary windows the user opens on
  purpose. Tauri v2 is the likely choice there."*

So the shape has been on file since 2026-08-04. What has never been written down
is the **third surface's scope** and, more importantly, **the rule that decides
which surface a new thing belongs on**.

Without that rule, four capabilities became keyboard chords, because a chord is
what you reach for when there is nowhere to put a control: `Ctrl+Alt+D`
explains, `Ctrl+Alt+C` copies, `Ctrl+Alt+S` saves, `Ctrl+Alt+O` reads pixels.
Each is discoverable only when a card happens to name it.

Meanwhile the passive card now carries an original, a translation, up to five
labelled rows and a footer inside 300 logical pixels, with both halves of the
focused pair underlined. It is at the limit of what a tooltip can hold, and the
next thing added to it will be the thing that breaks it.

## Decision

**Three surfaces, and one rule.**

### 1. Passive hover card

Native. Click-through, never activates, no controls, no tabs, no durable write.
Unchanged by this ADR, and the reason for the other two is to keep it that way.

### 2. Pinned interactive detail

Native, exactly as ADR-0004 already decided. Opened **only** by an explicit
action, never by a dwell. **One at a time**: pinned details do not stack, so the
screen cannot fill with them.

It may hold the full original and translation, the explanation, the provider and
model, and the controls that have been living as chords: copy, save, note, open
in the library.

### 3. Main application window

An ordinary window opened from the tray, holding the library, review, settings,
diagnostics and onboarding.

**The first run may open it at onboarding. Later launches are tray-only.** A
hover works with it closed, and closing it leaves the product as it is today.

It may be a webview, which ADR-0004 already permits for exactly these screens.
**Tauri v2 is the direction**, and
[#164](https://github.com/swarea/mori-lens/issues/164) is the feasibility and
regression spike that confirms or refutes it rather than a comparison whose
answer is open.

### The rule

> **A control belongs on the first surface that can hold it without taking
> focus from what the reader is doing.**

The passive card can hold none. That is not a limitation to work around; it is
what the card is.

## Consequences

**Lens becomes an application rather than a tray tool.** The tray is still how
it runs, and there is now somewhere for a reader to go.

**The chords stop being the only way to reach a capability.** They stay, because
a reader who has learnt one should not lose it, but the pinned detail becomes
where a new control goes and the chord list stops growing.

**A new platform surface, twice.** A window is Win32 or a webview now and AppKit
later, and `lens-core` learns about neither
([ADR-0003](ADR-0003-rust-core-platform-adapters.md)). ADR-0004 already sketches
the macOS shape as a non-activating `NSPanel` hosting a webview.

**The pinned detail is the first surface that can take focus**, and therefore
the first that can be typed into. A note is the reader's own words rather than
screen text, and `docs/privacy-model.md` needs that distinction before a note
field exists.

**This ADR decides no storage, no history, no account and no signing.** Each has
its own issue and its own later record:
[#162](https://github.com/swarea/mori-lens/issues/162),
[#163](https://github.com/swarea/mori-lens/issues/163),
[#169](https://github.com/swarea/mori-lens/issues/169),
[#170](https://github.com/swarea/mori-lens/issues/170). An earlier draft of this
ADR ruled on all four and got two of them wrong, which is the argument for
keeping a decision record to one decision.

**It is not free, and it is not first.** This is the largest addition since the
overlay, and it competes with the work that stands between this product and its
first outside reader: [#140](https://github.com/swarea/mori-lens/issues/140) and
[#170](https://github.com/swarea/mori-lens/issues/170). It comes after those.
