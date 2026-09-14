// Apps section: per-application rules management

import { escapeHtml } from './ui.js';
import { state } from './state.js';

export function renderApps() {
  const el = document.getElementById('apps-list');
  if (!el) return;
  if (!state.currentSettings || !state.currentSettings.applications || state.currentSettings.applications.length === 0) {
    el.innerHTML = '<p style="color:#888;font-size:13px">No per-application rules.</p>';
    return;
  }
  let html = '<table>';
  for (let i = 0; i < state.currentSettings.applications.length; i++) {
    const [name, tier] = state.currentSettings.applications[i];
    html += '<tr><td>' + escapeHtml(name) + '</td><td>' +
      '<select data-app-index="' + i + '" class="app-tier-select">' +
      '<option value="allow"' + (tier === 'allow' ? ' selected' : '') + '>Allow</option>' +
      '<option value="modifier"' + (tier === 'modifier' ? ' selected' : '') + '>Modifier</option>' +
      '<option value="deny"' + (tier === 'deny' ? ' selected' : '') + '>Deny</option>' +
      '</select></td><td style="width:60px;text-align:right">' +
      '<button class="btn app-remove-btn" data-app-index="' + i + '" style="padding:2px 8px;font-size:12px">Remove</button></td></tr>';
  }
  html += '</table>';
  el.innerHTML = html;
  // Attach event listeners using event delegation
  attachAppHandlers();
}

function attachAppHandlers() {
  document.querySelectorAll('.app-tier-select').forEach(sel => {
    sel.addEventListener('change', function() {
      updateAppTier(parseInt(this.dataset.appIndex), this.value);
    });
  });
  document.querySelectorAll('.app-remove-btn').forEach(btn => {
    btn.addEventListener('click', function() {
      removeApp(parseInt(this.dataset.appIndex));
    });
  });
}

export function updateAppTier(index, tier) {
  state.currentSettings.applications[index][1] = tier;
  state.appsDirty = true;
}

export function removeApp(index) {
  state.currentSettings.applications.splice(index, 1);
  state.appsDirty = true;
  renderApps();
}

export function addApp() {
  const nameInput = document.getElementById('new-app-name');
  const tierSelect = document.getElementById('new-app-tier');
  if (!nameInput || !tierSelect || !state.currentSettings) return;
  const name = nameInput.value.trim();
  const tier = tierSelect.value;
  if (!name) return;
  state.currentSettings.applications.push([name, tier]);
  nameInput.value = '';
  state.appsDirty = true;
  renderApps();
}
