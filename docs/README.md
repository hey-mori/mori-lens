# Mori Lens Documentation

This directory contains the technical and product documentation for Mori Lens.

## Product Documentation

- **[mori-lens-project-goal.md](mori-lens-project-goal.md)** - What Mori Lens is, what it isn't, and the honest inventory of implemented versus planned capabilities. The authoritative source for capability claims.
- **[roadmap.md](roadmap.md)** - Milestone gates and what must be true before the next milestone starts. Each gate is evidence, not agreement.
- **[privacy-model.md](privacy-model.md)** - The privacy constraints that define the product. Written before the code that has to keep it.

## Architecture

- **[architecture.md](architecture.md)** - The pipeline, crate layout, provider architecture, and acquisition contract. Descriptive, not aspirational.
- **[decisions/](decisions/)** - Architectural Decision Records (ADRs). Constraints, not suggestions.
  - [ADR-0001](decisions/ADR-0001-independent-repository.md) - Independent repository
  - [ADR-0002](decisions/ADR-0002-accessibility-first-acquisition.md) - Accessibility-first acquisition
  - [ADR-0003](decisions/ADR-0003-rust-core-platform-adapters.md) - Rust core, platform adapters
  - [ADR-0004](decisions/ADR-0004-native-overlay.md) - Native overlay
  - [ADR-0005](decisions/ADR-0005-two-speed-ai.md) - Two-speed AI
  - [ADR-0006](decisions/ADR-0006-no-shared-package-yet.md) - No shared package yet
  - [ADR-0007](decisions/ADR-0007-local-first-inference.md) - Local-first inference
  - [ADR-0008](decisions/ADR-0008-probe-before-product.md) - Probe before product
  - [ADR-0011](decisions/ADR-0011-glass-needs-direct2d.md) - Glass needs Direct2D
  - [ADR-0012](decisions/ADR-0012-three-product-surfaces.md) - Three product surfaces
  - [ADR-0013](decisions/ADR-0013-the-local-library.md) - The local library
  - [ADR-0014](decisions/ADR-0014-three-tiers-of-history.md) - Three tiers of history
  - [ADR-0015](decisions/ADR-0015-the-library-schema.md) - The library schema
  - [ADR-0017](decisions/ADR-0017-signed-binary-distribution.md) - Signed binary distribution
  - [ADR-0018](decisions/ADR-0018-mori-process-boundary.md) - Mori process boundary
  - [ADR-0019](decisions/ADR-0019-provider-runtime.md) - Provider runtime

## Evidence and Measurement

- **[benchmarks/](benchmarks/)** - Latency, idle cost, model comparison, and acquisition measurements. 50+ measurement sessions.
- **[compatibility/windows-app-matrix.md](compatibility/windows-app-matrix.md)** - Per-application accessibility and OCR results, filled in by running the probe.
- **[evidence/](evidence/)** - Audit reports and evidence documents.

## Development

- **[commit-format.md](commit-format.md)** - Commit message format specification.
- **[git-workflow.md](git-workflow.md)** - Branch, pull request, and merge workflow.
- **[what-finished-means.md](what-finished-means.md)** - What "finished" means for this product.

## Assets

- **[assets/](assets/)** - Application icon and mark images.

## Research

- **[research/](research/)** - Hover text acquisition research and inherited context from Mori.
