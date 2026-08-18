# What a webview weighs, 2026-08-13

[#164](https://github.com/swarea/mori-lens/issues/164) stage two, first
question. Before asking whether Tauri v2 coexists with this process, it is worth
knowing what it brings with it, because one of the numbers turns out to be about
this repository rather than about Tauri.

Measured outside the tree, in a throwaway crate, so nothing here was committed
to find it out.

## The numbers

```
this workspace, today                     22 crates
a bare crate with `tauri` added          383 crates
tauri version                            2.11.5, webview2-com 0.38.2
cold release build of that bare crate    1m 40s
its target directory                     907 MB
```

**Twenty-two.** That is the whole of what Mori Lens builds on: no async runtime,
no HTTP client, no JSON crate, no CLI parser. `README.md` says it in passing,
about the engine: *spoken to over hand-written HTTP with no client dependency*.
The number is a property of this product and not an accident.

Adding a webview makes it **383**, and `--no-default-features` makes no
difference to that count.

## What that costs, specifically

**`scripts/check.sh` runs `cargo clippy --workspace --all-targets` and
`cargo test --workspace`.** A crate in this workspace is a crate the gate
builds, so the gate inherits whatever the shell brings. The gate is run before
every commit in this repository and its speed is a working condition rather
than a detail.

Two ways out exist and both are decisions rather than tricks: exclude the shell
from the default members, or put it in a second workspace. Either splits the
gate, which is its own cost.

**907 MB of build artefacts** for a crate whose `main` is empty. That is a
developer's disk rather than a reader's, and this machine ran out of disk once
already during this project.

## What is good news

**It builds here, clean, in 1m 40s from cold.** No toolchain hunting, no
WebView2 install, no C++ build tools prompt. That was the risk worth checking
first and it is not a risk.

## What this does not measure

**The binary a reader downloads.** The crate built here has an empty `main` and
links almost nothing, so its 119 KB says nothing. A real size needs a window,
which needs `tauri-build`, a config and a frontend, and that belongs with the
integration attempt rather than with a weighing.

**Coexistence**, which is the actual stage two question: whether a webview lives
in a process that already owns a Win32 message loop, a tray icon, global
hotkeys, a layered click-through overlay and a UI Automation apartment. Nothing
here touches it.

**Idle memory**, for the same reason. The floor to beat is in
[`2026-08-13-the-shell-baseline.md`](2026-08-13-the-shell-baseline.md): 53 to 55
MB, and a native window inside this process adds nothing measurable to it.

## What it means for the decision

Nothing here argues against the approved direction. **It prices it.**

The dependency count is the number to put in front of whoever decides, because
it is the one that changes something true about this repository rather than
something true about the shell. 22 to 383 is not a build-time detail; it is the
difference between a product whose whole tree a person could read and one whose
tree they trust.

If that price is worth paying, the next thing to measure is whether it works at
all beside the message loop, and that is the question the rest of stage two is.
