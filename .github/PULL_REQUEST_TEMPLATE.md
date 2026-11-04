<!--
Title: `<type>: <lower-case imperative summary>`, describing the net change
rather than the branch. See docs/commit-format.md.

These headings are a floor, not a form. Replace them with ones that fit the
change, and write prose where prose is clearer. Three paragraphs that say what
happened beat five headings with one sentence under each.
-->

## What

<!-- One or two sentences. The commit messages carry the detail. -->

Closes #

## How it was verified

<!--
A green `cargo test` does not verify a platform adapter: UI Automation needs a
live desktop, so `cargo test` compiles crates/platform-* and exercises nothing.
Say which of code, test, or a live run each claim rests on.

- scripts/check.sh, with the outcome. It is the actual gate; CI runs the same
  script on a clean checkout
- Adapter change: a probe run against named applications, pasted, and
  transcribed into docs/compatibility/windows-app-matrix.md. Never paste
  captured text.
- Performance claim: cite a file in docs/benchmarks/
-->

## What went wrong on the way

<!--
The wrong belief that cost an hour is usually the most valuable thing here. A
pull request that narrates only the successful path teaches nobody.

Delete this heading only if nothing surprised you, which is rarer than it feels.
-->

## Notes

<!--
Trade-offs, follow-ups, anything deliberately left out.

If this contradicts an accepted decision record, stop: the superseding ADR
lands first, on its own. If it establishes a new constraint, say which ADR
should record it.
-->
