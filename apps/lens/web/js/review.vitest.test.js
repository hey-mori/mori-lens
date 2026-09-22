// Review behavioral tests using production review.js module
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
import { loadReview } from './review.js';

const SAMPLE_REVIEW_SESSION = {
  items: [
    {
      id: 1,
      original: 'Hello world',
      answer: '안녕하세요 세계',
      note: 'Common greeting',
    },
    {
      id: 2,
      original: 'Good morning',
      answer: '좋은 아침',
      note: '',
    },
  ],
};

describe('Review module', () => {
  let fixture;

  beforeEach(() => {
    vi.clearAllMocks();
    fixture = createControlCenterFixture();

    // Add review-specific elements
    const reviewSection = document.getElementById('review-section');
    reviewSection.innerHTML = `
      <div id="review-summary"></div>
      <div id="review-list"></div>
      <div id="review-card" style="display:none">
        <div id="review-original"></div>
        <div id="review-note" style="display:none"></div>
        <div id="review-answer"></div>
        <button id="review-show-btn">Show Answer</button>
        <button id="review-known-btn" style="display:none">Known</button>
        <button id="review-later-btn" style="display:none">Later</button>
        <button id="review-archive-btn" style="display:none">Archive</button>
        <button id="review-undo-btn" style="display:none">Undo</button>
      </div>
    `;

    // Configure default responses
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_review_session') {
        return SAMPLE_REVIEW_SESSION;
      }
      if (cmd === 'library_later_count') {
        return 2;
      }
      if (cmd === 'library_set_review_state') {
        return { success: true };
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });
  });

  afterEach(() => {
    cleanupFixture(fixture);
    vi.restoreAllMocks();
  });

  it('loads review session with items', async () => {
    await loadReview();

    // Verify library_review_session was called
    const sessionCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_review_session');
    expect(sessionCalls.length).toBe(1);

    // Verify library_later_count was called for summary
    const countCalls = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'library_later_count');
    expect(countCalls.length).toBeGreaterThan(0);

    // Verify review card is visible
    const card = document.getElementById('review-card');
    expect(card.style.display).toBe('block');

    // Verify first item is shown
    const original = document.getElementById('review-original');
    expect(original.textContent).toBe('Hello world');

    // Verify note is shown when present
    const note = document.getElementById('review-note');
    expect(note.style.display).toBe('block');
    expect(note.textContent).toBe('Common greeting');
  });

  it('handles empty review queue', async () => {
    // Configure empty session
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_review_session') {
        return { items: [] };
      }
      if (cmd === 'library_later_count') {
        return 0;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadReview();

    // Verify empty message shown
    const list = document.getElementById('review-list');
    expect(list.innerHTML).toContain('review.empty');

    // Verify card is hidden
    const card = document.getElementById('review-card');
    expect(card.style.display).toBe('none');
  });

  it('handles backend error gracefully', async () => {
    // Configure error response
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_review_session') {
        throw new Error('Backend error');
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    // Should not throw
    await expect(loadReview()).resolves.not.toThrow();

    // Verify error message shown
    const list = document.getElementById('review-list');
    expect(list.innerHTML).toContain('review.load.error');
  });

  it('hides note when not present', async () => {
    // Configure session with item without note
    mocks.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'library_review_session') {
        return {
          items: [
            {
              id: 1,
              original: 'Test',
              answer: '테스트',
              note: '',
            },
          ],
        };
      }
      if (cmd === 'library_later_count') {
        return 1;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    });

    await loadReview();

    // Verify note is hidden
    const note = document.getElementById('review-note');
    expect(note.style.display).toBe('none');
  });
});
