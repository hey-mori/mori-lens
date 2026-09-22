// Diagnostics / About page

import { invoke } from './tauri.js';
import { escapeHtml } from './ui.js';
import { t } from './i18n.js';

export async function loadAbout() {
  try {
    const diag = await invoke('get_diagnostics');
    const el = document.getElementById('about-content');
    el.innerHTML = '<table>' +
      '<tr><td>Version</td><td>' + (diag.version || 'unknown') + '</td></tr>' +
      '<tr><td>Build</td><td>' + (diag.build || 'unknown') + '</td></tr>' +
      '<tr><td>Data location</td><td>%APPDATA%\\Mori Lens\\</td></tr>' +
      '</table>';
  } catch (e) {
    console.error('loadAbout failed:', e);
  }
}

export async function exportDiagnostics() {
  try {
    var json = await invoke('export_diagnostics');
    var textarea = document.getElementById('diagnostics-json');
    var resultDiv = document.getElementById('diagnostics-result');
    if (textarea && resultDiv) {
      textarea.value = json;
      resultDiv.style.display = 'block';
    }
  } catch (e) {
    console.error('exportDiagnostics failed:', e);
    alert(t('diagnostics.export.error', { error: e }));
  }
}

export async function copyDiagnostics(btn) {
  try {
    var textarea = document.getElementById('diagnostics-json');
    if (!textarea) return;
    textarea.select();
    textarea.setSelectionRange(0, 99999);
    var copied = document.execCommand('copy');
    var originalText = btn.textContent;
    if (copied) {
      btn.textContent = t('diagnostics.copy.success');
      btn.style.background = '#2e7d32';
      btn.style.color = 'white';
    } else {
      btn.textContent = t('diagnostics.copy.failure');
      btn.style.background = '#c62828';
      btn.style.color = 'white';
    }
    setTimeout(function() {
      btn.textContent = originalText;
      btn.style.background = '';
      btn.style.color = '';
    }, 2000);
  } catch (e) {
    console.error('copyDiagnostics failed:', e);
    alert(t('diagnostics.clipboard.failure', { error: e }));
  }
}

export async function copyPullCommand(button, command) {
  try {
    await navigator.clipboard.writeText(command);
    const originalText = button.textContent;
    button.textContent = t('diagnostics.copied.short');
    button.style.background = '#2e7d32';
    button.style.color = 'white';
    setTimeout(() => {
      button.textContent = originalText;
      button.style.background = '';
      button.style.color = '';
    }, 1500);
  } catch (e) {
    console.error('Copy failed:', e);
    button.textContent = t('diagnostics.failed.short');
    setTimeout(() => {
      button.textContent = t('diagnostics.copy.button');
    }, 1500);
  }
}

export function attachPullCommandHandlers() {
  document.querySelectorAll('.copy-pull-btn').forEach(button => {
    button.addEventListener('click', function() {
      const command = this.getAttribute('data-command');
      copyPullCommand(this, command);
    });
  });
}

export async function secureWipe() {
  if (!confirm(t('diagnostics.delete.confirm.1'))) {
    return;
  }
  if (!confirm(t('diagnostics.delete.confirm.2'))) {
    return;
  }
  try {
    await invoke('library_secure_wipe');
    alert(t('diagnostics.delete.success'));
    import('./library.js').then(m => m.loadLibrary());
  } catch (e) {
    alert(t('diagnostics.delete.error', { error: e }));
  }
}
