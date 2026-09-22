// Models behavioral tests using production models.js module
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';

// Use vi.hoisted to create mocks before vi.mock is hoisted
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  t: vi.fn((key, params) => {
    if (params) return `${key}:${JSON.stringify(params)}`;
    return key;
  }),
  formatSize: vi.fn((bytes) => `${bytes} B`),
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
  formatSize: mocks.formatSize,
}));

// Mock state module
vi.mock('./state.js', () => ({
  state: { currentSettings: null },
}));

// Mock local_ai module
vi.mock('./local_ai.js', () => ({
  refreshLocalAiStatus: mocks.refreshLocalAiStatus,
}));

// Import production module after mocking
import { refreshModels } from './models.js';

const SAMPLE_MODELS = [
  {
    id: 'llama3',
    display_name: 'Llama 3',
    size_bytes: 4000000000,
  },
  {
    id: 'mistral',
    display_name: 'Mistral',
    size_bytes: 3500000000,
  },
];

describe('Models module', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Add models-specific elements
    const settingsSection = document.getElementById('settings-section');
    const modelsContainer = document.createElement('div');
    modelsContainer.innerHTML = `
      <div id="provider-dot" style="background:#888"></div>
      <span id="provider-label">Checking...</span>
      <span id="provider-endpoint"></span>
      <div id="models-list"></div>
      <select id="f-quick-model-select">
        <option value="">Select model</option>
      </select>
    `;
    settingsSection.appendChild(modelsContainer);

    // Set initial provider values
    document.getElementById('f-quick-engine').value = 'ollama';
    document.getElementById('f-quick-endpoint').value = '127.0.0.1:11434';

    // Configure default responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'check_provider') {
        return true;
      }
      if (cmd === 'list_models') {
        return SAMPLE_MODELS;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('refreshes models when provider is reachable', async () => {
    await refreshModels('llama3');

    // Verify check_provider was called
    const checkCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'check_provider');
    expect(checkCalls.length).toBe(1);
    expect(checkCalls[0][1].provider).toBe('ollama');
    expect(checkCalls[0][1].endpoint).toBe('127.0.0.1:11434');

    // Verify list_models was called
    const listCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'list_models');
    expect(listCalls.length).toBe(1);
    expect(listCalls[0][1].provider).toBe('ollama');

    // Verify provider status is green
    const dot = document.getElementById('provider-dot');
    expect(dot.style.background).toBe('rgb(34, 170, 34)'); // #2a2 in RGB

    // Verify label shows running
    const label = document.getElementById('provider-label');
    expect(label.textContent).toContain('models.running');

    // Verify models list is populated
    const listEl = document.getElementById('models-list');
    expect(listEl.children.length).toBeGreaterThan(0);

    // Verify model select is populated
    const selectEl = document.getElementById('f-quick-model-select');
    expect(selectEl.options.length).toBeGreaterThan(1);
  });

  it('shows not reachable when provider is unavailable', async () => {
    // Configure provider unreachable
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'check_provider') {
        return false;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await refreshModels(null);

    // Verify check_provider was called
    const checkCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'check_provider');
    expect(checkCalls.length).toBe(1);

    // Verify list_models was NOT called
    const listCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'list_models');
    expect(listCalls.length).toBe(0);

    // Verify provider status is red
    const dot = document.getElementById('provider-dot');
    expect(dot.style.background).toBe('rgb(204, 0, 0)'); // #c00 in RGB

    // Verify label shows not reachable
    const label = document.getElementById('provider-label');
    expect(label.textContent).toContain('models.not.reachable');

    // Verify message to start provider
    const listEl = document.getElementById('models-list');
    expect(listEl.textContent).toContain('models.start.and.refresh');
  });

  it('shows empty state when no models installed', async () => {
    // Configure empty models list
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'check_provider') {
        return true;
      }
      if (cmd === 'list_models') {
        return [];
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await refreshModels(null);

    // Verify provider is reachable
    const dot = document.getElementById('provider-dot');
    expect(dot.style.background).toBe('rgb(34, 170, 34)');

    // Verify empty message shown
    const listEl = document.getElementById('models-list');
    expect(listEl.textContent).toContain('models.none.installed');

    // Verify select has only auto option
    const selectEl = document.getElementById('f-quick-model-select');
    expect(selectEl.options.length).toBe(1);
  });

  it('handles lmstudio provider correctly', async () => {
    // Switch to lmstudio
    document.getElementById('f-quick-engine').value = 'lmstudio';
    document.getElementById('f-quick-endpoint').value = '127.0.0.1:1234';

    await refreshModels(null);

    // Verify check_provider was called with lmstudio
    const checkCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'check_provider');
    expect(checkCalls[0][1].provider).toBe('lmstudio');
    expect(checkCalls[0][1].endpoint).toBe('127.0.0.1:1234');
  });

  it('marks selected model in list', async () => {
    await refreshModels('llama3');

    // Verify models list contains selected indicator
    const listEl = document.getElementById('models-list');
    const html = listEl.innerHTML;
    expect(html).toContain('models.selected');
  });
});
