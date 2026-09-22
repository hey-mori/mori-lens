// Shared UI helpers used across all sections

import { t } from './i18n.js';

export function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export function formatSize(bytes) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(0) + ' MB';
  return bytes + ' B';
}

export function setStatus(msg, persistent = false) {
  const active = document.querySelector('.section.active');
  if (!active) return;
  const statusEl = active.querySelector('[id$="-status"]');
  if (statusEl) {
    statusEl.textContent = msg;
    if (!persistent) {
      setTimeout(() => { statusEl.textContent = ''; }, 3000);
    }
  }
}

export function formatAiStatus(provider, status, model, pullCommand, _lane) {
  const providerName = provider === 'lmstudio' ? 'LM Studio' : 'Ollama';
  if (status === 'unreachable') {
    return '<span style="color:#888">' + providerName + ' 연결 필요</span>';
  } else if (status === 'ready') {
    return '<span style="color:#2e7d32">✓</span> ' + escapeHtml(model || '');
  } else if (status === 'missing') {
    let result = '<span style="color:#c00">✗</span> ' + escapeHtml(model || '') + ' <span style="color:#c00">· 설치 안 됨</span>';
    if (pullCommand) {
      const safeCommand = escapeHtml(pullCommand);
      result += '<div style="margin-top:4px;font-family:monospace;font-size:11px;background:#f5f5f5;padding:6px 8px;border-radius:3px;display:flex;justify-content:space-between;align-items:center">';
      result += '<span>' + safeCommand + '</span>';
      result += '<button class="btn copy-pull-btn" data-command="' + safeCommand + '" style="padding:2px 8px;font-size:10px;margin-left:8px">복사</button>';
      result += '</div>';
    }
    return result;
  } else if (status === 'auto_ready') {
    return '<span style="color:#2e7d32">✓</span> Auto <span style="color:#888;font-size:11px">· 모델 사용 가능</span>';
  } else if (status === 'auto_no_models') {
    return '<span style="color:#f57c00">!</span> Auto <span style="color:#f57c00;font-size:11px">· 설치된 모델 없음</span>';
  } else if (status === 'error') {
    return '<span style="color:#c00">✗</span> ' + escapeHtml(model || '') + ' <span style="color:#c00">· 모델 목록 확인 불가</span>';
  } else {
    return '<span style="color:#888">알 수 없음</span>';
  }
}

export function truncateSafe(str, maxChars) {
  const chars = Array.from(str);
  if (chars.length <= maxChars) return str;
  return chars.slice(0, maxChars).join('') + '...';
}

export function populateSelect(id, values) {
  const sel = document.getElementById(id);
  if (!sel) return;
  const current = sel.value;
  sel.innerHTML = '<option value="">' + escapeHtml(t('common.filter.all')) + '</option>';
  for (const v of values) {
    sel.innerHTML += '<option value="' + escapeHtml(v) + '"' + (v === current ? ' selected' : '') + '>' + escapeHtml(v) + '</option>';
  }
}
