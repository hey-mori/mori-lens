# A word with no counterpart, 2026-08-13

[`2026-08-11-one-word-and-its-counterpart.md`](2026-08-11-one-word-and-its-counterpart.md)
ended with a list of what it had not measured, and named this one exactly:

> A particle, an article, or a word the translation drops entirely is the case
> that will produce a confident wrong answer, and none was tried.

Then a selftest walked into it. The focused word was `workloom.orbit.app`, a
URL, and the card carried a `번역에서` row with fourteen characters in it. A URL
has no counterpart in a Korean sentence.

## The numbers

Six words, each with nothing in its translation to point at. Every one of them
should come back as nothing.

```
word                 first                 second                 kept
workloom.orbit.app   "workloom.orbit.app"  "workloom.orbit.app을"  -
the                  "각"                   "제공자"                -
a                    "캡처"                  "작"                   -
is                   "은"                   "은"                    -
on                   "올려"                  "포인터"                -
rectangle            "타입"                  "타입"                  타입

1 of 6 produced a counterpart where there is none to produce
```

## Two mechanisms, and both earned their place

**Disagreement caught four.** The URL is the nicest of them: one question
answered `workloom.orbit.app` and the other `workloom.orbit.app을`, differing by
the object particle Korean attaches to a foreign noun. Exact equality, which
[ADR-0013's neighbour](2026-08-11-one-word-and-its-counterpart.md) argued for
against containment, is what turns that one character into a refusal.

**Membership caught the fifth, on its own.** Both questions agreed that `is` is
`은`, which is the topic particle in the form a dictionary gives it. The
translation says `포인터는`, with `는`. `은` is not in the string, so the span was
refused by the check that has nothing to do with agreement.

That is worth recording because it was not the reason the membership check was
written. It was written against a model that invents text, and here it caught a
model that was very nearly right.

## The one that got through cannot happen

`rectangle` is not in `The acquirer returns a typed error`. Both questions
answered `타입`, they agreed, `타입` is in the translation, and it is 2 characters
of a 16 character sentence. Every check passed and the answer is wrong.

**The product cannot construct that request.** `DeepRequest::word` comes from
`segmented.word_text()`, which is a slice of the sentence being explained. A
word that is not in the sentence is a case this measurement can build and the
code cannot.

So the honest reading is **five of five on the cases that can occur**, and the
sixth is evidence about the guard rather than about the product. It is left in
because a guard that only works when its input is well formed is worth knowing
about, and because the day something else starts supplying that field, this row
is what says what happens.

## What this does not measure

- **Six cases, one direction, one model.** English into Korean.
- **Whether the four that disagreed would keep disagreeing.** Requests go out at
  `temperature: 0`, so the same question gives the same answer today, and a
  model release is a different machine.
- **A word whose counterpart is real but rare**, where a refusal costs the
  reader something. Everything here is a case where refusing is right, so
  nothing here says what the rule costs when it is wrong.
