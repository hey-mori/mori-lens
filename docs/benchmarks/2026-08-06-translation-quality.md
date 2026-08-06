# Translation quality, 2026-08-06

A reader said the translations were disappointing and asked whether a
different model was the answer. This measures it.

Machine: the same one every other benchmark in this directory used.
Build `791d2bf`. Four installed models, five sentences, `quick` in every
case so the prompt and the budget are the shipping ones.

Run it again with:

```
cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 \
  compare_what_each_installed
```

## What triggered it

The card, on `The test correctly refuses to pass vacuously.`:

> 테스트는 비어있게 통과하지 않습니다.

`correctly` is gone. That is not a clumsy rendering, it is a dropped
adverb, and the adverb is the point of the sentence.

## The five sentences, every installed model

Sources are English written in this repository. Printing them and their
translations is allowed here and nowhere near a hover:
[`../privacy-model.md`](../privacy-model.md) forbids printing what was
**read**, and a benchmark that cannot show its output is not one.

### 1. The test correctly refuses to pass vacuously.

| Model | Output |
|---|---|
| `translategemma` | 테스트는 올바르게 비어있는 상태를 거부합니다. |
| `qwen2.5:3b` | 테스트는 무의미하게 통과를 거절한다. |
| **`qwen2.5-coder:7b`** | 테스트는 공허하게 통과하지 않습니다. |
| `gemma4:e4b` | 테스트는 공허하게 통과하는 것을 올바르게 거부합니다. |

Only `gemma4` keeps both `correctly` and `refuses to pass`. The shipping
model drops the adverb. `qwen2.5:3b` attaches it to the wrong verb.

### 2. Acquisition is solved and targeting is not.

| Model | Output |
|---|---|
| `translategemma` | 획득은 완료되었지만, 표적 지정은 아직 이루어지지 않았습니다. |
| `qwen2.5:3b` | 수집은 해결되었으나 타겟팅은 이루어지지 않았습니다 |
| **`qwen2.5-coder:7b`** | 취득은 해결되었지만, 타겟팅은 아직 이루어지지 않았습니다. |
| `gemma4:e4b` | 획득(취득)은 해결되었고, 타겟팅은 그렇지 않습니다. |

`is not` means *is not solved*. Three of the four render it as *has not
been done yet*, which is a different claim about a different thing.
`gemma4` is right and pays for it with a parenthetical gloss nobody
asked for.

### 3. A card placed on the wrong sentence is worse than no card.

| Model | Output |
|---|---|
| `translategemma` | 잘못된 문장에 카드를 놓는 것보다 카드가 없는 것이 더 나쁘다. |
| `qwen2.5:3b` | 권장되지 않은 카드를 놓은 문장보다 더 나쁩니다. |
| **`qwen2.5-coder:7b`** | 잘못된 문장에 카드를 놓은 것보다 카드가 없는 것이 더 나쁘다. |
| `gemma4:e4b` | 잘못된 문장에 카드를 놓는 것이 아예 카드를 놓지 않는 것보다 더 나쁩니다. |

**The shipping model states the opposite of the source.** So does
`translategemma`. The sentence is a design principle from this
repository's own documentation, and both models turned it into its
negation, fluently, with nothing on the card to suggest anything was
wrong.

This is the finding. A clumsy translation is a nuisance; a reversed one
is the product telling somebody the wrong thing in a confident voice.

### 4. The pointer must rest before anything is read, and neither path runs on a mouse move.

| Model | Output |
|---|---|
| `translategemma` | 포인터는 먼저 정지해야 하며, 어떤 경로도 마우스 움직임에 따라 변경되지 않아야 합니다. |
| `qwen2.5:3b` | 점퍼가 움직이기 전에 항상 읽을 내용이 없도록 하여야 합니다. 또한 마우스 이동이 기반되지 않는 두 가지 경로도 존재해야 합니다. |
| **`qwen2.5-coder:7b`** | 포인터가 읽기 전에 반드시 위치해야 하며, 어떤 경로도 마우스 움직임으로 실행되지 않습니다. |
| `gemma4:e4b` | 포인터는 어떤 것이든 읽히기 전에 멈춰야 하며, 두 경로 모두 마우스 이동으로 작동하지 않는다. |

`rest` becomes 위치해야 (must be positioned) in the shipping model,
which loses the entire idea the sentence exists to state. `qwen2.5:3b`
renders `pointer` as 점퍼 again, exactly as it did on 2026-08-05.

### 5. I would rather it failed loudly than passed for a reason nobody checked.

| Model | Output |
|---|---|
| `translategemma` | 저는 아무도 확인하지 않은 이유로 성공하는 것보다, 크게 실패하는 것을 선호합니다. |
| `qwen2.5:3b` | 나ughty한 이유로 실패하는 것이 좋겠다는 말보다는… |
| **`qwen2.5-coder:7b`** | 그런 경우는 심각하게 실패하는 것이 낫다. 이유를 확인하지 않으면 아무리 잘 보이더라도 거짓으로 간주될 수 있다. |
| `gemma4:e4b` | 아무도 확인하지 않은 이유로 통과하는 것보다 차라리 크게 실패하는 것이 낫겠다. |

The shipping model **invents a second sentence** that is not in the
source. `qwen2.5:3b` emits `나ughty`, which is a token boundary showing
through.

## Warm latency, the two worth choosing between

The table above loads a different model for every call, so its
milliseconds are load times. These are warm: two discarded runs, then
five measured.

| Model | Median | Samples |
|---|---|---|
| `qwen2.5-coder:7b` | **1,613ms** | 1345, 1576, 1613, 1759, 1780 |
| `gemma4:e4b` | **15,250ms** | 9010, 10013, 15250, 15527, 15824 |

**Nine and a half times slower.** A hover card that arrives fifteen
seconds after the pointer settled is not a hover card; the pointer has
moved, the reader has moved on, and the whole premise of the product is
that the answer is there when you look up.

So the choice on this machine today is between a model that is fast and
sometimes says the opposite of the source, and a model that is right and
too slow to use. Neither is the product.

## It is not the prompt

The obvious cheap explanation is that the system prompt is too much for
a 7B model: an unguessable fence, three sentences saying everything
inside it is data, a ban on preamble, a statement that the output
language is not negotiable, an instruction not to drop clauses, and a
rule about identifiers.

Measured against a prompt with nothing in it but *translate this into
Korean*, same model, same sentences:

| Source | Defended | Bare |
|---|---|---|
| The test correctly refuses to pass vacuously. | 테스트는 공허하게 통과하지 않습니다. | 이 테스트는 명백하게 잘못된 경우에 올바르게 실패합니다. |
| Acquisition is solved and targeting is not. | 취득이 해결되었고, 타겟팅은 이루어지지 않았습니다. | 인수 문제는 해결되었지만, 타겟팅은 여전히 해결되지 않았습니다. |
| I would rather it failed loudly than… | 그런 경우는 심각하게 실패하는 것이 낫다. 이유를 확인하지 않은 사람들은… | 나는 아무도 확인하지 않은 이유로 실패하는 것을, 성공하는 것보다 훨씬 선호합니다. |

The bare prompt is not better. It is wrong differently: the first
sentence becomes something not in the source at all, and the third
inverts which outcome is preferred. Only the second improves, and it
improves by being wrong in a smaller way.

**The defences are free.** Whatever is wrong here, removing the
injection protection would not fix it and would cost the one thing this
repository has measured a small model failing at four times out of four.

## The hole in the benchmark that chose this model

`docs/benchmarks/2026-08-05-model-comparison.md` concluded that **size
beat specialisation**, on the evidence that `qwen2.5-coder:7b` rendered
`pointer` and `output` correctly where `qwen2.5:3b` produced 점퍼 and
출제.

That comparison was 7B-**coder** against 3B-**general**. It varies two
things at once and attributes the result to one of them. `qwen2.5:7b`,
the same size and the same family without the code specialisation, was
never run, so nothing in this repository establishes that a code model
was the right choice at that size. It establishes that 7B beat 3B.

That is worth stating plainly because the conclusion has been load
bearing since: it is why the family preference names `qwen2.5` and why a
code model is not on the list of things a hover must not use.

## Identifiers, named rather than described

A separate defect from the same session, and one that is fixed.

A reader photographed a card whose source was Korean prose carrying English
identifiers. The answer had transliterated `EngineDataSheetService` into
엔진데이터시트서비스. That is worse than not translating the sentence: a
sentence a reader can already mostly read came back with the one part they
could act on destroyed.

The system prompt already said *code identifiers, file paths and URLs stay
verbatim*. A 7B model applying that rule to a bare CamelCase word in the
middle of a sentence evidently does not.

So they are listed. `lens_core::translate::identifiers_in` finds tokens no
ordinary word could be, an underscore inside, a lower-then-upper camel hump,
a slash with something after it, or a scheme, and the prompt names them.
That turns a rule the model has to apply into a list it has to copy.

Measured over three sentences on `qwen2.5-coder:7b`:

```
insway의 EngineDataSheetService 에 export_csv 를 넣고, POST /projects/{id}/... 
  kept    EngineDataSheetService
  kept    export_csv
  kept    /projects/{id}/insbase/tables/{table}/export

The build failed because cargo fmt --check found a difference in src/main.rs
  kept    src/main.rs

Call getUserProfile with the user_id and the request will return a row
  kept    getUserProfile
  kept    user_id

6 of 6 identifiers survived
```

The line from the card now comes back with all three intact.

**These came off a screen**, so the tokens are filtered to a closed
character set rather than a list of forbidden ones: a list of what may not
appear has to be kept complete forever against strings arriving from a
stranger's display, and a list of what may is closed by construction. This
and the focused word are the only two places screen text is spliced into
instructions rather than fenced, and there is a test that a token carrying a
newline and an injection cannot break out of the line it sits in.

**Not fixed by this:** the proper noun beside them. `insway` came back as
인웨이. A company name is not an identifier by any shape rule, and teaching
the extractor to catch capitalised words would list every proper noun in
every sentence and train the model to leave names untranslated, which is the
opposite of what a reader wants.

## What to do

Neither of the two candidates is shippable and the cause is not the
prompt, so the answer is a model that is not installed. Two are worth
pulling and measuring the same way, roughly 9.5GB together:

- **`qwen2.5:7b`** · 4.7GB. Closes the hole above by isolating
  coder-versus-general at a fixed size. Cheap to run, and if it matches
  the coder model on latency and beats it on prose, the current default
  is simply wrong.
- **`exaone3.5:7.8b`** · 4.8GB. Built by LG AI Research as a
  Korean-English bilingual model rather than an English model that has
  seen Korean, which is what every model measured here is. Same size
  class as the current default, so the latency should be comparable.

Both are guesses until measured. Neither is a recommendation to ship:
the recommendation is to run
`compare_what_each_installed_model_makes_of_the_same_sentences` and
`measure_warm_latency_of_the_two_candidates` with them installed, and
decide on the output.

**Not on the list, and deliberately:** anything above about 8B, and
anything that reasons before answering. `gemma4:e4b` is the measurement
of what that costs, and 15 seconds is the answer.
