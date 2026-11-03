# Licensing Decision Memo

**Date**: 2026-09-25  
**Status**: Decision pending owner input

## Current State

The repository is **all rights reserved**. The README states:

> Not licensed for use. The author holds the copyright and has granted no
> permission to use, copy, modify or distribute this code.

This is accurate as of v0.1.0. Nothing has shipped publicly, and the repository is private.

## Future Licensing Options

When the repository is made public, the owner must choose a license. The options are:

### Option 1: Permissive (MIT or Apache-2.0)

**Pros:**
- Maximum adoption and contribution
- Compatible with all dependencies (all permissive)
- Standard for open-source Rust projects
- Easy to understand

**Cons:**
- Allows proprietary derivatives
- No copyleft protection

**Recommendation:** MIT or Apache-2.0 (dual-licensed)

### Option 2: Copyleft (GPL-3.0 or AGPL-3.0)

**Pros:**
- Ensures derivatives remain open
- Strong community protection

**Cons:**
- GPL-2.0-only is incompatible with Apache-2.0 dependencies; GPL-3.0 is compatible with them
- AGPL may deter commercial adoption
- More complex license compliance

**Note:** Would require verifying all dependencies are GPL-compatible.

### Option 3: Source-Available (BSL, SSPL, etc.)

**Pros:**
- Source visible but restricted use
- Protects commercial interests

**Cons:**
- Not OSI-approved open source
- May deter contributors
- Non-standard licenses create friction

## Dependency Analysis

All current dependencies use permissive licenses:
- Rust crates: MIT or Apache-2.0 for almost all of them. The app build
  (`apps/lens`) also pulls in five MPL-2.0 crates through Tauri's HTML
  tooling: `cssparser`, `cssparser-macros`, `dtoa-short`, `selectors` and
  `option-ext`. Checked with `cargo metadata` on 2026-11-03.
- JavaScript: MIT, MPL-2.0
- External providers (Ollama, LM Studio): MIT, user-installed

No strong-copyleft (GPL, LGPL, AGPL) dependency. MPL-2.0 is file-level copyleft: it binds changes to those crates' own files, which Lens uses unmodified, so it places no condition on the licence Lens itself chooses beyond keeping their notices. No licence choice faces a compatibility barrier.

## Decision Required

The owner must decide:

1. **When** to make the repository public
2. **Which license** to apply (if any)
3. **Whether** to apply it retroactively to v0.1.0 or only future versions

## Mechanical Preparation

Regardless of the decision, the repository is prepared:

- ✅ THIRD-PARTY-NOTICES.md documents all dependencies
- ✅ Asset provenance documented in docs/assets/README.md
- ✅ No strong-copyleft dependencies to complicate licensing (MPL-2.0 crates are used unmodified)
- ✅ All commits are from one person, under several commit identities (easy relicensing)

## Recommendation

Apply MIT OR Apache-2.0 dual license when making the repository public. This is the path of least resistance, maximizes adoption, and is compatible with all dependencies.

The decision is the owner's. This memo documents the options and preparation.
