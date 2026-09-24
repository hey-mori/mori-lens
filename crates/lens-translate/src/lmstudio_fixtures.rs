//! LM Studio API schema fixtures for contract tests.
//!
//! These fixtures represent the official LM Studio native v1 API schema as
//! documented at https://lmstudio.ai/docs/developer/rest/list.
//!
//! LM Studio has two API surfaces:
//! - Native v1: `GET /api/v1/models` (management plane)
//! - OpenAI-compatible: `POST /v1/chat/completions` (inference plane)
//! - OpenAI-compatible: `GET /v1/models` (inference plane model list)
//!
//! The native v1 model list uses top-level key `"models"` (not `"data"`):
//! ```json
//! {"models": [{"key": "...", "type": "llm", ...}]}
//! ```
//! A 2xx response missing the `"models"` key is malformed.
//!
//! The inference plane /v1/models uses OpenAI format:
//! ```json
//! {"object": "list", "data": [{"id": "...", ...}]}
//! ```
//!
//! The native v1 model entry shape includes:
//! - `type`: Model type ("llm" or "embedding")
//! - `publisher`: Publisher name (e.g., "lmstudio-community")
//! - `key`: Canonical model identifier (e.g., "llama-3.1-8b")
//! - `display_name`: Human-readable label (e.g., "LLaMA 3.1 8B")
//! - `size_bytes`: Model size in bytes
//! - `loaded_instances`: Array of currently loaded instances (empty if not loaded)
//!   Each instance has `id` and `config.context_length`.
//! - `max_context_length`: Maximum context window size
//! - `format`: Model format (e.g., "gguf")
//!
//! All tests using these fixtures are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

/// A sample LM Studio inference plane /v1/models response.
///
/// This is the OpenAI-compatible format returned by `GET /v1/models`.
/// Used to determine inference visibility (JIT state).
pub const LM_STUDIO_INFERENCE_MODELS: &str = r#"{
  "object": "list",
  "data": [
    {
      "id": "llama-3.1-8b",
      "object": "model",
      "owned_by": "organization"
    },
    {
      "id": "qwen2.5-7b",
      "object": "model",
      "owned_by": "organization"
    }
  ]
}"#;

/// Empty inference plane response (JIT disabled, no models loaded).
pub const LM_STUDIO_INFERENCE_MODELS_EMPTY: &str = r#"{
  "object": "list",
  "data": []
}"#;

/// Inference plane response with only llama-3.1-8b visible.
/// Used to test JIT semantics where only some models are exposed.
pub const LM_STUDIO_INFERENCE_MODELS_WITH_LLAMA: &str = r#"{
  "object": "list",
  "data": [
    {
      "id": "llama-3.1-8b",
      "object": "model",
      "owned_by": "organization"
    }
  ]
}"#;

/// LM Studio native v1 model list fixture with two LLM models.
///
/// Schema per https://lmstudio.ai/docs/developer/rest/list:
/// - `type`: "llm" for translation-capable models
/// - `publisher`: publisher name
/// - `key`: canonical model identifier
/// - `display_name`: human-readable label
/// - `size_bytes`: model size in bytes
/// - `loaded_instances`: array of loaded instances (empty if not loaded)
/// - `max_context_length`: maximum context window size
/// - `format`: model format (e.g., "gguf")
///
/// Both models in this fixture have empty loaded_instances (not loaded).
pub const LM_STUDIO_TWO_LLM_MODELS: &str = r#"{
  "models": [
    {
      "type": "llm",
      "publisher": "lmstudio-community",
      "key": "llama-3.1-8b",
      "display_name": "LLaMA 3.1 8B",
      "size_bytes": 4294967296,
      "loaded_instances": [],
      "max_context_length": 131072,
      "format": "gguf"
    },
    {
      "type": "llm",
      "publisher": "lmstudio-community",
      "key": "qwen2.5-7b",
      "display_name": "Qwen 2.5 7B",
      "size_bytes": 3221225472,
      "loaded_instances": [],
      "max_context_length": 131072,
      "format": "gguf"
    }
  ]
}"#;

/// A sample LM Studio native v1 models list response with mixed model types.
///
/// This is the response from `GET /api/v1/models`.
/// Note: `loaded_instances` is empty for unloaded models.
pub const LM_STUDIO_MODELS_RESPONSE: &str = r#"{
  "models": [
    {
      "key": "llama-3.1-8b",
      "display_name": "LLaMA 3.1 8B",
      "type": "llm",
      "size_bytes": 4700000000,
      "loaded_instances": []
    },
    {
      "key": "qwen2.5-7b",
      "display_name": "Qwen 2.5 7B",
      "type": "llm",
      "size_bytes": 4300000000,
      "loaded_instances": [
        {"id": "qwen2.5-7b-0", "config": {"context_length": 4096}}
      ]
    },
    {
      "key": "nomic-embed-text",
      "display_name": "Nomic Embed Text",
      "type": "embedding",
      "size_bytes": 278000000,
      "loaded_instances": []
    }
  ]
}"#;

/// A sample LM Studio native v1 models list response with only LLM models.
pub const LM_STUDIO_LLM_MODELS_ONLY: &str = r#"{
  "models": [
    {
      "key": "llama-3.1-8b",
      "display_name": "LLaMA 3.1 8B",
      "type": "llm",
      "size_bytes": 4700000000,
      "loaded_instances": []
    },
    {
      "key": "qwen2.5-7b",
      "display_name": "Qwen 2.5 7B",
      "type": "llm",
      "size_bytes": 4300000000,
      "loaded_instances": []
    }
  ]
}"#;

/// A sample LM Studio native v1 models list response with only embedding models.
pub const LM_STUDIO_EMBEDDING_MODELS_ONLY: &str = r#"{
  "models": [
    {
      "key": "nomic-embed-text",
      "display_name": "Nomic Embed Text",
      "type": "embedding",
      "size_bytes": 278000000,
      "loaded_instances": []
    }
  ]
}"#;

/// An empty models list response.
pub const LM_STUDIO_EMPTY_MODELS: &str = r#"{
  "models": []
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

/// Two models with identical display names but different keys.
///
/// Regression fixture: identity resolution must use canonical key,
/// not display_name. If display_name were used as identity, looking up
/// either model by "My Model" would be ambiguous.
pub const LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES: &str = r#"{
  "models": [
    {
      "key": "llama-3.1-8b",
      "display_name": "My Model",
      "type": "llm",
      "size_bytes": 4700000000,
      "loaded_instances": []
    },
    {
      "key": "qwen2.5-7b",
      "display_name": "My Model",
      "type": "llm",
      "size_bytes": 4300000000,
      "loaded_instances": [
        {"id": "qwen2.5-7b-0", "config": {"context_length": 4096}}
      ]
    }
  ]
}"#;

/// A model with no type field. Fail-closed: must be excluded.
pub const LM_STUDIO_MISSING_TYPE: &str = r#"{
  "models": [
    {
      "key": "mystery-model",
      "display_name": "Mystery Model",
      "size_bytes": 1000000000,
      "loaded_instances": []
    },
    {
      "key": "known-llm",
      "display_name": "Known LLM",
      "type": "llm",
      "size_bytes": 2000000000,
      "loaded_instances": []
    }
  ]
}"#;

/// A model with an unknown future type. Fail-closed: must be excluded.
pub const LM_STUDIO_UNKNOWN_TYPE: &str = r#"{
  "models": [
    {
      "key": "future-model",
      "display_name": "Future Model",
      "type": "multimodal",
      "size_bytes": 8000000000,
      "loaded_instances": []
    },
    {
      "key": "known-llm",
      "display_name": "Known LLM",
      "type": "llm",
      "size_bytes": 2000000000,
      "loaded_instances": []
    }
  ]
}"#;

/// An official-shape LM Studio native v1 response with all documented fields.
///
/// Per https://lmstudio.ai/docs/developer/rest/list, each model entry includes:
/// type, publisher, key, display_name, size_bytes, loaded_instances (with id
/// and config.context_length), max_context_length, and format.
pub const LM_STUDIO_OFFICIAL_SHAPE: &str = r#"{
  "models": [
    {
      "type": "llm",
      "publisher": "lmstudio-community",
      "key": "llama-3.1-8b",
      "display_name": "LLaMA 3.1 8B",
      "size_bytes": 4700000000,
      "loaded_instances": [
        {"id": "llama-3.1-8b-0", "config": {"context_length": 8192}}
      ],
      "max_context_length": 131072,
      "format": "gguf"
    },
    {
      "type": "llm",
      "publisher": "bartowski",
      "key": "qwen2.5-7b",
      "display_name": "Qwen 2.5 7B",
      "size_bytes": 4300000000,
      "loaded_instances": [],
      "max_context_length": 32768,
      "format": "gguf"
    }
  ]
}"#;

/// Malformed: empty JSON object. A 2xx with no "models" key is malformed.
pub const LM_STUDIO_MALFORMED_EMPTY_OBJECT: &str = r#"{}"#;

/// Malformed: stale envelope using "data" instead of "models".
/// The native v1 API uses "models" as the top-level key.
pub const LM_STUDIO_MALFORMED_STALE_ENVELOPE: &str = r#"{
  "data": []
}"#;

/// Malformed: truncated JSON. The "models" key is present but the array is
/// incomplete: an object starts but never closes, and the array lacks `]`.
pub const LM_STUDIO_MALFORMED_TRUNCATED: &str = r#"{
  "models": [
    {
      "key": "truncated"#;

/// Malformed: an explicit LLM entry with no `key` field.
/// A type=="llm" model without canonical identity is not a valid model.
pub const LM_STUDIO_MALFORMED_LLM_MISSING_KEY: &str = r#"{
  "models": [
    {
      "display_name": "No Key Model",
      "type": "llm",
      "size_bytes": 1000000000,
      "loaded_instances": []
    }
  ]
}"#;

/// Malformed: an explicit LLM entry with no `size_bytes` field.
pub const LM_STUDIO_MALFORMED_LLM_MISSING_SIZE: &str = r#"{
  "models": [
    {
      "key": "no-size-model",
      "display_name": "No Size Model",
      "type": "llm",
      "loaded_instances": []
    }
  ]
}"#;

/// Malformed: an explicit LLM entry with no `loaded_instances` field.
pub const LM_STUDIO_MALFORMED_LLM_MISSING_LOADED: &str = r#"{
  "models": [
    {
      "key": "no-loaded-model",
      "display_name": "No Loaded Model",
      "type": "llm",
      "size_bytes": 1000000000
    }
  ]
}"#;

/// Malformed: the models array has complete objects but no closing `]`.
pub const LM_STUDIO_MALFORMED_UNCLOSED_ARRAY: &str = r#"{
  "models": [
    {
      "key": "orphan",
      "display_name": "Orphan",
      "type": "llm",
      "size_bytes": 1000000000,
      "loaded_instances": []
    }
"#;

/// A model with whitespace in loaded_instances array.
/// Tests that `[ ]` (empty with whitespace) is correctly detected as not loaded.
pub const LM_STUDIO_LOADED_WHITESPACE: &str = r#"{
  "models": [
    {
      "key": "whitespace-model",
      "display_name": "Whitespace Model",
      "type": "llm",
      "size_bytes": 1000000000,
      "loaded_instances": [ ]
    }
  ]
}"#;

/// Malformed: field name appearing only inside a string value, not as an actual field.
/// The `has_field` helper must not mistake value text for a field key.
pub const LM_STUDIO_MALFORMED_FIELD_NAME_IN_VALUE: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "foo",
      "display_name": "loaded_instances",
      "size_bytes": 123
    }
  ]
}"#;

/// Malformed: missing comma between model objects.
/// A structurally closed but syntactically invalid JSON must not parse successfully.
pub const LM_STUDIO_MALFORMED_MISSING_COMMA: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "one",
      "display_name": "One",
      "size_bytes": 1,
      "loaded_instances": []
    }
    {
      "type": "llm",
      "key": "two",
      "display_name": "Two",
      "size_bytes": 2,
      "loaded_instances": []
    }
  ]
}"#;

/// Malformed: size_bytes is a string instead of a number.
/// Wrong JSON type for required field is malformed.
pub const LM_STUDIO_MALFORMED_SIZE_AS_STRING: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "bad-size",
      "display_name": "Bad Size",
      "size_bytes": "123",
      "loaded_instances": []
    }
  ]
}"#;

/// Malformed: loaded_instances is an object instead of an array.
/// Wrong JSON type for required field is malformed.
pub const LM_STUDIO_MALFORMED_LOADED_AS_OBJECT: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "bad-loaded",
      "display_name": "Bad Loaded",
      "size_bytes": 123,
      "loaded_instances": {}
    }
  ]
}"#;

/// Malformed: key is null instead of a string.
/// Wrong JSON type for required field is malformed.
pub const LM_STUDIO_MALFORMED_KEY_NULL: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": null,
      "display_name": "Null Key",
      "size_bytes": 123,
      "loaded_instances": []
    }
  ]
}"#;

/// Malformed: models is not an array (it's an object).
pub const LM_STUDIO_MALFORMED_MODELS_NOT_ARRAY: &str = r#"{
  "models": {}
}"#;

/// Malformed: an explicit LLM entry with no `display_name` field.
pub const LM_STUDIO_MALFORMED_LLM_MISSING_DISPLAY_NAME: &str = r#"{
  "models": [
    {
      "key": "no-display-model",
      "type": "llm",
      "size_bytes": 1000000000,
      "loaded_instances": []
    }
  ]
}"#;

/// Valid: model with escaped characters in display_name.
/// Tests that JSON string escaping is handled correctly by the parser.
pub const LM_STUDIO_ESCAPED_DISPLAY_NAME: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "escaped-model",
      "display_name": "Model with \"quotes\" and \\backslash",
      "size_bytes": 1000000000,
      "loaded_instances": []
    }
  ]
}"#;

/// Valid: model with unknown extra fields.
/// Tests that the parser tolerates additional fields not in the minimal schema.
pub const LM_STUDIO_EXTRA_FIELDS: &str = r#"{
  "models": [
    {
      "type": "llm",
      "key": "extra-model",
      "display_name": "Extra Model",
      "size_bytes": 1000000000,
      "loaded_instances": [],
      "publisher": "test-publisher",
      "architecture": "transformer",
      "quantization": "q4_k_m",
      "params_string": "1B",
      "max_context_length": 4096,
      "format": "gguf",
      "capabilities": ["text-generation"],
      "description": "A test model with extra fields",
      "some_future_field": "value"
    }
  ]
}"#;

/// A 500 Internal Server Error response body.
pub const LM_STUDIO_INTERNAL_ERROR: &str = r#"{
  "error": {
    "message": "Internal server error",
    "type": "server_error",
    "code": "internal_error"
  }
}"#;
