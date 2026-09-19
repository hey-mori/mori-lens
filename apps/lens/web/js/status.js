// Status page: loadStatus, togglePause, quitApp, toggleAutostart, refreshLocalAi

import { invoke } from './tauri.js';
import { escapeHtml, formatAiStatus, setStatus } from './ui.js';
import { state } from './state.js';
import { refreshLocalAiStatus } from './local_ai.js';

export async function loadStatus() {
  try {
    const [diag, runtime] = await Promise.all([
      invoke('get_diagnostics'),
      invoke('get_runtime_status'),
    ]);
    const localAi = state.cachedLocalAiStatus;
    const el = document.getElementById('status-content');
    const paused = diag && diag.paused;
    const pauseLabel = paused ? '다시 시작' : '일시정지';
    const pauseStyle = paused
      ? 'background:#2e7d32;color:white;border-color:#2e7d32'
      : 'background:#f57c00;color:white;border-color:#f57c00';
    let html = '<div style="margin-bottom:20px">';
    html += '<div style="font-size:12px;color:#888;margin-bottom:8px">상태</div>';
    html += '<div style="display:flex;gap:8px;align-items:center">';
    if (paused) {
      html += '<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:#f57c00"></span>';
      html += '<span style="color:#f57c00;font-size:13px">일시정지됨</span>';
    } else {
      html += '<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:#2e7d32"></span>';
      html += '<span style="color:#2e7d32;font-size:13px">실행 중</span>';
    }
    html += '</div>';
    html += '<div style="display:flex;gap:8px;margin-top:12px">';
    html += '<button class="btn" id="pause-btn" style="padding:8px 20px;' + pauseStyle + '">' + pauseLabel + '</button>';
    html += '<button class="btn" id="quit-btn" style="padding:8px 20px;color:#c00;border-color:#c00">Mori Lens 종료</button>';
    html += '</div>';
    html += '</div>';

    // Local AI readiness section
    html += '<h2>로컬 AI</h2>';
    html += '<table>';
    if (localAi) {
      const quickIcon = localAi.quick_reachable
        ? '<span style="color:#2e7d32">✓</span>'
        : '<span style="color:#c00">✗</span>';
      const quickEndpointText = localAi.quick_reachable ? '연결됨' : '연결 안 됨';
      html += '<tr><td>Quick</td><td>' + quickIcon + ' ' + quickEndpointText + '</td></tr>';

      const quickModelStatus = formatAiStatus(localAi.quick_provider, localAi.quick_status, localAi.quick_model, localAi.quick_pull_command, 'quick');
      html += '<tr><td></td><td>' + quickModelStatus + '</td></tr>';

      const deepIcon = localAi.deep_reachable
        ? '<span style="color:#2e7d32">✓</span>'
        : '<span style="color:#c00">✗</span>';
      const deepEndpointText = localAi.deep_reachable ? '연결됨' : '연결 안 됨';
      html += '<tr><td>Deep</td><td>' + deepIcon + ' ' + deepEndpointText + '</td></tr>';

      const deepModelStatus = formatAiStatus(localAi.deep_provider, localAi.deep_status, localAi.deep_model, localAi.deep_pull_command, 'deep');
      html += '<tr><td></td><td>' + deepModelStatus + '</td></tr>';
    } else if (state.localAiLoadState === 'ERROR') {
      html += '<tr><td>Quick</td><td><span style="color:#c00">상태 확인 실패</span></td></tr>';
      html += '<tr><td>Deep</td><td><span style="color:#c00">상태 확인 실패</span></td></tr>';
    } else {
      html += '<tr><td>Quick</td><td>확인 중...</td></tr>';
      html += '<tr><td>Deep</td><td>확인 중...</td></tr>';
    }
    html += '</table>';
    html += '<div style="margin-top:12px;display:flex;gap:8px">';
    html += '<button class="btn" id="refresh-local-ai-btn" style="padding:6px 12px;font-size:12px">새로고침</button>';
    html += '<button class="btn" id="run-health-check-btn" style="padding:6px 12px;font-size:12px">헬스체크 실행</button>';
    html += '</div>';
    html += '<div id="health-check-result" style="margin-top:12px;display:none"></div>';

    // Runtime configuration table
    if (runtime) {
      html += '<h2>런타임</h2><table>';
      html += '<tr><td>History</td><td>' + (runtime.history_enabled ? '사용 중 (' + runtime.history_days + '일)' : '사용 안 함') + '</td></tr>';
      html += '<tr><td>Library</td><td>' + (runtime.library_persistent ? '저장됨' : '세션만') + '</td></tr>';
      var autostartLabel;
      var autostartToggle = '';
      if (runtime.autostart_enabled === null) {
        autostartLabel = '지원 안 함';
      } else {
        autostartLabel = runtime.autostart_enabled ? '사용' : '사용 안 함';
        var nextLabel = runtime.autostart_enabled ? '사용' : '사용 안 함';
        autostartToggle = ' <button class="btn" id="autostart-toggle-btn" style="padding:2px 10px;font-size:0.85em">' + nextLabel + '로 전환</button>';
      }
      html += '<tr><td>Windows 시작</td><td>' + autostartLabel + autostartToggle + '</td></tr>';
      // Restart status
      if (runtime.restart_required) {
        var reasonLabels = {
          'quick_engine': 'Quick 엔진',
          'quick_model': 'Quick 모델',
          'quick_endpoint': 'Quick 엔드포인트',
          'deep_engine': 'Deep 엔진',
          'deep_model': 'Deep 모델',
          'deep_endpoint': 'Deep 엔드포인트',
          'ocr_mode': 'OCR 모드'
        };
        var mappedReasons = runtime.restart_reasons.map(function(key) {
          return reasonLabels[key] || key;
        });
        var reasonText = mappedReasons.length > 0 ? ' (' + mappedReasons.join(', ') + ')' : '';
        html += '<tr><td style="color:#f57c00;font-weight:500">재시작 필요' + reasonText + '</td><td style="color:#f57c00">변경사항을 적용하려면 Mori Lens를 다시 시작하세요</td></tr>';
      } else {
        html += '<tr><td>재시작 상태</td><td style="color:#2e7d32">필요 없음</td></tr>';
      }
      html += '</table>';
    }
    if (!diag || !diag.build) {
      html += '<p class="loading">세션 데이터가 아직 없습니다.</p>';
      el.innerHTML = html;
      return;
    }
    html += '<div style="margin-bottom:16px">';
    html += '<span class="stat"><span class="stat-value">' + diag.cards + '</span> <span class="stat-label">cards</span></span>';
    html += '<span class="stat"><span class="stat-value">' + diag.settled_hovers + '</span> <span class="stat-label">settled hovers</span></span>';
    html += '</div>';
    if (diag.refusals && diag.refusals.length > 0) {
      html += '<h2>Refusals</h2><table>';
      for (const [reason, count] of diag.refusals) {
        const pct = diag.settled_hovers > 0 ? Math.round(count / diag.settled_hovers * 100) : 0;
        html += '<tr class="refusal-row"><td>' + reason + '</td><td>' + count + ' (' + pct + '%)</td></tr>';
      }
      html += '</table>';
    }
    if (diag.configuration && diag.configuration.length > 0) {
      html += '<h2>Configuration</h2><table>';
      for (const [label, value] of diag.configuration) {
        html += '<tr><td>' + label + '</td><td>' + value + '</td></tr>';
      }
      html += '</table>';
    }
    // Recent diagnostic events
    html += '<div id="recent-diagnostics-section" style="margin-top:16px"></div>';
    // Diagnostics export button
    html += '<div style="margin-top:20px;padding-top:16px;border-top:1px solid #eee">';
    html += '<button class="btn" id="export-diagnostics-btn" style="padding:8px 16px;font-size:12px">진단 정보 보기</button>';
    html += '<p style="font-size:11px;color:#888;margin-top:8px">버그 보고용 진단 정보를 표시합니다. 캡처된 텍스트는 포함되지 않습니다.</p>';
    html += '<div id="diagnostics-result" style="margin-top:12px;display:none">';
    html += '<textarea id="diagnostics-json" readonly style="width:100%;height:200px;font-family:monospace;font-size:11px;padding:8px;border:1px solid #ddd;border-radius:4px;resize:vertical"></textarea>';
    html += '<button class="btn" id="copy-diagnostics-btn" style="margin-top:8px;padding:6px 12px;font-size:11px">클립보드에 복사</button>';
    html += '</div>';
    html += '</div>';
    // Close hint
    html += '<p style="font-size:11px;color:#999;margin-top:20px">이 창을 닫아도 Mori Lens는 시스템 트레이에서 계속 실행됩니다.</p>';
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
  btn.textContent = '실행 중...';
  resultEl.style.display = 'block';
  resultEl.innerHTML = '<p style="color:#888;font-size:12px">헬스체크 실행 중...</p>';

  try {
    const result = await invoke('run_health_check');
    const quickLabel = result.quick_state === 'ready' ? '✓ 준비됨' :
                       result.quick_state === 'degraded' ? '⚠ 성능 저하' : '✗ 사용 불가';
    const deepLabel = result.deep_state === 'ready' ? '✓ 준비됨' :
                      result.deep_state === 'degraded' ? '⚠ 성능 저하' : '✗ 사용 불가';
    const quickColor = result.quick_state === 'ready' ? '#2e7d32' :
                       result.quick_state === 'degraded' ? '#f57c00' : '#c00';
    const deepColor = result.deep_state === 'ready' ? '#2e7d32' :
                      result.deep_state === 'degraded' ? '#f57c00' : '#c00';

    let html = '<div style="font-size:12px;padding:12px;background:rgba(0,0,0,0.02);border-radius:4px">';
    html += '<div style="margin-bottom:8px"><strong>Quick:</strong> <span style="color:' + quickColor + '">' + quickLabel + '</span>';
    if (result.quick_failure) html += ' <span style="color:#888">(' + result.quick_failure + ')</span>';
    html += '</div>';
    html += '<div style="margin-bottom:8px"><strong>Deep:</strong> <span style="color:' + deepColor + '">' + deepLabel + '</span>';
    if (result.deep_failure) html += ' <span style="color:#888">(' + result.deep_failure + ')</span>';
    html += '</div>';
    html += '<div style="color:#888;font-size:11px">소요 시간: ' + result.latency_ms + 'ms</div>';
    html += '</div>';
    resultEl.innerHTML = html;

    // Refresh the status page to update the event ring display
    await loadRecentDiagnostics();
  } catch (e) {
    resultEl.innerHTML = '<p style="color:#c00;font-size:12px">헬스체크 실패: ' + escapeHtml(String(e)) + '</p>';
  } finally {
    btn.disabled = false;
    btn.textContent = '헬스체크 실행';
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

    const componentLabels = {
      runtime: '런타임',
      acquisition: '텍스트 수집',
      quick: 'Quick',
      deep: 'Deep',
      ocr: 'OCR',
      library: '라이브러리',
      settings: '설정',
      shortcut: '단축키',
      health_check: '헬스체크'
    };

    const kindLabels = {
      failure: '실패',
      degraded: '성능 저하',
      recovered: '복구',
      health_check: '헬스체크'
    };

    const failureLabels = {
      connection_failed: '연결 실패',
      timeout: '시간 초과',
      http_server: 'HTTP 서버 오류',
      http_client: 'HTTP 클라이언트 오류',
      model_missing: '모델 없음',
      malformed: '응답 형식 오류',
      inference_failed: '추론 실패',
      settings_invalid: '설정 오류',
      library_degraded: '라이브러리 성능 저하',
      shortcut_unavailable: '단축키 사용 불가',
      worker_stopped: '워커 중단'
    };

    let html = '<h2 style="font-size:14px;margin-bottom:8px">최근 진단 이벤트</h2>';
    html += '<div style="font-size:11px;color:#888;margin-bottom:8px">세션 중 발생한 비정상 이벤트 (최대 128개)</div>';
    html += '<table style="font-size:12px">';
    // Show most recent first
    for (let i = events.length - 1; i >= 0; i--) {
      const e = events[i];
      const comp = componentLabels[e.component] || e.component;
      const kind = kindLabels[e.kind] || e.kind;
      const failure = e.failure_class ? (failureLabels[e.failure_class] || e.failure_class) : '';
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
