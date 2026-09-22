# P14 Targeting Research Conclusion

| Field | Value |
|-------|-------|
| Date | 2026-09-22 |
| Issues | #14, #34 |
| Status | Research complete, no new production path identified |

## Question

Can UI Automation targeting be improved for applications where the accessibility
tree exposes text in containers too coarse for point resolution, without resorting
to OCR?

## Background

Mori Lens acquires text under the pointer using the platform accessibility tree
first, falling back to a small local OCR crop when the tree exposes nothing
useful. Most applications provide per-sentence or per-word elements that the
pointer can resolve. Two important targets do not:

- **VS Code** (issue #14): The editor document exists in the accessibility tree,
  but `ElementFromPoint` resolves to coarse containers holding hundreds of lines.
  Individual sentences cannot be targeted.

- **KakaoTalk** (issue #34): The conversation surface behaves as a single
  scrollable `Document` region. The pointer cannot resolve which message it is
  over.

Both cases raise the question: is there a UIA technique that provides finer
granularity without violating the product's latency and privacy constraints?

## Evidence

### VS Code (#14)

**What was measured (2026-08-04 through 2026-09-11):**

- Initial sweep: 247 points, 7 acquired (3%), all `50020` Text controls (window
  chrome), zero Document or Edit elements.
- Follow-up (2026-08-07): VS Code document text *does* exist in the
  accessibility tree. The tree holds three times as many distinct words as the
  screen shows. The problem is not absence of text but absence of point-targetable
  elements.
- `GetVisibleRanges` (GVR) measured (PR #356, 2026-09-09):
  - Container discovery: 100%
  - Latency p50: ~950ms
  - Latency p95: ~1070ms
  - Chain budget: ~400ms
  - GVR is 2.4x over budget at p50.

**What was not measured:**

- Whether VS Code responds to `WM_GETOBJECT` wake on a freshly launched editor
  (the 2026-08-04 sweep drove the wake explicitly but that was not a controlled
  test).
- Per-line resolution inside large text containers via alternative UIA text
  range APIs.

**Decision:**

Synchronous GVR production fallback rejected. Latency exceeds the interaction
budget by a factor that cannot be recovered through implementation improvements.
Geometry is useful as research evidence, not as a production path.

### KakaoTalk (#34)

**What was measured (2026-08-05):**

- Friend list sweep: 104 points, 14 acquired (13%), 2 on-target (14%), distance
  p50 57px, p95 221px. The conversation surface collapses to a single scrollable
  region.
- Conversation sweep: 128 points, 7 acquired, 0% on-target. Wake pass: same
  result.
- Additional windows: 80 and 72 points, all `unsupported`. No text pattern.

**What was not measured:**

- Direct KakaoTalk `GetVisibleRanges` latency. VS Code GVR timings (~950ms p50)
  are from VS Code, not KakaoTalk. They suggest GVR latency is
  provider-dependent but do not establish KakaoTalk's own latency.

**Decision:**

Line-unit expansion does not solve the geometry problem. The context range
already expands to a paragraph or line. The rectangles are 167-370px from the
pointer. A different unit of the same badly-located range does not help.

The distance gate correctly suppresses off-target hits. Lens shows nothing
rather than the wrong message. That is correct behaviour and it leaves the
product doing very little in the messenger a Korean reader is most likely to be
reading a second language in.

### Other applications

For contrast, measured 2026-08-04 to 2026-08-05:

- **Chromium-based applications** (Chrome, Edge): Per-word elements, high
  on-target rate.
- **Electron chat** (unspecified, 2026-08-05): 22 points, 22 distinct targets,
  each message and each word its own element.
- **Claude desktop** (Electron, 2026-08-04): `50020` at 69-75% on-target.

Electron itself is not the barrier. The problem is what specific applications do
inside their Electron shells.

## Targeting correctness

| Application | Point resolution | On-target rate | Production path |
|-------------|------------------|----------------|-----------------|
| VS Code editor | Coarse containers | ~3% | OCR fallback |
| KakaoTalk conversation | Single Document region | 0% | OCR fallback |
| Chromium | Per-word | High | Accessibility |
| Electron (general) | Varies by app | Variable | Accessibility |

## Latency

| Technique | Measured latency | Budget | Viable |
|-----------|------------------|--------|--------|
| Accessibility direct | 3-7ms p50 | 400ms | Yes |
| VS Code GVR | ~950ms p50 | 400ms | No |
| KakaoTalk GVR | Not measured | 400ms | Unknown |

## Privacy impact

Both VS Code and KakaoTalk fallback paths use OCR, which requires a small local
screen crop. This is the privacy-sensitive path that ADR-0002 reserves for cases
where accessibility provides nothing. The product correctly:

- Attempts accessibility first.
- Falls back to OCR only when accessibility gives nothing useful.
- Never records or transmits the cropped image.
- Requires explicit user action (Manual OCR shortcut) or per-app configuration
  to enable automatic OCR fallback.

## Production viability

No new accessibility-only production targeting path is justified by the current
evidence.

VS Code GVR exceeds the interaction budget. KakaoTalk's accessibility tree lacks
useful point granularity, and direct GVR remains unmeasured but there is no
evidence yet that it would provide a viable low-latency production path even if
measured.

The current product decision is:

- Retain accessibility-first acquisition.
- Retain conservative refusal for unprovable targeting (the distance gate
  correctly suppresses wrong targets).
- Retain explicit/manual OCR fallback per ADR-0002 for applications where
  accessibility does not provide point-targetable text.

## Future research boundary

This conclusion does not foreclose future investigation. New research would be
warranted if:

- A UIA API emerges that provides per-line or per-sentence resolution inside
  coarse containers at acceptable latency.
- VS Code or KakaoTalk change their accessibility implementations to expose
  finer-grained elements.
- A new application class presents a similar targeting problem with different
  characteristics.

Until then, the product's OCR fallback path is the correct and privacy-safe
response to these targeting limitations.

## Remaining open questions

- VS Code: Does the editor respond to `WM_GETOBJECT` wake differently on a
  freshly launched instance with a known file open? This is a cheap experiment
  but unlikely to change the production decision given the GVR latency evidence.
- KakaoTalk: What is direct GVR latency? This would require KakaoTalk installed
  and a conversation in front. Even if measured, the fundamental issue is that
  the conversation surface is a single Document region, so GVR may not provide
  per-message targeting regardless of latency.

These questions are documented but not blocking. The product decision stands.
