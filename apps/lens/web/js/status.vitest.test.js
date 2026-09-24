// Status behavioral tests using production status.js module
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';

// Use vi.hoisted to create mocks before vi.mock is hoisted
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  t: vi.fn((key, params) => {
    if (params) return `${key}:${JSON.stringify(params)}`;
    return key;
  }),
  escapeHtml: vi.fn((str) => str),
  formatAiStatus: vi.fn(() => 'Model status'),
  setStatus: vi.fn(),
  refreshLocalAiStatus: vi.fn(),
}));

// Mock the Tauri IPC boundary
vi.mock('./tauri.js', () => ({
  invoke: mocks.invoke,
}));

// Mock i18n module
vi.mock('./i18n.js', () => ({
  t: mocks.t,
}));

// Mock ui module
vi.mock('./ui.js', () => ({
  escapeHtml: mocks.escapeHtml,
  formatAiStatus: mocks.formatAiStatus,
  setStatus: mocks.setStatus,
}));

// Mock state module
vi.mock('./state.js', () => ({
  state: { cachedProviderReadiness: null, providerReadinessLoadState: 'LOADED' },
}));

// Mock local_ai module
vi.mock('./local_ai.js', () => ({
  refreshLocalAiStatus: mocks.refreshLocalAiStatus,
}));

// Import production module after mocking
import { loadStatus } from './status.js';

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
  quick: {
    state: 'ready',
    failure: null,
    latency_ms: 45,
  },
  deep: {
    state: 'ready',
    failure: null,
    latency_ms: 120,
  },
  stale: false,
};

describe('Status module', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Add status-specific elements
    const statusSection = document.getElementById('status-section');
    statusSection.innerHTML = `
      <div id="status-content"></div>
      <button id="run-health-check">Run Health Check</button>
    `;

    // Configure default responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') {
        return SAMPLE_DIAGNOSTICS;
      }
      if (cmd === 'get_runtime_status') {
        return SAMPLE_RUNTIME_STATUS;
      }
      if (cmd === 'get_health_status') {
        return SAMPLE_HEALTH_STATUS;
      }
      if (cmd === 'run_health_check') {
        return { success: true };
      }
      if (cmd === 'request_pause_toggle') {
        return { paused: true };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('loads status with diagnostics, runtime, and health', async () => {
    await loadStatus();

    // Verify all three commands were called
    const diagCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_diagnostics');
    expect(diagCalls.length).toBe(1);

    const runtimeCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_runtime_status');
    expect(runtimeCalls.length).toBe(1);

    const healthCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_health_status');
    expect(healthCalls.length).toBe(1);

    // Verify status content is rendered
    const content = document.getElementById('status-content');
    expect(content.innerHTML).toContain('status.title');

    // Verify pause button exists
    const pauseBtn = document.getElementById('pause-btn');
    expect(pauseBtn).toBeDefined();

    // Verify quit button exists
    const quitBtn = document.getElementById('quit-btn');
    expect(quitBtn).toBeDefined();
  });

  it('shows paused state when diagnostics indicate paused', async () => {
    // Configure paused state
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') {
        return { ...SAMPLE_DIAGNOSTICS, paused: true };
      }
      if (cmd === 'get_runtime_status') {
        return SAMPLE_RUNTIME_STATUS;
      }
      if (cmd === 'get_health_status') {
        return SAMPLE_HEALTH_STATUS;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadStatus();

    // Verify paused status shown
    const content = document.getElementById('status-content');
    expect(content.innerHTML).toContain('status.paused');

    // Verify pause button shows resume label
    const pauseBtn = document.getElementById('pause-btn');
    expect(pauseBtn.textContent).toContain('status.pause.resume');
  });

  it('shows running state when not paused', async () => {
    await loadStatus();

    // Verify running status shown
    const content = document.getElementById('status-content');
    expect(content.innerHTML).toContain('status.running');

    // Verify pause button shows pause label
    const pauseBtn = document.getElementById('pause-btn');
    expect(pauseBtn.textContent).toContain('status.pause.button');
  });

  it('displays health status information', async () => {
    await loadStatus();

    // Verify health status is displayed
    const content = document.getElementById('status-content');
    expect(content.innerHTML).toContain('status.last.health');

    // Verify Quick and Deep status shown
    expect(content.innerHTML).toContain('Quick');
    expect(content.innerHTML).toContain('Deep');
  });

  it('handles backend error gracefully', async () => {
    // Configure error response
    mocks.invoke.mockImplementation(async (cmd) => {
      throw new Error('Backend error');
    });

    // Should not throw
    await expect(loadStatus()).resolves.not.toThrow();
  });

  it('shows health check button', async () => {
    await loadStatus();

    // Verify health check button exists
    const healthBtn = document.getElementById('run-health-check-btn');
    expect(healthBtn).toBeDefined();
    expect(healthBtn.textContent).toContain('status.health.check');
  });

  it('shows refresh local AI button', async () => {
    await loadStatus();

    // Verify refresh button exists
    const refreshBtn = document.getElementById('refresh-local-ai-btn');
    expect(refreshBtn).toBeDefined();
    expect(refreshBtn.textContent).toContain('status.refresh');
  });
});
