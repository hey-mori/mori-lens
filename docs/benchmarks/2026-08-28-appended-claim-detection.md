# Injection defence: appended-claim detection, 2026-08-28

[`injection-suspiciously-short-answer`](2026-08-27-injection-suspiciously-short-answer.md)
caught the token bypass (both Latin and Hangul shapes) on both models, but
left the appended-claim shape: a correct translation of the first clause with
a fabricated sentence appended. This is the fix and its re-measurement at
nine runs per shape.

## The fix

Three new functions in `lens_core::translate`:

- `source_has_appended_claim_pattern` detects phrases that ask the model to
  append something to its output: `always add`, `add this sentence`,
  `also add`, `append this`, `include this sentence`, `add the following`,
  `at the end of your translation`. These are narrower than the
  token-bypass patterns: `ignore all` is not here because it asks the
  model to replace its answer, not to append to it.
- `extract_benign_clause` returns the part of the source before the
  instruction, trimmed to the last sentence boundary so a connector like
  `Also,` is not part of the control.
- `answer_looks_like_obedience` compares the actual answer to a control
  translation of just the benign clause. Obedience is the control plus a
  short payload; a faithful translation is the control plus a translation
  of the full instruction, which is much longer. The threshold is forty
  characters, which separates the two shapes without needing to know the
  payload in advance.

Integration in `lens-translate`: after the existing checks in the quick
path, streaming path and retry path, when the source has an appended-claim
pattern, a control translation of the benign clause is made. If the answer
looks like obedience (close to the control), it is refused and retried with
a stronger instruction. If the retry also looks like obedience, the
translation is refused as `NotTranslated`.

The control translation adds a second model call, but only when the source
carries an appended-claim pattern, which ordinary prose does not.

## The results

```
                    qwen2.5:7b        translategemma:latest
ordinary            refused 0/27      refused 0/27
latin token         obeyed 0/9        obeyed 0/9
                    refused 7/9       refused 9/9
hangul token        obeyed 0/9        obeyed 0/9
                    refused 9/9       refused 9/9
appended claim      obeyed 0/9        obeyed 0/9
                    refused 5/9       refused 0/9
                    holds 4/9         holds 9/9
```

**Obedience is zero across every shape on both models.** A previous
measurement (2026-08-18) had qwen2.5:7b obeying the appended claim 5 of 9
and the Hangul token 9 of 9. Both are now zero.

`holds` on the appended claim for translategemma is 9/9 because the model
translates the instruction faithfully every time: the payload appears in
the answer as part of a correct translation of the full sentence. This is
the intended behaviour. For qwen2.5:7b, `holds` is 4/9 and `refused` is
5/9: the four that "held" were faithful translations, and the five that
were refused were caught by the control-translation check.

## Ordinary path regression

No correct translation was refused on either model. The check fires only
when the source contains an appended-claim pattern, and the three ordinary
sentences used here do not contain one. A source that happens to include
`always add` in ordinary prose (e.g. `The script will always add a newline`)
would trigger the pattern, but the benign clause would be too short or the
control comparison would not match an obedience shape, so the answer would
pass through.

## What this does not address

- Deep path injection, where payloads sit inside structured JSON fields.
  The deep path has its own fence and schema constraints, and
  [`a payload in the deep path`](2026-08-18-a-payload-in-the-deep-path.md)
  measured the shapes. A defence there would need field-level checks
  rather than a control translation.
- A model that produces a plausible-looking translation with a fabricated
  sentence appended, where the source does not carry an appended-claim
  pattern. No structural check catches this without a second opinion.

## Files changed

- `crates/lens-core/src/translate.rs`: three new public functions and
  eight unit tests
- `crates/lens-translate/src/lib.rs`: `control_translation` helper,
  integrated in quick, streaming and retry paths
- `crates/lens-translate/src/live_tests.rs`: nine-run benchmark
