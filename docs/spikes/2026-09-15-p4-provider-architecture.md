# P4 Provider Architecture Spike: Investigation

**Date:** 2026-09-15
**Status:** In Progress
**Goal:** Discover abstraction boundary by comparing Ollama with second provider

## Current Ollama Implementation Inventory

### Transport & Protocol

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| Default endpoint | `127.0.0.1:11434` (line 47) | OLLAMA-SPECIFIC |
| HTTP client | Custom `http` module (line 37) | GENERIC |
| Timeout handling | Per-request Duration | GENERIC |
| Model lease | `"30m"` keep_alive (line 124) | OLLAMA-SPECIFIC |

### Model Discovery

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| List models API | `/api/tags` endpoint (line 62) | OLLAMA-SPECIFIC |
| Model info structure | `{name, size}` (line 51-54) | GENERIC |
| Check reachable | `/api/tags` with 2s timeout (line 77-79) | OLLAMA-SPECIFIC |
| Default model | `qwen2.5:7b` (line 94) | OLLAMA-SPECIFIC (model naming) |

### Inference

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| Generate endpoint | `/api/generate` (line 248, 755) | OLLAMA-SPECIFIC |
| Request format | JSON with model, prompt, stream, options, keep_alive | OLLAMA-SPECIFIC |
| Response parsing | Extract "response" key from JSON (line 762) | OLLAMA-SPECIFIC |
| Error detection | Check for error in response body (line 259) | OLLAMA-SPECIFIC |
| Streaming | Not implemented (uses non-streaming) | GENERIC |

### Health & Readiness

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| Check reachable | `/api/tags` endpoint | OLLAMA-SPECIFIC |
| Check resident | `/api/ps` endpoint (line 211) | OLLAMA-SPECIFIC |
| Prewarm | `/api/generate` with minimal prompt (line 233-263) | OLLAMA-SPECIFIC |
| Release | `/api/generate` with keep_alive=0 (line 278-291) | OLLAMA-SPECIFIC |

### Configuration

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| Quick/Deep separation | Two OllamaTranslator instances | GENERIC |
| Endpoint configuration | String field in OllamaTranslator | GENERIC |
| Model configuration | String field in OllamaTranslator | GENERIC |
| Model pinning | `pinned: bool` field (line 138) | GENERIC |

### Error Taxonomy

| Error | Current Implementation | Classification |
|-------|----------------------|----------------|
| Unavailable | Endpoint not reachable | GENERIC (concept), OLLAMA-SPECIFIC (detection) |
| ModelMissing | Model not in /api/tags response | OLLAMA-SPECIFIC |
| Malformed | Response missing "response" key | OLLAMA-SPECIFIC |
| Timeout | HTTP timeout | GENERIC |

### Integration Points

| Component | Current Implementation | Classification |
|-----------|----------------------|----------------|
| Prewarm at startup | `OllamaTranslator::prewarm()` | GENERIC (concept), OLLAMA-SPECIFIC (implementation) |
| Status checks | `check_reachable()`, `get_local_ai_status` | GENERIC (concept), OLLAMA-SPECIFIC (implementation) |
| Library provenance | Model name stored | GENERIC |

## Key Observations

### 1. Translator Trait is Already Provider-Neutral

The `Translator` trait in `lens-core::translate` has only two methods:
- `quick(&self, request: &QuickRequest) -> Result<QuickTranslation, TranslateError>`
- `quick_streaming()` with default implementation

**Conclusion:** The trait itself does not need changes. It already abstracts the translation concept.

### 2. Ollama-Specific Code is Concentrated

All Ollama-specific assumptions are in `crates/lens-translate/src/lib.rs`:
- Endpoint URLs: `/api/tags`, `/api/generate`, `/api/ps`
- Request/response JSON structure
- Model lease (keep_alive) parameter
- Error detection in response body

### 3. Standalone Functions Need Separation

Currently mixed with OllamaTranslator:
- `list_models(endpoint)` - Ollama-specific API
- `check_reachable(endpoint)` - Ollama-specific API

These are called from settings UI before any translator is built.

### 4. Model Discovery is Translator Responsibility

Currently:
- `OllamaTranslator::installed_models()` calls `list_models()`
- `OllamaTranslator::resolve_model()` uses installed_models()
- Settings UI calls `list_models()` directly

**Question:** Should model discovery be part of Translator trait, or separate?

### 5. Health/Readiness is Ollama-Specific

Currently:
- `check_reachable()` checks `/api/tags`
- `is_resident()` checks `/api/ps`
- `prewarm()` sends minimal `/api/generate` request
- `release()` sends `/api/generate` with keep_alive=0

**Question:** Should health/readiness be part of Translator trait, or separate concern?

## Second Provider Candidates

### LM Studio Local Server

**API:** OpenAI-compatible `/v1/chat/completions`
**Pros:**
- OpenAI-compatible API (industry standard)
- Fully local
- Windows/macOS
- Model listing via `/v1/models`

**Cons:**
- Different API structure than Ollama
- May not have equivalent of `/api/ps` for resident models
- Different keep_alive semantics

**Reveals:**
- Request/response format differences
- Model discovery API differences
- Health check differences

### llama.cpp Server

**API:** OpenAI-compatible or custom
**Pros:**
- Fully local
- Cross-platform
- Low-level control

**Cons:**
- Less stable API
- More configuration burden
- Varies by build

**Reveals:**
- API stability concerns
- Configuration complexity

### Ollama OpenAI-Compatible Endpoint

**API:** Ollama's `/v1/chat/completions` compatibility layer
**Pros:**
- Same runtime, different API
- Tests API abstraction without runtime change

**Cons:**
- Doesn't reveal runtime differences
- May not expose all Ollama features

**Reveals:**
- API abstraction boundary (but not runtime boundary)

## Recommended Second Provider

**LM Studio** - reveals the most abstraction boundary differences:
- Different API format (OpenAI vs Ollama native)
- Different model discovery
- Different health/readiness model
- Different keep_alive semantics
- Real-world usage (not just compatibility layer)

## LM Studio API Investigation

**Evidence Level:** STATIC/SPEC VERIFIED (based on official LM Studio documentation and OpenAI API specification)

### API Surface 1: OpenAI-Compatible API

**Default Port:** 1234 (configurable)  
**Base URL:** `http://localhost:1234`

#### Endpoints

**GET /v1/models**
- Lists downloaded models (behavior depends on JIT loading settings)
- Response format (OpenAI-compatible):
```json
{
  "object": "list",
  "data": [
    {
      "id": "model-name",
      "object": "model",
      "created": 1234567890,
      "owned_by": "organization"
    }
  ]
}
```
- **Important:** Under JIT loading, this may expose all downloaded models, not just loaded ones
- No size information in standard response
- **Role:** Inference-facing model discovery (OpenAI-compatible clients)

**POST /v1/chat/completions**
- Main inference endpoint
- Request schema:
```json
{
  "model": "model-name",
  "messages": [
    {"role": "system", "content": "You are a translation engine..."},
    {"role": "user", "content": "Translate this text"}
  ],
  "temperature": 0.7,
  "max_tokens": 1024,
  "stream": false
}
```
- Response schema (non-streaming):
```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1234567890,
  "model": "model-name",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "translated text"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 100,
    "completion_tokens": 50,
    "total_tokens": 150
  }
}
```
- Streaming: `stream: true` returns SSE events
- No keep_alive equivalent (model stays loaded until explicitly unloaded or server restarts)

**POST /v1/responses** (newer endpoint)
- Alternative to /v1/chat/completions
- Similar schema, different response structure
- Not needed for Mori Lens (chat/completions sufficient)

### API Surface 2: Native REST API

**Base URL:** `http://localhost:1234/api/v1`

#### Endpoints

**GET /api/v1/models**
- Lists downloaded models with richer management metadata
- Response includes:
```json
{
  "models": [
    {
      "id": "model-name",
      "name": "Model Display Name",
      "type": "gguf",
      "architecture": "llama",
      "quantization": "Q4_K_M",
      "size": 4131637248,
      "loaded_instances": [
        {
          "instance_id": "inst_xxx",
          "status": "loaded",
          "loaded_at": 1234567890
        }
      ]
    }
  ]
}
```
- **Key field:** `loaded_instances` array (empty if not loaded, non-empty if loaded)
- Model state interpretation:
  - `loaded_instances` is empty → MODEL_AVAILABLE (downloaded but not loaded)
  - `loaded_instances` is non-empty → MODEL_LOADED (in memory)
- Includes size and architecture information
- **Role:** Provider management and lifecycle control

**POST /api/v1/models/load**
- Explicitly load a model into memory
- Request:
```json
{
  "model": "model-name"
}
```
- Response:
```json
{
  "success": true,
  "model_instance_id": "inst_xxx"
}
```
- **Important:** Returns `model_instance_id` which is required for unload
- Model stays loaded until explicitly unloaded or automatic eviction policy triggers
- **Ownership:** Mori Lens becomes owner of this instance

**POST /api/v1/models/unload**
- Explicitly unload a model from memory
- Request:
```json
{
  "instance_id": "inst_xxx"
}
```
- Response:
```json
{
  "success": true
}
```
- **Important:** Requires `instance_id`, not model name
- **Ownership semantics:** Only unload instances that Mori Lens created
- Do not unload instances created by user or other clients

**Model Download APIs**
- Download models from Hugging Face or direct URLs
- Not needed for Mori Lens (user manages models manually)

### Key Differences from Ollama

| Aspect | Ollama | LM Studio |
|--------|--------|-----------|
| **Default Port** | 11434 | 1234 |
| **Inference API** | `/api/generate` (native) | `/v1/chat/completions` (OpenAI-compatible) |
| **Model Discovery** | `/api/tags` | `/v1/models` or `/api/v1/models` |
| **Resident Model Check** | `/api/ps` | `/api/v1/models` (state field) |
| **Model Loading** | Implicit (first request) or prewarm | Explicit load/unload or implicit |
| **Model Lease** | `keep_alive` parameter (e.g., "30m") | No lease; model stays until unloaded |
| **Request Format** | `{model, prompt, stream, options, keep_alive}` | `{model, messages, temperature, max_tokens, stream}` |
| **Response Format** | `{response: "text"}` | `{choices: [{message: {content: "text"}}]}` |
| **Error in Body** | 200 with error field possible | Standard HTTP error codes |
| **System Prompt** | Part of prompt string | Separate message with role: "system" |
| **Streaming** | SSE with `{response: "chunk"}` | SSE with OpenAI format |

### Two API Surfaces: Role Separation

**Inference:** OpenAI-compatible `/v1/chat/completions`
- Industry standard format
- Well-documented
- Compatible with many tools
- Sufficient for Mori Lens needs
- **Role:** Inference plane only

**Model Discovery:** Native `/api/v1/models` (not `/v1/models`)
- Includes `loaded_instances` for state detection
- Includes size and architecture metadata
- **Role:** Management plane (discovery, readiness, status)
- **Note:** `/v1/models` may expose all downloaded models under JIT loading, making it unsuitable for precise state detection

**Model Lifecycle:** Native `/api/v1/models/load` and `/api/v1/models/unload`
- OpenAI-compatible API has no load/unload endpoints
- Native API provides explicit control with instance ownership
- **Role:** Management plane (lifecycle control)
- **Important:** Lifecycle operations have ownership semantics (instance_id tracking)

**Conclusion:** Two-plane architecture
- **Inference plane:** OpenAI-compatible API (Translator trait)
- **Management plane:** Native REST API (ProviderRuntime trait)

**Key finding:** The two surfaces serve different concerns and should not be conflated.

## Concrete Comparison Matrix

| Aspect | Ollama | LM Studio | Abstraction Impact |
|--------|--------|-----------|-------------------|
| **Provider identity** | Ollama | LM Studio | Need provider enum or identity |
| **Default endpoint** | `127.0.0.1:11434` | `127.0.0.1:1234` | Provider-specific defaults |
| **Endpoint normalization** | User-provided or default | User-provided or default | Same pattern |
| **Health / reachability** | GET `/api/tags` 2s timeout | GET `/v1/models` 2s timeout | Different endpoints, same concept |
| **Downloaded model discovery** | GET `/api/tags` → `{name, size}` | GET `/api/v1/models` → `{id, name, size, state}` | Different response format |
| **Loaded/resident model discovery** | GET `/api/ps` → check name in list | GET `/api/v1/models` → check state="loaded" | Different mechanism |
| **Model identifier semantics** | `name:tag` (e.g., `qwen2.5:7b`) | `id` (e.g., `model-name`) | Different naming conventions |
| **Explicit model selection** | `model` field in request | `model` field in request | Same concept |
| **No-model behavior** | Empty list from `/api/tags` | Empty list from `/v1/models` | Same concept |
| **Missing-model behavior** | 200 with error in body | 404 or error response | Different error handling |
| **Automatic/JIT loading** | Yes (first request loads model) | Yes (first request loads model) | Same behavior |
| **Explicit load** | Implicit via prewarm | POST `/api/v1/models/load` | Different mechanism |
| **Explicit unload** | `keep_alive=0` in generate request | POST `/api/v1/models/unload` | Different mechanism |
| **Inference endpoint** | POST `/api/generate` | POST `/v1/chat/completions` | Completely different |
| **Request schema** | `{model, prompt, stream, options, keep_alive}` | `{model, messages, temperature, max_tokens, stream}` | Incompatible formats |
| **System prompt representation** | Part of `prompt` string | Separate `messages[{role: "system"}]` | Different structure |
| **User prompt representation** | `prompt` field | `messages[{role: "user"}]` | Different structure |
| **Response schema** | `{response: "text"}` | `{choices: [{message: {content: "text"}}]}` | Incompatible formats |
| **Streaming support** | SSE with `{response: "chunk"}` | SSE with OpenAI format | Different event format |
| **Non-streaming support** | Yes | Yes | Same concept |
| **Timeout semantics** | Per-request Duration | Per-request Duration | Same concept |
| **Connection refused** | HTTP error → Unavailable | HTTP error → Unavailable | Same mapping |
| **HTTP non-2xx** | Mapped to TranslateError | Mapped to TranslateError | Same mapping |
| **Malformed JSON** | Missing "response" key → Malformed | Missing "choices" → Malformed | Different detection |
| **Provider error body** | Check "error" field in 200 response | Standard HTTP error codes | Different detection |
| **Cancellation possibility** | HTTP client cancellation | HTTP client cancellation | Same concept |
| **Prewarm semantics** | Minimal generate request + keep_alive | Explicit load request | Different mechanism |
| **Quick / Deep separate endpoint support** | Two Translator instances | Two Translator instances | Same pattern |
| **Authentication possibility** | None (local-only) | None (local-only) | Same |
| **Fully-local guarantee** | Yes | Yes | Same |

## State Model Analysis: Available vs Loaded vs Ready

### Current Ollama States

```
UNREACHABLE
  └─ (endpoint doesn't respond)

REACHABLE
  ├─ NO_MODELS (empty /api/tags)
  └─ HAS_MODELS
      ├─ MODEL_AVAILABLE (in /api/tags but not in /api/ps)
      └─ MODEL_LOADED (in /api/ps)
          └─ (implicitly READY - can accept requests)
```

**Ollama-specific:**
- `/api/tags` = downloaded models
- `/api/ps` = currently loaded models
- No explicit "ready" state; loaded = ready
- `keep_alive` controls how long model stays loaded

### LM Studio States

```
UNREACHABLE
  └─ (endpoint doesn't respond)

REACHABLE
  ├─ NO_MODELS (empty /api/v1/models)
  └─ HAS_MODELS
      ├─ MODEL_AVAILABLE (loaded_instances is empty)
      └─ MODEL_LOADED (loaded_instances is non-empty)
          └─ (implicitly READY - can accept requests)
```

**LM Studio-specific:**
- `/api/v1/models` includes `loaded_instances` array
- `loaded_instances` empty → model downloaded but not loaded
- `loaded_instances` non-empty → model in memory
- No `keep_alive`; model stays until unloaded or automatic eviction
- Explicit load/unload APIs with instance_id tracking

### Provider-Neutral State Model (Conceptual Vocabulary)

Both providers support these conceptual states:

```
UNREACHABLE
  └─ endpoint doesn't respond

REACHABLE
  └─ HAS_MODELS
      ├─ MODEL_AVAILABLE (downloaded but not loaded)
      └─ MODEL_LOADED (loaded and ready)
```

**Key insight:** The distinction between "available" and "loaded" is what matters for UX:

- **Available:** Model is downloaded and can be loaded
- **Loaded:** Model is in memory and ready for inference

**Conceptual UX states:**

| State | Meaning | User sees |
|-------|---------|-----------|
| UNREACHABLE | Provider not running | "Ollama/LM Studio not reachable" |
| NO_MODELS | Provider running, no models | "No models installed" |
| MODEL_AVAILABLE | Model downloaded, not loaded | Model name with "click to load" or auto-load on first use |
| MODEL_LOADED | Model in memory, ready | Model name with green indicator |
| MODEL_MISSING | Requested model not downloaded | "Model X not installed · pull command" |

**Important:** This is conceptual vocabulary for spike analysis, not a production enum. The actual type in P5/P6 should be determined by UI/status requirements:
- NO_MODELS and MODEL_MISSING are query-context-dependent
- Some UIs may not need to distinguish them
- Start with minimal types, expand if needed

**Detection mechanism** is provider-specific:
- Ollama: Check `/api/tags` and `/api/ps`
- LM Studio: Check `/api/v1/models` `loaded_instances` field

**Question:** Should this state model be part of Translator trait, or separate ProviderRuntime trait?

## Architecture Decision

### Option A: Current Translator Trait Sufficient

**Approach:** Keep Translator trait as-is, add second implementation, minimal constructor/factory.

**Changes:**
- Rename `OllamaTranslator` → keep as-is
- Add `LmStudioTranslator` with same trait
- Settings: Add `provider` field (enum: Ollama, LmStudio)
- Factory: Match on provider to construct appropriate translator
- Model discovery: Add provider-specific `list_models` functions
- Status: Add provider-specific `check_reachable` functions

**Pros:**
- Minimal change
- No new abstractions
- Translator trait remains simple

**Cons:**
- Model discovery and health checks are scattered (not in trait)
- Settings UI needs to know about provider to call right functions
- Code duplication for management operations

**Verdict:** Viable but leads to scattered management code.

### Option B: Translator Remains Inference-Only, Add ProviderRuntime

**Approach:** Keep Translator for inference, add separate ProviderRuntime for discovery/readiness/status.

**Changes:**
- Translator trait: Unchanged (quick, quick_streaming)
- New trait: `ProviderRuntime`
  - `fn reachability(&self) -> Reachability`
  - `fn list_models(&self) -> Result<Vec<ModelInfo>>`
  - `fn model_state(&self, model: &str) -> ModelState`
  - `fn prewarm(&self, model: &str) -> Result<Duration>`
  - `fn release(&self, model: &str)`
- Implementations: `OllamaRuntime`, `LmStudioRuntime`
- Translator implementations hold reference to runtime
- Settings UI uses ProviderRuntime for model discovery
- Status page uses ProviderRuntime for health checks

**Pros:**
- Clear separation: inference vs management
- Provider-neutral management operations
- Settings UI works with any provider
- Testable independently

**Cons:**
- New abstraction (but justified by concrete evidence)
- Two traits instead of one

**Verdict:** Clean separation, justified by evidence. Recommended.

### Option C: Current Contracts Cannot Represent Second Provider

**Approach:** Translator trait needs changes to support second provider.

**Analysis:**
- Translator trait (quick, quick_streaming) is already provider-neutral
- No changes needed to inference abstraction
- Only management operations need provider awareness

**Verdict:** Rejected. Current Translator trait is sufficient for inference.

### Option B — Accepted, Refined

**Approach:** Keep Translator for inference, introduce small provider-management boundary for discovery/readiness/status. Do NOT make explicit load/unload mandatory common operations yet.

**Rationale:**

1. **Evidence shows two distinct concerns:**
   - **Inference** (Translator trait) - already provider-neutral, no changes needed
   - **Management** (discovery, readiness, status) - needs provider-aware boundary

2. **Confirmed common management concerns:**
   - Provider identity (which provider is configured)
   - Reachability (is provider endpoint responding)
   - Available model discovery (what models are downloaded)
   - Readiness / loaded-state observation (is model in memory)

3. **Provider-specific or capability-dependent concerns:**
   - Explicit load (LM Studio has this, Ollama uses implicit/JIT)
   - Explicit unload (LM Studio requires instance_id, Ollama uses keep_alive=0)
   - Lease / keep_alive (Ollama-specific concept)
   - Instance ownership (LM Studio tracks instance_id)
   - JIT loading (LM Studio feature)
   - Auto-eviction (LM Studio feature)

4. **Lifecycle actions are NOT mandatory common denominator:**
   - Ollama: prewarm = tiny generate request + keep_alive lease
   - LM Studio: explicit load returns instance_id, unload requires instance_id
   - These are different lifecycle models, not the same abstraction
   - Ownership semantics differ (LM Studio tracks instance ownership, Ollama does not)

5. **Architecture risk documented:**
   - Current Ollama `release()` sends `keep_alive=0` for a model
   - In multi-client local runtime, no guarantee Mori is the only owner
   - This is existing behavior, not changed in P4
   - P5 should not create generic `release(model)` that ignores ownership
   - LM Studio unload requires `instance_id`, forcing explicit ownership tracking

**Decision:**

```rust
// Minimal common boundary (confirmed by evidence)
pub trait ProviderRuntime {
    fn provider_identity(&self) -> Provider;
    fn reachability(&self) -> Reachability;
    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError>;
    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError>;
}

// Lifecycle operations are capability-specific, NOT in trait
// May be added later as optional capabilities after ownership semantics are defined
```

**What this means for P5:**
- First PR introduces provider-aware discovery/readiness boundary
- Production interface has only the 4 confirmed common methods
- LM Studio explicit load/unload is NOT added unless necessity is confirmed
- Lifecycle actions remain provider-specific implementation details
- If lifecycle abstraction is needed later, it will be separate capability with ownership tracking

## P5 Implementation Plan

### Phase 1: Provider-Aware Discovery/Readiness Boundary

**Files to change:**
- `crates/lens-core/src/translate.rs` - Add ProviderRuntime trait (4 methods only)
- `crates/lens-translate/src/lib.rs` - Refactor OllamaTranslator to use OllamaRuntime
- `crates/lens-translate/src/ollama_runtime.rs` - New file: OllamaRuntime implementation
- `crates/lens-translate/src/lm_studio_runtime.rs` - New file: LmStudioRuntime implementation (stub initially)

**New types:**
```rust
// In lens-core::translate
pub trait ProviderRuntime {
    fn provider_identity(&self) -> Provider;
    fn reachability(&self) -> Reachability;
    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError>;
    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError>;
}

pub enum Provider {
    Ollama,
    LmStudio,
}

pub enum Reachability {
    Unreachable,
    Reachable,
}

// Conceptual vocabulary from spike - actual type determined by UI requirements
// Start minimal, expand if needed
pub enum ModelState {
    Available { name: String, size: u64 },
    Loaded { name: String },
    // Missing may be query-context-dependent, not added unless needed
}

// Existing ModelInfo moves from lens-translate to lens-core
```

**Settings changes:**
- `apps/lens/src/settings.rs` - Add `provider` field
- `crates/lens-core/src/settings.rs` - Add `Provider` enum
- Migration: Default to `Provider::Ollama` for backward compatibility

**Tests:**
- Unit tests for ProviderRuntime trait
- Contract tests for OllamaRuntime
- Fake server tests for LmStudioRuntime (no real LM Studio required)

**Note:** Lifecycle operations (prewarm, release, load, unload) remain as implementation details of each provider, not part of the common trait. If explicit lifecycle control is needed later, it will be added as optional capabilities with proper ownership semantics.

### Phase 2: LM Studio Implementation

**Files to change:**
- `crates/lens-translate/src/lm_studio_runtime.rs` - Implement LmStudioRuntime
- `crates/lens-translate/src/lm_studio_translator.rs` - New file: LmStudioTranslator
- `crates/lens-translate/src/lib.rs` - Export new types

**New types:**
- `LmStudioRuntime` - Implements ProviderRuntime for LM Studio
- `LmStudioTranslator` - Implements Translator for LM Studio

**Implementation details:**
- Use `/v1/chat/completions` for inference
- Use `/api/v1/models` for discovery and state
- Use `/api/v1/models/load` and `/api/v1/models/unload` for lifecycle
- Map LM Studio errors to TranslateError

**Tests:**
- Fake HTTP server tests for LM Studio API contract
- Integration tests with mock LM Studio responses
- Comparison tests: same request to Ollama and LM Studio should produce same result

### Phase 3: Factory and Settings Integration

**Files to change:**
- `apps/lens/src/tauri_app.rs` - Factory to construct provider-appropriate translator
- `apps/lens/src/settings.rs` - Wire provider field to translator construction
- `apps/lens/web/js/settings.js` - UI for provider selection
- `apps/lens/web/js/models.js` - Use provider-neutral model discovery
- `apps/lens/web/js/status.js` - Use provider-neutral status checks

**Changes:**
- Settings page: Add provider dropdown (Ollama / LM Studio)
- Models page: Use ProviderRuntime for model listing
- Status page: Use ProviderRuntime for health checks
- Factory: Match on provider to construct OllamaTranslator or LmStudioTranslator

**Tests:**
- Settings migration tests
- Factory tests
- UI tests (manual or automated)

### Phase 4: Error Mapping and Edge Cases

**Files to change:**
- `crates/lens-translate/src/lm_studio_translator.rs` - Error mapping
- `crates/lens-core/src/translate.rs` - TranslateError enum (if changes needed)

**Error mapping:**
- LM Studio 404 → TranslateError::ModelMissing
- LM Studio connection refused → TranslateError::Unavailable
- LM Studio malformed JSON → TranslateError::Malformed
- LM Studio timeout → TranslateError::Timeout (already exists)

**Edge cases:**
- Model name format differences (Ollama `name:tag` vs LM Studio `id`)
- Default model selection (provider-specific defaults)
- Quick/Deep with different providers (supported)

**Tests:**
- Error mapping tests
- Edge case tests

### Phase 5: Documentation and Cleanup

**Files to change:**
- `docs/architecture.md` - Document ProviderRuntime
- `docs/decisions/ADR-0008-provider-abstraction.md` - New ADR (if needed)
- `README.md` - Mention LM Studio support

**Documentation:**
- ADR for provider abstraction (if decision is significant enough)
- Architecture doc update
- README update

**Cleanup:**
- Remove duplicate code
- Ensure all tests pass
- Update #527 to mark P4 COMPLETE, P5 next

### Sequencing

**PR 1:** ProviderRuntime common boundary (Phase 1)
- Add trait with 4 confirmed common methods (provider_identity, reachability, list_models, model_state)
- Refactor Ollama to use OllamaRuntime
- No prewarm/release in trait (lifecycle remains implementation detail)
- Tests pass
- No behavior change

**PR 2:** LM Studio implementation (Phase 2)
- Implement LmStudioRuntime and LmStudioTranslator
- Fake server tests
- Not yet wired to settings

**PR 3:** Factory and settings integration (Phase 3)
- Wire provider field
- Update UI
- End-to-end tests

**PR 4:** Error mapping and edge cases (Phase 4)
- Complete error handling
- Edge case tests

**PR 5:** Documentation (Phase 5)
- ADR, architecture doc, README

### Backward Compatibility

- Default provider: Ollama
- Existing settings files work without migration
- Ollama behavior unchanged
- LM Studio is opt-in

### Default Ollama Behavior Preservation

- All existing Ollama tests pass
- OllamaTranslator behavior unchanged
- OllamaRuntime wraps existing logic
- No breaking changes

## Sources

### LM Studio (official)

- REST API overview: https://lmstudio.ai/docs/developer/rest
- List models: https://lmstudio.ai/docs/developer/rest/list
- Load model: https://lmstudio.ai/docs/developer/rest/load
- Unload model: https://lmstudio.ai/docs/developer/rest/unload
- OpenAI compatibility: https://lmstudio.ai/docs/developer/openai-compat

### Ollama (official)

- API documentation: https://github.com/ollama/ollama/blob/main/docs/api.md

### OpenAI

- API specification: https://platform.openai.com/docs/api-reference

### Evidence level

STATIC/SPEC VERIFIED for all API contracts. No runtime testing performed against live LM Studio or Ollama instances during this spike.
