# ADR-0011 · The card is themed now and made of glass later

**Status** · Accepted, 2026-08-05

## Context

The card is a plain dark rectangle. Glass, in the sense Apple and Windows
11 both mean, suits this product: a lens over the screen ought to look
like one, and the material carries the idea that the answer is *above*
what is being read rather than replacing it.

Advice written for a React and Tauri product does not transfer. The card
is a native Win32 window painted with GDI
([ADR-0004](ADR-0004-native-overlay.md)), so `backdrop-filter`, CSS
tokens, and Tauri's window-effects API describe a surface Lens does not
have. Those apply to a settings window, which does not exist yet.

### What was measured, 2026-08-05, Windows 11 build 26200

Every material API succeeds on the card's own window:

```
DWMWA_SYSTEMBACKDROP_TYPE = TRANSIENTWINDOW  -> Ok(())    reads back as 3
DWMWA_WINDOW_CORNER_PREFERENCE = ROUND       -> Ok(())
DwmExtendFrameIntoClientArea(-1)             -> Ok(())
SetLayeredWindowAttributes(alpha 244)        -> Ok(())    backdrop still 3
```

So the platform is willing. That is not the obstacle.

**The obstacle is GDI.** Two things follow from painting with it:

- A DWM backdrop is drawn **behind** the window. The card currently
  fills its whole client area with an opaque brush, so the acrylic is
  there and completely hidden. Seeing it requires a transparent client
  area.
- GDI text has **no alpha**. On a surface that is transparent or
  extended into the frame, `DrawTextW` writes pixels with a zero alpha
  channel, which is the long-standing reason black text vanishes on
  glass. Antialiased edges are worse than the glyph bodies.

Neither is a bug to fix. They are what GDI is. Glass on a GDI surface
means fighting the drawing model on every redraw of a card that redraws
several times per hover.

There is a second, quieter argument for the same conclusion: this is a
**reading** tool. DirectWrite lays out and hints text visibly better
than `DrawTextW`, and it handles Hangul, Latin, and mixed runs in one
paragraph properly rather than by accident.

## Decision

**Split the work in two, and do the half that does not need a new
renderer first.**

1. **A theme is data, in `lens-core`.** Colours, sizes, spacing, and
   opacity are a `Theme` value with no platform types, chosen from a
   small set of named presets. It is pure, so contrast ratios and
   readability rules are unit-testable without a screen, and the macOS
   overlay will consume the same values.

2. **Presets, not a settings panel.** Four, and no editor for blur
   radius or shadow density. A user picking from finished looks gets a
   better result than one assembling one, and every knob is a
   combination somebody has to be responsible for.

3. **The card keeps GDI for now**, and takes the parts of the platform
   that cost nothing: rounded corners, and per-theme colour. It gets
   `DWMSBT_TRANSIENTWINDOW` only when its painting can survive it.

4. **Glass arrives with an alpha-correct renderer**, as its own change,
   behind the same `Theme`. Direct2D with DirectWrite is the leading
   candidate and not the only one: Direct2D and GDI interoperate through
   a shared surface, so the card's text could move without the whole
   painter being replaced. Which of those is right is a question for
   that change, not this one.

   Until then a theme's `translucency` is honoured by the uniform window
   alpha. That is a **weaker effect than glass and a different one**: it
   fades the text along with the surface, which is why every contrast
   figure in `lens-core::theme` composites the ink as well.

5. **The system's accessibility settings win over any preset.**
   Transparency-reduced, high-contrast, and animation-reduced are read
   from the OS and override the theme rather than appearing as
   preferences of their own.

## Consequences

- The card can look considerably better this week without a renderer
  rewrite, and the rewrite is not blocked by theming decisions later.
- **A second text renderer is real work**, and it is accepted here
  rather than discovered halfway through a visual change.
- Two painting paths will exist briefly. That is a cost, and it is
  bounded by the `Theme` boundary: the GDI path and the Direct2D path
  read the same values.
- The honest cost of deferring: until Direct2D lands, a theme called
  glass is a translucent rectangle. The preset names must not promise
  the material before it exists.
- If Direct2D turns out to interact badly with `WS_EX_NOACTIVATE` and
  the focus guarantee, **the focus guarantee wins** and glass does not
  ship. That property is not negotiable against an appearance.
