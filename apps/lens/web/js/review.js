// Review page: review later items

import { invoke } from './tauri.js';

let reviewItems = [];
let currentReviewIndex = 0;

export async function loadReview() {
  const list = document.getElementById('review-list');
  const summary = document.getElementById('review-summary');
  try {
    const count = await invoke('library_later_count');
    summary.innerHTML = `<span style="font-size:24px;font-weight:600">${count}</span> <span style="color:#888;font-size:12px">items for later review</span>`;

    if (count === 0) {
      list.innerHTML = '<p style="color:#888;padding:20px 0">No items for later review. Save items to see them here.</p>';
      document.getElementById('review-card').style.display = 'none';
      return;
    }

    const page = await invoke('library_later_list', { offset: 0 });
    reviewItems = page.items;
    currentReviewIndex = 0;
    list.innerHTML = '';
    showReviewCard();
  } catch (e) {
    list.innerHTML = `<p class="error">Failed to load review list: ${e}</p>`;
  }
}

function showReviewCard() {
  const card = document.getElementById('review-card');
  if (currentReviewIndex >= reviewItems.length) {
    card.style.display = 'none';
    document.getElementById('review-list').innerHTML = '<p style="color:#2e7d32;padding:20px 0">All done! No more items to review.</p>';
    return;
  }
  const item = reviewItems[currentReviewIndex];
  card.style.display = 'block';
  document.getElementById('review-original').textContent = item.original;
  document.getElementById('review-answer').textContent = item.answer;
  document.getElementById('review-answer').style.display = 'none';
  document.getElementById('review-show-btn').style.display = '';
  document.getElementById('review-known-btn').style.display = 'none';
  document.getElementById('review-later-btn').style.display = 'none';
  document.getElementById('review-archive-btn').style.display = 'none';
  document.getElementById('review-list').innerHTML = `<p style="color:#888;font-size:12px">Item ${currentReviewIndex + 1} of ${reviewItems.length}</p>`;
}

export function showReviewAnswer() {
  document.getElementById('review-answer').style.display = 'block';
  document.getElementById('review-show-btn').style.display = 'none';
  document.getElementById('review-known-btn').style.display = '';
  document.getElementById('review-later-btn').style.display = '';
  document.getElementById('review-archive-btn').style.display = '';
}

export async function setReviewState(newState) {
  const item = reviewItems[currentReviewIndex];
  try {
    await invoke('library_set_review_state', { id: item.id, state_: newState });
    currentReviewIndex++;
    showReviewCard();
    const count = await invoke('library_later_count');
    document.getElementById('review-summary').innerHTML = `<span style="font-size:24px;font-weight:600">${count}</span> <span style="color:#888;font-size:12px">items for later review</span>`;
  } catch (e) {
    alert('Failed to update review state: ' + e);
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
}
