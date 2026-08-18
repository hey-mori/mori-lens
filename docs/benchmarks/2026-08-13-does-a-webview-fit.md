# Does a webview fit beside the message loop, 2026-08-13

[#164](https://github.com/swarea/mori-lens/issues/164) stage two, the question
that could not be answered outside the tree.

`lens.exe` owns a Win32 message loop, a tray icon, global hotkeys, a layered
click-through overlay on its own thread, and a UI Automation apartment. Tauri
wants the main thread's event loop and does not give it back. **That is a
structural conflict rather than an incidental one.**

`apps/shell-spike` starts everything from Tauri's `setup`, which runs on the
main thread before the event loop takes it, and uses the **real** components
rather than imitations: the overlay, the tray and the acquirer the product
ships. An imitation that coexists says nothing about the thing that has to.

## They coexist

```
overlay      started
tray         created
acquirer     built, so the apartment was shared
card         asked for at 80,80
hotkey       registered, delivery untested
```

Nothing refused. The overlay keeps its own thread and pump. The tray is created
on the main thread before Tauri takes it. `UiaTextAcquirer::new` succeeds, which
means the apartment Tauri had already initialised was one the acquirer could
live with, and that call site was written to tolerate `RPC_E_CHANGED_MODE` for
exactly this kind of reason.

## What it costs, and why the number is the best case

```
msedgewebview2 already running    19 processes, 499.5 MB
spike core process               13.1 MB
webview added                    0 processes, 19.4 MB
attributable total               32.6 MB
```

**This machine was already running nineteen WebView2 processes.** The runtime
pools them, so the spike added none and grew existing ones by 19.4 MB.

On a machine with no WebView2 application running, the first one pays for the
runtime itself. Nothing here measures that, and it is the number a reader with a
clean machine would actually pay.

Against the floor in
[`2026-08-13-the-shell-baseline.md`](2026-08-13-the-shell-baseline.md), 53 to 55
MB with a native window costing nothing measurable: **a webview shell takes idle
from about 54 MB to about 87 MB in the best case.**

## What is untested, and one of them matters

**Hotkey delivery.** `RegisterHotKey` succeeded. Whether `WM_HOTKEY` reaches the
handler needs a keypress, and a machine cannot press a key. The mechanism is
plausible, since Tauri's loop dispatches thread messages, and plausible is not
measured.

**Whether the card drew.** The overlay was asked to show one and nobody looked.
The welcome window shipped blank while its own live check passed, because that
check measured whether a pointer could reach the lesson and never whether an eye
could see it. A blank window owns its points exactly as well as a painted one.

**Tray clicks**, for the same reason as the hotkey.

**A hover, end to end**, with acquisition and a translation while the loop runs.
The acquirer was built and never asked a question.

**A clean machine**, per the memory section above.

## What this does not change

The spike is **excluded from the workspace** in the root manifest.
`scripts/check.sh` runs `cargo clippy --workspace` and `cargo test --workspace`,
and both still see 22 crates. Deleting `apps/shell-spike/` leaves the product
exactly as it is, and nothing in `apps/lens` refers to it.

## Where this leaves the decision

The approved direction survives its first real test. The structural conflict was
the risk and it is not one: Tauri takes the main thread and the parts of Lens
that needed a thread already had their own.

What is left before this becomes product code is the list above, and the two
entries on it that a machine cannot answer are the two that a reader would
notice first.
