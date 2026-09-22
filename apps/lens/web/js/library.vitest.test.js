// Library behavioral tests using production library.js module
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
  populateSelect: vi.fn(),
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
  populateSelect: mocks.populateSelect,
}));

// Import production module after mocking
import { loadLibrary, searchLibrary, applyFilters, clearFilters } from './library.js';

const SAMPLE_LIBRARY_PAGE = {
  total: 3,
  items: [
    {
      id: 1,
      kind: 'translation',
      source_text: 'Hello',
      target_text: '안녕하세요',
      source_language: 'en',
      target_language: 'ko',
      app: 'Browser',
      created_at: 1234567890,
      review_state: 'later',
    },
    {
      id: 2,
      kind: 'analysis',
      source_text: 'World',
      target_text: '세계',
      source_language: 'en',
      target_language: 'ko',
      app: 'Editor',
      created_at: 1234567891,
      review_state: 'known',
    },
    {
      id: 3,
      kind: 'translation',
      source_text: 'Test',
      target_text: '테스트',
      source_language: 'en',
      target_language: 'ko',
      app: 'Browser',
      created_at: 1234567892,
      review_state: 'later',
    },
  ],
};

describe('Library module', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Add library-specific elements
    const librarySection = document.getElementById('library-section');
    librarySection.innerHTML = `
      <input id="lib-search" type="text" placeholder="Search...">
      <select id="lib-filter-kind">
        <option value="">All kinds</option>
        <option value="translation">Translation</option>
        <option value="analysis">Analysis</option>
      </select>
      <select id="lib-filter-app">
        <option value="">All apps</option>
        <option value="Browser">Browser</option>
        <option value="Editor">Editor</option>
      </select>
      <select id="lib-filter-source">
        <option value="">All sources</option>
        <option value="en">English</option>
        <option value="ko">한국어</option>
      </select>
      <select id="lib-filter-target">
        <option value="">All targets</option>
        <option value="ko">한국어</option>
        <option value="en">English</option>
      </select>
      <select id="lib-filter-state">
        <option value="">All states</option>
        <option value="known">Known</option>
        <option value="later">Later</option>
      </select>
      <select id="lib-filter-tag">
        <option value="">All tags</option>
        <option value="important">Important</option>
        <option value="review">Review</option>
      </select>
      <div id="lib-count"></div>
      <div id="lib-list"></div>
    `;

    // Configure default responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_distinct_apps') {
        return ['Browser', 'Editor'];
      }
      if (cmd === 'library_distinct_source_languages') {
        return ['en', 'ko'];
      }
      if (cmd === 'library_distinct_target_languages') {
        return ['ko', 'en'];
      }
      if (cmd === 'library_distinct_tags') {
        return ['important', 'review'];
      }
      if (cmd === 'library_query') {
        return SAMPLE_LIBRARY_PAGE;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('loads library with filter options and initial page', async () => {
    await loadLibrary();

    // Verify filter options were loaded
    const distinctAppsCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_distinct_apps');
    expect(distinctAppsCalls.length).toBe(1);

    const distinctSourcesCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_distinct_source_languages');
    expect(distinctSourcesCalls.length).toBe(1);

    const distinctTargetsCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_distinct_target_languages');
    expect(distinctTargetsCalls.length).toBe(1);

    const distinctTagsCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_distinct_tags');
    expect(distinctTagsCalls.length).toBe(1);

    // Verify library_query was called
    const queryCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_query');
    expect(queryCalls.length).toBe(1);
    expect(queryCalls[0][1].offset).toBe(0);

    // Verify populateSelect was called for each filter
    expect(mocks.populateSelect).toHaveBeenCalled();

    // Verify count displayed
    const countEl = document.getElementById('lib-count');
    expect(countEl.textContent).toContain('3');
  });

  it('searches library with query', async () => {
    await loadLibrary();

    // Enter search query
    const searchInput = document.getElementById('lib-search');
    searchInput.value = 'test query';

    // Perform search
    searchLibrary();

    // Wait for async fetch
    await new Promise(resolve => setTimeout(resolve, 0));

    // Verify library_query was called with search term
    const queryCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_query');
    expect(queryCalls.length).toBe(2); // Initial load + search
    expect(queryCalls[1][1].query).toBe('test query');
    expect(queryCalls[1][1].offset).toBe(0);
  });

  it('applies filters correctly', async () => {
    await loadLibrary();

    // Set filter values
    document.getElementById('lib-filter-kind').value = 'translation';
    document.getElementById('lib-filter-app').value = 'Browser';
    document.getElementById('lib-filter-source').value = 'en';
    document.getElementById('lib-filter-target').value = 'ko';
    document.getElementById('lib-filter-state').value = 'later';
    document.getElementById('lib-filter-tag').value = 'important';

    // Apply filters
    applyFilters();

    // Wait for async fetch
    await new Promise(resolve => setTimeout(resolve, 0));

    // Verify library_query was called with filters
    const queryCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_query');
    expect(queryCalls.length).toBe(2); // Initial load + filter apply
    const lastCall = queryCalls[1][1];
    expect(lastCall.kind).toBe('translation');
    expect(lastCall.app).toBe('Browser');
    expect(lastCall.source_language).toBe('en');
    expect(lastCall.target_language).toBe('ko');
    expect(lastCall.review_state).toBe('later');
    expect(lastCall.tag).toBe('important');
  });

  it('clears filters correctly', async () => {
    await loadLibrary();

    // Set some filters
    document.getElementById('lib-filter-kind').value = 'translation';
    document.getElementById('lib-filter-app').value = 'Browser';
    applyFilters();

    // Clear filters
    clearFilters();

    // Wait for async fetch
    await new Promise(resolve => setTimeout(resolve, 0));

    // Verify library_query was called with null filters
    const queryCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_query');
    expect(queryCalls.length).toBe(3); // Initial load + filter apply + clear
    const lastCall = queryCalls[2][1];
    expect(lastCall.kind).toBe(null);
    expect(lastCall.app).toBe(null);
    expect(lastCall.offset).toBe(0);

    // Verify filter selects were cleared
    expect(document.getElementById('lib-filter-kind').value).toBe('');
    expect(document.getElementById('lib-filter-app').value).toBe('');
  });

  it('handles empty library state', async () => {
    // Configure empty response
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_distinct_apps' ||
          cmd === 'library_distinct_source_languages' ||
          cmd === 'library_distinct_target_languages' ||
          cmd === 'library_distinct_tags') {
        return [];
      }
      if (cmd === 'library_query') {
        return { total: 0, items: [] };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadLibrary();

    // Verify empty state message shown
    const listEl = document.getElementById('lib-list');
    expect(listEl.innerHTML).toContain('library.empty.default');

    // Verify count shows 0
    const countEl = document.getElementById('lib-count');
    expect(countEl.textContent).toContain('0');
  });

  it('handles backend error gracefully', async () => {
    // Configure error response
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_query') {
        throw new Error('Backend error');
      }
      if (cmd.startsWith('library_distinct_')) {
        return [];
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    // Should not throw
    await expect(loadLibrary()).resolves.not.toThrow();
  });
});
