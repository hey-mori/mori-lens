# One word and its counterpart, 2026-08-11

The morning's measurement
([`2026-08-11-aligning-the-translation.md`](2026-08-11-aligning-the-translation.md))
asked a local model to split a sentence and its translation into constituents
and pair them. It said: verify the spans in code, drop what fails, and colour
what survives.

**That recommendation was wrong, and this is the measurement that says so.**

## Asking for less roles made it worse, not better

The first run asked for `{role, source, target}` and found the roles unreliable
while the linkage looked usable. The obvious next move was to ask for the
linkage alone.

```
                       pairs   source verbatim   target verbatim   whole sentences
with roles               20        100%               80%              2 of 5
linkage only, en->ko     28        100%               67%              2 of 5
linkage only, ko->en     23        100%               65%              0 of 5
```

Worse on every column that matters, and the first measurement of Korean as the
source is the worst of the three.

## And the spans that passed were nonsense

This is the finding. Every pair below is a substring of the string it claims to
come from, so **every one of them passes the check the morning proposed**:

```
'The'      -> '제공자'          'provider' -> '는'
'returns'  -> '각 렌더링된 줄당 하나의 직사각형을 반환합니다.'
'.'        -> '나가지 않습니다.'
'에'        -> 'does not have'
```

A full stop coloured the same as a verb phrase. An article coloured the same as
the noun, and the noun coloured the same as a particle.

**Membership is not correctness**, and for a whole-sentence parse the two are far
apart. Verify-and-drop protects against a model that invents text. It does not
protect against a model that pairs real text wrongly, which is what this one
does.

## The question the product can actually ask

Lens already knows which word the pointer is on. So ask about that word alone,
and about nothing else.

```
word         in it  not all  share   counterpart
empty        yes    yes      0.13    '비어'
clicking     yes    yes      0.13    '클릭하지'
machine      yes    yes      0.08    '기계'
sentence     yes    yes      0.06    '틀린'
rectangle    yes    yes      0.13    '직사각형'
acquirer     yes    yes      0.10    '취득자'
capture      yes    yes      0.09    '캡처'
trigger      yes    yes      0.19    '트리거'

usable   8 of 8 (100%)      median 1.3s
```

Eight of eight by the checks, against 67% for the parse, and **1.3 seconds
against 4.5**.

`share` is the counterpart's length over the translation's, because the
trivially correct answer to "which part means this" is "all of it", and a
counterpart that is most of the sentence highlights nothing.

**And one of those eight is wrong.** `sentence` came back as `틀린`, which means
*wrong*. The right answer is `문장`. It passes both checks and it is a false
thing to teach somebody learning the language.

## Asking twice catches it

Two differently framed questions, and a highlight only where they return the
**same span**. The second asks from the other end: not *what translates this
word* but *what would you translate back as this word*.

```
word        first        second       agree   right
empty       '비어'        '비어있었습니다'  no      yes
clicking    '클릭하지'     '클릭'         no      yes
machine     '기계'        '기계'         yes     yes
sentence    '틀린'        '틀린 문장'     no      NO
rectangle   '직사각형'     '직사각형'      yes     yes
acquirer    '취득자'      '취득자'        yes     yes
capture     '캡처'        '캡처'         yes     yes
trigger     '트리거'      '트리거'        yes     yes

agreed              5 of 8
  and were right    5
  and were wrong    0
disagreed and wrong 1
median for both     2.3s
```

**Nothing false survives, and the one error is exactly what the disagreement
lands on.**

### Exact equality, and not containment

Two of the three drops were right answers whose spans differed in extent:
`비어` against `비어있었습니다`, `클릭하지` against `클릭`. One contains the
other, so relaxing the rule to "either contains the other" would recover both.

**It would also recover `틀린` against `틀린 문장`**, which is the error. The
relaxation buys two correct highlights and one wrong one, and a wrong highlight
is the thing this feature must not produce. Exact equality is what keeps the
precision at 5 of 5.

## What this means for [#148](https://github.com/swarea/mori-lens/issues/148)

**The colour parse is not buildable on this model.** Not with verification, not
with dropping, not with a better prompt: three prompts were tried and the best
of them pairs a full stop with a verb.

**One word and its counterpart is buildable**, and it fits what the product
already is: the pointer is on a word, Lens knows which, and the answer is one
span. When the two questions disagree, nothing is highlighted, which is the same
discipline as everywhere else here. Unprovable is treated as unproven.

## The prompts above are not the ones the product can send

**The measured prompts name the source language.** "Here is an English sentence
and its Korean translation", "translate into English as". Lens does not know the
source language: acquisition reads characters and never a locale, and the only
language in a `DeepRequest` is the reader's own.

So the numbers above do not transfer, and the product's own prompts were
measured separately, by
`what_the_two_questions_keep` in `crates/lens-translate/src/live_tests.rs`:

```
  word         kept          first         second        right
  empty        "-"           "비어"          "비어있었습니다."    -
  clicking     "-"           "클릭하지 말고"     "클릭"          -
  machine      "기계"          "기계"          "기계"          yes
  sentence     "틀린"          "틀린"          "틀린"          NO
  rectangle    "직사각형"        "직사각형"        "직사각형"        yes
  acquirer     "취득자"         "취득자"         "취득자"         yes
  capture      "캡처"          "캡처"          "캡처"          yes
  trigger      "트리거"         "트리거"         "트리거"         yes

  kept 6 of 8, right 5 of 6
```

**Worse, and in the way that matters.** Naming the source language is what made
the second question independent enough to disagree about `sentence`; without it
both questions answer `틀린` and the error survives.

That is what ships: **one wrong row in six**, on a card that already carries
`AI 설명 · 사실 확인 필요`. It is worth saying plainly rather than rounding to
the benchmark's 5 of 5, and the live check above is in the tree so the figure is
re-measured rather than remembered.

## What has not been measured

- **Eight cases, one direction, one model, one judge.** "Right" above is one
  reader's opinion, and the reader wrote the sentences.
- **Korean as the source**, for the one-word question. The parse got worse in
  that direction and this may too.
- **Where it goes.** The quick card is where an original and a translation sit
  together, and it answers in 1.5s; adding 2.3s to it would more than double a
  hover. The deep path already costs seconds and already knows the focused word,
  but its card does not show the translation. That is a product decision and
  this measurement does not make it.
- ~~**A word with no counterpart.**~~ Measured on 2026-08-13:
  [`2026-08-13-a-word-with-no-counterpart.md`](2026-08-13-a-word-with-no-counterpart.md).
  Five of five refused on the cases the product can construct, by two different
  mechanisms. The prediction that it would produce a confident wrong answer was
  right about the one case the product cannot construct and wrong about the
  five it can.
