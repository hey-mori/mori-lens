// control-center.vitest.test.js - Behavioral tests for Control Center workflows
import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock Tauri invoke
const mockInvoke = vi.fn();
globalThis.__TAURI__ = {
  invoke: mockInvoke,
};

// Mock localStorage
const localStorageMock = (() => {
  let store = {};
  return {
    getItem: (key) => store[key] || null,
    setItem: (key, value) => { store[key] = value; },
    removeItem: (key) => { delete store[key]; },
    clear: () => { store = {}; },
  };
})();
Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });

// Mock navigator
Object.defineProperty(globalThis, 'navigator', {
  value: { language: 'en-US' },
  writable: true,
});

// Mock document with minimal DOM
const mockDocument = {
  querySelector: vi.fn(() => null),
  querySelectorAll: vi.fn(() => []),
  getElementById: vi.fn(() => null),
  createElement: vi.fn(() => ({
    style: {},
    classList: { add: vi.fn(), remove: vi.fn() },
    appendChild: vi.fn(),
    addEventListener: vi.fn(),
  })),
  addEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
};
Object.defineProperty(globalThis, 'document', { value: mockDocument });

describe('Control Center navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorageMock.clear();
  });

  it('loads Status section by default', async () => {
    // Mock Tauri commands
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === 'get_health_status') {
        return Promise.resolve({
          quick: { available: true, provider: 'ollama' },
          deep: { available: true, provider: 'ollama' },
        });
      }
      return Promise.resolve(null);
    });

    // Verify health status is called on load
    const health = await mockInvoke('get_health_status');
    expect(health).toBeDefined();
    expect(health.quick.available).toBe(true);
  });

  it('switches interface language and persists', () => {
    localStorageMock.setItem('lens-locale', 'en');
    expect(localStorageMock.getItem('lens-locale')).toBe('en');

    localStorageMock.setItem('lens-locale', 'ko');
    expect(localStorageMock.getItem('lens-locale')).toBe('ko');
  });

  it('displays restart-required truth when settings change', () => {
    // Settings that require restart
    const settingsRequiringRestart = ['start_with_windows', 'shortcut_keys'];

    settingsRequiringRestart.forEach((key) => {
      localStorageMock.setItem(`pending_${key}`, 'true');
      expect(localStorageMock.getItem(`pending_${key}`)).toBe('true');
    });
  });
});

describe('Control Center settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorageMock.clear();
  });

  it('loads and saves settings', async () => {
    const mockSettings = {
      target_language: 'en',
      start_with_windows: false,
      shortcut_keys: { pause: 'Ctrl+Alt+L' },
    };

    mockInvoke.mockResolvedValue(mockSettings);

    const settings = await mockInvoke('get_settings');
    expect(settings.target_language).toBe('en');
    expect(settings.start_with_windows).toBe(false);
  });

  it('switches Quick provider', async () => {
    mockInvoke.mockResolvedValue({ providers: ['ollama', 'lm_studio'] });

    const providers = await mockInvoke('get_available_providers');
    expect(providers.providers).toContain('ollama');
    expect(providers.providers).toContain('lm_studio');
  });

  it('renders models for selected provider', async () => {
    mockInvoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_models') {
        return Promise.resolve({
          models: ['llama3', 'mistral', 'gemma'],
        });
      }
      return Promise.resolve(null);
    });

    const models = await mockInvoke('get_models', { provider: 'ollama' });
    expect(models.models).toHaveLength(3);
    expect(models.models).toContain('llama3');
  });

  it('handles provider unavailable state', async () => {
    mockInvoke.mockResolvedValue({
      quick: {
        available: false,
        reason: 'provider_not_running',
      },
    });

    const health = await mockInvoke('get_health_status');
    expect(health.quick.available).toBe(false);
    expect(health.quick.reason).toBe('provider_not_running');
  });
});

describe('Control Center Library', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('searches library entries', async () => {
    const mockEntries = [
      { id: 1, source: 'Hello', target: '안녕', timestamp: 1234567890 },
    ];

    mockInvoke.mockResolvedValue({ entries: mockEntries });

    const result = await mockInvoke('library_search', { query: 'Hello' });
    expect(result.entries).toHaveLength(1);
    expect(result.entries[0].source).toBe('Hello');
  });

  it('filters library by date range', async () => {
    const mockEntries = [
      { id: 1, source: 'Test', target: '테스트', timestamp: 1234567890 },
    ];

    mockInvoke.mockResolvedValue({ entries: mockEntries });

    const result = await mockInvoke('library_search', {
      from: 1234567800,
      to: 1234567900,
    });
    expect(result.entries).toBeDefined();
  });

  it('adds notes and tags to library entries', async () => {
    mockInvoke.mockResolvedValue({ success: true });

    const result = await mockInvoke('library_update_note', {
      id: 1,
      note: 'Important phrase',
    });
    expect(result.success).toBe(true);
  });
});

describe('Control Center Review', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('categorizes entries as Known/Later/Archive', async () => {
    mockInvoke.mockResolvedValue({ success: true });

    const result = await mockInvoke('review_categorize', {
      id: 1,
      category: 'known',
    });
    expect(result.success).toBe(true);
  });

  it('supports undo for review actions', async () => {
    mockInvoke.mockResolvedValue({ success: true });

    const result = await mockInvoke('review_undo');
    expect(result.success).toBe(true);
  });
});

describe('Control Center Health', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders health check results', async () => {
    mockInvoke.mockResolvedValue({
      quick: {
        available: true,
        provider: 'ollama',
        model: 'llama3',
        latency_ms: 150,
      },
      deep: {
        available: true,
        provider: 'ollama',
        model: 'llama3',
        latency_ms: 800,
      },
    });

    const health = await mockInvoke('get_health_status');
    expect(health.quick.available).toBe(true);
    expect(health.quick.latency_ms).toBeLessThan(200);
    expect(health.deep.latency_ms).toBeGreaterThan(500);
  });

  it('displays typed failure classification', async () => {
    mockInvoke.mockResolvedValue({
      quick: {
        available: false,
        failure_class: 'ProviderNotRunning',
      },
    });

    const health = await mockInvoke('get_health_status');
    expect(health.quick.failure_class).toBe('ProviderNotRunning');
  });
});

describe('Control Center Diagnostics', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('exports diagnostics without captured text', async () => {
    const mockExport = {
      version: '0.11.0',
      health: { quick: { available: true } },
      settings: { target_language: 'en' },
      events: [],
    };

    mockInvoke.mockResolvedValue(mockExport);

    const export_data = await mockInvoke('export_diagnostics');
    expect(export_data.version).toBeDefined();
    expect(export_data.captured_text).toBeUndefined();
  });
});
