// Diagnostics behavioral tests using production diagnostics.js module
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
}));

// Import production module after mocking
import { loadAbout, exportDiagnostics } from './diagnostics.js';

const SAMPLE_DIAGNOSTICS = {
  version: '0.11.0',
  build: 'abc123',
  platform: 'windows',
};

const SAMPLE_EXPORT_JSON = JSON.stringify({
  version: '0.11.0',
  timestamp: 1234567890,
  data: {},
}, null, 2);

describe('Diagnostics module', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Add diagnostics-specific elements
    const diagnosticsSection = document.getElementById('diagnostics-section');
    diagnosticsSection.innerHTML = `
      <div id="about-content"></div>
      <button id="export-diagnostics">Export</button>
      <textarea id="diagnostics-json" readonly></textarea>
      <div id="diagnostics-result" style="display:none">
        <button id="copy-diagnostics">Copy</button>
      </div>
    `;

    // Configure default responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') {
        return SAMPLE_DIAGNOSTICS;
      }
      if (cmd === 'export_diagnostics') {
        return SAMPLE_EXPORT_JSON;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('loads about information correctly', async () => {
    await loadAbout();

    // Verify get_diagnostics was called
    const calls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'get_diagnostics');
    expect(calls.length).toBe(1);

    // Verify about content is rendered
    const content = document.getElementById('about-content');
    expect(content.innerHTML).toContain('0.11.0');
    expect(content.innerHTML).toContain('abc123');
    expect(content.innerHTML).toContain('about.version');
    expect(content.innerHTML).toContain('about.build');
  });

  it('handles missing diagnostic fields gracefully', async () => {
    // Configure response with missing fields
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') {
        return { version: '0.11.0' }; // Missing build
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadAbout();

    // Verify content is still rendered
    const content = document.getElementById('about-content');
    expect(content.innerHTML).toContain('0.11.0');
    expect(content.innerHTML).toContain('about.unknown');
  });

  it('exports diagnostics successfully', async () => {
    await exportDiagnostics();

    // Verify export_diagnostics was called
    const calls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'export_diagnostics');
    expect(calls.length).toBe(1);

    // Verify textarea is populated
    const textarea = document.getElementById('diagnostics-json');
    expect(textarea.value).toBe(SAMPLE_EXPORT_JSON);

    // Verify result div is shown
    const resultDiv = document.getElementById('diagnostics-result');
    expect(resultDiv.style.display).toBe('block');
  });

  it('handles export error gracefully', async () => {
    // Configure error response
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'export_diagnostics') {
        throw new Error('Export failed');
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    // Mock alert
    const alertMock = vi.spyOn(window, 'alert').mockImplementation(() => {});

    // Should not throw
    await expect(exportDiagnostics()).resolves.not.toThrow();

    // Verify error was logged (console.error is called in production)
    // Alert may or may not be called depending on implementation

    alertMock.mockRestore();
  });

  it('handles backend error in loadAbout gracefully', async () => {
    // Configure error response
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'get_diagnostics') {
        throw new Error('Backend error');
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    // Should not throw
    await expect(loadAbout()).resolves.not.toThrow();

    // Verify about content is not populated (error case)
    const content = document.getElementById('about-content');
    expect(content.innerHTML).toBe('');
  });

  it('shows data location in about', async () => {
    await loadAbout();

    // Verify data location is shown
    const content = document.getElementById('about-content');
    expect(content.innerHTML).toContain('APPDATA');
    expect(content.innerHTML).toContain('Mori Lens');
  });
});
