# ADR-0007 · Local-first inference over an external Ollama

**Status** · Accepted, 2026-08-04

Not yet built. Recorded now because it fixes the trust model, which
cannot be retrofitted.

## Context

Lens reads whatever the user is reading. That includes private
messages, internal documents, unreleased code, and medical or financial
pages. A tool with that reach earns trust once and loses it once.

A cloud translation API would be faster to integrate and would translate
better. It would also mean the contents of the user's screen leave the
machine on hover, continuously, without a deliberate action per request.
That is not a setting to be defaulted, it is a property of the product.

The local option is real now. Ollama serves an HTTP API on
`127.0.0.1:11434`, keeps models resident between calls, supports JSON
Schema-constrained output, and streams. Mori already drives it from Rust
and measured the latency ([ADR-0005](ADR-0005-two-speed-ai.md)).

Bundling a model is the other candidate and is rejected: it adds
gigabytes to the installer, freezes the model choice, and duplicates a
runtime most target users already have.

## Decision

**Inference is local by default, through an Ollama instance the user
already runs, reached over `127.0.0.1`. No model is bundled. Cloud
engines are off, and turning one on is per-provider and explicit.**

Concretely:

- Ollama is an external dependency, discovered at startup via
  `GET /api/tags` and `GET /api/ps`. It is not installed, launched, or
  managed by Lens.
- Ollama being absent is a **degraded state, not an error**: the cache
  still answers, the tray says what is missing, and no modal blocks the
  screen.
- A cloud provider requires an explicit per-provider grant. An unknown
  endpoint is treated as external, never optimistically as local. A
  remote Ollama address is a cloud provider by this definition, because
  the text leaves the machine.
- The engine that produced an answer is visible on the card. The user
  can always tell whether that text left the machine.
- Model output is **untrusted data**. Screen text is never merged into a
  system prompt, the model gets no tool access, and nothing it returns is
  executed, opened, or followed. A web page saying "ignore previous
  instructions" is content to be translated, not an instruction.

### Model roles, and what is actually installed

Two roles, deliberately separable, per [ADR-0005](ADR-0005-two-speed-ai.md):

| Role | Wanted | Why |
|------|--------|-----|
| Quick translation | a translation-specialised small model | consistent output shape, minimal tokens |
| Deep analysis | a general instruct model | free-form reasoning about grammar |

On the development machine, `ollama list` on 2026-08-04 reports exactly:

```
qwen2.5:3b          1.9 GB
qwen2.5-coder:7b    4.7 GB
gemma4:e4b          9.6 GB
```

Two corrections to the planning notes this project started from, both
found by running the command rather than trusting the note:

- **`translategemma` is not installed.** It is a candidate for the quick
  path and has to be pulled and benchmarked before being named as the
  default. Its Gemma terms of use also need review before shipping.
- **`qwen2.5:7b` is not installed** either. The general instruct model
  available today is `qwen2.5:3b`, which is what Mori measured with.

So the shipping default is decided by a benchmark that has not been run,
not by this ADR. What this ADR fixes is that the router must treat the
model set as **discovered, not assumed**, and degrade in a stated order
when the preferred model is missing.

**Addendum, 2026-08-06.** The listing above is what the machine held on
2026-08-04 and stays as written, because it is what the decision was made
against. Both of the corrections it names have since been resolved by
running the benchmark it asks for, and a reader arriving here should not
take either as current:

```
qwen2.5:7b             4.7 GB
translategemma:latest  3.3 GB
```

`translategemma` was pulled and measured on 2026-08-05: it is a chat
model with a translation-sounding name and it reverses the meaning of a
sentence this repository uses as a test. `qwen2.5:7b` was pulled and
measured on 2026-08-06 against `qwen2.5-coder:7b`, and it is what the
router now chooses
([`../benchmarks/2026-08-06-model-at-one-size.md`](../benchmarks/2026-08-06-model-at-one-size.md)).

The decision is unchanged and was not the model. It was that the set is
discovered rather than assumed, and that is what let the model change
twice without a line of routing code being written for either.

## Consequences

- Translation quality is bounded by what a small local model can do.
  That is the price of the promise, and it is stated in the product copy
  rather than hidden.
- Users must install Ollama and pull a model. Onboarding has to make
  that concrete, and the app has to stay usable while it is unfinished.
- Latency depends on the user's hardware, so the performance budget in
  [`../research/hover-text-acquisition.md`](../research/hover-text-acquisition.md)
  is a target measured per machine, never a promise.
- Nothing about the trust model needs to change if a cloud engine is
  added later, because the boundary and the disclosure are built first.
