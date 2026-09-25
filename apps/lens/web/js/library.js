// Library page: search, filter, export, import, detail view, notes, tags, Mori integration

import { invoke } from './tauri.js';
import { escapeHtml, populateSelect } from './ui.js';
import { t } from './i18n.js';

let libraryOffset = 0;
let libraryQuery = '';
let libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null, tag: null };
let moriAvailable = null;

export async function loadLibrary() {
  libraryOffset = 0;
  libraryQuery = '';
  libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null, tag: null };
  const searchInput = document.getElementById('lib-search');
  if (searchInput) searchInput.value = '';
  document.getElementById('lib-filter-kind').value = '';
  document.getElementById('lib-filter-app').value = '';
  document.getElementById('lib-filter-source').value = '';
  document.getElementById('lib-filter-target').value = '';
  document.getElementById('lib-filter-state').value = '';
  document.getElementById('lib-filter-tag').value = '';
  await Promise.all([loadFilterOptions(), fetchLibraryPage()]);
}

async function loadFilterOptions() {
  try {
    const [apps, sources, targets, tags] = await Promise.all([
      invoke('library_distinct_apps'),
      invoke('library_distinct_source_languages'),
      invoke('library_distinct_target_languages'),
      invoke('library_distinct_tags'),
    ]);
    populateSelect('lib-filter-app', apps);
    populateSelect('lib-filter-source', sources);
    populateSelect('lib-filter-target', targets);
    populateSelect('lib-filter-tag', tags);
  } catch (e) {
    // Log detailed error for debugging, show safe message to user.
    console.error('Failed to load filter options:', e);
    // Do not expose raw backend errors (may contain paths, SQLite details, etc.).
    // Filter options are non-critical; Library still works without them.
  }
}

export function searchLibrary() {
  const searchInput = document.getElementById('lib-search');
  libraryQuery = searchInput ? searchInput.value.trim() : '';
  libraryOffset = 0;
  fetchLibraryPage();
}

export function applyFilters() {
  const kind = document.getElementById('lib-filter-kind').value || null;
  const app = document.getElementById('lib-filter-app').value || null;
  const source_language = document.getElementById('lib-filter-source').value || null;
  const target_language = document.getElementById('lib-filter-target').value || null;
  const review_state = document.getElementById('lib-filter-state').value || null;
  const tag = document.getElementById('lib-filter-tag').value || null;
  libraryFilters = { kind, app, source_language, target_language, review_state, tag };
  libraryOffset = 0;
  fetchLibraryPage();
}

export function clearFilters() {
  libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null, tag: null };
  document.getElementById('lib-filter-kind').value = '';
  document.getElementById('lib-filter-app').value = '';
  document.getElementById('lib-filter-source').value = '';
  document.getElementById('lib-filter-target').value = '';
  document.getElementById('lib-filter-state').value = '';
  document.getElementById('lib-filter-tag').value = '';
  libraryOffset = 0;
  fetchLibraryPage();
}

async function fetchLibraryPage() {
  const listEl = document.getElementById('lib-list');
  const countEl = document.getElementById('lib-count');
  try {
    // Always use the canonical query command for consistent semantics
    const page = await invoke('library_query', {
      query: libraryQuery,
      kind: libraryFilters.kind,
      app: libraryFilters.app,
      source_language: libraryFilters.source_language,
      target_language: libraryFilters.target_language,
      review_state: libraryFilters.review_state,
      tag: libraryFilters.tag,
      offset: libraryOffset
    });
    countEl.textContent = t('library.items.count', { count: page.total });
    if (page.items.length === 0) {
      listEl.innerHTML = '<p style="color:#888;font-size:13px">' +
        (libraryQuery ? escapeHtml(t('library.items.match', { query: libraryQuery })) : escapeHtml(t('library.empty.default'))) + '</p>';
      return;
    }
    let html = '<table style="max-width:100%">';
    for (const item of page.items) {
      const date = new Date(item.created_at * 1000).toLocaleDateString();
      const kind = item.kind === 'analysis' ? 'analysis' : 'translation';
      const review = item.review_state || 'later';
      const reviewBadge = review === 'known' ? '<span style="color:#2e7d32;font-size:11px" title="' + escapeHtml(t('library.review.known')) + '"> ✓</span>'
        : review === 'later' ? '<span style="color:#f57c00;font-size:11px" title="' + escapeHtml(t('library.review.later')) + '"> ◐</span>'
        : review === 'archive' ? '<span style="color:#888;font-size:11px" title="' + escapeHtml(t('library.review.archive')) + '"> ▣</span>'
        : '<span style="color:#888;font-size:11px" title="' + escapeHtml(t('library.review.unknown')) + '"> ○</span>';
      const preview = item.original.length > 60 ? item.original.slice(0, 60) + '...' : item.original;
      html += '<tr style="cursor:pointer" data-lib-item-id="' + item.id + '" class="lib-item-row">';
      html += '<td style="width:60%;color:#333">' + escapeHtml(preview) + '</td>';
      html += '<td style="width:15%;color:#888;font-size:12px">' + kind + reviewBadge + '</td>';
      html += '<td style="width:15%;color:#888;font-size:12px">' + escapeHtml(item.app || '') + '</td>';
      html += '<td style="width:10%;color:#888;font-size:12px;text-align:right">' + date + '</td>';
      html += '</tr>';
    }
    html += '</table>';
    if (page.has_more) {
      html += '<button class="btn" id="load-more-lib-btn" style="margin-top:8px;padding:4px 12px;font-size:12px">' + escapeHtml(t('library.load.more')) + '</button>';
    }
    listEl.innerHTML = html;
    // Attach handlers for library item rows and load more
    attachLibraryPageHandlers();
  } catch (e) {
    listEl.innerHTML = '<p class="error">' + escapeHtml(t('library.save.error', { error: String(e) })) + '</p>';
  }
}

function attachLibraryPageHandlers() {
  document.querySelectorAll('.lib-item-row').forEach(row => {
    row.addEventListener('click', function() {
      showLibraryItem(parseInt(this.dataset.libItemId));
    });
  });
  const loadMoreBtn = document.getElementById('load-more-lib-btn');
  if (loadMoreBtn) loadMoreBtn.addEventListener('click', loadMoreLibrary);
}

function loadMoreLibrary() {
  libraryOffset += 50;
  fetchLibraryPage();
}

export async function showLibraryItem(id) {
  const detailEl = document.getElementById('lib-detail');
  try {
    const item = await invoke('library_get', { id });
    if (!item) {
      detailEl.innerHTML = '<p class="error">' + escapeHtml(t('library.detail.not.found')) + '</p>';
      detailEl.style.display = 'block';
      return;
    }
    const date = new Date(item.created_at * 1000).toLocaleString();
    let html = '<div style="display:flex;justify-content:space-between;align-items:start;margin-bottom:12px">';
    html += '<div>';
    html += '<div style="font-size:12px;color:#888;margin-bottom:4px">' + item.kind + ' · ' + escapeHtml(item.app || 'unknown') + ' · ' + date + '</div>';
    if (item.model) html += '<div style="font-size:11px;color:#aaa">' + escapeHtml(item.model) + '</div>';
    html += '</div>';
    html += '<button class="btn lib-delete-btn" data-id="' + item.id + '" style="padding:4px 10px;font-size:12px;color:#c00;border-color:#c00">' + escapeHtml(t('common.delete')) + '</button>';
    html += '<button class="btn mori-send-btn" data-id="' + item.id + '" id="mori-send-btn-' + item.id + '" style="padding:4px 10px;font-size:12px;margin-left:8px;display:none">' + escapeHtml(t('library.send.to.mori')) + '</button>';
    html += '</div>';
    html += '<div style="margin-bottom:12px;padding:12px;background:white;border-radius:4px;border:1px solid #eee">';
    html += '<div style="font-size:13px;color:#333;line-height:1.5">' + escapeHtml(item.original) + '</div>';
    html += '</div>';
    html += '<div style="margin-bottom:12px;padding:12px;background:white;border-radius:4px;border:1px solid #eee">';
    html += '<div style="font-size:13px;color:#333;line-height:1.5">' + escapeHtml(item.answer) + '</div>';
    if (item.caveat) html += '<div style="font-size:12px;color:#888;margin-top:8px;font-style:italic">' + escapeHtml(item.caveat) + '</div>';
    html += '</div>';
    html += '<div style="margin-top:12px">';
    html += '<label style="font-size:12px;color:#666;display:block;margin-bottom:4px">' + escapeHtml(t('library.note')) + '</label>';
    html += '<textarea id="lib-note-' + item.id + '" style="width:100%;min-height:60px;padding:8px;border:1px solid #ccc;border-radius:4px;font:inherit;resize:vertical;background:white">' + escapeHtml(item.note || '') + '</textarea>';
    html += '<button class="btn btn-primary lib-save-note-btn" data-id="' + item.id + '" style="margin-top:8px;padding:4px 12px;font-size:12px">' + escapeHtml(t('library.save.note')) + '</button>';
    html += '<span id="lib-note-status-' + item.id + '" style="margin-left:8px;font-size:12px;color:#888"></span>';
    html += '</div>';
    html += '<div style="margin-top:16px">';
    html += '<label style="font-size:12px;color:#666;display:block;margin-bottom:4px">' + escapeHtml(t('library.tags')) + '</label>';
    html += '<input type="text" id="lib-tags-' + item.id + '" placeholder="' + escapeHtml(t('library.tags.placeholder')) + '" style="width:100%;max-width:300px;padding:6px 8px;border:1px solid #ccc;border-radius:4px;font:inherit;background:white">';
    html += '<button class="btn btn-primary lib-save-tags-btn" data-id="' + item.id + '" style="margin-top:8px;padding:4px 12px;font-size:12px">' + escapeHtml(t('library.save.tags')) + '</button>';
    html += '<span id="lib-tags-status-' + item.id + '" style="margin-left:8px;font-size:12px;color:#888"></span>';
    html += '</div>';
    detailEl.innerHTML = html;
    detailEl.style.display = 'block';

    // Attach detail handlers
    attachLibraryDetailHandlers(item.id);

    // Load tags for this item
    try {
      const tags = await invoke('library_get_tags', { id });
      const tagsInput = document.getElementById('lib-tags-' + item.id);
      if (tagsInput) tagsInput.value = tags.join(', ');
    } catch (e) {
      // Ignore tag loading errors.
    }

    // Show Send to Mori button if available
    try {
      const available = await checkMoriAvailable();
      const btn = document.getElementById('mori-send-btn-' + item.id);
      if (btn && available) btn.style.display = '';
    } catch (e) {
      // Ignore availability check errors.
    }
  } catch (e) {
    detailEl.innerHTML = '<p class="error">' + escapeHtml(t('library.save.error', { error: String(e) })) + '</p>';
    detailEl.style.display = 'block';
  }
}

function attachLibraryDetailHandlers(itemId) {
  const deleteBtn = detailEl_query('.lib-delete-btn');
  if (deleteBtn) deleteBtn.addEventListener('click', function() { deleteLibraryItem(parseInt(this.dataset.id)); });

  const moriBtn = detailEl_query('.mori-send-btn');
  if (moriBtn) moriBtn.addEventListener('click', function() { sendToMori(parseInt(this.dataset.id)); });

  const saveNoteBtn = detailEl_query('.lib-save-note-btn');
  if (saveNoteBtn) saveNoteBtn.addEventListener('click', function() { saveLibraryNote(parseInt(this.dataset.id)); });

  const saveTagsBtn = detailEl_query('.lib-save-tags-btn');
  if (saveTagsBtn) saveTagsBtn.addEventListener('click', function() { saveLibraryTags(parseInt(this.dataset.id)); });
}

function detailEl_query(selector) {
  const detailEl = document.getElementById('lib-detail');
  return detailEl ? detailEl.querySelector(selector) : null;
}

async function deleteLibraryItem(id) {
  if (!confirm(t('library.delete.confirm'))) return;
  try {
    await invoke('library_delete', { id });
    document.getElementById('lib-detail').style.display = 'none';
    await fetchLibraryPage();
  } catch (e) {
    alert(t('library.save.error', { error: e }));
  }
}

async function checkMoriAvailable() {
  try {
    const status = await invoke('mori_status');
    moriAvailable = status.status === 'supported';
  } catch (e) {
    moriAvailable = false;
  }
  return moriAvailable;
}

async function sendToMori(id) {
  const { truncateSafe } = await import('./ui.js');
  try {
    const item = await invoke('library_get', { id });
    if (!item) { alert(t('library.transfer.not.found')); return; }
    const preview = truncateSafe(item.original, 100);
    const answer = truncateSafe(item.answer, 100);
    const msg = t('library.mori.confirm', { original: preview, answer, kind: item.kind }) +
      (item.note ? t('library.note.prefix') + item.note : '') +
      (item.tags && item.tags.length ? t('library.tags.prefix') + item.tags.join(', ') : '');
    if (!confirm(msg)) return;
    const transferId = await invoke('mori_send_item', { item_id: id });
    alert(t('library.transfer.success', { id: transferId }));
  } catch (e) {
    alert(t('library.transfer.error', { error: e }));
  }
}

async function saveLibraryNote(id) {
  const textarea = document.getElementById('lib-note-' + id);
  const statusEl = document.getElementById('lib-note-status-' + id);
  if (!textarea || !statusEl) return;
  try {
    await invoke('library_update_note', { id, note: textarea.value });
    statusEl.textContent = t('library.save.success');
    setTimeout(() => { statusEl.textContent = ''; }, 2000);
  } catch (e) {
    statusEl.textContent = t('library.save.error', { error: e });
    statusEl.style.color = '#c00';
  }
}

async function saveLibraryTags(id) {
  const input = document.getElementById('lib-tags-' + id);
  const statusEl = document.getElementById('lib-tags-status-' + id);
  if (!input || !statusEl) return;
  try {
    const tags = input.value.split(',').map(t => t.trim()).filter(t => t.length > 0);
    await invoke('library_set_tags', { id, tags });
    statusEl.textContent = t('library.save.success');
    setTimeout(() => { statusEl.textContent = ''; }, 2000);
  } catch (e) {
    statusEl.textContent = t('library.save.error', { error: e });
    statusEl.style.color = '#c00';
  }
}

export async function exportLibrary() {
  try {
    const markdown = await invoke('library_export');
    if (!markdown) { alert(t('library.export.empty')); return; }
    downloadBlob(markdown, 'text/markdown', 'saved.md');
  } catch (e) {
    alert(t('library.export.error', { error: e }));
  }
}

export async function exportLibraryJson() {
  try {
    const json = await invoke('library_export_json');
    if (!json || json === '[]') { alert(t('library.export.empty')); return; }
    downloadBlob(json, 'application/json', 'library.json');
  } catch (e) {
    alert(t('library.export.error', { error: e }));
  }
}

export async function exportLibraryJsonl() {
  try {
    const jsonl = await invoke('library_export_jsonl');
    if (!jsonl || jsonl.trim().split('\n').length <= 1) { alert(t('library.export.empty')); return; }
    downloadBlob(jsonl, 'application/x-ndjson', 'library.jsonl');
  } catch (e) {
    alert(t('library.export.error', { error: e }));
  }
}

export async function importLibraryJsonl() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.jsonl,.ndjson';
  input.onchange = async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const text = await file.text();
      const count = await invoke('library_import_jsonl', { jsonl: text });
      alert(t('library.import.success', { count }));
      loadLibrary();
    } catch (e) {
      alert(t('library.import.error', { error: e }));
    }
  };
  input.click();
}

function downloadBlob(content, mimeType, filename) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export function initLibraryHandlers() {
  const searchBtn = document.getElementById('lib-search-btn');
  if (searchBtn) searchBtn.addEventListener('click', searchLibrary);

  const searchInput = document.getElementById('lib-search');
  if (searchInput) searchInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') searchLibrary();
  });

  const exportMdBtn = document.getElementById('lib-export-md-btn');
  if (exportMdBtn) exportMdBtn.addEventListener('click', exportLibrary);

  const exportJsonBtn = document.getElementById('lib-export-json-btn');
  if (exportJsonBtn) exportJsonBtn.addEventListener('click', exportLibraryJson);

  const exportJsonlBtn = document.getElementById('lib-export-jsonl-btn');
  if (exportJsonlBtn) exportJsonlBtn.addEventListener('click', exportLibraryJsonl);

  const importJsonlBtn = document.getElementById('lib-import-jsonl-btn');
  if (importJsonlBtn) importJsonlBtn.addEventListener('click', importLibraryJsonl);

  document.getElementById('lib-filter-kind').addEventListener('change', applyFilters);
  document.getElementById('lib-filter-app').addEventListener('change', applyFilters);
  document.getElementById('lib-filter-source').addEventListener('change', applyFilters);
  document.getElementById('lib-filter-target').addEventListener('change', applyFilters);
  document.getElementById('lib-filter-state').addEventListener('change', applyFilters);

  const clearFiltersBtn = document.getElementById('lib-clear-filters-btn');
  if (clearFiltersBtn) clearFiltersBtn.addEventListener('click', clearFilters);

  // Listen for the detail window's "Open in Library" button
  import('./tauri.js').then(t => {
    t.listen('open-library-item', async (event) => {
      document.querySelectorAll('.sidebar-item').forEach(i => i.classList.remove('active'));
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));
      document.querySelector('[data-section="library"]').classList.add('active');
      document.getElementById('section-library').classList.add('active');
      await loadLibrary();
      if (event.payload) {
        await showLibraryItem(event.payload);
      }
    });
  });
}

// Re-render on locale change
document.addEventListener('locale-changed', () => {
  if (document.getElementById('section-library')?.classList.contains('active')) {
    loadLibrary();
  }
});
