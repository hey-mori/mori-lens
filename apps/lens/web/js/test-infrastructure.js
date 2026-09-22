// Test infrastructure for Control Center behavioral tests
// Provides strict IPC mock harness and DOM fixtures

import { vi } from 'vitest';

// Strict IPC mock harness
// Unknown command names fail immediately to catch frontend/backend drift
export function createIpcMock() {
  const knownCommands = new Set([
    // Settings
    'get_settings',
    'save_settings',
    // Library
    'library_distinct_apps',
    'library_distinct_source_languages',
    'library_distinct_target_languages',
    'library_distinct_tags',
    'library_query',
    'library_get',
    'library_get_tags',
    'library_delete',
    'library_update_note',
    'library_set_tags',
    'library_export',
    'library_export_json',
    'library_export_jsonl',
    'library_import_jsonl',
    // Review
    'library_review_session',
    'library_later_count',
    'library_set_review_state',
    // Models
    'check_provider',
    'list_models',
    'test_model',
    // Status
    'get_diagnostics',
    'get_runtime_status',
    'get_health_status',
    'run_health_check',
    'get_diagnostic_events',
    'request_pause_toggle',
    'request_quit',
    'set_autostart',
    // Mori
    'mori_status',
    'mori_send_item',
  ]);

  const responses = new Map();
  const callLog = [];

  const mockInvoke = vi.fn(async (cmd, args) => {
    callLog.push({ cmd, args });

    if (!knownCommands.has(cmd)) {
      throw new Error(`Unknown IPC command: ${cmd}. This indicates frontend/backend drift.`);
    }

    const response = responses.get(cmd);
    if (response === undefined) {
      throw new Error(`No mock response configured for command: ${cmd}`);
    }

    if (response instanceof Error) {
      throw response;
    }

    return response;
  });

  return {
    invoke: mockInvoke,
    setResponse: (cmd, response) => responses.set(cmd, response),
    setError: (cmd, error) => responses.set(cmd, error),
    getCalls: () => callLog,
    getCallsFor: (cmd) => callLog.filter(c => c.cmd === cmd),
    reset: () => {
      mockInvoke.mockClear();
      callLog.length = 0;
      responses.clear();
    },
  };
}

// Minimal DOM fixture for Control Center
// Derived from actual index.html structure
export function createControlCenterFixture() {
  const fixture = document.createElement('div');
  fixture.id = 'control-center';
  fixture.innerHTML = `
    <nav id="nav">
      <button data-section="status" class="nav-btn active">Status</button>
      <button data-section="library" class="nav-btn">Library</button>
      <button data-section="review" class="nav-btn">Review</button>
      <button data-section="settings" class="nav-btn">Settings</button>
      <button data-section="diagnostics" class="nav-btn">Diagnostics</button>
    </nav>

    <section id="status-section" class="section active">
      <div id="health-status"></div>
      <button id="run-health-check">Run Health Check</button>
    </section>

    <section id="library-section" class="section">
      <input id="library-search" type="text" placeholder="Search...">
      <select id="library-app-filter">
        <option value="">All apps</option>
      </select>
      <div id="library-items"></div>
    </section>

    <section id="review-section" class="section">
      <div id="review-item"></div>
      <button id="review-known">Known</button>
      <button id="review-later">Later</button>
      <button id="review-archive">Archive</button>
      <button id="review-undo">Undo</button>
    </section>

    <section id="settings-section" class="section">
      <form id="settings-form">
        <select id="f-language"></select>
        <select id="interface-language-select">
          <option value="en">English</option>
          <option value="ko">한국어</option>
        </select>
        <input id="f-dwell" type="number">
        <select id="f-theme">
          <option value="white">White</option>
          <option value="black">Black</option>
        </select>
        <select id="f-text-size">
          <option value="small">Small</option>
          <option value="medium">Medium</option>
          <option value="large">Large</option>
        </select>
        <input id="f-translucency" type="text">
        <select id="f-ocr">
          <option value="auto">Auto</option>
          <option value="manual">Manual</option>
        </select>
        <input id="f-profile" type="text">
        <select id="f-hover-scope">
          <option value="word">Word</option>
          <option value="sentence">Sentence</option>
        </select>
        <select id="f-default-tier">
          <option value="translation">Translation</option>
          <option value="analysis">Analysis</option>
        </select>
        <input id="f-history" type="checkbox">
        <input id="f-history-days" type="number">
        <input id="f-history-store-app" type="checkbox">
        <input id="f-history-excluded" type="text">
        <select id="f-quick-engine">
          <option value="ollama">Ollama</option>
          <option value="lmstudio">LM Studio</option>
        </select>
        <input id="f-quick-endpoint" type="text">
        <select id="f-quick-model-select"></select>
        <select id="f-deep-engine">
          <option value="ollama">Ollama</option>
          <option value="lmstudio">LM Studio</option>
        </select>
        <input id="f-deep-model" type="text">
        <input id="f-deep-endpoint" type="text">
        <button id="save-settings" type="button">Save</button>
      </form>
      <div id="settings-status"></div>
    </section>

    <section id="diagnostics-section" class="section">
      <button id="export-diagnostics">Export</button>
      <div id="diagnostics-status"></div>
    </section>
  `;

  document.body.appendChild(fixture);
  return fixture;
}

export function cleanupFixture(fixture) {
  if (fixture && fixture.parentNode) {
    fixture.parentNode.removeChild(fixture);
  }
}
