# Tauri v2 E2E Testing Research

**Date:** 2026-09-22
**Status:** Research phase

## Background

Tauri v2 applications require E2E testing to verify:
- Window creation and visibility
- UI navigation and interaction
- Settings persistence across restarts
- IPC communication between frontend and backend
- Cross-platform behavior (Windows, macOS)

## Current State

Mori Lens uses Tauri v2 with:
- Main window (800x600, initially hidden)
- Detail window (480x400, initially hidden)
- Frontend in `web/` directory
- Backend commands registered in Rust
- No existing E2E test infrastructure

## Testing Approaches

### 1. WebDriverIO with Tauri Service

**Status:** Experimental, community-maintained

**Pros:**
- Familiar Selenium/WebDriver API
- Cross-browser testing capabilities
- Can test actual rendered UI
- JavaScript-based, aligns with frontend stack

**Cons:**
- Requires Tauri WebDriver server (experimental)
- Setup complexity on Windows and macOS
- May require specific Tauri configuration
- Limited official support

**Setup Requirements:**
```bash
npm install --save-dev @wdio/cli @wdio/local-runner @wdio/mocha-framework
npm install --save-dev wdio-tauri-service
```

**Challenges:**
- Tauri WebDriver must be built and running
- Platform-specific binaries may be needed
- CI integration requires headless or virtual display setup

### 2. Playwright with Custom Integration

**Status:** Manual integration required

**Pros:**
- Modern, well-maintained testing framework
- Excellent documentation and community
- Built-in wait mechanisms and assertions
- Good debugging tools

**Cons:**
- No official Tauri support
- Requires custom launcher for Tauri app
- May need to test web content separately from native window

**Approach:**
- Test frontend in browser with mocked IPC
- Test native integration separately
- Combine results for full coverage

### 3. Rust-Based Integration Tests

**Status:** Supported by Tauri

**Pros:**
- Native Rust testing infrastructure
- Direct access to Tauri APIs
- Can test window creation, IPC, etc.
- No additional dependencies

**Cons:**
- Cannot test frontend UI rendering
- Limited to backend/window management testing
- Requires Tauri test utilities

**Example:**
```rust
#[cfg(test)]
mod tests {
    use tauri::test;

    #[tokio::test]
    async fn test_window_creation() {
        let app = test::mock_app();
        // Test window creation, IPC, etc.
    }
}
```

### 4. Hybrid Approach (Recommended)

**Strategy:** Combine multiple testing layers

1. **Frontend Unit Tests (Vitest + jsdom)** — COMPLETE (P19A)
   - Test all frontend modules in isolation
   - Mock Tauri IPC boundary
   - 40 tests across 7 modules

2. **Frontend Integration Tests (Vitest + jsdom)**
   - Test module interactions
   - Verify DOM manipulation
   - Test user workflows

3. **Backend Unit Tests (cargo test)** — EXISTING
   - 577+ tests in lens-core
   - Test all business logic
   - Platform-agnostic

4. **Tauri Integration Tests (Rust)**
   - Test window creation
   - Test IPC command registration
   - Test settings persistence
   - Use Tauri's test utilities

5. **Smoke Tests (Shell scripts)**
   - Build the application
   - Launch the application
   - Verify window appears
   - Check basic functionality

## Recommendation

**Start with Hybrid Approach:**

1. **Phase 1: Rust Integration Tests** (P19B Phase 1)
   - Add integration tests to `apps/lens/src/`
   - Test window creation and IPC
   - Test settings persistence
   - Run in CI with `cargo test`

2. **Phase 2: Smoke Tests** (P19B Phase 2)
   - Create `scripts/test-e2e.sh`
   - Build application
   - Launch and verify window
   - Test basic interactions
   - Run in CI on Windows and macOS

3. **Phase 3: WebDriverIO (Optional)** (P19B Phase 3)
   - Evaluate WebDriverIO setup complexity
   - If feasible, add UI automation tests
   - Focus on critical user workflows

## Next Steps

1. Research Tauri's built-in test utilities
2. Create Rust integration test scaffold
3. Test window creation and IPC
4. Add settings persistence tests
5. Integrate into CI pipeline

## References

- Tauri v2 documentation: https://v2.tauri.app/
- Tauri test utilities: https://v2.tauri.app/develop/testing/
- WebDriverIO Tauri service: https://webdriver.io/docs/tauri-service/
