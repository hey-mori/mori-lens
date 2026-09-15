//! LM Studio API schema fixtures for contract tests.
//!
//! These fixtures represent the official LM Studio native v1 API schema as
//! documented at https://lmstudio.ai/docs/api/v1.
//!
//! LM Studio has two API surfaces:
//! - Native v1: `GET /api/v1/models` (management plane)
//! - OpenAI-compatible: `POST /v1/chat/completions` (inference plane)
//!
//! The native v1 model schema uses:
//! - `id`: Model identifier (e.g., "llama-3.1-8b")
//! - `name`: Human-readable name (e.g., "LLaMA 3.1 8B")
//! - `type`: Model type ("llm" or "embedding")
//! - `size`: Model size in bytes
//! - `loaded_instances`: Array of currently loaded instances (empty if not loaded)
//!
//! All tests using these fixtures are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

/// A sample LM Studio native v1 models list response with mixed model types.
///
/// This is the response from `GET /api/v1/models`.
/// Note: `loaded_instances` is empty for unloaded models.
pub const LM_STUDIO_MODELS_RESPONSE: &str = r#"{
  "data": [
    {
      "id": "llama-3.1-8b",
      "name": "LLaMA 3.1 8B",
      "type": "llm",
      "size": 4700000000,
      "loaded_instances": []
    },
    {
      "id": "qwen2.5-7b",
      "name": "Qwen 2.5 7B",
      "type": "llm",
      "size": 4300000000,
      "loaded_instances": [
        {"context_window": 4096}
      ]
    },
    {
      "id": "nomic-embed-text",
      "name": "Nomic Embed Text",
      "type": "embedding",
      "size": 278000000,
      "loaded_instances": []
    }
  ]
}"#;

/// A sample LM Studio native v1 models list response with only LLM models.
pub const LM_STUDIO_LLM_MODELS_ONLY: &str = r#"{
  "data": [
    {
      "id": "llama-3.1-8b",
      "name": "LLaMA 3.1 8B",
      "type": "llm",
      "size": 4700000000,
      "loaded_instances": []
    },
    {
      "id": "qwen2.5-7b",
      "name": "Qwen 2.5 7B",
      "type": "llm",
      "size": 4300000000,
      "loaded_instances": []
    }
  ]
}"#;

/// A sample LM Studio native v1 models list response with only embedding models.
pub const LM_STUDIO_EMBEDDING_MODELS_ONLY: &str = r#"{
  "data": [
    {
      "id": "nomic-embed-text",
      "name": "Nomic Embed Text",
      "type": "embedding",
      "size": 278000000,
      "loaded_instances": []
    }
  ]
}"#;

/// An empty models list response.
pub const LM_STUDIO_EMPTY_MODELS: &str = r#"{
  "data": []
}"#;

/// A sample LM Studio v1 chat completion request (OpenAI-compatible).
///
/// LM Studio uses the OpenAI-compatible /v1/chat/completions endpoint for inference.
pub const LM_STUDIO_CHAT_REQUEST: &str = r#"{
  "model": "llama-3.1-8b",
  "messages": [
    {
      "role": "system",
      "content": "You are a translation engine."
    },
    {
      "role": "user",
      "content": "Translate: Hello world"
    }
  ],
  "temperature": 0,
  "max_tokens": 100,
  "stream": false
}"#;

/// A sample LM Studio v1 chat completion response (OpenAI-compatible).
pub const LM_STUDIO_CHAT_RESPONSE: &str = r#"{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1725000003,
  "model": "llama-3.1-8b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "안녕 세상"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 20,
    "completion_tokens": 5,
    "total_tokens": 25
  }
}"#;

/// An authentication error response from LM Studio.
///
/// LM Studio returns 401 Unauthorized when the API token is missing or invalid.
pub const LM_STUDIO_UNAUTHORIZED_RESPONSE: &str = r#"{
  "error": {
    "message": "Invalid or missing API token",
    "type": "authentication_error",
    "code": "invalid_api_key"
  }
}"#;

/// A forbidden error response from LM Studio.
///
/// LM Studio returns 403 Forbidden when the token lacks required permissions.
pub const LM_STUDIO_FORBIDDEN_RESPONSE: &str = r#"{
  "error": {
    "message": "Insufficient permissions",
    "type": "permission_error",
    "code": "forbidden"
  }
}"#;

/// A model not found error response from LM Studio.
pub const LM_STUDIO_MODEL_NOT_FOUND_RESPONSE: &str = r#"{
  "error": {
    "message": "Model 'nonexistent-model' not found",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}"#;
