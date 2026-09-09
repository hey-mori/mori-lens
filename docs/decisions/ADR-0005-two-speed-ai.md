# ADR-0005 · Quick translation and deep analysis are separate paths

**Status** · Accepted, 2026-08-04

Not yet built. Recorded now because it determines the request lifecycle
and the cache key, both of which are designed before the first model
call.

## Context

The tempting shape is one call: send the sentence, ask for a
translation plus the word's meaning plus the clause structure plus the
grammar plus the pronunciation, render whatever comes back.

Mori measured that shape against a real local model and published the
numbers (`docs/design/language-loop.md` §6, `qwen2.5:3b`):

| | total | first token | output tokens |
|---|---:|---:|---:|
| two sections | 1,370ms | 946ms | 40 |
| follow-up 구조 | 917ms | | 15 |
| follow-up 문법 | 1,142ms | | 29 |
| everything at once | 4,372ms | | |

Four seconds is not a hover. It is not even a chord-triggered panel;
Mori moved off it. And hover fires far more often than a chord does, so
Lens would pay that cost per glance instead of per deliberate ask.

The generation cost is proportional to output tokens, and nobody reads
eight sections for every word they look at.

## Decision

**Two independent paths, with separate models, prompts, caches, and
timeouts.**

**Quick translation** runs on every accepted hover:

- source language detection
- the sentence, translated
- the pointed-at word's meaning in this context
- nothing else

**Deep analysis** runs only when the user opens it:

- clause structure, grammar, idiom, register
- lemma, part of speech, pronunciation
- literal versus natural rendering
- profile-specific notes (developer English, exam English)

Rules that follow:

- A hover never triggers deep analysis, no matter how long it rests.
- Deep analysis is grounded on the quick translation already on screen
  rather than re-deriving it, so opening it costs one focused generation
  and not a second full analysis.
- The two use different cache keys, because the prompt version and the
  model differ.
- A follow-up is offered only where it can answer. No clause structure
  for a single word, no pronunciation for a paragraph. A button whose
  only possible reply is "not applicable" spends a call and teaches the
  user to distrust the buttons.
- Deep analysis output is labelled as an AI explanation and shown beside
  the source, never presented as settled fact. Grammar analysis from a
  small local model is frequently wrong in ways a learner cannot detect.

## Consequences

- The common case pays roughly one generation of about 40 output
  tokens, which is the only version of this product that can feel
  instant.
- Two prompt families, two schemas, and two cache namespaces to
  maintain. Accepted.
- The quick path can use a translation-specialised model while the deep
  path uses a general instruct model, since they are separate calls
  ([ADR-0007](ADR-0007-local-first-inference.md)).
- Users who want everything must click for it. That is the correct
  default for a tool that fires on hover, and the click is one press.

---

*Implementation status (updated 2026-09-09):* **IMPLEMENTED.** All
requirements verified with evidence:

- **Separate models:** `AiRuntimeConfig` holds separate `RuntimeEngine`
  for Quick and Deep, each with its own model. Settings schema has
  separate `[quick]` and `[deep]` sections.
- **Separate endpoints:** Each `RuntimeEngine` has its own endpoint.
  The worker creates `quick_translator` and `deep_translator` with
  independent endpoints.
- **Separate translators:** Two `OllamaTranslator` instances, one for
  each path. Quick jobs use `quick_translator.quick_streaming()`, Deep
  jobs use `deep_translator.deep()`.
- **Separate prompts:** `PROMPT_VERSION` for Quick, `DEEP_PROMPT_VERSION`
  for Deep (existed prior to this implementation).
- **Separate caches:** `TranslationCache` for Quick, `AnalysisCache`
  for Deep. Cache keys include the model name, ensuring different models
  produce different keys even for the same request.
- **Separate timeouts:** `QUICK_TIMEOUT` (20s) for Quick, `DEEP_TIMEOUT`
  (60s) for Deep. The `deep()` method uses `DEEP_TIMEOUT` explicitly,
  not the translator's default timeout.
- **Model provenance:** Save and Detail paths record the actual model
  that produced the result by checking the card body type (Analysis vs
  Translation).
- **Shutdown semantics:** Both `quick_releaser` and `deep_releaser` are
  released on shutdown, ensuring both models are unloaded from memory.
- **CLI semantics:** `--model` applies to Quick only (documented in help
  text). `--endpoint` applies to both for backwards compatibility. Deep
  is configured via settings.toml `[deep]` section.
- **Live settings:** Model/endpoint changes require restart. The product
  detects changes and displays "restart to use it" message.

`Ctrl+Alt+D` triggers deep analysis; hover alone triggers quick
translation only.
