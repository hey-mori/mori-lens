// Library page: search, filter, export, import, detail view, notes, tags, Mori integration

import { invoke } from './tauri.js';
import { escapeHtml, populateSelect } from './ui.js';

let libraryOffset = 0;
let libraryQuery = '';
let libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null };
let moriAvailable = null;

export async function loadLibrary() {
  libraryOffset = 0;
  libraryQuery = '';
  libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null };
  const searchInput = document.getElementById('lib-search');
  if (searchInput) searchInput.value = '';
  document.getElementById('lib-filter-kind').value = '';
  document.getElementById('lib-filter-app').value = '';
  document.getElementById('lib-filter-source').value = '';
  document.getElementById('lib-filter-target').value = '';
  document.getElementById('lib-filter-state').value = '';
  await Promise.all([loadFilterOptions(), fetchLibraryPage()]);
}

async function loadFilterOptions() {
  try {
    const [apps, sources, targets] = await Promise.all([
      invoke('library_distinct_apps'),
      invoke('library_distinct_source_languages'),
      invoke('library_distinct_target_languages'),
    ]);
    populateSelect('lib-filter-app', apps);
    populateSelect('lib-filter-source', sources);
    populateSelect('lib-filter-target', targets);
  } catch (e) {
    // Ignore; dropdowns stay at "All".
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
  libraryFilters = { kind, app, source_language, target_language, review_state };
  libraryOffset = 0;
  fetchLibraryPage();
}

export function clearFilters() {
  libraryFilters = { kind: null, app: null, source_language: null, target_language: null, review_state: null };
  document.getElementById('lib-filter-kind').value = '';
  document.getElementById('lib-filter-app').value = '';
  document.getElementById('lib-filter-source').value = '';
  document.getElementById('lib-filter-target').value = '';
  document.getElementById('lib-filter-state').value = '';
  libraryOffset = 0;
  fetchLibraryPage();
}

async function fetchLibraryPage() {
  const listEl = document.getElementById('lib-list');
  const countEl = document.getElementById('lib-count');
  try {
    let page;
    const hasFilters = libraryFilters.kind || libraryFilters.app || libraryFilters.source_language || libraryFilters.target_language || libraryFilters.review_state;
    if (libraryQuery) {
      page = await invoke('library_search', { query: libraryQuery, offset: libraryOffset });
    } else if (hasFilters) {
      page = await invoke('library_filter', {
        kind: libraryFilters.kind,
        app: libraryFilters.app,
        source_language: libraryFilters.source_language,
        target_language: libraryFilters.target_language,
        review_state: libraryFilters.review_state,
        offset: libraryOffset
      });
    } else {
      page = await invoke('library_list', { offset: libraryOffset });
    }
    countEl.textContent = page.total + ' item' + (page.total !== 1 ? 's' : '');
    if (page.items.length === 0) {
      listEl.innerHTML = '<p style="color:#888;font-size:13px">' +
        (libraryQuery ? 'No items match "' + escapeHtml(libraryQuery) + '".' : 'No saved items yet.') + '</p>';
      return;
    }
    let html = '<table style="max-width:100%">';
    for (const item of page.items) {
      const date = new Date(item.created_at * 1000).toLocaleDateString();
      const kind = item.kind === 'analysis' ? 'analysis' : 'translation';
      const review = item.review_state || 'later';
      const reviewBadge = review === 'known' ? '<span style="color:#2e7d32;font-size:11px" title="Known"> ✓</span>'
        : review === 'later' ? '<span style="color:#f57c00;font-size:11px" title="Later"> ◐</span>'
        : review === 'archive' ? '<span style="color:#888;font-size:11px" title="Archive"> ▣</span>'
        : '<span style="color:#888;font-size:11px" title="Unknown"> ○</span>';
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
      html += '<button class="btn" id="load-more-lib-btn" style="margin-top:8px;padding:4px 12px;font-size:12px">Load more</button>';
    }
    listEl.innerHTML = html;
    // Attach handlers for library item rows and load more
    attachLibraryPageHandlers();
  } catch (e) {
    listEl.innerHTML = '<p class="error">Error: ' + escapeHtml(String(e)) + '</p>';
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
      detailEl.innerHTML = '<p class="error">Item not found.</p>';
      detailEl.style.display = 'block';
      return;
    }
    const date = new Date(item.created_at * 1000).toLocaleString();
    let html = '<div style="display:flex;justify-content:space-between;align-items:start;margin-bottom:12px">';
    html += '<div>';
    html += '<div style="font-size:12px;color:#888;margin-bottom:4px">' + item.kind + ' · ' + escapeHtml(item.app || 'unknown') + ' · ' + date + '</div>';
    if (item.model) html += '<div style="font-size:11px;color:#aaa">' + escapeHtml(item.model) + '</div>';
    html += '</div>';
    html += '<button class="btn lib-delete-btn" data-id="' + item.id + '" style="padding:4px 10px;font-size:12px;color:#c00;border-color:#c00">Delete</button>';
    html += '<button class="btn mori-send-btn" data-id="' + item.id + '" id="mori-send-btn-' + item.id + '" style="padding:4px 10px;font-size:12px;margin-left:8px;display:none">Send to Mori</button>';
    html += '</div>';
    html += '<div style="margin-bottom:12px;padding:12px;background:white;border-radius:4px;border:1px solid #eee">';
    html += '<div style="font-size:13px;color:#333;line-height:1.5">' + escapeHtml(item.original) + '</div>';
    html += '</div>';
    html += '<div style="margin-bottom:12px;padding:12px;background:white;border-radius:4px;border:1px solid #eee">';
    html += '<div style="font-size:13px;color:#333;line-height:1.5">' + escapeHtml(item.answer) + '</div>';
    if (item.caveat) html += '<div style="font-size:12px;color:#888;margin-top:8px;font-style:italic">' + escapeHtml(item.caveat) + '</div>';
    html += '</div>';
    html += '<div style="margin-top:12px">';
    html += '<label style="font-size:12px;color:#666;display:block;margin-bottom:4px">Note</label>';
    html += '<textarea id="lib-note-' + item.id + '" style="width:100%;min-height:60px;padding:8px;border:1px solid #ccc;border-radius:4px;font:inherit;resize:vertical;background:white">' + escapeHtml(item.note || '') + '</textarea>';
    html += '<button class="btn btn-primary lib-save-note-btn" data-id="' + item.id + '" style="margin-top:8px;padding:4px 12px;font-size:12px">Save note</button>';
    html += '<span id="lib-note-status-' + item.id + '" style="margin-left:8px;font-size:12px;color:#888"></span>';
    html += '</div>';
    html += '<div style="margin-top:16px">';
    html += '<label style="font-size:12px;color:#666;display:block;margin-bottom:4px">Tags</label>';
    html += '<input type="text" id="lib-tags-' + item.id + '" placeholder="comma, separated, tags" style="width:100%;max-width:300px;padding:6px 8px;border:1px solid #ccc;border-radius:4px;font:inherit;background:white">';
    html += '<button class="btn btn-primary lib-save-tags-btn" data-id="' + item.id + '" style="margin-top:8px;padding:4px 12px;font-size:12px">Save tags</button>';
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
    detailEl.innerHTML = '<p class="error">Error: ' + e + '</p>';
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
  if (!confirm('Delete this item? This cannot be undone.')) return;
  try {
    await invoke('library_delete', { id });
    document.getElementById('lib-detail').style.display = 'none';
    await fetchLibraryPage();
  } catch (e) {
    alert('Error: ' + e);
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
    if (!item) { alert('Item not found.'); return; }
    const preview = truncateSafe(item.original, 100);
    const answer = truncateSafe(item.answer, 100);
    const msg = 'Send to Mori?\n\nOriginal: ' + preview + '\n\nAnswer: ' + answer + '\n\nKind: ' + item.kind +
      (item.note ? '\nNote: ' + item.note : '') +
      (item.tags && item.tags.length ? '\nTags: ' + item.tags.join(', ') : '');
    if (!confirm(msg)) return;
    const transferId = await invoke('mori_send_item', { item_id: id });
    alert('Sent to Mori.\nTransfer ID: ' + transferId);
  } catch (e) {
    alert('Error sending to Mori: ' + e);
  }
}

async function saveLibraryNote(id) {
  const textarea = document.getElementById('lib-note-' + id);
  const statusEl = document.getElementById('lib-note-status-' + id);
  if (!textarea || !statusEl) return;
  try {
    await invoke('library_update_note', { id, note: textarea.value });
    statusEl.textContent = 'Saved';
    setTimeout(() => { statusEl.textContent = ''; }, 2000);
  } catch (e) {
    statusEl.textContent = 'Error: ' + e;
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
    statusEl.textContent = 'Saved';
    setTimeout(() => { statusEl.textContent = ''; }, 2000);
  } catch (e) {
    statusEl.textContent = 'Error: ' + e;
    statusEl.style.color = '#c00';
  }
}

export async function exportLibrary() {
  try {
    const markdown = await invoke('library_export');
    if (!markdown) { alert('Library is empty.'); return; }
    downloadBlob(markdown, 'text/markdown', 'saved.md');
  } catch (e) {
    alert('Export failed: ' + e);
  }
}

export async function exportLibraryJson() {
  try {
    const json = await invoke('library_export_json');
    if (!json || json === '[]') { alert('Library is empty.'); return; }
    downloadBlob(json, 'application/json', 'library.json');
  } catch (e) {
    alert('Export failed: ' + e);
  }
}

export async function exportLibraryJsonl() {
  try {
    const jsonl = await invoke('library_export_jsonl');
    if (!jsonl || jsonl.trim().split('\n').length <= 1) { alert('Library is empty.'); return; }
    downloadBlob(jsonl, 'application/x-ndjson', 'library.jsonl');
  } catch (e) {
    alert('Export failed: ' + e);
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
      alert(`Imported ${count} item(s). Duplicates were skipped.`);
      loadLibrary();
    } catch (e) {
      alert('Import failed: ' + e);
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
