// Review page: review later items.
//
// Session semantics:
// - On entry, take a snapshot of all current later items.
// - Each item is shown at most once per session.
// - Known/Archive: remove from later set in DB.
// - Keep Later: item stays in DB as later, but is not re-presented this session.
// - Completion copy reflects actual DB truth (library_later_count), not in-memory state.

import { invoke } from './tauri.js';
import { escapeHtml } from './ui.js';

let sessionItems = [];
let seenActions = new Map();
let actionInFlight = false;
let reviewedCount = 0;
let sessionSize = 0;
let historyStack = []; // for undo: [{id, action}]

export async function loadReview() {
  const list = document.getElementById('review-list');
  const summary = document.getElementById('review-summary');
  const card = document.getElementById('review-card');
  try {
    const page = await invoke('library_review_session');
    sessionItems = page.items;
    seenActions = new Map();
    actionInFlight = false;
    reviewedCount = 0;
    sessionSize = sessionItems.length;
    historyStack = [];

    if (sessionItems.length === 0) {
      summary.innerHTML = '';
      list.innerHTML = '<p style="color:#888;padding:20px 0">No items for later review. Save items to see them here.</p>';
      card.style.display = 'none';
      return;
    }

    await updateSummaryFromDb();
    list.innerHTML = '';
    showNextReviewCard();
  } catch (e) {
    list.innerHTML = `<p class="error">Failed to load review list: ${escapeHtml(String(e))}</p>`;
  }
}

async function updateSummaryFromDb() {
  const count = await invoke('library_later_count');
  const summary = document.getElementById('review-summary');
  summary.innerHTML = `<span style="font-size:24px;font-weight:600">${escapeHtml(String(count))}</span> <span style="color:#888;font-size:12px">items for later review</span>`;
}

function findNextUnseenIndex() {
  for (let i = 0; i < sessionItems.length; i++) {
    if (!seenActions.has(sessionItems[i].id)) {
      return i;
    }
  }
  return -1;
}

function showNextReviewCard() {
  const card = document.getElementById('review-card');
  const list = document.getElementById('review-list');
  const idx = findNextUnseenIndex();

  if (idx === -1) {
    card.style.display = 'none';
    showSessionComplete();
    return;
  }

  const item = sessionItems[idx];
  card.style.display = 'block';
  document.getElementById('review-original').textContent = item.original;
  const noteEl = document.getElementById('review-note');
  if (item.note && item.note.trim()) {
    noteEl.textContent = item.note;
    noteEl.style.display = 'block';
  } else {
    noteEl.style.display = 'none';
  }
  document.getElementById('review-answer').textContent = item.answer;
  document.getElementById('review-answer').style.display = 'none';
  document.getElementById('review-show-btn').style.display = '';
  document.getElementById('review-known-btn').style.display = 'none';
  document.getElementById('review-later-btn').style.display = 'none';
  document.getElementById('review-archive-btn').style.display = 'none';
  document.getElementById('review-undo-btn').style.display = historyStack.length > 0 ? '' : 'none';
  setReviewButtonsEnabled(true);
  list.innerHTML = `<p style="color:#888;font-size:12px">Reviewed ${escapeHtml(String(reviewedCount))} of ${escapeHtml(String(sessionSize))} this round</p>`;
}

async function showSessionComplete() {
  const list = document.getElementById('review-list');
  const count = await invoke('library_later_count');
  let message;
  if (count === 0) {
    message = '<p style="color:#2e7d32;padding:20px 0">All reviewed. No items for later review.</p>';
  } else {
    message = `<p style="color:#2e7d32;padding:20px 0">Review round complete. ${escapeHtml(String(count))} items remain for later.</p>`;
  }

  // Show undo button if there's history
  if (historyStack.length > 0) {
    message += `<div style="padding:20px 0;text-align:center">
      <button id="review-undo-btn-completion" class="btn" style="padding:8px 16px;background:#1976d2;color:white;border-color:#1976d2">Undo last action</button>
    </div>`;
  }

  list.innerHTML = message;

  // Attach undo button handler
  const undoBtn = document.getElementById('review-undo-btn-completion');
  if (undoBtn) {
    undoBtn.addEventListener('click', undoLastAction);
  }
}

function setReviewButtonsEnabled(enabled) {
  document.getElementById('review-show-btn').disabled = !enabled;
  document.getElementById('review-known-btn').disabled = !enabled;
  document.getElementById('review-later-btn').disabled = !enabled;
  document.getElementById('review-archive-btn').disabled = !enabled;
}

export function showReviewAnswer() {
  document.getElementById('review-answer').style.display = 'block';
  document.getElementById('review-show-btn').style.display = 'none';
  document.getElementById('review-known-btn').style.display = '';
  document.getElementById('review-later-btn').style.display = '';
  document.getElementById('review-archive-btn').style.display = '';
}

export async function setReviewState(newState) {
  if (actionInFlight) return;
  actionInFlight = true;
  setReviewButtonsEnabled(false);

  const idx = findNextUnseenIndex();
  if (idx === -1) {
    actionInFlight = false;
    setReviewButtonsEnabled(true);
    return;
  }

  const item = sessionItems[idx];
  try {
    if (newState !== 'later') {
      await invoke('library_set_review_state', { id: item.id, state_: newState });
    }
    seenActions.set(item.id, newState);
    historyStack.push({ id: item.id, action: newState });
    reviewedCount++;
    await updateSummaryFromDb();
    showNextReviewCard();
  } catch (e) {
    alert('Failed to update review state: ' + e);
    setReviewButtonsEnabled(true);
  } finally {
    actionInFlight = false;
  }
}

export function initReviewHandlers() {
  const showBtn = document.getElementById('review-show-btn');
  if (showBtn) showBtn.addEventListener('click', showReviewAnswer);

  const knownBtn = document.getElementById('review-known-btn');
  if (knownBtn) knownBtn.addEventListener('click', () => setReviewState('known'));

  const laterBtn = document.getElementById('review-later-btn');
  if (laterBtn) laterBtn.addEventListener('click', () => setReviewState('later'));

  const archiveBtn = document.getElementById('review-archive-btn');
  if (archiveBtn) archiveBtn.addEventListener('click', () => setReviewState('archive'));

  const undoBtn = document.getElementById('review-undo-btn');
  if (undoBtn) undoBtn.addEventListener('click', undoLastAction);

  // Keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    // Ignore if typing in input/textarea
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
    if (e.target.isContentEditable) return;

    const card = document.getElementById('review-card');
    const cardVisible = card && card.style.display !== 'none';

    // Ctrl+Z works regardless of card visibility
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
      e.preventDefault();
      undoLastAction();
      return;
    }

    // Other shortcuts only work when card is visible
    if (!cardVisible) return;

    const showBtn = document.getElementById('review-show-btn');
    const answerVisible = document.getElementById('review-answer').style.display === 'block';

    if ((e.key === ' ' || e.key === 'Enter') && !answerVisible && showBtn.style.display !== 'none') {
      e.preventDefault();
      showReviewAnswer();
    } else if (e.key === 'k' || e.key === 'K') {
      if (answerVisible && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setReviewState('known');
      }
    } else if (e.key === 'l' || e.key === 'L') {
      if (answerVisible && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setReviewState('later');
      }
    } else if (e.key === 'a' || e.key === 'A') {
      if (answerVisible && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setReviewState('archive');
      }
    }
  });
}

export async function undoLastAction() {
  if (actionInFlight || historyStack.length === 0) return;

  const last = historyStack.pop();
  if (!last) return;

  actionInFlight = true;
  setReviewButtonsEnabled(false);

  try {
    // Revert DB state if action was Known or Archive
    if (last.action !== 'later') {
      await invoke('library_set_review_state', { id: last.id, state_: 'later' });
    }
    // Remove from seen map so it shows again
    seenActions.delete(last.id);
    reviewedCount--;
    await updateSummaryFromDb();
    showNextReviewCard();
  } catch (e) {
    alert('Failed to undo: ' + e);
    // Re-add to history on failure
    historyStack.push(last);
    setReviewButtonsEnabled(true);
  } finally {
    actionInFlight = false;
  }
}
