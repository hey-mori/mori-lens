<!--
Keep this small enough to read in one sitting. If it touches more than a couple
of areas, it is usually two pull requests.
-->

## What

<!-- One or two sentences. The commit messages carry the detail. -->

## Why

Closes #

## How it was verified

<!--
A green `cargo test` is not evidence that an acquisition change works. UI
Automation cannot be asserted headlessly. Delete rows that do not apply.
-->

- [ ] `scripts/check.sh` (or `--full`) · paste the outcome, since remote CI is
      manual-only today and this is the actual gate
- [ ] **Adapter change** · probe run against named applications, pasted below,
      and transcribed into `docs/compatibility/windows-app-matrix.md`
- [ ] **Core change** · covered by unit tests, named below
- [ ] **Performance claim** · cites a file in `docs/benchmarks/`

<!--
Paste the probe output for anything touching crates/platform-*. Name the
applications and the DPI scaling. Never paste captured text.
-->

## Decisions

<!--
Does this contradict an accepted ADR? If so, the superseding ADR lands first,
on its own. If it establishes a new constraint, say which ADR should record it.
-->

## Notes

<!-- Trade-offs, follow-ups, anything deliberately left out. -->
