# Architecture decision records

An ADR records a decision that constrains later work, together with the
context that made it right and the consequences accepted along with it.
It is not a design document and not a plan. If a reader can change the
code without reading it, it is not an ADR.

## Why this repository keeps them separately

A design document describes what the software should do, and it is
rewritten whenever that changes. A decision record describes a choice
that was made once, and it must **not** be rewritten, because its value
is that it still says what was believed at the time.

Lens needs the second kind because its hardest choices are about the
operating system and are expensive to reverse. Choosing the
accessibility tree over OCR fixed the permission model, the idle cost,
the failure taxonomy, and the shape of the macOS adapter. Six months
from now the useful question is not "what do we do", it is "what did we
think we knew when we decided this, and is any of it still true". A
paragraph inside a living document cannot answer that, because it will
have been edited by then.

## Why the filenames carry `ADR-`

`ADR-0004-native-overlay.md` rather than `0004-native-overlay.md`, for a
mundane reason that compounds: these files are usually met as an editor
tab, a search result, or a link in an issue, all of which show the
filename without its directory. `0004-native-overlay` could be anything;
`ADR-0004-native-overlay` says what it is and that it is citable.

## Format

Each file is `ADR-NNNN-kebab-slug.md` and holds:

- **Status** · Accepted, Superseded by ADR-NNNN, or Proposed
- **Context** · what is true that forces a choice
- **Decision** · one sentence, then what it means concretely
- **Consequences** · what this costs, including what it makes harder

## Rules

- An ADR is never edited to change its decision. It is superseded by a
  new one, and the old one gets a `Superseded by` line. The record of a
  reversal is worth more than a tidy directory.
- A pull request that contradicts an accepted ADR does not merge. The
  superseding ADR lands first, on its own.
- Numbers are never reused.

## Index

| # | Decision | Status |
|---|----------|--------|
| [ADR-0001](ADR-0001-independent-repository.md) | Mori Lens is an independent repository | Accepted |
| [ADR-0002](ADR-0002-accessibility-first-acquisition.md) | Accessibility first, OCR as fallback | Accepted |
| [ADR-0003](ADR-0003-rust-core-platform-adapters.md) | A Rust core with thin platform adapters | Accepted |
| [ADR-0004](ADR-0004-native-overlay.md) | The overlay is a native window, not a webview | Accepted |
| [ADR-0005](ADR-0005-two-speed-ai.md) | Quick translation and deep analysis are separate paths | Accepted |
| [ADR-0006](ADR-0006-no-shared-package-yet.md) | No shared package with Mori yet | Accepted |
| [ADR-0007](ADR-0007-local-first-inference.md) | Local-first inference over an external Ollama | Accepted |
| [ADR-0008](ADR-0008-probe-before-product.md) | Measure the acquisition rate before building the product | Partly superseded by ADR-0010 |
| [ADR-0009](ADR-0009-pointer-distance-gate.md) | A resolution too far from the pointer is suppressed | Accepted |
| [ADR-0010](ADR-0010-acquisition-is-proven-enough.md) | Acquisition is proven enough to build on | Accepted |
| [ADR-0011](ADR-0011-glass-needs-direct2d.md) | The card is themed now and made of glass later | Accepted |
