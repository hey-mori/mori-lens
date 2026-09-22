// Apps section: per-application rules management

import { escapeHtml } from './ui.js';
import { state } from './state.js';
import { t } from './i18n.js';

export function renderApps() {
  const el = document.getElementById('apps-list');
  if (!el) return;
  if (!state.currentSettings || !state.currentSettings.applications || state.currentSettings.applications.length === 0) {
    el.innerHTML = '<p style="color:#888;font-size:13px">' + escapeHtml(t('apps.empty')) + '</p>';
    return;
  }
  let html = '<table><thead><tr><th style="text-align:left;padding-bottom:8px">Application</th><th style="text-align:left;padding-bottom:8px">Tier</th><th style="text-align:left;padding-bottom:8px">OCR</th><th></th></tr></thead><tbody>';
  for (let i = 0; i < state.currentSettings.applications.length; i++) {
    const app = state.currentSettings.applications[i];
    // Structured object: { name, tier, ocr }
    const name = app.name;
    const safeName = escapeHtml(name);
    const tier = app.tier;
    const ocr = app.ocr || 'inherit';
    const tierLabel = 'Tier for ' + safeName;
    const ocrLabel = 'OCR behavior for ' + safeName;
    html += '<tr><td>' + safeName + '</td><td>' +
      '<label for="app-tier-' + i + '" class="sr-only">' + tierLabel + '</label>' +
      '<select id="app-tier-' + i + '" data-app-index="' + i + '" class="app-tier-select" aria-label="' + tierLabel + '">' +
      '<option value="allow"' + (tier === 'allow' ? ' selected' : '') + '>' + escapeHtml(t('general.default.tier.allow')) + '</option>' +
      '<option value="modifier"' + (tier === 'modifier' ? ' selected' : '') + '>' + escapeHtml(t('general.default.tier.modifier')) + '</option>' +
      '<option value="deny"' + (tier === 'deny' ? ' selected' : '') + '>' + escapeHtml(t('general.default.tier.deny')) + '</option>' +
      '</select></td><td>' +
      '<label for="app-ocr-' + i + '" class="sr-only">' + ocrLabel + '</label>' +
      '<select id="app-ocr-' + i + '" data-app-index="' + i + '" class="app-ocr-select" aria-label="' + ocrLabel + '">' +
      '<option value="inherit"' + (ocr === 'inherit' ? ' selected' : '') + '>' + escapeHtml(t('apps.ocr.inherit')) + '</option>' +
      '<option value="off"' + (ocr === 'off' ? ' selected' : '') + '>' + escapeHtml(t('apps.ocr.off')) + '</option>' +
      '<option value="manual"' + (ocr === 'manual' ? ' selected' : '') + '>' + escapeHtml(t('apps.ocr.manual')) + '</option>' +
      '<option value="automatic"' + (ocr === 'automatic' ? ' selected' : '') + '>' + escapeHtml(t('apps.ocr.automatic')) + '</option>' +
      '</select></td><td style="width:60px;text-align:right">' +
      '<button class="btn app-remove-btn" data-app-index="' + i + '" aria-label="Remove ' + safeName + '" style="padding:2px 8px;font-size:12px">' + escapeHtml(t('apps.remove')) + '</button></td></tr>';
  }
  html += '</tbody></table>';
  el.innerHTML = html;
  // Attach event listeners using event delegation
  attachAppHandlers();
}

// Re-render on locale change
document.addEventListener('locale-changed', () => {
  if (document.getElementById('section-apps')?.classList.contains('active')) {
    renderApps();
  }
});

function attachAppHandlers() {
  document.querySelectorAll('.app-tier-select').forEach(sel => {
    sel.addEventListener('change', function() {
      updateAppTier(parseInt(this.dataset.appIndex), this.value);
    });
  });
  document.querySelectorAll('.app-ocr-select').forEach(sel => {
    sel.addEventListener('change', function() {
      updateAppOcr(parseInt(this.dataset.appIndex), this.value);
    });
  });
  document.querySelectorAll('.app-remove-btn').forEach(btn => {
    btn.addEventListener('click', function() {
      removeApp(parseInt(this.dataset.appIndex));
    });
  });
}

export function updateAppTier(index, tier) {
  // Structured object: update tier property
  state.currentSettings.applications[index].tier = tier;
  state.appsDirty = true;
}

export function updateAppOcr(index, ocr) {
  // Structured object: update ocr property. "inherit" means null (use global).
  state.currentSettings.applications[index].ocr = (ocr === 'inherit') ? null : ocr;
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
  const ocrSelect = document.getElementById('new-app-ocr');
  if (!nameInput || !tierSelect || !state.currentSettings) return;
  const name = nameInput.value.trim();
  const tier = tierSelect.value;
  const ocr = ocrSelect ? ocrSelect.value : 'inherit';
  if (!name) return;
  // Structured object: { name, tier, ocr }. "inherit" means null (use global).
  state.currentSettings.applications.push({ name, tier, ocr: (ocr === 'inherit') ? null : ocr });
  nameInput.value = '';
  state.appsDirty = true;
  renderApps();
}
