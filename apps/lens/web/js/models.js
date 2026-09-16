// Models page: refreshModels, testModel, refreshLocalAiStatus

import { invoke } from './tauri.js';
import { formatSize, escapeHtml } from './ui.js';
import { state } from './state.js';

const PROVIDER_DEFAULT_ENDPOINTS = {
  ollama: '127.0.0.1:11434',
  lmstudio: '127.0.0.1:1234'
};

function providerDisplayName(provider) {
  return provider === 'lmstudio' ? 'LM Studio' : 'Ollama';
}

function getDefaultEndpoint(provider) {
  return PROVIDER_DEFAULT_ENDPOINTS[provider] || PROVIDER_DEFAULT_ENDPOINTS.ollama;
}

export async function refreshModels(selectedModel) {
  const provider = document.getElementById('f-quick-engine').value || 'ollama';
  const endpoint = document.getElementById('f-quick-endpoint').value || getDefaultEndpoint(provider);
  const dot = document.getElementById('provider-dot');
  const label = document.getElementById('provider-label');
  const epLabel = document.getElementById('provider-endpoint');
  const listEl = document.getElementById('models-list');
  const selectEl = document.getElementById('f-quick-model-select');

  if (!dot || !label || !epLabel || !listEl || !selectEl) return;

  const providerName = providerDisplayName(provider);
  epLabel.textContent = endpoint;

  const reachable = await invoke('check_provider', { provider, endpoint });
  if (!reachable) {
    dot.style.background = '#c00';
    label.textContent = providerName + ' not reachable';
    label.style.color = '#c00';
    listEl.innerHTML = '<p style="color:#888;font-size:13px">Start ' + providerName + ' and refresh.</p>';
    selectEl.innerHTML = '<option value="">Auto (best available)</option>';
    return;
  }

  dot.style.background = '#2a2';
  label.textContent = providerName + ' running';
  label.style.color = '#2a2';

  try {
    const models = await invoke('list_models', { provider, endpoint });
    if (!models || models.length === 0) {
      listEl.innerHTML = '<p style="color:#888;font-size:13px">No models installed.</p>';
      selectEl.innerHTML = '<option value="">Auto (best available)</option>';
      return;
    }
    let html = '<table>';
    for (const m of models) {
      const isSel = selectedModel === m.id;
      html += '<tr><td style="width:60%">' + escapeHtml(m.display_name) + '</td><td>' + formatSize(m.size_bytes) + '</td>';
      html += '<td style="width:80px;text-align:right">';
      if (isSel) html += '<span style="color:#0066cc;font-size:12px">selected</span>';
      html += '</td></tr>';
    }
    html += '</table>';
    listEl.innerHTML = html;

    let opts = '<option value="">Auto (best available)</option>';
    for (const m of models) {
      opts += '<option value="' + escapeHtml(m.id) + '"' + (selectedModel === m.id ? ' selected' : '') + '>' + escapeHtml(m.display_name) + '</option>';
    }
    selectEl.innerHTML = opts;
  } catch (e) {
    listEl.innerHTML = '<p class="error">Error: ' + escapeHtml(String(e)) + '</p>';
  }
}

export async function testModel() {
  const provider = document.getElementById('f-quick-engine').value || 'ollama';
  const endpoint = document.getElementById('f-quick-endpoint').value || getDefaultEndpoint(provider);
  const model = document.getElementById('f-quick-model-select').value;
  const resultEl = document.getElementById('test-result');
  const btn = document.getElementById('test-btn');
  if (!model) {
    resultEl.innerHTML = '<span class="error">Select a model first.</span>';
    return;
  }
  btn.disabled = true;
  btn.textContent = 'Testing...';
  resultEl.innerHTML = '';
  try {
    const result = await invoke('test_model', { provider, endpoint, model });
    resultEl.innerHTML = '<strong>' + escapeHtml(String(result.model)) + '</strong> (' + escapeHtml(String(result.latency_ms)) + 'ms)<br>' + escapeHtml(String(result.translated));
  } catch (e) {
    resultEl.innerHTML = '<span class="error">Error: ' + escapeHtml(String(e)) + '</span>';
  }
  btn.disabled = false;
  btn.textContent = 'Test';
}

export async function refreshLocalAiStatus() {
  try {
    state.cachedLocalAiStatus = await invoke('get_local_ai_status');
    state.localAiLoadState = 'LOADED';
  } catch (e) {
    console.error('refreshLocalAiStatus failed:', e);
    state.cachedLocalAiStatus = null;
    state.localAiLoadState = 'ERROR';
  }
}
