# What a short source comes back as, 2026-08-18

A reader's own session log carried this line:

```
card  original=6 chars  translated=6 chars
```

Six characters in, six out. The guards say nothing could have checked it:
`reads_as_the_source` declines a source under eight words and
`never_reached_the_target` declines one under three, so **a one-word source
passes through with no check at all**.

That was an argument. This is the measurement, and it killed the argument and
found two other things.

## The echo is not happening

Nine short sources, `qwen2.5:7b`:

```
source                   in  out  what came back
stale                     5    3  부패한
empty                     5   48  #LENS-5a23bfc686af5a74#\n공백\n#LENS-5a23bfc686af5a7
commit                    6    2  커밋
the lease                 9    6  임대 계약서
a stale element          15    6  오래된 요소
LedgerDataSheetService   22   10  레저데이터시트서비스
export_csv               10   10  export_csv
windows-rs 0.62          15   15  windows-r스 0.62
OK                        2    7  확인했습니다.

came back exactly as sent   1 of 9
```

**One of nine, and it is the one where keeping it is correct.** `export_csv` is
an identifier and the prompt asks for identifiers verbatim. The short sources
are translated, and the 6-to-6 line in the log was a coincidence of length.

A rule built on the inference would have been a rule against nothing.

## The fence reached the card

```
#LENS-5a23bfc686af5a74#
공백
#LENS-5a23bfc686af5a7
```

`공백` is the correct answer for `empty`. The rest is the request's own frame,
with the closing marker cut short because the token budget ran out inside it.

The quick prompt says *no preamble, no notes, no quotes, no markers, no
explanation*, and **nothing checked**. There was no code anywhere that looked at
an answer for the fence.

`lens_core::translate::without_the_fence` takes it back off, on both quick paths
and on every field of a deep analysis. It cuts prefixes of **this request's own
fence** rather than anything beginning `#LENS-`, so a reader hovering over this
repository's own source keeps their text.

Refusing instead of stripping would have thrown away a correct translation to
punish a formatting mistake.

```
after   empty  →  공백          (2 chars)
        empty  →  [malformed]   when the reply is only the frame
```

The second is the honest outcome: nothing survives the strip, the retry finds
nothing either, and the reader gets an error rather than a card with markers on
it.

## The identifiers were transliterated, and that is not fixed here

```
LedgerDataSheetService   →  레저데이터시트서비스
windows-rs 0.62          →  windows-r스 0.62
```

The first is the exact failure `identifiers_in` was written against, and its
comment names this very token. The second is worse: **one character inside the
identifier**, leaving a string that looks almost right.

The mitigation lists identifiers in the prompt so the model copies them. It does
not hold when the source is short enough that the identifier is most of it. That
is a separate defect with a separate cause and it gets its own issue rather than
being folded into this one.

## What is not measured

- **Nine sources, one model, two runs.** The fence leak appeared once and became
  a `malformed` on the second run, so it is intermittent and its rate is
  unknown.
- **How often a real session hits it.** The reader's log has one candidate line
  and it turned out not to be this.
- **The deep path's fence.** The strip is applied there for the same reason, and
  a leak has not been observed on it.
