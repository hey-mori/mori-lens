// History page: persistent history

import { invoke } from './tauri.js';
import { escapeHtml } from './ui.js';

let historyOffset = 0;

export async function loadHistory() {
  historyOffset = 0;
  try {
    const runtime = await invoke('get_runtime_status');
    const descEl = document.getElementById('history-description');
    if (descEl && runtime) {
      if (runtime.history_enabled) {
        const days = runtime.history_days === 0 ? 'forever' : runtime.history_days + '일';
        descEl.textContent = 'Persistent History 사용 중 · 보관 기간: ' + days;
        descEl.style.color = '#2e7d32';
      } else {
        descEl.textContent = 'Persistent History는 꺼져 있습니다. General 설정에서 켤 수 있습니다.';
        descEl.style.color = '#888';
      }
    }
  } catch (e) {
    // Ignore; description stays at default.
  }
  await fetchHistoryPage();
}

async function fetchHistoryPage() {
  const listEl = document.getElementById('history-list');
  const countEl = document.getElementById('history-count');
  try {
    const [page, count] = await Promise.all([
      invoke('history_list', { offset: historyOffset }),
      invoke('history_count')
    ]);
    countEl.textContent = count + ' entr' + (count !== 1 ? 'ies' : 'y');
    if (!page.entries || page.entries.length === 0) {
      listEl.innerHTML = '<p style="color:#888;font-size:13px">No history entries yet.</p>';
      return;
    }
    let html = '<table style="max-width:100%">';
    for (const entry of page.entries) {
      const date = new Date(entry.created_at * 1000).toLocaleDateString();
      const preview = entry.original.length > 60 ? entry.original.slice(0, 60) + '...' : entry.original;
      const answer = entry.answer.length > 60 ? entry.answer.slice(0, 60) + '...' : entry.answer;
      const provenanceLabel = entry.provenance === 'ocr' ? '📷' : entry.provenance === 'accessibility' ? '♿' : entry.provenance || '?';
      const provenanceTitle = entry.provenance === 'ocr' ? 'OCR (screen reading)' : entry.provenance === 'accessibility' ? 'Accessibility API' : entry.provenance || 'Unknown';
      const providerLabel = entry.provider_scope === 'external' ? '🌐' : entry.provider_scope === 'local' ? '' : entry.provider_scope || '?';
      const providerTitle = entry.provider_scope === 'external' ? 'External provider (text left machine)' : entry.provider_scope === 'local' ? 'Local inference' : entry.provider_scope || 'Unknown provider';
      html += '<tr>';
      html += '<td style="width:40%;color:#333">' + escapeHtml(preview) + '</td>';
      html += '<td style="width:35%;color:#666">' + escapeHtml(answer) + '</td>';
      html += '<td style="width:5%;text-align:center;font-size:14px" title="' + provenanceTitle + '">' + provenanceLabel + '</td>';
      html += '<td style="width:5%;text-align:center;font-size:14px" title="' + providerTitle + '">' + providerLabel + '</td>';
      html += '<td style="width:10%;color:#888;font-size:12px">' + escapeHtml(entry.app || '') + '</td>';
      html += '<td style="width:5%;color:#888;font-size:12px">' + date + '</td>';
      html += '<td style="width:5%;text-align:right">';
      html += '<button class="btn history-promote-btn" data-id="' + entry.id + '" style="padding:2px 6px;font-size:11px" title="Save to library">Save</button> ';
      html += '<button class="btn history-delete-btn" data-id="' + entry.id + '" style="padding:2px 6px;font-size:11px;color:#c00;border-color:#c00" title="Delete">✕</button>';
      html += '</td>';
      html += '</tr>';
    }
    html += '</table>';
    if (page.has_more) {
      html += '<button class="btn" id="load-more-history-btn" style="margin-top:8px;padding:4px 12px;font-size:12px">Load more</button>';
    }
    listEl.innerHTML = html;
    attachHistoryHandlers();
  } catch (e) {
    listEl.innerHTML = '<p class="error">Error: ' + escapeHtml(String(e)) + '</p>';
  }
}

function attachHistoryHandlers() {
  document.querySelectorAll('.history-promote-btn').forEach(btn => {
    btn.addEventListener('click', function() { promoteHistoryEntry(parseInt(this.dataset.id)); });
  });
  document.querySelectorAll('.history-delete-btn').forEach(btn => {
    btn.addEventListener('click', function() { deleteHistoryEntry(parseInt(this.dataset.id)); });
  });
  const loadMoreBtn = document.getElementById('load-more-history-btn');
  if (loadMoreBtn) loadMoreBtn.addEventListener('click', loadMoreHistory);
}

function loadMoreHistory() {
  historyOffset += 50;
  fetchHistoryPage();
}

async function deleteHistoryEntry(id) {
  if (!confirm('Delete this history entry?')) return;
  try {
    await invoke('history_delete', { id });
    await fetchHistoryPage();
  } catch (e) {
    alert('Error: ' + e);
  }
}

async function promoteHistoryEntry(id) {
  try {
    await invoke('history_promote', { id });
    alert('Saved to library.');
  } catch (e) {
    alert('Error: ' + e);
  }
}

export async function pruneHistory() {
  const retentionDays = 30;
  const retentionSecs = retentionDays * 24 * 60 * 60;
  if (!confirm(`Prune history entries older than ${retentionDays} days?`)) return;
  try {
    const count = await invoke('history_prune', { retentionSecs });
    alert('Pruned ' + count + ' entries.');
    await fetchHistoryPage();
  } catch (e) {
    alert('Error: ' + e);
  }
}

export function initHistoryHandlers() {
  const pruneBtn = document.getElementById('history-prune-btn');
  if (pruneBtn) pruneBtn.addEventListener('click', pruneHistory);
}
