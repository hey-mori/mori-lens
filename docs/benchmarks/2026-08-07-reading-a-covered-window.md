# Can a covered window be read, 2026-08-07

A reader asked whether OCR means the window has to be in front, and
whether that is the best that can be done.

It is three questions with three answers, and only the third needed
measuring.

## 1. Accessibility, by coordinate: yes, topmost, and that is correct

The product's hover path is `ElementFromPoint`, which answers with
whatever is **topmost** at a coordinate. So the window under the pointer
is the visible one by definition.

That is not a limitation to work around. The pointer is the question:
a reader points at text they can see, and text they cannot see is text
they are not pointing at.

## 2. Accessibility, by handle: no, it reads behind already

`hover-probe inventory` asks each window's provider what it holds by
**handle** rather than by coordinate, and its own output says what that
buys:

> Asks each window's provider what text it holds, by handle rather than
> by coordinate, so a covered window answers too.

So the accessibility half of this was never blocked by occlusion. What
is blocked is turning a handle answer into a *pointer* answer, which is
the product's question and not the tree's.

## 3. OCR: `BitBlt` cannot, `PrintWindow` can, and it works here

`capture::rectangle` copies from the desktop DC, so it sees whatever is
on top. That is the right thing for a band around the pointer.

`PrintWindow` with `PW_RENDERFULLCONTENT` is different: it asks the
window to render **itself** into an off-screen device context, so what
comes back is that window's content whatever is in front of it.

The usual objection is that it fails on GPU-composited surfaces, which
is exactly what Chromium and Electron are, and those are the two
applications this repository's matrix cares most about. So it was
measured rather than assumed.

Every top-level window on this desktop, one call each:

| Window | Size | Rendered | Ink |
|---|---|---|---:|
| chrome | 956x925 | yes | 100.0% |
| claude | 1536x958 | yes | 98.5% |
| WindowsTerminal | 1183x608 | yes | 99.2% |
| SourceTree | 1389x860 | yes | 99.8% |
| **Code** | 1440x900 | yes | **99.4%** |
| explorer | 1344x879 | yes | 95.6% |
| chrome | 1600x1000 | yes | 99.5% |
| Notepad | 761x583 | yes | 92.9% |
| ApplicationFrameHost | 1200x825 | yes | 99.2% |
| TextInputHost | 1536x960 | yes | 0.0% |
| explorer | 1600x1000 | yes | 7.9% |

Ink is the share of pixels differing from the corner pixel, which is a
crude "did anything get drawn". Shape only: nothing about what the
pixels contained is printed, and nothing is written anywhere
([`../privacy-model.md`](../privacy-model.md)).

**Chromium and Electron both rendered.** `TextInputHost` at 0.0% is the
IME host, which is a transparent surface with nothing on it, and the
7.9% `explorer` is a shell surface that is mostly one colour. Neither is
a failure of the mechanism.

Run it again with:

```
cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
  measure_whether_a_covered_window
```

## So it is possible. It is still not what the product should do.

Two reasons, and the first is the product one.

**The pointer is the question.** A capture of a window nobody is looking
at answers a question nobody asked. There is no hover over a covered
window, so there is nothing for the answer to attach to.

**The privacy model bounds a capture to a band around the pointer**, 760
by 180 pixels, 6.6% of a 1080p screen, and says so in a section about
how much of somebody's screen passes through memory. A tool that can
render whole windows the user cannot see is a different tool, and it is
the one this product is defined against.

## Where it does belong: the compatibility matrix

The matrix has a section called "The occlusion limit" that ends:

> The consequence for a compatibility session: bring one application
> forward, sweep it, then the next.

That limit is real friction and it has already produced a wrong reading.
On 2026-08-06 a sweep of VS Code reported 22% acquisition against the
2.8% on record, which looked like an overturned result and was 74% of
the window being covered
([`../compatibility/windows-app-matrix.md`](../compatibility/windows-app-matrix.md)).

`PrintWindow` does not help the **sweep**, because `ElementFromPoint`
still answers with the topmost window whatever the pixels say. It helps
the **OCR probe**, which today reads the foreground window and could
read a named one instead. That would make `hover-probe ocr --only Code`
answerable without rearranging the desktop, which is what issues #14 and
#34 have been waiting on a person to do.

Recorded as a measured possibility with a stated scope rather than
built, because building it into the probe is a change to a measurement
instrument and should be its own piece of work.
