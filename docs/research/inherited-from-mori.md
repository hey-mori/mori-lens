# What Mori already proved, and what does not carry over

Mori ships a language capability that reads another application's
*selection* through UI Automation on Windows. Lens does not import that
code ([ADR-0001](../decisions/ADR-0001-independent-repository.md),
[ADR-0006](../decisions/ADR-0006-no-shared-package-yet.md)), but the facts Mori
measured on a live desktop are portable in a way a Rust module is not,
and re-discovering them would cost days.

Source, for anyone who wants the original:

- `swarea/mori` · `src-tauri/src/language/capture_windows.rs`
- `swarea/mori` · `docs/design/language-loop.md` §4 and §6

Everything below is a claim Mori verified by running, not by reading
documentation. Where Lens's situation differs, that is called out.

## 1. Provider behaviour, measured

Mori's probe results, from `language-loop.md` §4:

| Provider | Cold selection read | Note |
|----------|--------------------|------|
| WPF text control | full text | control type `Edit`, exact character count |
| Chromium / Electron | **nothing, until woken** | Chrome, VS Code, Slack, Claude Code |
| Win32 `EDIT` (WinForms, Notepad-class) | **no text pattern at all** | no accessibility text to read |

The second row is the important one, because a browser is where most
people meet a second language.

## 2. Chromium builds no accessibility tree until asked

Chromium, and therefore every Electron application, does not construct
an accessibility tree until it believes an assistive client is present.
A cold call returns a stub with no text pattern. **This is not a failed
read. The tree does not exist yet.**

The documented trigger is a `WM_GETOBJECT` message for `OBJID_CLIENT`.
Mori sends it only after an empty read, waits, and retries once.
Measured on a live Chrome window: `NoTextPattern` before, and a
`Document` control answering afterwards.

Mori's constants:

```
wake message timeout   200ms   SendMessageTimeoutW, SMTO_ABORTIFHUNG
settle after wake      140ms   before re-reading
```

`SMTO_ABORTIFHUNG` matters: this is a synchronous message into a
foreign, possibly wedged UI thread.

**Where Lens differs, and it is a real problem.** Mori pays this cost
once per deliberate keypress. Lens would pay it per hover, and 340ms of
wake plus settle is longer than the entire hover budget. So Lens cannot
simply copy the retry.

**What Lens built first, and what it measured.** The design written here
was *per-window, once*, remembered for the lifetime of the window. That
was built, and it produced a failure a reader described exactly:

> Alt을 눌렀는데 처음에는 반응이 없고, 두 번째에는 못한다고 하고,
> 세 번째에는 번역된 걸 보여주고

A renderer that has not finished building its tree fails the retry, and
*once* meant the second hover sent no message and made no retry at all.
The tree finished on its own while the reader tried again.

So the design is now **poll, throttled per window**: the message goes
out, the tree is asked for every 25ms until 420ms, and a window that did
not wake may be asked again after two seconds. Forgetting a failed wake
entirely was tried in between and was worse, because every failing point
then paid the full wait: a sweep managed 45 points in twenty seconds
before the live check that caught it gave up.

Both halves are affordable now for a reason that was not true when this
was written: acquisition runs on its own thread (issue #42), so waiting
costs the card latency and costs the pointer and the pause key nothing.

The measured cost of the wake message itself, taken 2026-08-06 across
four runs, is **about 0.1ms**. The 340ms in the paragraph above is the
message plus a fixed sleep, and the sleep was all of it.
See `benchmarks/2026-08-06-accessibility-wake.md`.

## 3. The element must be re-fetched after waking

The provider that answers after activation is a **different object**
from the stub that answered before it. Re-reading through the old
element handle returns the old, empty answer. Mori calls
`GetFocusedElement` a second time; Lens will need a second
`ElementFromPoint`.

## 4. COM apartments cannot be assumed

A call may arrive on a worker thread whose apartment we do not own.
Every entry point initialises COM for the duration of the call and
tolerates `RPC_E_CHANGED_MODE`, which means the thread already had a
usable apartment that is not ours to tear down. `S_FALSE` means COM was
already initialised on this thread and our uninitialise is still
required.

Mori's `ComScope` encodes this in an RAII guard so an early `?` cannot
leave a thread in an apartment it created. Lens re-derives the same
guard; the comment in `crates/platform-windows/src/com.rs` names the
origin.

## 5. The secure-field check comes first, and again

`IsPassword` is read **before** any text is materialised into a string,
so a password field's contents never exist as a Rust value. It is
checked again after a wake-and-retry, because that is a new element.

`IsPassword` is unsupported by many providers, and Mori treats an error
as "not secure" on the read path while refusing the more invasive
clipboard path outright when it cannot prove the field is safe. Lens
takes the stricter line everywhere, because it fires on hover rather
than on a keypress: **an unreadable secure check is treated as
protected.**

## 6. A diagnostic must never print the text

Mori's `CaptureProbe` reports how far the chain got, the control type,
and a character *count*. It cannot carry a selection, by construction.
A diagnostic that echoed whatever the user had highlighted would be a
worse privacy problem than the feature it checks. Lens's probe holds
the same line ([ADR-0008](../decisions/ADR-0008-probe-before-product.md)).

## 7. Placement is a pure function and should be tested as one

Mori's `overlay::place_overlay` is unit tested for below-right
preference, edge flipping, negative-origin secondary monitors, and an
overlay larger than its screen. It is the part of that feature that
never needed debugging against a live desktop. Lens puts placement in
`lens-core` for the same reason
([ADR-0004](../decisions/ADR-0004-native-overlay.md)).

## 8. Answer latency, measured on this hardware

From `language-loop.md` §6, with `qwen2.5:3b`:

| | total | first token | output tokens |
|---|---:|---:|---:|
| two sections | 1,370ms | 946ms | 40 |
| follow-up 구조 | 917ms | | 15 |
| follow-up 문법 | 1,142ms | | 29 |
| everything at once | 4,372ms | | |

This is the evidence behind [ADR-0005](../decisions/ADR-0005-two-speed-ai.md). It
also sets a realistic expectation: on this machine, a first token in
under a second is roughly the floor, so the overlay must draw its frame
before the translation arrives rather than waiting for it.

## 9. Build honesty

`custom-protocol` embeds the frontend at compile time, so rebuilding
only the web bundle changes nothing about a running desktop binary.
Mori records that this cost one round of "the fix does not work" against
a binary that predated the fix.

Lens has no webview in the hover path, which removes this failure mode
from the hot loop. It returns with the settings UI, and the lesson
generalises: **before believing a manual test, check that the binary
under test is newer than the change.**

## 10. What does not carry over at all

- **Hover is a harder question than selection.** `GetFocusedElement`
  has one unambiguous answer. `ElementFromPoint` plus `RangeFromPoint`
  must resolve an arbitrary coordinate to an element, expand to a word,
  find the enclosing sentence, and return rectangles that match what the
  user sees at every DPI scale. None of that is exercised by Mori.
- **Frequency changes the design.** Mori reads on a keypress, a few
  times an hour. Lens reads whenever a pointer settles. Every cost Mori
  can absorb once, Lens has to amortise or avoid.
- **Mori has no bounding rectangles.** It anchors on the cursor
  position. Lens needs the text's own rectangles to underline the source
  and to place a card that does not cover it.
- **Mori's clipboard fallback is not available to Lens.** Synthesising
  Ctrl+C into whatever is focused is defensible on an explicit keypress
  and indefensible on hover.
