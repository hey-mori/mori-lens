//! Ollama runtime: discovery, readiness, and status.
//!
//! Implements the [`ProviderRuntime`] trait for Ollama, wrapping the existing
//! Ollama-specific logic for model listing and readiness checks.

use lens_core::model_management::{DownloadProgress, DownloadStatus, ManagementError};
use lens_core::translate::{
    ModelInfo, ModelState, Provider, ProviderRuntime, Reachability, TranslateError,
};

use crate::DEFAULT_ENDPOINT;

/// Ollama runtime for provider management operations.
///
/// Wraps the HTTP calls to Ollama's `/api/tags` and `/api/ps` endpoints,
/// providing a provider-neutral interface over Ollama-specific logic.
#[derive(Debug, Clone)]
pub struct OllamaRuntime {
    endpoint: String,
}

impl Default for OllamaRuntime {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_owned(),
        }
    }
}

impl OllamaRuntime {
    /// Create a runtime pointed at a specific Ollama endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// The endpoint this runtime is pointed at.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Pull a model from Ollama, reporting progress via callback.
    ///
    /// Sends POST /api/pull with NDJSON streaming response. Each JSON line is
    /// parsed and mapped to a [`DownloadStatus`], passed to `on_progress`.
    ///
    /// Returns the final status when the stream completes successfully, or a
    /// [`ManagementError`] if the operation fails.
    ///
    /// # Progress phases
    ///
    /// Ollama's pull API emits several status phases:
    /// - "pulling manifest" (no progress data)
    /// - "downloading" (with digest, total, completed bytes)
    /// - "verifying sha256" (per-layer verification)
    /// - "writing manifest" (finalization)
    /// - "success" (terminal)
    ///
    /// Progress is tracked per digest layer. The latest "downloading" line's
    /// values are reported. Terminal success is determined by explicit
    /// `{"status":"success"}` from the provider, not by byte arithmetic.
    ///
    /// # Timeout
    ///
    /// The timeout is an I/O inactivity timeout (connect, read, write), not a
    /// total operation duration. A multi-GB download may legitimately run much
    /// longer than the timeout value as long as progress keeps arriving.
    pub fn pull_model(
        &self,
        model: &str,
        on_progress: &mut dyn FnMut(DownloadStatus),
    ) -> Result<DownloadStatus, ManagementError> {
        // Build request body: {"model": "model", "stream": true}
        // Ollama API uses "model" field (not "name" from older examples).
        // stream: true enables streaming progress objects (default behavior).
        let body = crate::json::object(&[
            ("model", crate::json::Value::Str(model)),
            ("stream", crate::json::Value::Raw("true")),
        ]);

        let mut final_status = DownloadStatus::Starting;
        let mut parse_error: Option<ManagementError> = None;

        // I/O inactivity timeout: applies to connect, read, and write operations.
        // Not a total operation duration; a large download may exceed this wall-clock
        // as long as data keeps flowing.
        let io_timeout = std::time::Duration::from_secs(300);

        let result = crate::http::post_streaming(
            &self.endpoint,
            "/api/pull",
            &body,
            io_timeout,
            &mut |line| {
                // Skip empty lines.
                if line.trim().is_empty() {
                    return true;
                }

                // Distinguish HTTP chunk framing from provider NDJSON.
                // Chunk-size lines are hexadecimal (e.g., "1a2f") and not valid JSON.
                // Provider NDJSON lines are JSON objects starting with '{'.
                if !line.starts_with('{') {
                    // Check if this is a valid HTTP chunk-size line (hexadecimal).
                    // If it's not hex and not JSON, it's malformed provider output.
                    if !is_valid_chunk_size_line(line) {
                        // Non-framing, non-JSON garbage: malformed provider output.
                        parse_error = Some(ManagementError::InvalidResponse);
                        return false;
                    }
                    // Valid chunk framing: skip.
                    return true;
                }

                // Parse each NDJSON line.
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
                let Ok(json) = parsed else {
                    // Malformed provider JSON: record error and stop.
                    parse_error = Some(ManagementError::InvalidResponse);
                    return false;
                };

                // Check for error field first.
                if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
                    final_status = DownloadStatus::Failed(classify_ollama_error(error));
                    return false; // Stop on error.
                }

                // Extract status field.
                let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");

                // Terminal success: explicit from provider.
                if status == "success" {
                    final_status = DownloadStatus::Completed;
                    on_progress(final_status.clone());
                    return false; // Stop on success.
                }

                // Progress observation: if total/completed fields are present,
                // this is a layer download progress update.
                // Ollama sends these for "pulling <digest>" statuses.
                let total = json.get("total").and_then(|v| v.as_u64());
                let completed = json.get("completed").and_then(|v| v.as_u64());

                if let (Some(total_bytes), Some(bytes_downloaded)) = (total, completed) {
                    let progress = DownloadProgress::new(bytes_downloaded, total_bytes);
                    final_status = DownloadStatus::Downloading(progress);
                    on_progress(final_status.clone());
                }

                // Other status phases (e.g., "pulling manifest", "verifying sha256",
                // "writing manifest") don't carry progress data, so we continue.
                // We don't track phase text in the normalized status.

                true // Continue reading.
            },
        );

        // If we encountered a parse error, return it.
        if let Some(error) = parse_error {
            return Err(error);
        }

        // Map HTTP errors to ManagementError.
        match result {
            Ok(_status) => {
                // If we encountered an error in the NDJSON stream, return it as Err.
                if let DownloadStatus::Failed(error) = final_status {
                    Err(error)
                } else {
                    Ok(final_status)
                }
            }
            Err(TranslateError::Unavailable) => Err(ManagementError::EndpointUnreachable),
            Err(TranslateError::Timeout) => Err(ManagementError::DownloadFailed),
            Err(TranslateError::HttpFailed(code)) => {
                if code == 401 || code == 403 {
                    Err(ManagementError::Unauthorized)
                } else if code == 404 {
                    Err(ManagementError::ModelNotFound)
                } else {
                    Err(ManagementError::DownloadFailed)
                }
            }
            Err(_) => Err(ManagementError::DownloadFailed),
        }
    }
}

/// Check if a line is a valid HTTP chunk-size line.
///
/// Chunk-size lines are hexadecimal (e.g., "1a2f", "0") and may have
/// chunk extensions after a semicolon (e.g., "1a2f;ext=value").
fn is_valid_chunk_size_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true; // Empty lines are acceptable.
    }
    // Split on semicolon to ignore chunk extensions.
    let size_part = trimmed.split(';').next().unwrap_or("").trim();
    if size_part.is_empty() {
        return false;
    }
    // Must be valid hexadecimal.
    size_part.chars().all(|c| c.is_ascii_hexdigit())
}

/// Classify an Ollama error message into a [`ManagementError`].
///
/// Ollama returns error messages as plain text in the "error" field.
/// This function maps common patterns to typed errors.
fn classify_ollama_error(error: &str) -> ManagementError {
    let lower = error.to_lowercase();
    if lower.contains("not found") || lower.contains("does not exist") {
        ManagementError::ModelNotFound
    } else if lower.contains("unauthorized") || lower.contains("forbidden") {
        ManagementError::Unauthorized
    } else if lower.contains("connection") || lower.contains("refused") {
        ManagementError::EndpointUnreachable
    } else {
        ManagementError::DownloadFailed
    }
}

impl ProviderRuntime for OllamaRuntime {
    fn provider_identity(&self) -> Provider {
        Provider::Ollama
    }

    fn reachability(&self) -> Reachability {
        // Use the existing check_reachable logic, which hits /api/tags
        if crate::check_reachable(&self.endpoint) {
            Reachability::Reachable
        } else {
            Reachability::Unreachable
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError> {
        crate::list_models(&self.endpoint)
    }

    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError> {
        // Check if the model is in the installed list
        let models = self.list_models()?;
        let model_info = models
            .iter()
            .find(|m| m.id == model)
            .ok_or_else(|| TranslateError::ModelMissing(model.to_owned()))?;

        // Check if the model is currently loaded (via /api/ps)
        let loaded = is_model_loaded(&self.endpoint, model)?;

        if loaded {
            Ok(ModelState::Loaded {
                name: model.to_owned(),
            })
        } else {
            Ok(ModelState::Available {
                name: model_info.id.clone(),
                size: model_info.size_bytes,
            })
        }
    }
}

/// Whether a model is currently loaded in Ollama's memory.
///
/// Queries `/api/ps` and checks if the model name appears in the response.
/// This is the same check used by the settings UI and the worker.
fn is_model_loaded(endpoint: &str, model: &str) -> Result<bool, TranslateError> {
    let (status, body) = crate::http::get(endpoint, "/api/ps", std::time::Duration::from_secs(2))?;
    // A non-2xx response should not be parsed as a successful probe: a 500
    // whose body happens to contain a model name must not report that model
    // as loaded.
    if !(200..=299).contains(&status) {
        return Err(TranslateError::HttpFailed(status));
    }

    // Use canonical JSON parsing path (same as lib.rs)
    let loaded_models = crate::json::string_values_of_key(&body, "name");
    Ok(loaded_models.iter().any(|name| name == model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_server::{FakeResponse, FakeServer};

    #[test]
    fn ollama_runtime_reports_ollama_identity() {
        let runtime = OllamaRuntime::default();
        assert_eq!(runtime.provider_identity(), Provider::Ollama);
    }

    #[test]
    fn ollama_runtime_uses_default_endpoint() {
        let runtime = OllamaRuntime::default();
        assert_eq!(runtime.endpoint(), DEFAULT_ENDPOINT);
    }

    #[test]
    fn ollama_runtime_accepts_custom_endpoint() {
        let runtime = OllamaRuntime::new("localhost:9999");
        assert_eq!(runtime.endpoint(), "localhost:9999");
    }

    // Contract tests using deterministic fake HTTP server.
    // These are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

    #[test]
    fn reachability_returns_reachable_when_endpoint_answers() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        assert_eq!(runtime.reachability(), Reachability::Reachable);
    }

    #[test]
    fn reachability_returns_unreachable_when_endpoint_does_not_answer() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    #[test]
    fn list_models_returns_empty_vec_when_no_models_installed() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let models = runtime.list_models().unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn list_models_returns_models_with_name_and_size() {
        let response_body = r#"{
            "models": [
                {"name": "llama3.1:8b", "size": 4700000000},
                {"name": "qwen2.5:7b", "size": 4300000000}
            ]
        }"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(response_body).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama3.1:8b");
        assert_eq!(models[0].display_name, "llama3.1:8b");
        assert_eq!(models[0].size_bytes, 4700000000);
        assert_eq!(models[1].id, "qwen2.5:7b");
        assert_eq!(models[1].display_name, "qwen2.5:7b");
        assert_eq!(models[1].size_bytes, 4300000000);
    }

    #[test]
    fn list_models_returns_unavailable_when_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.list_models();
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_available_when_model_is_installed_but_not_loaded() {
        // First call: list_models via /api/tags
        let tags_response = r#"{"models":[{"name":"llama3.1:8b","size":4700000000}]}"#;
        // Second call: is_model_loaded via /api/ps (model not loaded)
        let ps_response = r#"{"models":[]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags"),
            FakeResponse::ok_json(ps_response).expect_get("/api/ps"),
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let state = runtime.model_state("llama3.1:8b").unwrap();

        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama3.1:8b");
                assert_eq!(size, 4700000000);
            }
            _ => panic!("Expected ModelState::Available, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_loaded_when_model_is_in_memory() {
        // First call: list_models via /api/tags
        let tags_response = r#"{"models":[{"name":"llama3.1:8b","size":4700000000}]}"#;
        // Second call: is_model_loaded via /api/ps (model is loaded)
        let ps_response = r#"{"models":[{"name":"llama3.1:8b"}]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags"),
            FakeResponse::ok_json(ps_response).expect_get("/api/ps"),
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let state = runtime.model_state("llama3.1:8b").unwrap();

        match state {
            ModelState::Loaded { name } => {
                assert_eq!(name, "llama3.1:8b");
            }
            _ => panic!("Expected ModelState::Loaded, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_model_missing_when_model_not_installed() {
        let tags_response = r#"{"models":[]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let result = runtime.model_state("nonexistent-model");

        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    #[test]
    fn model_state_returns_unavailable_when_tags_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.model_state("llama3.1:8b");

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_unavailable_when_ps_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.model_state("llama3.1:8b");

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    // P20B: Ollama pull adapter contract tests.
    // These verify the POST /api/pull NDJSON parsing and error mapping.

    #[test]
    fn pull_model_sends_correct_post_body() {
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: "{\"status\":\"success\"}\n".to_owned(),
            expect: Some(crate::fake_server::RequestExpectation {
                method: Some("POST".to_owned()),
                path: Some("/api/pull".to_owned()),
                headers: vec![],
                body: Some(r#"{"model":"llama3.1:8b","stream":true}"#.to_owned()),
            }),
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_ok());
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0], DownloadStatus::Completed);
    }

    #[test]
    fn pull_model_returns_completed_on_terminal_success() {
        // Ollama returns {"status":"success"} as terminal state.
        // We do not distinguish "already present" from "just downloaded" here;
        // the caller can refresh inventory if needed.
        let server = FakeServer::start(vec![
            FakeResponse::ok_json("{\"status\":\"success\"}\n").expect_post("/api/pull")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DownloadStatus::Completed);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0], DownloadStatus::Completed);
    }

    #[test]
    fn pull_model_reports_download_progress() {
        // Simulate a multi-phase download with progress updates.
        // Ollama uses status strings like "pulling <digest>" with digest/total/completed fields.
        let ndjson = r#"{"status":"pulling manifest"}
{"status":"pulling sha256:abc123","digest":"sha256:abc123","total":1000000,"completed":250000}
{"status":"pulling sha256:abc123","digest":"sha256:abc123","total":1000000,"completed":750000}
{"status":"verifying sha256 abc123","digest":"sha256:abc123"}
{"status":"writing manifest"}
{"status":"success"}
"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DownloadStatus::Completed);

        // Should have received: Starting (implicit), two Downloading, then Completed.
        // The callback is only called for Downloading and terminal states.
        assert!(
            statuses.len() >= 2,
            "expected at least 2 progress callbacks"
        );

        // Check that we got download progress with correct values.
        let downloading_statuses: Vec<_> = statuses
            .iter()
            .filter(|s| matches!(s, DownloadStatus::Downloading(_)))
            .collect();
        assert_eq!(
            downloading_statuses.len(),
            2,
            "expected 2 downloading updates"
        );

        // Verify the final status is Completed.
        assert_eq!(*statuses.last().unwrap(), DownloadStatus::Completed);
    }

    #[test]
    fn pull_model_reports_completion_after_download() {
        let ndjson = r#"{"status":"pulling sha256:abc","digest":"sha256:abc","total":500,"completed":500}
{"status":"success"}
"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("qwen2.5:7b", &mut |s| statuses.push(s));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DownloadStatus::Completed);
        assert_eq!(*statuses.last().unwrap(), DownloadStatus::Completed);
    }

    #[test]
    fn pull_model_maps_model_not_found_error() {
        let ndjson = r#"{"error":"model \"nonexistent\" not found"}"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("nonexistent", &mut |s| statuses.push(s));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ManagementError::ModelNotFound);
    }

    #[test]
    fn pull_model_maps_unauthorized_error() {
        let ndjson = r#"{"error":"unauthorized access"}"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ManagementError::Unauthorized);
    }

    #[test]
    fn pull_model_returns_invalid_response_on_malformed_json() {
        // Lines starting with '{' but containing invalid JSON are malformed provider data.
        // This is distinct from HTTP chunk framing (hex digits without '{').
        let ndjson = r#"{"status":"pulling manifest"}
{not valid json}
{"status":"success"}
"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ManagementError::InvalidResponse);
    }

    #[test]
    fn pull_model_skips_http_chunk_framing_lines() {
        // HTTP chunked transfer encoding sends hex size lines (e.g., "1a2f") before each chunk.
        // These do not start with '{' and should be skipped as transport framing, not provider data.
        let ndjson = r#"1a2f
{"status":"pulling manifest"}
4b
{"status":"pulling sha256:abc","digest":"sha256:abc","total":100,"completed":50}
10
{"status":"success"}
0

"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DownloadStatus::Completed);
    }

    #[test]
    fn pull_model_rejects_non_json_garbage() {
        // Non-JSON, non-chunk-framing lines are malformed provider output.
        // This is distinct from valid HTTP chunk framing (hex digits).
        let ndjson = r#"{"status":"pulling manifest"}
garbage-provider-output
{"status":"success"}
"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ManagementError::InvalidResponse);
    }

    #[test]
    fn pull_model_reports_endpoint_unreachable_on_connection_failure() {
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let mut statuses = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| statuses.push(s));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ManagementError::EndpointUnreachable);
    }

    #[test]
    fn pull_model_progress_callback_receives_correct_values() {
        let ndjson = r#"{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":250}
{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":750}
{"status":"success"}
"#;
        let server = FakeServer::start(vec![FakeResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/x-ndjson".to_owned())],
            body: ndjson.to_owned(),
            expect: None,
        }
        .expect_post("/api/pull")]);

        let runtime = OllamaRuntime::new(server.address());
        let mut progress_values = vec![];
        let result = runtime.pull_model("llama3.1:8b", &mut |s| {
            if let DownloadStatus::Downloading(p) = s {
                progress_values.push((p.bytes_downloaded, p.total_bytes));
            }
        });

        assert!(result.is_ok());
        assert_eq!(progress_values.len(), 2);
        assert_eq!(progress_values[0], (250, Some(1000)));
        assert_eq!(progress_values[1], (750, Some(1000)));
    }
}
