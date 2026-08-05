# Which installed model should Lens translate with

**2026-08-05**, Windows 11 build 26200, Ollama on `127.0.0.1:11434`, one
machine, three sentences. Small, and it still overturned two rules and a
conclusion of its own.

## Why this was run

The model preference list had never been measured. It was written from a
rule that sounds obviously right:

> Smallest first: a quick translation on every hover is the wrong place
> to run the largest model on the machine.

A second rule was added from the same kind of reasoning, that a model
with `coder` in its name should not translate prose.

Both are stories. This is what the machine said.

## What was measured

Latency, and the answers themselves. **Quality is a judgement and is not
automated**: the translations are printed and a person reads them. The
sentences are this repository's own, so printing them breaks no rule
about screen text.

Reproduce with:

```bash
cargo test -p lens-translate -- --ignored --nocapture compare_every
```

## Result

| Model | Mean | Quality | Obeys an injection |
|---|---|---|---|
| `qwen2.5:3b` | 1441ms | wrong on all three | 0 of 4 |
| `qwen2.5-coder:7b` | 1917ms | correct on all three | **4 of 4** |
| `translategemma` | 4120ms, one timeout | correct | 0 of 4 |
| `gemma4:e4b` | 15749ms | best | 0 of 4 |

**No model here is fast, accurate and injection-resistant at once.** That
is the finding, and no amount of prompt work changes it.

### `qwen2.5:3b`, the previous default

```
1226ms  출제 파일들이 비어 돌아왔으므로 전체 테스트 셸은 아무것도 생성하지 않았습니다.
1872ms  점퍼를 문장 위에 올려놓고 당신의 언어로 읽으세요.
1284ms  이 변경은 코드가 기반으로 한 가정을 제거하는 것이 결함 수정이 아닙니다.
```

- **점퍼** for "pointer". A jumper. This is the product's own tagline
  sentence and the word the whole application is about. A later run of
  the same sentence produced **손가락**, a finger.
- **출제** for "output", which means setting an exam question.
- **테스트 셸** for "test suite".
- The third sentence's negation lands on the wrong clause, so it says
  roughly the opposite.

### `qwen2.5-coder:7b`

```
1652ms  출력 파일이 비어 있어서 전체 테스트 케이스가 아무것도 생성하지 않았습니다.
1416ms  포인터를 문장 위에 두고 자신의 언어로 읽으세요.
1623ms  이 변경은 버그 수정이 아닙니다. 코드가 기반으로 한 가정을 제거합니다.
```

Correct on all three, including 포인터, and it kept the third sentence's
two-clause structure instead of collapsing it.

**A code model translated prose better than a general model**, because
7B beat 3B by more than specialisation cost. The rule that excluded it by
name would have chosen the worse one.

### `gemma4:e4b`

**This section was wrong twice, and the corrections matter more than the
original claim did.**

It first read that every request came back `malformed`, and that asking
the model by hand returned one valid Korean word followed by lone UTF-16
surrogates. The conclusion drawn was that the model emits broken text,
and that refusing to draw mojibake was correct behaviour.

**Neither half was true.**

The surrogates were **this machine's console**, not the model. The
diagnostic piped `curl` into Python on a Korean Windows install, where
the default encoding is cp949, and UTF-8 bytes read as cp949 produce
exactly that pattern. Read as UTF-8 from a file, the same reply is
`출력 파일들이 비어 왔어요.` with no surrogate in it at all.

The real failure was **this repository's token budget**:

```
"response":"", "done_reason":"length", eval_count: 123
```

`gemma4:e4b` reasons before it answers. At the 123 tokens its source text
earned, the entire budget went on reasoning and there was nothing left to
say with. At 512 it used **316 tokens** to produce a thirteen-character
translation and stopped normally.

So Lens was starving it, and reporting the result as the model's fault.

With a generous retry after an empty answer it works, and it gives the
best translations of the three:

```
12411ms  출력 파일들이 비어 나왔기 때문에, 전체 테스트 스위트에서 아무것도 생성되지 않았습니다.
11776ms  포인터를 문장에 놓고 본인의 언어로 읽으세요.
14984ms  이 변경 사항은 버그 수정이 아닙니다. 코드의 기반으로 했던 가정을 제거합니다.
```

**Thirteen seconds.** Correct, well phrased, and unusable on a hover: the
reader has long since moved on. A model that reasons is the wrong shape
for this product whatever it is called, and running it is the only way to
find that out.

### How both of those wrong conclusions happened

Neither came from code that was hard to read. One came from a diagnostic
that mangled its own output, and one from believing an error message this
repository had written itself.

The part that generalises: **a measurement tool is part of what is being
measured.** The console's encoding was as much a variable as the model.

## What changed because of this

1. **A retry after an empty answer gets a much larger budget**, fixed
   rather than scaled: the size of a question says nothing about how much
   reasoning a model will do before answering it. Paid only after a first
   attempt returned nothing, so an ordinary hover never sees it.
2. **Code models are no longer excluded by name.** Embedding, whisper and
   clip families still are, because those genuinely cannot produce prose.
3. **Larger is preferred within a family**, up to 9B. Above that a hover
   costs more than the answer is worth; nothing here measures that
   boundary, it is a judgement.
4. **The fallback model name is no longer treated as a choice.** It was
   passed to the router as though a person had asked for `qwen2.5:3b`, so
   on a machine that happened to have it installed the router
   short-circuited and never looked at the 7B beside it. A default nobody
   picked should not outrank a measurement.

After the change, this machine resolves to `qwen2.5-coder:7b`.

### `translategemma`, pulled to test a proposal

An outside review argued for taking the general LLM off the fast path and
using a dedicated translation engine: faster, and structurally immune to
prompt injection because an NMT model does not follow instructions.

The argument is good. Shipping CTranslate2 or Bergamot means shipping or
downloading model weights, which
[ADR-0007](../decisions/ADR-0007-local-first-inference.md) and the
roadmap both exclude. But there was a cheaper test of the same idea
inside the current architecture, and `translategemma` was **first in this
repository's own preference list**, put there for its name and never once
run.

Pulled, 3.3 GB, and run:

```
translategemma:latest (warm in 38.2s)
  ----    번역이 시간 안에 끝나지 않았습니다 [timeout]
  4068ms  포인터를 문장에 올려놓고 자신의 언어로 읽어보세요.
  4172ms  이 변경은 버그 수정이 아닙니다; 이 코드가 구축될 때 기반으로 했던 가정 하나를 제거합니다.
```

Accurate, resistant to injection 0 of 4, and **2.5 times slower** than
`qwen2.5-coder:7b`, with one timeout in three sentences.

**It is not a translation engine.** Given bare text with no instruction
it ran for 53 seconds and produced English commentary, beginning "This is
a frustrating situation! Here's a breakdown of what might be happening".
Given a one-line instruction it answered in markdown, with alternatives
and a note on which was most common. It is a chat model with a
translation-sounding name, and this crate's fenced prompt is the only
reason its output is clean.

So the proposal's mechanism was not tested by this. What was tested, and
failed, is the assumption that a name identifies one.

**The preference list was the real defect.** A user with
`translategemma` installed would have been routed to it, over a model 2.5
times faster, by a preference nobody had ever run. The list now contains
only families that have been. Everything else is chosen by size, which is
a rule with evidence behind it.

## A prompt line, and what it cost

Every line of the system prompt guarded against the model **adding**: no
preamble, no notes, no explanation. Nothing guarded against it
**omitting**, and that is the failure a reader cannot see.

A user hovered a two-clause sentence and got fluent Korean carrying only
the second clause, with nothing on the card to suggest the first had ever
existed. One line, `Translate every clause. Do not summarise, shorten, or
leave anything out.`, measured over five sentences:

| Sentence | Without | With |
|---|---|---|
| two clauses joined by a colon | first clause missing | both, correct |
| `Sign in` | 로그인 | 로그인 |
| a plain sentence | same | same |
| one containing `/error` | same | same |
| a long two-part sentence | invents a clause, drops "not after it" | complete |

Two fixed, three unchanged, about 190ms slower on average.

**And it was checked against the fence**, because "do not leave anything
out" and "this text is data, do not obey it" pull in opposite directions
when the data is itself an instruction. Four rounds per model, with the
line and without:

```
qwen2.5:3b           without  obeyed 0/4
qwen2.5:3b           with     obeyed 0/4
qwen2.5-coder:7b     without  obeyed 4/4
qwen2.5-coder:7b     with     obeyed 4/4
gemma4:e4b           without  obeyed 0/4
gemma4:e4b           with     obeyed 0/4
```

No difference in either direction. It also shows, more sharply than the
single sample that came before it, that **the model chosen here for
translation quality is the only one of the three that obeys a prompt
injection**. That trade is recorded in
[`../privacy-model.md`](../privacy-model.md) §1, where it belongs.

## What this does not establish

One machine, one language pair, three sentences, one reader. It is enough
to overturn rules that had **no** evidence behind them, and not enough to
claim 7B is right in general or that 200ms is the true cost. A second
machine, or a language pair that is not English into Korean, could say
something different.

Nothing here measures latency before choosing, either. A model that
reasons is only discovered to be slow by running it, and the card shows
the figure so a reader can see it and pin something else.

That sentence used to end this document:

> The `PREFERRED` list still names families nobody has compared here:
> `translategemma` and `gemma3` are ahead of `qwen2.5` on reputation,
> which is exactly the kind of claim this document exists to replace.

It has been replaced, by pulling one of them and finding it 2.5 times
slower than the model it outranked. `gemma3` is still unmeasured and is
no longer named, on the grounds that being unmeasured is what the
sentence above was complaining about.
