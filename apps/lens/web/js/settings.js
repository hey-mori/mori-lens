// Settings (General) page: loadSettings, saveSettings, applySettingsToForm

import { invoke } from './tauri.js';
import { setStatus } from './ui.js';
import { state } from './state.js';
import { refreshLocalAiStatus } from './local_ai.js';

const PROVIDER_DEFAULT_ENDPOINTS = {
  ollama: '127.0.0.1:11434',
  lmstudio: '127.0.0.1:1234'
};

export async function loadSettings() {
  state.currentSettings = await invoke('get_settings');
  applySettingsToForm(state.currentSettings);
  attachProviderChangeHandler();
  import('./apps.js').then(m => m.renderApps());
  renderShortcuts(state.currentSettings.shortcuts);
  import('./models.js').then(m => m.refreshModels(state.currentSettings.quick_model));
}

export function applySettingsToForm(settings) {
  document.getElementById('f-language').value = settings.language;
  document.getElementById('f-dwell').value = settings.dwell_ms;
  document.getElementById('f-theme').value = settings.theme;
  document.getElementById('f-text-size').value = settings.text_size;
  document.getElementById('f-translucency').value = settings.translucency;
  document.getElementById('f-ocr').value = settings.ocr;
  document.getElementById('f-profile').value = settings.profile;
  document.getElementById('f-hover-scope').value = settings.hover_scope;
  document.getElementById('f-default-tier').value = settings.default_tier;
  document.getElementById('f-history').checked = settings.history;
  document.getElementById('f-history-days').value = settings.history_days;
  document.getElementById('f-history-store-app').checked = settings.history_store_app;
  document.getElementById('f-history-excluded').value = (settings.history_excluded || []).join(',');
  const quickEngineSelect = document.getElementById('f-quick-engine');
  quickEngineSelect.value = settings.quick_engine || 'ollama';
  quickEngineSelect.dataset.currentProvider = settings.quick_engine || 'ollama';
  document.getElementById('f-quick-endpoint').value = settings.quick_endpoint;
  document.getElementById('f-quick-model-select').value = settings.quick_model || '';
  document.getElementById('f-deep-engine').value = settings.deep_engine;
  document.getElementById('f-deep-model').value = settings.deep_model || '';
  document.getElementById('f-deep-endpoint').value = settings.deep_endpoint;
}

export async function saveSettings() {
  const view = {
    language: document.getElementById('f-language').value,
    dwell_ms: parseInt(document.getElementById('f-dwell').value) || 320,
    theme: document.getElementById('f-theme').value,
    text_size: document.getElementById('f-text-size').value,
    translucency: document.getElementById('f-translucency').value,
    ocr: document.getElementById('f-ocr').value,
    profile: document.getElementById('f-profile').value,
    hover_scope: document.getElementById('f-hover-scope').value,
    default_tier: document.getElementById('f-default-tier').value,
    history: document.getElementById('f-history').checked,
    history_days: parseInt(document.getElementById('f-history-days').value) || 0,
    history_store_app: document.getElementById('f-history-store-app').checked,
    history_excluded: document.getElementById('f-history-excluded').value
      .split(',').map(s => s.trim().toLowerCase()).filter(s => s.length > 0),
    quick_engine: document.getElementById('f-quick-engine').value,
    quick_model: document.getElementById('f-quick-model-select').value || null,
    quick_endpoint: document.getElementById('f-quick-endpoint').value,
    deep_engine: document.getElementById('f-deep-engine').value,
    deep_model: document.getElementById('f-deep-model').value || null,
    deep_endpoint: document.getElementById('f-deep-endpoint').value,
    applications: state.currentSettings.applications,
    shortcuts: collectShortcuts()
  };
  try {
    const outcome = await invoke('save_settings', { settings: view });
    state.currentSettings = outcome.settings;
    applySettingsToForm(outcome.settings);
    import('./apps.js').then(m => m.renderApps());

    // Refresh Local AI status after successful save to reflect new configuration.
    // The status represents saved configuration (not active runtime), so refreshing
    // immediately is correct even when restart is required. The restart indicator
    // separately shows that active runtime differs from saved configuration.
    await refreshLocalAiStatus();

    if (outcome.restart_required) {
      setStatus('저장됨 · 변경사항을 적용하려면 Mori Lens를 다시 시작하세요', true);
    } else {
      setStatus('저장됨 · 바로 적용되었습니다');
    }
  } catch (e) {
    setStatus('Error: ' + e);
  }
}

function attachProviderChangeHandler() {
  const quickEngineSelect = document.getElementById('f-quick-engine');
  if (!quickEngineSelect || quickEngineSelect.dataset.providerHandlerAttached) return;
  quickEngineSelect.dataset.providerHandlerAttached = 'true';

  quickEngineSelect.addEventListener('change', () => {
    const newProvider = quickEngineSelect.value;
    const oldProvider = quickEngineSelect.dataset.currentProvider || 'ollama';

    // Clear model selection (models are provider-specific)
    const modelSelect = document.getElementById('f-quick-model-select');
    if (modelSelect) {
      modelSelect.value = '';
    }

    // Immediately update endpoint to reflect what backend will do
    const endpointInput = document.getElementById('f-quick-endpoint');
    if (endpointInput) {
      const currentEndpoint = endpointInput.value;
      const oldDefault = getDefaultEndpoint(oldProvider);
      const newDefault = getDefaultEndpoint(newProvider);

      if (currentEndpoint === oldDefault) {
        // User hasn't customized endpoint, show new provider's default
        endpointInput.value = newDefault;
      }
      // Otherwise, keep the custom endpoint (backend will validate it)
    }

    // Update stored provider for next change
    quickEngineSelect.dataset.currentProvider = newProvider;

    // Refresh model list for the new provider
    import('./models.js').then(m => m.refreshModels(null));
  });
}

const ACTION_LABELS = {
  pause: 'Pause/Resume',
  manual_ocr: 'Manual OCR',
  deep: 'Deep Analysis',
  copy: 'Copy Card',
  save: 'Save to Library',
  pin: 'Pin Card'
};

function renderShortcuts(shortcuts) {
  const container = document.getElementById('shortcuts-list');
  if (!container) return;

  // Clear existing content
  container.innerHTML = '';

  for (const binding of shortcuts.bindings) {
    const { action, shortcut, default_shortcut, can_unbind } = binding;
    const label = ACTION_LABELS[action] || action;

    // Create field container
    const fieldDiv = document.createElement('div');
    fieldDiv.className = 'field';

    // Create label with proper association
    const labelEl = document.createElement('label');
    const inputId = `shortcut-${action}`;
    labelEl.htmlFor = inputId;
    labelEl.textContent = label;

    // Create input
    const input = document.createElement('input');
    input.type = 'text';
    input.id = inputId;
    input.dataset.shortcutAction = action;
    input.value = shortcut || '';
    input.placeholder = default_shortcut ? `Default: ${default_shortcut}` : 'e.g. Ctrl+Alt+P';
    input.style.fontFamily = 'monospace';

    // Add hint for unbindable actions
    if (can_unbind) {
      const hint = document.createElement('small');
      hint.style.color = '#888';
      hint.style.marginLeft = '8px';
      hint.textContent = '(optional)';
      fieldDiv.appendChild(hint);
    }

    fieldDiv.appendChild(labelEl);
    fieldDiv.appendChild(input);
    container.appendChild(fieldDiv);
  }
}

function collectShortcuts() {
  const bindings = [];
  const inputs = document.querySelectorAll('[data-shortcut-action]');
  inputs.forEach(input => {
    const action = input.dataset.shortcutAction;
    const value = input.value.trim();
    bindings.push({
      action,
      shortcut: value || null
    });
  });
  return { bindings };
}

export function restoreShortcutDefaults() {
  // Get current settings to access default_shortcut values
  if (!state.currentSettings || !state.currentSettings.shortcuts) {
    return;
  }

  const inputs = document.querySelectorAll('[data-shortcut-action]');
  inputs.forEach(input => {
    const action = input.dataset.shortcutAction;
    const binding = state.currentSettings.shortcuts.bindings.find(b => b.action === action);
    if (binding && binding.default_shortcut) {
      input.value = binding.default_shortcut;
    }
  });

  // Show status message
  setStatus('Defaults restored · Click Save to apply');
}
