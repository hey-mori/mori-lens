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

## Next Steps

1. Investigate LM Studio API contract
2. Compare with Ollama API contract
3. Identify concrete differences
4. Answer key questions:
   - Should model discovery be in Translator trait?
   - Should health/readiness be in Translator trait?
   - What is the minimal abstraction needed?
5. Decide: minimal change vs minimal abstraction
