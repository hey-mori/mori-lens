# When a model changes language partway, 2026-08-13

[#177](https://github.com/hey-mori/mori-lens/pull/177) found a card ending
mid-phrase. The cause was not the token budget: the model had switched to
Chinese partway through the answer, the third-language rule removed the tail,
and the surviving Korean went onto the card as though it were the whole thing.
The card now says `일부만 번역됨`.

**A caveat is the right answer to a rare fault and the wrong answer to a common
one.** If this happens to one sentence in three, a footer is not the fix. Nobody
had counted.

Seven sentences this repository wrote, 32 to 296 characters, `qwen2.5:7b`
against `translategemma`, which has been installed since 2026-08-06 and had
never been asked a question.

## The numbers

```
qwen2.5:7b                          translategemma
src   out   cut    secs             src   out   cut    secs
 32    16    -     14.7              32    17    -     27.7
 35    21    -      1.7              35    24    -      3.3
 53    25    -      1.7              53    31    -      3.4
103    46    -      2.0             103    54    -      3.6
132    79    -      2.9             132    61    -      3.4
181    94    -      3.9             181   109    -      3.9
296    89   YES     3.9             296   165    -      5.0

1 of 7 cut, median 2.9s             0 of 7 cut, median 3.6s
```

The first row of each column is a cold model load and not a translation. It is
left in because removing it would hide what a reader pays on their first hover
of a session, which is separately
[#140](https://github.com/hey-mori/mori-lens/issues/140)'s subject.

## What it says

**Rare rather than common, on the model in use.** One sentence in seven, and it
was the longest. So the caveat is a proportionate response and the fix that
shipped is the right size.

**And the other model did not do it at all.** `translategemma` came through all
seven whole, including the 296-character sentence that defeated `qwen2.5:7b`.
More than that: it produced **165 characters where qwen produced 89**, and it is
longer on five of the seven.

The cost is **0.7 seconds at the median**, 2.9 against 3.6.

## What this does not say

**Nothing about whether the Korean is better.** Longer is not better. This
measures completeness and length, which a machine can see, and says nothing
about accuracy, register, or whether a reader would rather have one than the
other. That is the M1-shaped question and it needs a person.

**Seven sentences, one direction, one machine, one run.** English into Korean.
Korean into English is untested for both, and the alignment benchmark found that
direction to be the worse one for `qwen2.5:7b`.

**Not a recommendation to change the default.** `choose_model` prefers the
larger model with a size tie-break, so `qwen2.5:7b` wins today. Switching on
completeness alone would be the same mistake as colouring a card from a
substring check: a measurement that is real and does not measure the thing that
matters.

## What would settle it

A reader looking at both answers for the same ten sentences without being told
which model produced which, and saying which they would rather have on a card.
That is cheap, it needs one person and twenty minutes, and it is the only
evidence that would justify moving the default.

Until then `translategemma` is a **candidate** and `qwen2.5:7b` is what ships.
