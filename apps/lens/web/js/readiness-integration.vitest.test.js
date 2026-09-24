// Integration test: proves production readiness action wiring
// Tests real status.js + readiness.js + operation-controller.js together
// Only mocks Tauri IPC boundary and minimal navigation
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  t: vi.fn((key) => key),
  showSection: vi.fn(),
}));

// Mock Tauri IPC
vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

// Mock i18n
vi.mock('./i18n.js', () => ({
  t: mocks.t,
}));

// Mock minimal navigation
vi.mock('./navigation.js', () => ({
  showSection: mocks.showSection,
}));

// Import real production modules (NOT mocked)
import { loadStatus, _resetCaches } from './status.js';
import { state } from './state.js';

const SAMPLE_DIAGNOSTICS = {
  version: '0.11.0',
  platform: 'windows',
  paused: false,
};

const SAMPLE_RUNTIME_STATUS = {
  running: true,
  uptime: 3600,
};

const SAMPLE_HEALTH_STATUS = {
  quick: { state: 'ready', failure: null, latency_ms: 45 },
  deep: { state: 'ready', failure: null, latency_ms: 120 },
  stale: false,
};

// Exact wire-format readiness fixtures matching Rust serde contract
const READINESS_DOWNLOAD = {
  quick_lane: 'no_compatible_model',
  quick_provider: 'ollama',
  quick_model: null,
  quick_effective_model: null,
  quick_provider_access: 'available',
  quick_model_inventory: { available: { models: [] } },
  quick_next_action: 'download',
  quick_can_download: true,
  quick_can_load: false,
  quick_supports_progress: true,
  quick_download_target: null,
  download: null,
  deep_lane: 'ready',
  deep_provider: 'ollama',
  deep_model: 'qwen2.5:14b',
  deep_configuration_valid: true,
};

const READINESS_LOAD = {
  quick_lane: 'model_present',
  quick_provider: 'lmstudio',
  quick_model: 'qwen2.5:7b',
  quick_effective_model: 'qwen2.5:7b',
  quick_provider_access: 'available',
  quick_model_inventory: { available: { models: ['qwen2.5:7b'] } },
  quick_next_action: 'load',
  quick_can_download: false,
  quick_can_load: true,
  quick_supports_progress: false,
  quick_download_target: null,
  download: null,
  deep_lane: 'ready',
  deep_provider: 'ollama',
  deep_model: 'qwen2.5:14b',
  deep_configuration_valid: true,
};

const READINESS_REFRESH = {
  quick_lane: 'endpoint_unreachable',
  quick_provider: 'ollama',
  quick_model: null,
  quick_effective_model: null,
  quick_provider_access: 'unreachable',
  quick_model_inventory: null,
  quick_next_action: 'refresh',
  quick_can_download: false,
  quick_can_load: false,
  quick_supports_progress: false,
  quick_download_target: null,
  download: null,
  deep_lane: 'ready',
  deep_provider: 'ollama',
  deep_model: 'qwen2.5:14b',
  deep_configuration_valid: true,
};

const READINESS_CONFIGURE_MODEL = {
  quick_lane: 'no_compatible_model',
  quick_provider: 'ollama',
  quick_model: null,
  quick_effective_model: null,
  quick_provider_access: 'available',
  quick_model_inventory: { available: { models: [] } },
  quick_next_action: 'configure_model',
  quick_can_download: false,
  quick_can_load: false,
  quick_supports_progress: false,
  quick_download_target: null,
  download: null,
  deep_lane: 'ready',
  deep_provider: 'ollama',
  deep_model: 'qwen2.5:14b',
  deep_configuration_valid: true,
};

function defaultMockImplementation() {
  mocks.invoke.mockImplementation(async (cmd) => {
    if (cmd === 'get_diagnostics') return SAMPLE_DIAGNOSTICS;
    if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
    if (cmd === 'get_health_status') return SAMPLE_HEALTH_STATUS;
    if (cmd === 'get_provider_readiness') return READINESS_DOWNLOAD;
    if (cmd === 'start_quick_model_download') return { success: true };
    if (cmd === 'load_lmstudio_instance') return { success: true };
    if (cmd === 'request_pause_toggle') return { paused: true };
    if (cmd === 'request_quit') return { success: true };
    if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
    if (cmd === 'set_autostart') return { success: true };
    if (cmd === 'run_health_check') return { success: true };
    if (cmd === 'get_diagnostic_events') return [];
    throw new Error(`Unexpected command: ${cmd}`);
  });
}

describe('Production readiness action integration', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    _resetCaches();
    // Reset shared state between tests
    state.cachedProviderReadiness = null;
    state.providerReadinessLoadState = 'NOT_LOADED';

    fixture = createControlCenterFixture();

    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `<div id="status-content"></div>`;

    // Set up window.showSection for configure-model action
    window.showSection = mocks.showSection;

    defaultMockImplementation();
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
    delete window.showSection;
    state.cachedProviderReadiness = null;
  });

  it('Download action click invokes start_quick_model_download exactly once', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;

    await loadStatus();

    const downloadBtn = document.querySelector('.readiness-action-btn[data-action="download"]');
    expect(downloadBtn).toBeTruthy();

    downloadBtn.click();

    // Wait for async handler
    await new Promise(resolve => setTimeout(resolve, 10));

    const downloadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'start_quick_model_download');
    expect(downloadCalls.length).toBe(1);
  });

  it('Load action click invokes load_lmstudio_instance and get_provider_readiness exactly once each', async () => {
    state.cachedProviderReadiness = READINESS_LOAD;

    await loadStatus();

    const loadBtn = document.querySelector('.readiness-action-btn[data-action="load"]');
    expect(loadBtn).toBeTruthy();

    // Reset mock to count only post-click calls
    mocks.invoke.mockClear();

    loadBtn.click();

    // Wait for async handler (load calls refreshProviderReadiness which is async)
    await new Promise(resolve => setTimeout(resolve, 50));

    const loadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'load_lmstudio_instance');
    const readinessCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_provider_readiness');

    expect(loadCalls.length).toBe(1);
    expect(readinessCalls.length).toBe(1);
  });

  it('Retry action click invokes get_provider_readiness exactly once', async () => {
    state.cachedProviderReadiness = READINESS_REFRESH;

    await loadStatus();

    const retryBtn = document.querySelector('.readiness-action-btn[data-action="refresh"]');
    expect(retryBtn).toBeTruthy();

    mocks.invoke.mockClear();

    retryBtn.click();

    await new Promise(resolve => setTimeout(resolve, 50));

    const readinessCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_provider_readiness');
    expect(readinessCalls.length).toBe(1);
  });

  it('Configure Model action click invokes showSection(models) exactly once', async () => {
    state.cachedProviderReadiness = READINESS_CONFIGURE_MODEL;

    await loadStatus();

    const configBtn = document.querySelector('.readiness-action-btn[data-action="configure-model"]');
    expect(configBtn).toBeTruthy();

    configBtn.click();

    await new Promise(resolve => setTimeout(resolve, 10));

    expect(mocks.showSection).toHaveBeenCalledWith('models');
    expect(mocks.showSection).toHaveBeenCalledTimes(1);
  });

  it('multiple passive loadStatus calls do not duplicate action handlers', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;

    await loadStatus();

    // Perform multiple passive loads (no _resetCaches between them)
    await loadStatus();
    await loadStatus();
    await loadStatus();

    const downloadBtn = document.querySelector('.readiness-action-btn[data-action="download"]');
    expect(downloadBtn).toBeTruthy();

    mocks.invoke.mockClear();

    downloadBtn.click();

    await new Promise(resolve => setTimeout(resolve, 10));

    const downloadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'start_quick_model_download');
    expect(downloadCalls.length).toBe(1);
  });
});

describe('Stable DOM identity without _resetCaches', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    _resetCaches();
    state.cachedProviderReadiness = null;
    state.providerReadinessLoadState = 'NOT_LOADED';

    fixture = createControlCenterFixture();

    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `<div id="status-content"></div>`;

    window.showSection = mocks.showSection;
    defaultMockImplementation();
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
    delete window.showSection;
    state.cachedProviderReadiness = null;
  });

  it('readiness action button survives passive loadStatus without _resetCaches', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;

    // First load
    await loadStatus();

    // Capture the actual button DOM node reference
    const btn1 = document.querySelector('.readiness-action-btn[data-action="download"]');
    expect(btn1).toBeTruthy();

    // Focus the button to verify focus survives
    btn1.focus();
    expect(document.activeElement).toBe(btn1);

    // Passive loadStatus (no _resetCaches between loads)
    await loadStatus();

    // Same button object must survive (DOM identity preserved)
    const btn2 = document.querySelector('.readiness-action-btn[data-action="download"]');
    expect(btn2).toBeTruthy();
    expect(btn1).toBe(btn2);

    // Click must still invoke command exactly once
    mocks.invoke.mockClear();
    btn2.click();
    await new Promise(resolve => setTimeout(resolve, 10));

    const downloadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'start_quick_model_download');
    expect(downloadCalls.length).toBe(1);
  });

  it('region DOM identity preserved across passive loads', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;

    await loadStatus();

    const overview1 = document.getElementById('status-overview-region');
    const readiness1 = document.getElementById('status-readiness-region');
    const health1 = document.getElementById('status-health-region');
    const content1 = document.getElementById('status-content');

    // Multiple passive loads without _resetCaches
    await loadStatus();
    await loadStatus();
    await loadStatus();

    const overview2 = document.getElementById('status-overview-region');
    const readiness2 = document.getElementById('status-readiness-region');
    const health2 = document.getElementById('status-health-region');
    const content2 = document.getElementById('status-content');

    expect(overview1).toBe(overview2);
    expect(readiness1).toBe(readiness2);
    expect(health1).toBe(health2);
    expect(content1).toBe(content2);
  });
});

describe('Status-content delegation regression', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    _resetCaches();
    state.cachedProviderReadiness = null;
    state.providerReadinessLoadState = 'NOT_LOADED';

    fixture = createControlCenterFixture();

    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `<div id="status-content"></div>`;

    window.showSection = mocks.showSection;
    defaultMockImplementation();
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
    delete window.showSection;
    state.cachedProviderReadiness = null;
  });

  it('Pause button works after multiple passive loadStatus calls', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;
    await loadStatus();
    await loadStatus();
    await loadStatus();

    const pauseBtn = document.getElementById('pause-btn');
    expect(pauseBtn).toBeTruthy();

    mocks.invoke.mockClear();
    pauseBtn.click();
    await new Promise(resolve => setTimeout(resolve, 50));

    const pauseCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'request_pause_toggle');
    expect(pauseCalls.length).toBe(1);
  });

  it('Quit button works after multiple passive loadStatus calls', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;
    await loadStatus();
    await loadStatus();
    await loadStatus();

    const quitBtn = document.getElementById('quit-btn');
    expect(quitBtn).toBeTruthy();

    mocks.invoke.mockClear();
    quitBtn.click();
    await new Promise(resolve => setTimeout(resolve, 10));

    const quitCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'request_quit');
    expect(quitCalls.length).toBe(1);
  });

  it('Refresh Local AI button works after multiple passive loadStatus calls', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;
    await loadStatus();
    await loadStatus();
    await loadStatus();

    const refreshBtn = document.getElementById('refresh-local-ai-btn');
    expect(refreshBtn).toBeTruthy();

    mocks.invoke.mockClear();
    refreshBtn.click();
    await new Promise(resolve => setTimeout(resolve, 50));

    // refreshReadinessAndRender calls get_provider_readiness
    const readinessCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_provider_readiness');
    expect(readinessCalls.length).toBeGreaterThanOrEqual(1);
  });

  it('no accumulated listeners after repeated passive loads', async () => {
    state.cachedProviderReadiness = READINESS_DOWNLOAD;

    // Load many times
    for (let i = 0; i < 10; i++) {
      await loadStatus();
    }

    const downloadBtn = document.querySelector('.readiness-action-btn[data-action="download"]');
    expect(downloadBtn).toBeTruthy();

    mocks.invoke.mockClear();
    downloadBtn.click();
    await new Promise(resolve => setTimeout(resolve, 10));

    // Exactly one download call, not 10
    const downloadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'start_quick_model_download');
    expect(downloadCalls.length).toBe(1);

    // Also verify pause still works exactly once
    const pauseBtn = document.getElementById('pause-btn');
    expect(pauseBtn).toBeTruthy();

    mocks.invoke.mockClear();
    pauseBtn.click();
    await new Promise(resolve => setTimeout(resolve, 50));

    const pauseCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'request_pause_toggle');
    expect(pauseCalls.length).toBe(1);
  });
});
