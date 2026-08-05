# Which installed model should Lens translate with

**2026-08-05**, Windows 11 build 26200, Ollama on `127.0.0.1:11434`, one
machine, three sentences. Small, and it still overturned a rule.

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

| Model | Mean | Verdict |
|---|---|---|
| `qwen2.5:3b` | 1460ms | wrong on all three |
| `qwen2.5-coder:7b` | 1655ms | correct on all three |
| `gemma4:e4b` | refused | emits broken UTF-8 |

### `qwen2.5:3b`, the previous default

```
1226ms  출제 파일들이 비어 돌아왔으므로 전체 테스트 셸은 아무것도 생성하지 않았습니다.
1872ms  점퍼를 문장 위에 올려놓고 당신의 언어로 읽으세요.
1284ms  이 변경은 코드가 기반으로 한 가정을 제거하는 것이 결함 수정이 아닙니다.
```

- **점퍼** for "pointer". A jumper. This is the product's own tagline
  sentence and the word the whole application is about.
- **출제** for "output", which means setting an exam question.
- **테스트 셸** for "test suite".
- The third sentence's negation lands on the wrong clause, so it says
  roughly the opposite.

### `qwen2.5-coder:7b`

```
1731ms  출력 파일들이 비어있어서 전체 테스트 케이스가 아무것도 생성하지 않았습니다.
1626ms  포인터를 문장 위에 두고 자신의 언어로 읽으세요.
1609ms  이 변경은 버그 수정이 아닙니다. 코드가 기반으로 한 가정을 제거합니다.
```

Correct on all three, including 포인터, and it kept the third sentence's
two-clause structure instead of collapsing it.

**A code model translated prose better than a general model**, because
7B beat 3B by more than specialisation cost. The rule that excluded it by
name would have chosen the worse one.

### `gemma4:e4b`

Every request came back as `malformed`, which reads as "the model
returned nothing". It does not. Asked directly:

```
response = '출력 \udced\udcec\udceb\udcec 비어 \udcec\udcec\udcec.'
```

Valid Korean for the first word and **lone UTF-16 surrogates** after it.
Refusing this is correct behaviour: mojibake on a card is worse than an
honest failure, and `번역 결과를 이해할 수 없습니다` is exactly what happened.
Not a Lens defect, and worth writing down so it is not rediscovered as
one. 41 seconds to load, as well.

## What changed because of this

1. **Code models are no longer excluded by name.** Embedding, whisper and
   clip families still are, because those genuinely cannot produce prose.
2. **Larger is preferred within a family**, up to 9B. Above that a hover
   costs more than the answer is worth; nothing here measures that
   boundary, it is a judgement.
3. **The fallback model name is no longer treated as a choice.** It was
   passed to the router as though a person had asked for `qwen2.5:3b`, so
   on a machine that happened to have it installed the router
   short-circuited and never looked at the 7B beside it. A default nobody
   picked should not outrank a measurement.

After the change, this machine resolves to `qwen2.5-coder:7b`.

## What this does not establish

One machine, one language pair, three sentences, one reader. It is enough
to overturn a rule that had **no** evidence behind it, and not enough to
claim 7B is right in general or that 200ms is the true cost. A second
machine, or a language pair that is not English into Korean, could say
something different.

The `PREFERRED` list still names families nobody has compared here:
`translategemma` and `gemma3` are ahead of `qwen2.5` on reputation, which
is exactly the kind of claim this document exists to replace.
