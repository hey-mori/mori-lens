// Models page: refreshModels, testModel

import { invoke } from './tauri.js';
import { formatSize } from './ui.js';
import { state } from './state.js';
import { refreshLocalAiStatus } from './local_ai.js';
import { t } from './i18n.js';

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
    label.textContent = t('models.not.reachable', { provider: providerName });
    label.style.color = '#c00';
    listEl.textContent = '';
    const p = document.createElement('p');
    p.style.color = '#888';
    p.style.fontSize = '13px';
    p.textContent = t('models.start.and.refresh', { provider: providerName });
    listEl.appendChild(p);
    selectEl.textContent = '';
    const autoOption = document.createElement('option');
    autoOption.value = '';
    autoOption.textContent = provider === 'ollama' ? t('models.auto.best') : t('models.select.model');
    selectEl.appendChild(autoOption);
    return;
  }

  dot.style.background = '#2a2';
  label.textContent = t('models.running', { provider: providerName });
  label.style.color = '#2a2';

  try {
    const models = await invoke('list_models', { provider, endpoint });
    if (!models || models.length === 0) {
      listEl.textContent = '';
      const p = document.createElement('p');
      p.style.color = '#888';
      p.style.fontSize = '13px';
      p.textContent = t('models.none.installed');
      listEl.appendChild(p);
      selectEl.textContent = '';
      const autoOption = document.createElement('option');
      autoOption.value = '';
      autoOption.textContent = provider === 'ollama' ? t('models.auto.best') : t('models.select.model');
      selectEl.appendChild(autoOption);
      return;
    }

    // Build table using DOM APIs (no innerHTML with string concatenation)
    listEl.textContent = '';
    const table = document.createElement('table');
    for (const m of models) {
      const tr = document.createElement('tr');

      const tdName = document.createElement('td');
      tdName.style.width = '60%';
      tdName.textContent = m.display_name;
      tr.appendChild(tdName);

      const tdSize = document.createElement('td');
      tdSize.textContent = formatSize(m.size_bytes);
      tr.appendChild(tdSize);

      const tdStatus = document.createElement('td');
      tdStatus.style.width = '80px';
      tdStatus.style.textAlign = 'right';
      if (selectedModel === m.id) {
        const span = document.createElement('span');
        span.style.color = '#0066cc';
        span.style.fontSize = '12px';
        span.textContent = t('models.selected');
        tdStatus.appendChild(span);
      }
      tr.appendChild(tdStatus);

      table.appendChild(tr);
    }
    listEl.appendChild(table);

    // Build select options using DOM APIs
    selectEl.textContent = '';
    const autoOption = document.createElement('option');
    autoOption.value = '';
    // Ollama supports auto-selection; LM Studio requires explicit model selection
    autoOption.textContent = provider === 'ollama' ? t('models.auto.best') : t('models.select.model');
    selectEl.appendChild(autoOption);

    for (const m of models) {
      const option = document.createElement('option');
      option.value = m.id;
      option.textContent = m.display_name;
      if (selectedModel === m.id) {
        option.selected = true;
      }
      selectEl.appendChild(option);
    }
  } catch (e) {
    listEl.textContent = '';
    const p = document.createElement('p');
    p.className = 'error';
    p.textContent = t('models.test.error', { error: String(e) });
    listEl.appendChild(p);
  }
}

export async function testModel() {
  const provider = document.getElementById('f-quick-engine').value || 'ollama';
  const endpoint = document.getElementById('f-quick-endpoint').value || getDefaultEndpoint(provider);
  const model = document.getElementById('f-quick-model-select').value;
  const resultEl = document.getElementById('test-result');
  const btn = document.getElementById('test-btn');
  if (!model) {
    resultEl.textContent = '';
    const span = document.createElement('span');
    span.className = 'error';
    span.textContent = t('models.select.first');
    resultEl.appendChild(span);
    return;
  }
  btn.disabled = true;
  btn.textContent = t('models.testing');
  resultEl.textContent = '';
  try {
    const result = await invoke('test_model', { provider, endpoint, model });
    const strong = document.createElement('strong');
    strong.textContent = String(result.model);
    resultEl.appendChild(strong);
    resultEl.appendChild(document.createTextNode(' (' + String(result.latency_ms) + 'ms)'));
    resultEl.appendChild(document.createElement('br'));
    resultEl.appendChild(document.createTextNode(String(result.translated)));
  } catch (e) {
    const span = document.createElement('span');
    span.className = 'error';
    span.textContent = t('models.test.error', { error: String(e) });
    resultEl.appendChild(span);
  }
  btn.disabled = false;
  btn.textContent = t('models.test.button');
}

// Re-export for backward compatibility
export { refreshLocalAiStatus };
