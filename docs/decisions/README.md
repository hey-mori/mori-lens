# Architecture decision records

An ADR records a decision that constrains later work, together with the
context that made it right and the consequences accepted along with it.
It is not a design document and not a plan. If a reader can change the
code without reading it, it is not an ADR.

## Why this repository has them, and where the layout comes from

Mori records its decisions inside living design documents, which works
because Mori's decisions mostly concern its own screens. Lens is
different in one way that matters: **its hardest decisions are about the
operating system, and they are expensive to reverse.** Choosing the
accessibility tree over OCR determines the permission model, the idle
cost, the failure taxonomy, and the shape of the macOS adapter. A
decision like that needs a stable address that an issue can cite, not a
paragraph inside a document that gets rewritten as the design moves.

The directory name and the file naming follow
[`swarea/areabase`](https://github.com/swarea/areabase), which is the
sibling repository that already keeps ADRs and had settled on
`docs/decisions/ADR-NNNN-slug.md`. There is no reason for two
repositories in one organisation to disagree about where a decision
lives, and the one with existing practice wins.

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
| [ADR-0008](ADR-0008-probe-before-product.md) | Measure the acquisition rate before building the product | Accepted |
