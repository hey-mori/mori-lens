// Settings behavioral tests using production settings.js module
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createControlCenterFixture, cleanupFixture } from './test-infrastructure.js';

// Use vi.hoisted to create mocks before vi.mock is hoisted
const { mockInvoke, mockState } = vi.hoisted(() => {
  return {
    mockInvoke: vi.fn(),
    mockState: { currentSettings: null },
  };
});

// Mock the Tauri IPC boundary - must be done before importing production modules
vi.mock('./tauri.js', () => ({
  invoke: mockInvoke,
  listen: vi.fn(),
}));

// Mock state module with mutable state
vi.mock('./state.js', () => ({
  state: mockState,
}));

// Mock ui module
vi.mock('./ui.js', () => ({
  setStatus: vi.fn(),
}));

// Mock local_ai module
vi.mock('./local_ai.js', () => ({
  refreshLocalAiStatus: vi.fn(),
}));

// Mock models module
vi.mock('./models.js', () => ({
  refreshModels: vi.fn(),
}));

// Mock apps module
vi.mock('./apps.js', () => ({
  renderApps: vi.fn(),
}));

// Mock i18n module
vi.mock('./i18n.js', () => ({
  getLocale: vi.fn(() => 'en'),
  setLocale: vi.fn(),
  t: vi.fn((key) => key),
}));

// Import production module after mocking
import { loadSettings, saveSettings, applySettingsToForm } from './settings.js';

describe('Settings module', () => {
  let fixture;

  beforeEach(() => {
    mockInvoke.mockClear();
    mockState.currentSettings = null;
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Configure default settings response
    mockInvoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'get_settings') {
        return {
          language: 'ko',
          dwell_ms: 320,
          theme: 'white',
          text_size: 'medium',
          translucency: '0.9',
          ocr: 'auto',
          profile: 'default',
          hover_scope: 'word',
          default_tier: 'translation',
          history: false,
          history_days: 0,
          history_store_app: false,
          history_excluded: [],
          quick_engine: 'ollama',
          quick_model: 'llama3',
          quick_endpoint: '127.0.0.1:11434',
          deep_engine: 'ollama',
          deep_model: 'llama3',
          deep_endpoint: '127.0.0.1:11434',
          shortcuts: {
            pause: 'Ctrl+Alt+L',
            copy: 'Ctrl+Alt+C',
            save: 'Ctrl+Alt+S',
          },
        };
      }
      if (cmd === 'save_settings') {
        return { success: true };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('loads settings through production loadSettings function', async () => {
    await loadSettings();

    // Verify get_settings was invoked
    const calls = mockInvoke.mock.calls.filter(([cmd]) => cmd === 'get_settings');
    expect(calls.length).toBe(1);

    // Verify form fields were populated
    expect(document.getElementById('f-language').value).toBe('ko');
    expect(document.getElementById('f-dwell').value).toBe('320');
    expect(document.getElementById('f-theme').value).toBe('white');
    expect(document.getElementById('f-quick-engine').value).toBe('ollama');
  });

  it('saves settings through production saveSettings function', async () => {
    // Load settings first
    await loadSettings();

    // Modify a form field
    document.getElementById('f-dwell').value = '500';

    // Save settings
    await saveSettings();

    // Verify save_settings was invoked with correct payload
    const calls = mockInvoke.mock.calls.filter(([cmd]) => cmd === 'save_settings');
    expect(calls.length).toBe(1);
    expect(calls[0][1].dwell_ms).toBe(500);
  });

  it('handles save_settings rejection', async () => {
    await loadSettings();

    // Configure save to fail
    mockInvoke.mockImplementation(async (cmd) => {
      if (cmd === 'save_settings') {
        throw new Error('Save failed');
      }
      if (cmd === 'get_settings') {
        return {
          language: 'ko',
          dwell_ms: 320,
          theme: 'white',
          text_size: 'medium',
          translucency: '0.9',
          ocr: 'auto',
          profile: 'default',
          hover_scope: 'word',
          default_tier: 'translation',
          history: false,
          history_days: 0,
          history_store_app: false,
          history_excluded: [],
          quick_engine: 'ollama',
          quick_model: 'llama3',
          quick_endpoint: '127.0.0.1:11434',
          deep_engine: 'ollama',
          deep_model: 'llama3',
          deep_endpoint: '127.0.0.1:11434',
          shortcuts: {
            pause: 'Ctrl+Alt+L',
            copy: 'Ctrl+Alt+C',
            save: 'Ctrl+Alt+S',
          },
        };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    // Attempt save - should not throw, but should handle error
    await expect(saveSettings()).resolves.not.toThrow();

    // Verify error handling occurred
    const calls = mockInvoke.mock.calls.filter(([cmd]) => cmd === 'save_settings');
    expect(calls.length).toBe(1);
  });

  it('applies settings to form correctly', () => {
    const settings = {
      language: 'en',
      dwell_ms: 400,
      theme: 'black',
      text_size: 'large',
      translucency: '0.8',
      ocr: 'manual',
      profile: 'reader',
      hover_scope: 'sentence',
      default_tier: 'analysis',
      history: true,
      history_days: 30,
      history_store_app: true,
      history_excluded: ['app1', 'app2'],
      quick_engine: 'lmstudio',
      quick_model: 'mistral',
      quick_endpoint: '127.0.0.1:1234',
      deep_engine: 'lmstudio',
      deep_model: 'mistral',
      deep_endpoint: '127.0.0.1:1234',
    };

    applySettingsToForm(settings);

    expect(document.getElementById('f-language').value).toBe('en');
    expect(document.getElementById('f-dwell').value).toBe('400');
    expect(document.getElementById('f-theme').value).toBe('black');
    expect(document.getElementById('f-quick-engine').value).toBe('lmstudio');
    expect(document.getElementById('f-history').checked).toBe(true);
    expect(document.getElementById('f-history-days').value).toBe('30');
  });
});
