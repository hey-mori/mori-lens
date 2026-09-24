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

describe('Production readiness action integration', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    _resetCaches();
    fixture = createControlCenterFixture();

    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `<div id="status-content"></div>`;

    // Set up window.showSection for configure-model action
    window.showSection = mocks.showSection;

    // Default mock responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') return SAMPLE_DIAGNOSTICS;
      if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
      if (cmd === 'get_health_status') return SAMPLE_HEALTH_STATUS;
      if (cmd === 'get_provider_readiness') {
        return {
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
      }
      if (cmd === 'start_quick_model_download') return { success: true };
      if (cmd === 'load_lmstudio_instance') return { success: true };
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
    delete window.showSection;
  });

  it('Download action click invokes start_quick_model_download exactly once', async () => {
    // Set up readiness state before loading status
    state.cachedProviderReadiness = {
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
    // Configure readiness with Load action
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') return SAMPLE_DIAGNOSTICS;
      if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
      if (cmd === 'get_health_status') return SAMPLE_HEALTH_STATUS;
      if (cmd === 'get_provider_readiness') {
        return {
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
      }
      if (cmd === 'load_lmstudio_instance') return { success: true };
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadStatus();

    const loadBtn = document.querySelector('.readiness-action-btn[data-action="load"]');
    expect(loadBtn).toBeTruthy();

    // Reset mock to count only post-click calls
    mocks.invoke.mockClear();

    loadBtn.click();

    // Wait for async handler
    await new Promise(resolve => setTimeout(resolve, 10));

    const loadCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'load_lmstudio_instance');
    const readinessCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_provider_readiness');

    expect(loadCalls.length).toBe(1);
    expect(readinessCalls.length).toBe(1);
  });

  it('Retry action click invokes get_provider_readiness exactly once', async () => {
    // Configure readiness with Refresh/Retry action
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') return SAMPLE_DIAGNOSTICS;
      if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
      if (cmd === 'get_health_status') return SAMPLE_HEALTH_STATUS;
      if (cmd === 'get_provider_readiness') {
        return {
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
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadStatus();

    const retryBtn = document.querySelector('.readiness-action-btn[data-action="refresh"]');
    expect(retryBtn).toBeTruthy();

    mocks.invoke.mockClear();

    retryBtn.click();

    await new Promise(resolve => setTimeout(resolve, 10));

    const readinessCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_provider_readiness');
    expect(readinessCalls.length).toBe(1);
  });

  it('Configure Model action click invokes showSection(models) exactly once', async () => {
    // Configure readiness with configure-model action
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') return SAMPLE_DIAGNOSTICS;
      if (cmd === 'get_runtime_status') return SAMPLE_RUNTIME_STATUS;
      if (cmd === 'get_health_status') return SAMPLE_HEALTH_STATUS;
      if (cmd === 'get_provider_readiness') {
        return {
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
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadStatus();

    const configBtn = document.querySelector('.readiness-action-btn[data-action="configure-model"]');
    expect(configBtn).toBeTruthy();

    configBtn.click();

    await new Promise(resolve => setTimeout(resolve, 10));

    expect(mocks.showSection).toHaveBeenCalledWith('models');
    expect(mocks.showSection).toHaveBeenCalledTimes(1);
  });

  it('multiple passive loadStatus calls do not duplicate action handlers', async () => {
    await loadStatus();

    // Perform multiple passive loads (no _resetCaches)
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
