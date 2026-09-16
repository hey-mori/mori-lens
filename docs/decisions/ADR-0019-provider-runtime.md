# ADR-0019 · Provider runtime architecture

**Status** · Accepted, 2026-09-16

## Context

Mori Lens uses local language models for translation. P4 spike
([#567](https://github.com/swarea/mori-lens/issues/567),
docs/spikes/2026-09-15-p4-provider-architecture.md) investigated adding
LM Studio support alongside the existing Ollama support. The spike
revealed a tension:

- Ollama exposes a single HTTP API surface for both inference
  (`/api/generate`, `/api/chat`) and management (`/api/tags`,
  `/api/show`).
- LM Studio exposes two distinct API surfaces: a native v1 API
  (`/api/v1/models`) for management and an OpenAI-compatible API
  (`/v1/chat/completions`) for inference.
- Lifecycle operations (load, unload, prewarm, release) are
  provider-specific. Ollama keeps models in memory by default. LM Studio
  requires explicit load/unload. Forcing a common lifecycle abstraction
  would be a lie.

The question: how do we support multiple providers without pretending
they are the same?

## Decision

**Two-plane architecture: separate inference from management.**

### Plane 1: Inference (Translator trait)

The `Translator` trait is inference-only and provider-neutral:

```rust
pub trait Translator {
    fn quick(&self, request: &QuickRequest) -> Result<QuickTranslation, TranslateError>;
    fn quick_streaming(&self, request: &QuickRequest, on_partial: &mut dyn FnMut(&str)) -> Result<QuickTranslation, TranslateError>;
}
```

Key properties:

- No `deep()` method. Deep analysis is a separate path
  (`Analyser` trait), not a translator capability.
- No lifecycle methods. Loading, unloading, prewarming, and releasing
  are provider-specific concerns, not inference concerns.
- No provider identity. The trait does not know or care which provider
  implements it.
- Optional pattern. When no model is selected, the Quick translator is
  `Option<Box<dyn Translator + Send + Sync>>`. The absence of a
  translator is a valid state, not an error.

### Plane 2: Management (ProviderRuntime trait)

The `ProviderRuntime` trait is management-only:

```rust
pub trait ProviderRuntime {
    fn provider_identity(&self) -> Provider;
    fn reachability(&self) -> Reachability;
    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError>;
    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError>;
}
```

Key properties:

- Discovery and readiness. The management plane answers "what is
  available?" and "can we reach it?"
- No lifecycle methods. If a provider needs explicit load/unload, that
  is a provider-specific implementation detail, not a trait method.
- Provider identity. The management plane knows which provider it
  represents. The inference plane does not.

### Factory dispatch

Two factory functions dispatch by `Provider` enum:

```rust
pub fn build_translator(provider: Provider, endpoint: &str, model: &str) -> Box<dyn Translator + Send + Sync> {
    match provider {
        Provider::Ollama => Box::new(OllamaTranslator::new(endpoint, model)),
        Provider::LmStudio => {
            let runtime = LmStudioRuntime::new(endpoint);
            Box::new(LmStudioTranslator::new(runtime, model))
        }
    }
}

pub fn build_provider_runtime(provider: Provider, endpoint: &str) -> Box<dyn ProviderRuntime> {
    match provider {
        Provider::Ollama => Box::new(OllamaRuntime::new(endpoint)),
        Provider::LmStudio => Box::new(LmStudioRuntime::new(endpoint)),
    }
}
```

LM Studio's inference translator internally holds an `LmStudioRuntime`
because LM Studio's inference API requires the same HTTP client as its
management API. This is an implementation detail, not an architectural
coupling.

### Capability matrix

| Capability | Ollama | LM Studio |
|---|---|---|
| Quick translation | Yes | Yes |
| Deep analysis | Yes | No |
| Model discovery | Yes | Yes |
| Reachability check | Yes | Yes |
| Model state query | Yes | Yes |
| Model lifecycle (load/unload) | Implicit (Ollama manages) | Explicit API (LM Studio supports load/unload; JIT on first request) |

Deep analysis is Ollama-only because the `Analyser` trait is implemented
only by `OllamaTranslator`. LM Studio honestly returns `None` when asked
for a Deep analyser. This is not a bug. It is a capability gap that the
UI respects.

### Local-first network boundary

Both providers are local. The endpoint must be a loopback address
(`127.0.0.1`, `localhost`, or `[::1]`). Non-loopback endpoints are
rejected at settings validation time.

Why: Mori Lens is a local-first product. Screen text must not leave the
machine by default. A cloud provider would violate that invariant. If a
cloud provider is wanted, it arrives as a new ADR that addresses the
privacy model, not as a silent extension of the local provider set.

### LM Studio dual API

LM Studio exposes two HTTP API surfaces:

1. **Native v1 API** (`/api/v1/models`): returns a list of available
   models with their state (loaded, not loaded, loading). Used for
   management (discovery, readiness).
2. **OpenAI-compatible API** (`/v1/chat/completions`): accepts
   OpenAI-format chat completion requests. Used for inference.

The two surfaces are distinct. The management surface uses a top-level
`"models"` key (not `"data"`). The inference surface uses the standard
OpenAI response format. The parser for each surface is typed and
provider-specific. There is no attempt to unify them.

Schema correction (PR #573): the native v1 `/api/v1/models` endpoint
returns `{"models": [...]}`, not `{"data": [...]}`. The initial
implementation assumed the OpenAI format and was corrected after
measuring the actual API.

### Ownership semantics

Mori Lens does not assume it owns the model or the runtime. The provider
may be running independently of Mori Lens. Mori Lens connects to it,
uses it, and disconnects. It does not start, stop, or manage the
provider's lifecycle unless the user explicitly asks.

This is an architectural risk. If the provider crashes, Mori Lens
reports the error. If the provider is not running, Mori Lens reports
unreachable. If the provider is running but the model is not loaded,
Mori Lens reports the model state. Mori Lens does not attempt to recover
by starting the provider or loading the model. That is the user's job.

## Consequences

- The inference plane is provider-neutral. Adding a new provider means
  implementing `Translator` (and optionally `Analyser`), not redesigning
  the trait.
- The management plane is provider-aware. Each provider's runtime knows
  its own API surface, its own model state semantics, and its own
  reachability contract.
- Lifecycle operations remain provider-specific. Ollama's implicit
  memory management and LM Studio's explicit load/unload are not forced
  into a common abstraction. The abstraction would be a lie.
- Deep analysis is honestly limited. LM Studio does not support it. The
  UI respects that limit. There is no attempt to fake it or work around
  it.
- The local-first boundary is enforced at settings validation time. A
  cloud provider requires a new ADR, not a configuration change.
- The two-plane split makes the capability matrix explicit. Quick
  translation is broadly supported. Deep analysis is narrowly supported.
  Management is universally supported. The UI reflects this.

## Rejected alternatives

### Combined Quick/Deep translator

**Proposal:** Add `deep()` to the `Translator` trait so a single trait
covers both inference modes.

**Rejection:** Deep analysis is a different contract. It takes different
inputs (a document, not a hover), runs for different durations (seconds
to minutes, not milliseconds), and produces different outputs (a
structured analysis, not a translation). Forcing it into the same trait
would be a lie. The `Analyser` trait exists separately because the
contracts are separate.

### Generic lifecycle abstraction

**Proposal:** Add `load()`, `unload()`, `prewarm()`, `release()` to
`ProviderRuntime` so all providers share a common lifecycle contract.

**Rejection:** Lifecycle semantics are provider-specific. Ollama keeps
models in memory by default and does not expose explicit load/unload.
LM Studio requires explicit load/unload. Forcing a common abstraction
would mean Ollama's implementation is a no-op and LM Studio's
implementation is a wrapper around its native API. The abstraction would
not simplify anything. It would hide the provider's actual behavior
behind a fake uniformity.

If a third provider arrives with yet another lifecycle model, the
question of whether to abstract will be revisited. Until then, the
provider-specific behavior is the truth, and the trait does not pretend
otherwise.

### Unified API parser

**Proposal:** Write a single parser that handles both Ollama's native
format and LM Studio's OpenAI-compatible format, dispatching on a
provider tag.

**Rejection:** The two formats are structurally different. Ollama's
`/api/generate` returns `{"response": "..."}`. LM Studio's
`/v1/chat/completions` returns `{"choices": [{"message": {"content":
"..."}}]}`. A unified parser would be a switch statement inside a
function, which is just a typed dispatch without the type safety.
Separate parsers with separate types are clearer and easier to test.

### Cloud provider support

**Proposal:** Allow configuring a cloud endpoint (OpenAI, Anthropic,
etc.) alongside local providers.

**Rejection:** Mori Lens is a local-first product. Screen text must not
leave the machine by default. A cloud provider violates that invariant.
If a cloud provider is wanted, it arrives as a new ADR that addresses
the privacy model, the trust boundary, and the user's explicit consent.
It does not arrive as a silent extension of the local provider set.
