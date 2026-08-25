# What the shell costs the gate, 2026-08-18

[`what a webview weighs`](2026-08-13-what-a-webview-weighs.md) priced Tauri at
22 crates against 383 and said this about it:

> `scripts/check.sh` runs `cargo clippy --workspace --all-targets` and
> `cargo test --workspace`. A crate in this workspace is a crate the gate
> builds, so the gate inherits whatever the shell brings. The gate is run
> before every commit in this repository and its speed is a working condition
> rather than a detail.

**The first sentence is true and the conclusion drawn from it is not.** The gate
was never timed. This times it.

## The measurement

`apps/shell-spike` was made a workspace member, the gate run, and the exclusion
put back.

```
                                    crates in Cargo.lock   cargo clippy --workspace
as shipped                          22                     18s
Tauri as a member, first run        440                    264s
Tauri as a member, nothing changed  440                    16s
Tauri as a member, one core edit    440                    12s
```

**The working gate does not get slower.** Cargo does not rebuild a dependency
that has not changed, so 418 extra crates are compiled once and then sit there.
The 12 seconds is not a typo and not a warm-cache artefact: it is a real edit to
`lens-core`, which every other crate in the tree depends on.

Note the count is 440 rather than the 383 measured in a throwaway crate.
Workspace feature unification pulls in more, which is worth knowing separately
from what it costs.

## What it does cost

**One cold build of 264 seconds**, paid on a fresh clone, after
`cargo clean`, on a dependency update, and by CI on every run if CI ever stops
being manual.

**Disk.** The workspace target directory is 8.2 GB with the shell built into it,
and the spike's own tree is a further 2.6 GB. This machine has run out of disk
once during this project, so that is a real number rather than a footnote.

## What survives of the original argument, and it is the important half

> 22 to 383 is not a build-time detail; it is the difference between a product
> whose whole tree a person could read and one whose tree they trust.

**That stands, and it is now the whole of the case.** The dependency count is an
argument about what a reader of this repository can verify for themselves, not
an argument about how long a commit takes. Those were bundled together and only
one of them was ever measured.

Anyone deciding the shell should be deciding on the trust argument. If the
answer was going to be "the gate would be too slow", it is not.

## What is not measured

- **CI.** Manual-only today, so nothing here is paid on a push. If it is turned
  on, every run pays the 264 seconds unless the cache holds.
- **`cargo test --workspace`**, separately. Only clippy was timed, and the test
  step compiles the same tree.
- **A real shell rather than the spike.** `apps/shell-spike` has one window, one
  HTML file and 133 lines of Rust. A product window would add its own code and
  its own dependencies on top.
- **Whether 440 stays 440.** Tauri's own tree moves between versions.
