# An identifier that did not survive, 2026-08-18

[#207](https://github.com/hey-mori/mori-lens/issues/207) was filed with an escape
hatch: *a measurement that it does not happen when the identifier sits inside a
sentence would make this a curiosity about degenerate inputs rather than a
defect a reader meets.*

**It happens inside a sentence.**

```
LedgerDataSheetService   alone       레저데이터시트서비스
                         a sentence  레저데이터시트서비스는 한 장당 한 행씩 반환합니다.
```

Three runs of three, both arms. The sentence arm is what a hover actually
produces.

## The mitigation is running and does not work

`identifiers_in` finds `LedgerDataSheetService` by its camel hump, and the
prompt lists it so the model copies it rather than applying a rule. Its own
comment names this exact token as the failure it was written against.

So the token is found, named, and rewritten anyway.

## Naming it harder changes nothing

Four promptings, two tokens, three samples of three:

```
as shipped       9 of 18
system harder    9 of 18
tail             13 of 18
both             10 of 18
```

**`as shipped` and `system harder` scored exactly 9 of 18 in all three
samples.** That is not a coincidence, it is `temperature: 0`: the three runs
inside a sample are the same answer three times, so the real sample is two
tokens, and a fraction out of 18 reads as more evidence than it is.

Read honestly: one token survives under every wording and the other survives
under none, except that the tail placement sometimes rescues it. A wording that
works two samples out of three is not a fix.

The tail result is also the **opposite** of
[`does hardening the fence help`](2026-08-18-does-hardening-the-fence-help.md),
where a line after the material took an injection from 9 of 18 to 18 of 18.
Placement is not a rule that generalises, which is a reason to measure each one
rather than carry a lesson across.

## So the card says it instead

`identifiers_lost` compares the identifiers the prompt named against the answer,
and the card carries `이름 바뀜` when one did not come back.

The translation around the name is usually right, so keeping it costs the reader
nothing and throwing it away costs them a good sentence. What they cannot afford
is copying a name that does not exist.

**Containment, not equality.** Korean attaches particles, so a surviving
`export_csv` arrives as `export_csv를`, and a rule about whole words would call
that a loss on every answer that was right.

## The worst case is the one not covered

```
windows-rs 0.62  →  windows-r스 0.62
```

One character inside the name, leaving a string that looks almost right. **No
caveat appears for it**, because `identifiers_in` does not recognise a hyphen
and so never named the token.

Teaching it about hyphens would tell a model to keep `well-known` verbatim,
which is the same shape and an ordinary word. The two cannot be separated by
punctuation, so this is left uncovered and said out loud rather than half-fixed.

## What is not measured

- **Two tokens, one model, three samples**, and `temperature: 0` makes the runs
  inside a sample one answer rather than three.
- **How often a reader meets it.** Their session log has no line that would show
  this, because the log counts characters and never prints them.
- **What the caveat costs.** No measurement says how often `identifiers_lost`
  fires on an answer that was actually fine.
- **The deep path.** Identifiers appear in a `lemma` row and nothing checks
  them there.
