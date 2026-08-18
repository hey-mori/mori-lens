# Which model takes the wrong sense, 2026-08-18

[`2026-08-18-a-word-without-its-sentence.md`](2026-08-18-a-word-without-its-sentence.md)
found something it was not looking for. `The bank of the river had eroded` came
back from `qwen2.5:7b` as `강의 은행`, a financial bank on a river, **with the
sentence present and the river named**.

That is not a scope question. It is a model question, and it is the one axis of
a model comparison a machine can settle without a person: eight sentences, each
naming the thing that rules a reading out, and a count of how often the reading
it rules out comes back anyway.

**Counting wrong is sound. Counting right is not.** An answer that avoids the
listed wrong reading may still be a bad translation for a reason nothing here
looks at, so this reports failures only and prints every answer.

## The two installed models

```
qwen2.5:7b                    warm in 1.3s
  bank      WRONG  강의 은행이 한 주 동안의 비 후에 침식되었다.
  spring           애니메이션에서 스프링이 카드를 원래 위치보다 과하게 움직였습니다.
  mouse            마우스는 책상 위를 이동하여 화면의 가장자리에 도달했습니다.
  pitcher          물통은 오후에 키보드 옆에 있었습니다.
  seal             창틀에 붙인 도장이 깨져서 비가 들어왔습니다.
  letter    ----   [malformed]
  left             그는 회의를 조기 떠나고 트레이 아이콘은 남아 있었습니다.
  plant     ----   [malformed]
  took a ruled-out reading   1 of 6 answered, mean 1581ms

translategemma:latest         warm in 8.6s
  bank             강둑이 일주일간의 비로 인해 침식되었습니다.
  spring           애니메이션에서 사용된 스프링 때문에 카드가 원래 위치를 벗어났습니다.
  mouse            마우스는 책상 위에서 화면의 가장자리를 향해 움직였다.
  pitcher          물통이 키보드 옆에 온종일 놓여 있었다.
  seal             창틀에 있는 문틈이 깨져서 비가 들어왔다.
  letter           우표가 붙지 않은 상태로 우편으로 도착했습니다.
  left             그는 회의를 일찍 마치고, 트레이 아이콘은 그대로 남아 있었다.
  plant            책상 위에 있는 식물은 2주 동안 물을 주지 않았습니다.
  took a ruled-out reading   0 of 8 answered, mean 2419ms
```

Run twice. The counts reproduced exactly and the means moved by under 3%.

## The number that is not in the count

**`qwen2.5:7b` answered six of eight.** `letter` and `plant` came back
`malformed`, meaning the reply did not parse, twice, after the retry an empty
answer already gets. On screen that is an error card.

So the comparison is not 1 wrong against 0 wrong. It is **1 wrong and 2 failures
out of 8, against 0 and 0**.

## And the count undercounts

`seal` is scored as a pass for both models and neither deserves it. `qwen2.5:7b`
answered `도장`, a signet, on a window frame. `translategemma` answered `문틈`, a
door gap. The reading each sentence ruled out was `물개`, the animal, and neither
took it, so the counter saw nothing.

This is the same brittleness the earlier benchmark hit, where `left` came back
as a synonym of the wrong sense and slipped past a fixed string. **A fixed-string
judge finds the failures it was told to look for and no others.** The printed
answers are in the test output for exactly this reason.

## What it costs

`translategemma` is **50% slower per answer**, 2419ms against 1581ms, and takes
**8.6s to load** against 1.3s. The quick card's budget is about 1.5s of answer
after 320ms of dwell, so this is not a free swap.

## The finding that makes this urgent

```
installed ["qwen2.5:7b", "translategemma:latest"]
Lens would choose Some("qwen2.5:7b")
```

`choose_model` ranks by the parameter count in a model's name.
**`translategemma:latest` has no number in its name**, so `parameter_billions`
returns nothing and it is dropped before ranking begins.

The model that answered eight of eight is not merely unranked. It is
**unreachable by default**, and a reader who installed it on purpose would still
be served by the other one unless they named it in `settings.conf`.

## What this does not measure

- **Eight cases, one direction**, English in and Korean out, on one machine.
- **Which is the better translator.** Nothing here judges fluency, register, or
  whether a clause was dropped. It counts one specific failure.
- **Whether the ruled-out readings are the right list.** They are one reader's
  judgement of what each sentence forbids, and the `seal` row shows the list is
  not complete.
- **`malformed` at any depth.** Two failures were observed and nothing here says
  why the reply did not parse, or whether it is stable.
- **The deep path.** Both models were asked only quick translations.
