// Recent page: session-only history

import { invoke } from './tauri.js';
import { escapeHtml } from './ui.js';
import { t } from './i18n.js';

export async function loadRecent() {
  const el = document.getElementById('recent-list');
  try {
    const cards = await invoke('recent_list');
    if (!cards || cards.length === 0) {
      el.innerHTML = '<p style="color:#888;font-size:13px">' + escapeHtml(t('recent.empty.state')) + '</p>';
      return;
    }
    let html = '<table style="max-width:100%">';
    for (const c of cards) {
      const time = new Date(c.shown_at * 1000).toLocaleTimeString();
      const preview = c.original.length > 60 ? c.original.slice(0, 60) + '...' : c.original;
      const answer = c.answer.length > 60 ? c.answer.slice(0, 60) + '...' : c.answer;
      html += '<tr>';
      html += '<td style="width:50%;color:#333">' + escapeHtml(preview) + '</td>';
      html += '<td style="width:35%;color:#666">' + escapeHtml(answer) + '</td>';
      html += '<td style="width:8%;color:#888;font-size:12px">' + escapeHtml(c.app || '') + '</td>';
      html += '<td style="width:7%;color:#888;font-size:12px;text-align:right">' + time + '</td>';
      html += '</tr>';
    }
    html += '</table>';
    el.innerHTML = html;
  } catch (e) {
    el.innerHTML = '<p class="error">' + escapeHtml(t('recent.error', { error: String(e) })) + '</p>';
  }
}

// Re-render on locale change
document.addEventListener('locale-changed', () => {
  if (document.getElementById('section-recent')?.classList.contains('active')) {
    loadRecent();
  }
});
