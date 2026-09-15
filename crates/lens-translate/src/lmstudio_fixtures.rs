//! LM Studio API schema fixtures for contract tests.
//!
//! These fixtures represent the official LM Studio v1 API schema as documented
//! at https://lmstudio.ai/docs/api/v1. They are used by contract tests to
//! validate the LmStudioRuntime and LmStudioTranslator implementations.
//!
//! All tests using these fixtures are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

/// LM Studio v1 model schema.
///
/// The official schema uses:
/// - `id`: Model identifier (e.g., "llama-3.1-8b")
/// - `object`: Always "model"
/// - `created`: Unix timestamp
/// - `owned_by`: Organization or user (e.g., "lmstudio", "user")
/// - `type`: Model type (e.g., "text", "embedding")
/// - `publisher_info`: Publisher metadata
/// - `model_format`: File format (e.g., "gguf")
/// - `quantization`: Quantization level (e.g., "Q4_K_M")
///
/// Example from LM Studio v1 API:
/// ```json
/// {
///   "id": "llama-3.1-8b",
///   "object": "model",
///   "created": 1725000000,
///   "owned_by": "lmstudio",
///   "type": "text",
///   "publisher_info": {...},
///   "model_format": "gguf",
///   "quantization": "Q4_K_M"
/// }
/// ```
/// A sample LM Studio v1 models list response with mixed model types.
pub const LM_STUDIO_MODELS_RESPONSE: &str = r#"{
  "object": "list",
  "data": [
    {
      "id": "llama-3.1-8b",
      "object": "model",
      "created": 1725000000,
      "owned_by": "lmstudio",
      "type": "text",
      "publisher_name": "LM Studio",
      "model_format": "gguf",
      "quantization": "Q4_K_M"
    },
    {
      "id": "qwen2.5-7b",
      "object": "model",
      "created": 1725000001,
      "owned_by": "user",
      "type": "text",
      "publisher_name": "User",
      "model_format": "gguf",
      "quantization": "Q4_K_M"
    },
    {
      "id": "nomic-embed-text",
      "object": "model",
      "created": 1725000002,
      "owned_by": "lmstudio",
      "type": "embedding",
      "publisher_name": "LM Studio",
      "model_format": "gguf",
      "quantization": "Q8_0"
    }
  ]
}"#;

/// A sample LM Studio v1 models list response with only text models.
pub const LM_STUDIO_TEXT_MODELS_ONLY: &str = r#"{
  "object": "list",
  "data": [
    {
      "id": "llama-3.1-8b",
      "object": "model",
      "created": 1725000000,
      "owned_by": "lmstudio",
      "type": "text"
    },
    {
      "id": "qwen2.5-7b",
      "object": "model",
      "created": 1725000001,
      "owned_by": "user",
      "type": "text"
    }
  ]
}"#;

/// A sample LM Studio v1 models list response with only embedding models.
pub const LM_STUDIO_EMBEDDING_MODELS_ONLY: &str = r#"{
  "object": "list",
  "data": [
    {
      "id": "nomic-embed-text",
      "object": "model",
      "created": 1725000002,
      "owned_by": "lmstudio",
      "type": "embedding"
    }
  ]
}"#;

/// An empty models list response.
pub const LM_STUDIO_EMPTY_MODELS: &str = r#"{
  "object": "list",
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
