// Status page: loadStatus, togglePause, quitApp, toggleAutostart, refreshLocalAi

import { invoke } from './tauri.js';
import { escapeHtml, formatAiStatus, setStatus } from './ui.js';
import { state } from './state.js';
import { refreshLocalAiStatus } from './local_ai.js';
import { t } from './i18n.js';

export async function loadStatus() {
  try {
    const [diag, runtime, health] = await Promise.all([
      invoke('get_diagnostics'),
      invoke('get_runtime_status'),
      invoke('get_health_status'),
    ]);
    const localAi = state.cachedLocalAiStatus;
    const el = document.getElementById('status-content');
    const paused = diag && diag.paused;
    const pauseLabel = paused ? t('status.pause.resume') : t('status.pause.button');
    const pauseStyle = paused
      ? 'background:#2e7d32;color:white;border-color:#2e7d32'
      : 'background:#f57c00;color:white;border-color:#f57c00';
    let html = '<div style="margin-bottom:20px">';
    html += '<div style="font-size:12px;color:#888;margin-bottom:8px">' + escapeHtml(t('status.title')) + '</div>';
    html += '<div style="display:flex;gap:8px;align-items:center">';
    if (paused) {
      html += '<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:#f57c00"></span>';
      html += '<span style="color:#f57c00;font-size:13px">' + escapeHtml(t('status.paused')) + '</span>';
    } else {
      html += '<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:#2e7d32"></span>';
      html += '<span style="color:#2e7d32;font-size:13px">' + escapeHtml(t('status.running')) + '</span>';
    }
    html += '</div>';
    html += '<div style="display:flex;gap:8px;margin-top:12px">';
    html += '<button class="btn" id="pause-btn" style="padding:8px 20px;' + pauseStyle + '">' + escapeHtml(pauseLabel) + '</button>';
    html += '<button class="btn" id="quit-btn" style="padding:8px 20px;color:#c00;border-color:#c00">' + escapeHtml(t('status.quit')) + '</button>';
    html += '</div>';
    html += '</div>';

    // Local AI readiness section
    html += '<h2>' + escapeHtml(t('status.local.ai')) + '</h2>';
    html += '<table>';
    if (localAi) {
      const quickIcon = localAi.quick_reachable
        ? '<span style="color:#2e7d32">✓</span>'
        : '<span style="color:#c00">✗</span>';
      const quickEndpointText = localAi.quick_reachable ? t('status.connected') : t('status.not.connected');
      html += '<tr><td>Quick</td><td>' + quickIcon + ' ' + escapeHtml(quickEndpointText) + '</td></tr>';

      const quickModelStatus = formatAiStatus(localAi.quick_provider, localAi.quick_status, localAi.quick_model, localAi.quick_pull_command, 'quick');
      html += '<tr><td></td><td>' + quickModelStatus + '</td></tr>';

      const deepIcon = localAi.deep_reachable
        ? '<span style="color:#2e7d32">✓</span>'
        : '<span style="color:#c00">✗</span>';
      const deepEndpointText = localAi.deep_reachable ? t('status.connected') : t('status.not.connected');
      html += '<tr><td>Deep</td><td>' + deepIcon + ' ' + escapeHtml(deepEndpointText) + '</td></tr>';

      const deepModelStatus = formatAiStatus(localAi.deep_provider, localAi.deep_status, localAi.deep_model, localAi.deep_pull_command, 'deep');
      html += '<tr><td></td><td>' + deepModelStatus + '</td></tr>';
    } else if (state.localAiLoadState === 'ERROR') {
      html += '<tr><td>Quick</td><td><span style="color:#c00">' + escapeHtml(t('status.check.failed')) + '</span></td></tr>';
      html += '<tr><td>Deep</td><td><span style="color:#c00">' + escapeHtml(t('status.check.failed')) + '</span></td></tr>';
    } else {
      html += '<tr><td>Quick</td><td>' + escapeHtml(t('status.checking')) + '</td></tr>';
      html += '<tr><td>Deep</td><td>' + escapeHtml(t('status.checking')) + '</td></tr>';
    }
    html += '</table>';
    html += '<div style="margin-top:12px;display:flex;gap:8px">';
    html += '<button class="btn" id="refresh-local-ai-btn" style="padding:6px 12px;font-size:12px">' + escapeHtml(t('status.refresh')) + '</button>';
    html += '<button class="btn" id="run-health-check-btn" style="padding:6px 12px;font-size:12px">' + escapeHtml(t('status.health.check')) + '</button>';
    html += '</div>';
    html += '<div id="health-check-result" style="margin-top:12px;display:none"></div>';

    // Cached health status from last health check
    if (health && (health.quick.state !== 'unknown' || health.deep.state !== 'unknown')) {
      html += '<div style="margin-top:12px;padding:12px;background:rgba(0,0,0,0.02);border-radius:4px;font-size:12px">';
      html += '<div style="color:#888;font-size:11px;margin-bottom:8px">' + escapeHtml(t('status.last.health')) + '</div>';
      const quickLabel = health.quick.state === 'ready' ? t('status.ready') :
                         health.quick.state === 'degraded' ? t('status.degraded') :
                         health.quick.state === 'running' ? '⟳ ' + t('status.running') : t('status.unavailable');
      const quickColor = health.quick.state === 'ready' ? '#2e7d32' :
                         health.quick.state === 'degraded' ? '#f57c00' :
                         health.quick.state === 'running' ? '#1976d2' : '#c00';
      html += '<div style="margin-bottom:4px"><strong>Quick:</strong> <span style="color:' + quickColor + '">' + quickLabel + '</span>';
      if (health.quick.failure) html += ' <span style="color:#888">(' + health.quick.failure + ')</span>';
      if (health.quick.latency_ms) html += ' <span style="color:#888">(' + health.quick.latency_ms + 'ms)</span>';
      html += '</div>';
      const deepLabel = health.deep.state === 'ready' ? t('status.ready') :
                        health.deep.state === 'degraded' ? t('status.degraded') :
                        health.deep.state === 'running' ? '⟳ ' + t('status.running') : t('status.unavailable');
      const deepColor = health.deep.state === 'ready' ? '#2e7d32' :
                        health.deep.state === 'degraded' ? '#f57c00' :
                        health.deep.state === 'running' ? '#1976d2' : '#c00';
      html += '<div><strong>Deep:</strong> <span style="color:' + deepColor + '">' + deepLabel + '</span>';
      if (health.deep.failure) html += ' <span style="color:#888">(' + health.deep.failure + ')</span>';
      if (health.deep.latency_ms) html += ' <span style="color:#888">(' + health.deep.latency_ms + 'ms)</span>';
      html += '</div>';
      if (health.stale) {
        html += '<div style="color:#f57c00;font-size:11px;margin-top:8px">' + escapeHtml(t('status.settings.changed')) + '</div>';
      }
      html += '</div>';
    }

    // Runtime configuration table
    if (runtime) {
      html += '<h2>' + escapeHtml(t('status.runtime')) + '</h2><table>';
      html += '<tr><td>History</td><td>' + (runtime.history_enabled ? escapeHtml(t('status.history.enabled', { days: runtime.history_days })) : escapeHtml(t('status.history.disabled'))) + '</td></tr>';
      html += '<tr><td>Library</td><td>' + (runtime.library_persistent ? escapeHtml(t('status.library.persistent')) : escapeHtml(t('status.library.session'))) + '</td></tr>';
      var autostartLabel;
      var autostartToggle = '';
      if (runtime.autostart_enabled === null) {
        autostartLabel = t('status.autostart.not.supported');
      } else {
        autostartLabel = runtime.autostart_enabled ? t('status.autostart.enabled') : t('status.autostart.disabled');
        var nextLabel = runtime.autostart_enabled ? t('status.autostart.enabled') : t('status.autostart.disabled');
        autostartToggle = ' <button class="btn" id="autostart-toggle-btn" style="padding:2px 10px;font-size:0.85em">' + escapeHtml(t('status.autostart.toggle', { state: nextLabel })) + '</button>';
      }
      html += '<tr><td>' + escapeHtml(t('status.autostart.label')) + '</td><td>' + escapeHtml(autostartLabel) + autostartToggle + '</td></tr>';
      // Restart status
      if (runtime.restart_required) {
        var reasonLabels = {
          'quick_engine': t('status.restart.reason.quick.engine'),
          'quick_model': t('status.restart.reason.quick.model'),
          'quick_endpoint': t('status.restart.reason.quick.endpoint'),
          'deep_engine': t('status.restart.reason.deep.engine'),
          'deep_model': t('status.restart.reason.deep.model'),
          'deep_endpoint': t('status.restart.reason.deep.endpoint'),
          'ocr_mode': t('status.restart.reason.ocr.mode')
        };
        var mappedReasons = runtime.restart_reasons.map(function(key) {
          return reasonLabels[key] || key;
        });
        var reasonText = mappedReasons.length > 0 ? ' (' + mappedReasons.join(', ') + ')' : '';
        html += '<tr><td style="color:#f57c00;font-weight:500">' + escapeHtml(t('status.restart.required')) + reasonText + '</td><td style="color:#f57c00">' + escapeHtml(t('status.restart.hint')) + '</td></tr>';
      } else {
        html += '<tr><td>' + escapeHtml(t('status.restart.required')) + '</td><td style="color:#2e7d32">' + escapeHtml(t('status.restart.not.required')) + '</td></tr>';
      }
      html += '</table>';
    }
    if (!diag || !diag.build) {
      html += '<p class="loading">' + escapeHtml(t('status.no.session.data')) + '</p>';
      el.innerHTML = html;
      return;
    }
    html += '<div style="margin-bottom:16px">';
    html += '<span class="stat"><span class="stat-value">' + diag.cards + '</span> <span class="stat-label">' + escapeHtml(t('status.cards')) + '</span></span>';
    html += '<span class="stat"><span class="stat-value">' + diag.settled_hovers + '</span> <span class="stat-label">' + escapeHtml(t('status.settled.hovers')) + '</span></span>';
    html += '</div>';
    if (diag.refusals && diag.refusals.length > 0) {
      html += '<h2>' + escapeHtml(t('status.refusals')) + '</h2><table>';
      for (const [reason, count] of diag.refusals) {
        const pct = diag.settled_hovers > 0 ? Math.round(count / diag.settled_hovers * 100) : 0;
        html += '<tr class="refusal-row"><td>' + reason + '</td><td>' + count + ' (' + pct + '%)</td></tr>';
      }
      html += '</table>';
    }
    if (diag.configuration && diag.configuration.length > 0) {
      html += '<h2>' + escapeHtml(t('status.configuration')) + '</h2><table>';
      for (const [label, value] of diag.configuration) {
        html += '<tr><td>' + label + '</td><td>' + value + '</td></tr>';
      }
      html += '</table>';
    }
    // Recent diagnostic events
    html += '<div id="recent-diagnostics-section" style="margin-top:16px"></div>';
    // Diagnostics export button
    html += '<div style="margin-top:20px;padding-top:16px;border-top:1px solid #eee">';
    html += '<button class="btn" id="export-diagnostics-btn" style="padding:8px 16px;font-size:12px">' + escapeHtml(t('status.view.diagnostics')) + '</button>';
    html += '<p style="font-size:11px;color:#888;margin-top:8px">' + escapeHtml(t('status.diagnostics.hint')) + '</p>';
    html += '<div id="diagnostics-result" style="margin-top:12px;display:none">';
    html += '<textarea id="diagnostics-json" readonly style="width:100%;height:200px;font-family:monospace;font-size:11px;padding:8px;border:1px solid #ddd;border-radius:4px;resize:vertical"></textarea>';
    html += '<button class="btn" id="copy-diagnostics-btn" style="margin-top:8px;padding:6px 12px;font-size:11px">' + escapeHtml(t('status.copy.to.clipboard')) + '</button>';
    html += '</div>';
    html += '</div>';
    // Close hint
    html += '<p style="font-size:11px;color:#999;margin-top:20px">' + escapeHtml(t('status.tray.hint')) + '</p>';
    el.innerHTML = html;
    // Attach event listeners after rendering
    attachStatusHandlers();
  } catch (e) {
    console.error('loadStatus failed:', e);
  }
}

function attachStatusHandlers() {
  const pauseBtn = document.getElementById('pause-btn');
  if (pauseBtn) pauseBtn.addEventListener('click', togglePause);

  const quitBtn = document.getElementById('quit-btn');
  if (quitBtn) quitBtn.addEventListener('click', quitApp);

  const refreshBtn = document.getElementById('refresh-local-ai-btn');
  if (refreshBtn) refreshBtn.addEventListener('click', refreshLocalAi);

  const autostartBtn = document.getElementById('autostart-toggle-btn');
  if (autostartBtn) autostartBtn.addEventListener('click', toggleAutostart);

  const exportBtn = document.getElementById('export-diagnostics-btn');
  if (exportBtn) exportBtn.addEventListener('click', () => {
    import('./diagnostics.js').then(m => m.exportDiagnostics());
  });

  const copyBtn = document.getElementById('copy-diagnostics-btn');
  if (copyBtn) copyBtn.addEventListener('click', function() {
    import('./diagnostics.js').then(m => m.copyDiagnostics(this));
  });

  const healthCheckBtn = document.getElementById('run-health-check-btn');
  if (healthCheckBtn) healthCheckBtn.addEventListener('click', runHealthCheck);

  // Load recent diagnostic events
  loadRecentDiagnostics();

  // Pull command copy buttons
  import('./diagnostics.js').then(m => m.attachPullCommandHandlers());
}

export async function refreshLocalAi() {
  await refreshLocalAiStatus();
  await loadStatus();
}

async function runHealthCheck() {
  const resultEl = document.getElementById('health-check-result');
  const btn = document.getElementById('run-health-check-btn');
  if (!resultEl || !btn) return;

  btn.disabled = true;
  btn.textContent = t('status.health.running');
  resultEl.style.display = 'block';
  resultEl.innerHTML = '<p style="color:#888;font-size:12px">' + escapeHtml(t('status.health.check.running')) + '</p>';

  try {
    const result = await invoke('run_health_check');
    const quickLabel = result.quick_state === 'ready' ? t('status.ready') :
                       result.quick_state === 'degraded' ? t('status.degraded') : t('status.unavailable');
    const deepLabel = result.deep_state === 'ready' ? t('status.ready') :
                      result.deep_state === 'degraded' ? t('status.degraded') : t('status.unavailable');
    const quickColor = result.quick_state === 'ready' ? '#2e7d32' :
                       result.quick_state === 'degraded' ? '#f57c00' : '#c00';
    const deepColor = result.deep_state === 'ready' ? '#2e7d32' :
                      result.deep_state === 'degraded' ? '#f57c00' : '#c00';

    let html = '<div style="font-size:12px;padding:12px;background:rgba(0,0,0,0.02);border-radius:4px">';
    html += '<div style="margin-bottom:8px"><strong>Quick:</strong> <span style="color:' + quickColor + '">' + quickLabel + '</span>';
    if (result.quick_failure) html += ' <span style="color:#888">(' + result.quick_failure + ')</span>';
    html += '</div>';
    html += '<div style="margin-bottom:8px"><strong>Deep:</strong> <span style="color:' + deepColor + '">' + escapeHtml(deepLabel) + '</span>';
    if (result.deep_failure) html += ' <span style="color:#888">(' + escapeHtml(result.deep_failure) + ')</span>';
    html += '</div>';
    html += '<div style="color:#888;font-size:11px">' + escapeHtml(t('status.health.check.elapsed', { ms: result.latency_ms })) + '</div>';
    html += '</div>';
    resultEl.innerHTML = html;

    // Refresh the status page to update the event ring display
    await loadRecentDiagnostics();
  } catch (e) {
    resultEl.innerHTML = '<p style="color:#c00;font-size:12px">' + escapeHtml(t('status.health.failed', { error: String(e) })) + '</p>';
  } finally {
    btn.disabled = false;
    btn.textContent = t('status.health.check');
  }
}

async function loadRecentDiagnostics() {
  const section = document.getElementById('recent-diagnostics-section');
  if (!section) return;

  try {
    const events = await invoke('get_diagnostic_events');
    if (!events || events.length === 0) {
      section.innerHTML = '';
      return;
    }

    const componentKeys = {
      runtime: 'status.component.runtime',
      acquisition: 'status.component.acquisition',
      library: 'status.component.library',
      settings: 'status.component.settings',
      shortcut: 'status.component.shortcut',
      health_check: 'status.component.health.check'
    };
    // Quick, Deep, OCR are product/protocol names, intentionally nonlocalized
    const componentPassthrough = { quick: 'Quick', deep: 'Deep', ocr: 'OCR' };

    const kindKeys = {
      failure: 'status.kind.failure',
      degraded: 'status.kind.degraded',
      recovered: 'status.kind.recovered',
      health_check: 'status.kind.health.check'
    };

    const failureKeys = {
      connection_failed: 'status.failure.connection.failed',
      timeout: 'status.failure.timeout',
      http_server: 'status.failure.http.server',
      http_client: 'status.failure.http.client',
      model_missing: 'status.failure.model.missing',
      malformed: 'status.failure.malformed',
      inference_failed: 'status.failure.inference.failed',
      settings_invalid: 'status.failure.settings.invalid',
      library_degraded: 'status.failure.library.degraded',
      shortcut_unavailable: 'status.failure.shortcut.unavailable',
      worker_stopped: 'status.failure.worker.stopped'
    };

    let html = '<h2 style="font-size:14px;margin-bottom:8px">' + escapeHtml(t('status.recent.events')) + '</h2>';
    html += '<div style="font-size:11px;color:#888;margin-bottom:8px">' + escapeHtml(t('status.recent.events.hint')) + '</div>';
    html += '<table style="font-size:12px">';
    // Show most recent first
    for (let i = events.length - 1; i >= 0; i--) {
      const e = events[i];
      const comp = componentKeys[e.component] ? t(componentKeys[e.component]) : (componentPassthrough[e.component] || e.component);
      const kind = kindKeys[e.kind] ? t(kindKeys[e.kind]) : e.kind;
      const failure = e.failure_class ? (failureKeys[e.failure_class] ? t(failureKeys[e.failure_class]) : e.failure_class) : '';
      const count = e.count > 1 ? ' (×' + e.count + ')' : '';
      const color = e.kind === 'failure' ? '#c00' :
                    e.kind === 'degraded' ? '#f57c00' :
                    e.kind === 'recovered' ? '#2e7d32' : '#555';
      html += '<tr>';
      html += '<td style="color:' + color + ';padding:4px 8px 4px 0">' + kind + '</td>';
      html += '<td style="padding:4px 8px 4px 0">' + comp + '</td>';
      html += '<td style="padding:4px 8px 4px 0">' + failure + count + '</td>';
      html += '<td style="color:#888;padding:4px 0">' + e.elapsed_ms + 'ms</td>';
      html += '</tr>';
    }
    html += '</table>';
    section.innerHTML = html;
  } catch (e) {
    console.error('loadRecentDiagnostics failed:', e);
    section.innerHTML = '';
  }
}

async function togglePause() {
  try {
    await invoke('request_pause_toggle');
    setTimeout(loadStatus, 200);
  } catch (e) {
    console.error('togglePause failed:', e);
  }
}

async function quitApp() {
  try {
    await invoke('request_quit');
  } catch (e) {
    console.error('quitApp failed:', e);
  }
}

async function toggleAutostart() {
  try {
    var runtime = await invoke('get_runtime_status');
    if (runtime.autostart_enabled === null) return;
    await invoke('set_autostart', { enabled: !runtime.autostart_enabled });
    setTimeout(loadStatus, 200);
  } catch (e) {
    console.error('toggleAutostart failed:', e);
  }
}
