# A word without its sentence, 2026-08-18

[#180](https://github.com/hey-mori/mori-lens/issues/180) proposes choosing the
scope on the hover: one modifier answers about the word under the pointer,
another about the sentence. The scope itself is free, because `SegmentedText`
already resolves both on every accepted hover.

What is not free is what gets **sent**. `QuickRequest::source_text` has carried
this comment since it was written:

> The sentence around the pointer. Always sent, even when the user is pointing
> at one word: a bare word makes a model guess, and it guesses confidently.

With an example attached, `gift` being a present in English and a poison in
German. **That was an assertion, not a measurement.** This is the measurement.

Six words whose Korean reading is decided by the sentence around them, each sent
twice on `qwen2.5:7b`: once inside its sentence, once alone.

## A word sent alone gets the wrong reading

```
word       forced reading  sent alone        took it
bank       둑               은행                NO
left       남               좌측                NO
lead       단서              리더                NO
spring     스프링             봄                 NO
current    현재              현재                yes
scale      크기              스케일               NO

lone word gave the reading its sentence forces   1 of 6
```

**One in six**, and the five failures fail in three different ways. `bank` and
`spring` took the other common sense. `left` took the other sense through a
synonym. `lead` took a **third** sense that was on neither list, coming back as
`리더`, a leader. `scale` was not translated at all, only transliterated.

The comment on the field is right, and now it is right with a number.

**And it is not even stably wrong.** A second run of the same test, same model,
same sentence, returned `주도권` for `lead`: initiative, a fourth sense, where
the first run said `리더`. The counts and the latencies reproduced within noise
(1 of 6, 9303ms against 5602ms) and that one cell did not. A reader hovering the
same word twice would get two different wrong answers.

## The saving is smaller than it looks

```
latency over all cases   in a sentence 9111ms · alone 5870ms
```

1518ms against 978ms per case. **About 540ms.**

An earlier run of the same test reported 25110ms against 6845ms, a fourfold
gap. That was the first request of the session against a cold model and it is
recorded here because a single cold run would have made the saving look four
times larger than it is.

So the trade on offer is **540ms for a wrong reading five times in six**.

## And the sentence does not always save it either

This was not the question, and it is the more uncomfortable result. The whole
sentence answers, which the test prints because this repository wrote them:

```
bank      강의 은행이 한 주 동안의 비 후에 침식되었다.
left      프로브가 나가면서 트레이 아이콘을 뒤에 남겨두었습니다.
lead      각 캡처는 OCR 경로가 따를 수 있는 리드입니다.
spring    애니메이션에서 스프링이 카드를 원래 위치보다 과하게 움직였습니다.
current   현재 구현은 접근성 트리부터 읽습니다.
scale     텍스트의 크기는 모니터별로 결정됩니다.
```

**`The bank of the river` came back as `강의 은행`**, a financial bank on a
river. The sentence was there, it named the river, and the model took the other
sense anyway. `lead` was transliterated rather than translated in the sentence
too.

Four of six sentence answers are right. The sentence is **necessary and not
sufficient**, and that belongs with the model comparison that is still open
rather than with this issue.

## What this settles for #180

The issue's own "What would overturn this" section named the outcome before the
run:

> A word-scoped request that drops the sentence would be faster and worse, so
> the scope may turn out to be about what the card **shows** rather than what
> the model is **asked**.

That is what happened. **Scope is a display decision.** A word-scoped hover
sends the same sentence it sends today, and the card shows the word and its
counterpart rather than the whole translation.

That path is already built and already measured:
[`2026-08-11-one-word-and-its-counterpart.md`](2026-08-11-one-word-and-its-counterpart.md)
keeps 6 of 8 and is right in 5 of those 6, by asking twice and highlighting only
where the two answers agree exactly. It costs a second question rather than a
cheaper one, which is the opposite of what the naive reading of #180 assumed.

## What this does not measure

- **One model, one direction, six words, one reader's judgement** of what each
  reading should be. One of the six expectations here was too narrow on the
  first run and was corrected: `scale` was expected as `배율` and the sentence
  answer said `크기`, which is right.
- **Whether a reader prefers the word card.** Nothing here asks that. It only
  says what the word card must be built from.
- **Korean as the source.** Every case is English in, Korean out.
- **Whether naming the word in the request helps the sentence answer**, which is
  a different question with its own live check already in the tree.
