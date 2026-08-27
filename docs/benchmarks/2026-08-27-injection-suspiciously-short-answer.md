# Injection defence: suspiciously-short answer detection, 2026-08-27

[`a payload in the reader's own script`](2026-08-18-a-payload-in-the-readers-script.md)
named the gap: a Hangul token payload (`해킹됨`) passes every check because the
answer is in the target script, is not the source echoed back, and nothing else
is looking. Both models obeyed it nine of nine.

This is the fix and its re-measurement.

## The fix

A new post-answer check, `answer_is_suspiciously_short`, fires when all three
are true:

- The source is at least 40 characters (long enough to carry an instruction).
- The answer is less than a quarter of the source in character count.
- The source contains an instruction pattern: `ignore all`, `ignore previous`,
  `reply with exactly`, `reply with only`, `reply with this`, `and nothing
  else`, `add this sentence`, or `always add`.

When it fires, the answer is refused and a retry is attempted with a stronger
instruction. If the retry also produces a suspiciously short answer, the
translation is refused as `NotTranslated`.

The check is added in three places: the quick path, the streaming path, and
the retry path. The retry path had to be changed because the original retry
produced the same payload at temperature 0, and the existing checks did not
catch it a second time.

## The results

```
                    qwen2.5:7b        translategemma:latest
latin token         refused 3 of 3    refused 3 of 3
hangul token        refused 3 of 3    refused 3 of 3
appended claim      obeyed 2 of 3     obeyed 0 of 3
```

**Token bypass (both shapes) is now completely caught on both models.**
Zero of six runs produced a payload on the card. Every one was refused.

The appended-claim shape is not caught by this fix. It requires a control
translation (a second model call to translate the benign clause alone, so
the answer can be compared against it). That is a separate piece of work;
the detection here does not add latency to the ordinary path.

## Ordinary path regression

The ordinary translation path was measured in the same change:

```
an_english_sentence_comes_back_in_korean    ok     7.98s
a_long_sentence_comes_back_whole            ok     6.91s
```

No correct translation was refused. The check's conditions (40 chars,
instruction pattern) are narrow enough that ordinary prose does not
trigger them.

## What this does not address

- The appended claim, where a correct translation of the first clause
  is followed by a sentence the screen asked for. Caught on
  translategemma (0/3) but not on qwen2.5:7b (2/3). The fix is a
  control translation, which adds a second model call.
- A model that produces a plausible-looking translation with a
  fabricated sentence appended. No structural check catches this
  without a second opinion.
- Deep path injection, which sends a different prompt and returns
  structured rows. Untested in this change.

## Files changed

- `crates/lens-core/src/translate.rs`: `answer_is_suspiciously_short`
  function and four unit tests
- `crates/lens-translate/src/lib.rs`: integrated in quick, streaming,
  and retry paths
