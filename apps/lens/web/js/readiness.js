// Provider readiness: compact readiness UI for the Status page.
//
// Uses `get_provider_readiness` (P20C) which returns the readiness domain
// types (LaneReadiness) plus ProviderAccessState, ModelInventoryView, and
// NextAction. The backend owns all provider-specific logic; the frontend
// renders what the backend says.

import { invoke } from './tauri.js';
import { state } from './state.js';
import { t } from './i18n.js';

/**
 * Lane readiness values from the backend.
 * These match lens_core::readiness::LaneReadiness snake_case serialization.
 */
export const LaneReadiness = {
  Unknown: 'unknown',
  RuntimeNotInstalled: 'runtime_not_installed',
  EndpointUnreachable: 'endpoint_unreachable',
  AuthenticationRequired: 'authentication_required',
  ProviderError: 'provider_error',
  NoCompatibleModel: 'no_compatible_model',
  ModelPresent: 'model_present',
  ModelDownloading: 'model_downloading',
  ModelLoading: 'model_loading',
  ModelFailed: 'model_failed',
  Ready: 'ready',
};

/**
 * Next action values from the backend.
 * These match lens_core::readiness::NextAction snake_case serialization.
 */
export const NextAction = {
  None: 'none',
  Refresh: 'refresh',
  Download: 'download',
  Load: 'load',
  ConfigureModel: 'configure_model',
  ResolveAuth: 'resolve_auth',
};

/**
 * Provider access state values from the backend.
 */
export const ProviderAccessState = {
  Available: 'available',
  Unknown: 'unknown',
  Unreachable: 'unreachable',
  AuthenticationRequired: 'authentication_required',
  ProviderError: 'provider_error',
};

/**
 * Normalize provider access state from backend wire format.
 *
 * Backend serializes variants with fields as objects: {"variant": {fields}}.
 * Simple variants serialize as strings. This function extracts the variant name
 * and preserves any associated data (e.g., reason).
 *
 * @param {string|object} accessState - Raw access state from backend
 * @returns {{kind: string, reason?: string}} Normalized access state with kind and optional reason
 */
export function normalizeAccessState(accessState) {
  if (typeof accessState === 'string') {
    return { kind: accessState };
  }
  if (typeof accessState === 'object' && accessState !== null) {
    // Object form: {"variant_name": {fields}}
    const keys = Object.keys(accessState);
    if (keys.length === 1) {
      const kind = keys[0];
      const data = accessState[kind];
      // Preserve reason field if present
      if (data && typeof data === 'object' && data.reason) {
        return { kind, reason: data.reason };
      }
      return { kind };
    }
  }
  return { kind: 'unknown' };
}

/**
 * Fetch provider readiness from the backend.
 *
 * Returns the ProviderReadinessView or null on error.
 * Updates state.cachedProviderReadiness.
 */
export async function refreshProviderReadiness() {
  try {
    state.cachedProviderReadiness = await invoke('get_provider_readiness');
    state.providerReadinessLoadState = 'LOADED';
  } catch (e) {
    console.error('refreshProviderReadiness failed:', e);
    state.cachedProviderReadiness = null;
    state.providerReadinessLoadState = 'ERROR';
  }
  return state.cachedProviderReadiness;
}

/**
 * Render the full readiness section for the Status page.
 *
 * Uses the backend's ProviderReadinessView to render both Quick and Deep
 * lanes with their readiness, access state, model inventory, and next action.
 *
 * @param {object} readiness - ProviderReadinessView from backend
 * @returns {string} HTML string for the readiness section
 */
export function renderReadinessSection(readiness) {
  let html = '<table>';
  if (!readiness) {
    html += '<tr><td>Quick</td><td>' + escapeHtml(t('status.checking')) + '</td></tr>';
    html += '<tr><td>Deep</td><td>' + escapeHtml(t('status.checking')) + '</td></tr>';
  } else {
    html += renderLaneRow('Quick', readiness.quick_lane, readiness.quick_provider,
      readiness.quick_model, readiness.quick_effective_model,
      readiness.quick_provider_access,
      readiness.quick_model_inventory, readiness.quick_next_action,
      readiness.download, readiness.quick_can_download, readiness.quick_can_load,
      readiness.quick_supports_progress, readiness.quick_download_target);
    // Deep lane: status-only in P20C. No action button rendered.
    // If deep_provider is null, the configuration is unsupported.
    html += renderDeepLaneRow(readiness);
  }
  html += '</table>';

  html += '<div style="margin-top:12px;display:flex;gap:8px">';
  html += '<button class="btn" id="refresh-local-ai-btn" style="padding:6px 12px;font-size:12px">' + escapeHtml(t('status.refresh')) + '</button>';
  html += '<button class="btn" id="run-health-check-btn" style="padding:6px 12px;font-size:12px">' + escapeHtml(t('status.health.check')) + '</button>';
  html += '</div>';
  html += '<div id="health-check-result" style="margin-top:12px;display:none"></div>';

  return html;
}

/**
 * Render the Deep lane row, handling unsupported configuration.
 *
 * P20C: Deep is Ollama-only. If the user configured LM Studio or an invalid
 * token for Deep, deep_provider is null and deep_configuration_valid is false.
 * In that case, show "Unsupported configuration" instead of a fake provider name.
 */
function renderDeepLaneRow(readiness) {
  if (!readiness.deep_configuration_valid || !readiness.deep_provider) {
    // Unsupported configuration
    let html = '<tr>';
    html += '<td>Deep</td>';
    html += '<td>';
    html += '<span style="color:#c00">✗</span> ';
    html += '<span style="color:#c00">' + escapeHtml(t('status.readiness.unsupported.config')) + '</span>';
    html += '</td>';
    html += '</tr>';
    html += '<tr>';
    html += '<td></td>';
    html += '<td style="font-size:12px;color:#555">';
    html += escapeHtml(t('status.readiness.deep.ollama.only'));
    html += '</td>';
    html += '</tr>';
    return html;
  }
  // Valid configuration: use standard lane rendering
  return renderLaneRow('Deep', readiness.deep_lane, readiness.deep_provider,
    readiness.deep_model, null,
    readiness.deep_provider_access,
    readiness.deep_model_inventory, readiness.deep_next_action,
    null, readiness.deep_can_download, readiness.deep_can_load,
    readiness.deep_supports_progress, null);
}

/**
 * Render a single lane row with readiness, provider access, model info,
 * and action button.
 */
function renderLaneRow(laneName, laneReadiness, provider, configuredModel,
    effectiveModel, accessState, inventory, nextAction, download,
    canDownload, canLoad, supportsProgress, downloadTarget) {
  const providerLabel = provider === 'lmstudio' ? 'LM Studio' : 'Ollama';
  const { icon, label, color } = readinessDisplay(laneReadiness);
  // Display effective model if available, otherwise configured model, otherwise auto label
  const displayModel = effectiveModel || configuredModel;
  const modelText = displayModel || (provider === 'ollama' ? t('models.auto.best') : t('models.select.model'));

  let html = '<tr>';
  html += '<td>' + escapeHtml(laneName) + '</td>';
  html += '<td>';
  html += '<span style="color:' + color + '">' + icon + '</span> ';
  html += '<span style="color:' + color + '">' + escapeHtml(label) + '</span>';
  html += '<span style="color:#888;font-size:11px;margin-left:8px">' + escapeHtml(providerLabel) + '</span>';
  html += '</td>';
  html += '</tr>';

  // Model and access info row
  html += '<tr>';
  html += '<td></td>';
  html += '<td style="font-size:12px;color:#555">';
  html += escapeHtml(modelText);

  // Show auth error if applicable
  const normalizedAccess = normalizeAccessState(accessState);
  if (normalizedAccess.kind === ProviderAccessState.AuthenticationRequired) {
    html += ' <span style="color:#c00">(' + escapeHtml(t('status.readiness.auth.required')) + ')</span>';
  } else if (normalizedAccess.kind === ProviderAccessState.ProviderError) {
    html += ' <span style="color:#c00">(' + escapeHtml(t('status.readiness.error')) + ')</span>';
  }

  // Show inventory info
  if (inventory && inventory.available) {
    const count = inventory.available.models ? inventory.available.models.length : 0;
    html += ' <span style="color:#888">(' + count + ' ' + escapeHtml(t('status.readiness.models.installed')) + ')</span>';
  } else if (inventory && inventory.lookup_failed) {
    html += ' <span style="color:#f57c00">(' + escapeHtml(t('status.readiness.inventory.error')) + ')</span>';
  }

  html += '</td>';
  html += '</tr>';

  // Download progress row (if active for this lane)
  if (download && isDownloadForLane(download, provider, configuredModel, laneName)) {
    html += renderDownloadProgressRow(download, supportsProgress);
  }

  // Action row based on backend's next_action
  const actionHtml = renderNextActionButton(nextAction, provider, canDownload, canLoad);
  if (actionHtml) {
    html += '<tr>';
    html += '<td></td>';
    html += '<td style="padding-top:4px">' + actionHtml + '</td>';
    html += '</tr>';
  }

  return html;
}

/**
 * Render a lane readiness row (legacy interface for backward compat).
 *
 * @param {string} laneName - "Quick" or "Deep"
 * @param {string} readiness - LaneReadiness value
 * @param {string} provider - Provider name
 * @param {string|null} model - Configured model name
 * @param {object} actions - { canDownload, canLoad, supportsProgress }
 * @param {object|null} download - Download snapshot
 * @returns {string} HTML string for the row
 */
export function renderLaneReadiness(laneName, readiness, provider, model, actions, download) {
  const providerLabel = provider === 'lmstudio' ? 'LM Studio' : 'Ollama';
  const { icon, label, color } = readinessDisplay(readiness);
  const modelText = model ? model : (provider === 'ollama' ? t('models.auto.best') : t('models.select.model'));

  let html = '<tr>';
  html += '<td>' + escapeHtml(laneName) + '</td>';
  html += '<td>';
  html += '<span style="color:' + color + '">' + icon + '</span> ';
  html += '<span style="color:' + color + '">' + escapeHtml(label) + '</span>';
  html += '<span style="color:#888;font-size:11px;margin-left:8px">' + escapeHtml(providerLabel) + '</span>';
  html += '</td>';
  html += '</tr>';

  html += '<tr>';
  html += '<td></td>';
  html += '<td style="font-size:12px;color:#555">' + escapeHtml(modelText) + '</td>';
  html += '</tr>';

  if (download && isDownloadForLane(download, provider, model, laneName)) {
    html += renderDownloadProgressRow(download, actions.supportsProgress);
  }

  return html;
}

/**
 * Map LaneReadiness to display properties.
 */
function readinessDisplay(readiness) {
  switch (readiness) {
    case LaneReadiness.Ready:
      return { icon: '✓', label: t('status.ready'), color: '#2e7d32' };
    case LaneReadiness.RuntimeNotInstalled:
      return { icon: '✗', label: t('status.readiness.not.installed'), color: '#c00' };
    case LaneReadiness.EndpointUnreachable:
      return { icon: '✗', label: t('status.readiness.unreachable'), color: '#c00' };
    case LaneReadiness.AuthenticationRequired:
      return { icon: '🔒', label: t('status.readiness.auth.required'), color: '#c00' };
    case LaneReadiness.ProviderError:
      return { icon: '✗', label: t('status.readiness.error'), color: '#c00' };
    case LaneReadiness.NoCompatibleModel:
      return { icon: '!', label: t('status.readiness.no.model'), color: '#f57c00' };
    case LaneReadiness.ModelPresent:
      return { icon: '⟳', label: t('status.readiness.model.present'), color: '#1976d2' };
    case LaneReadiness.ModelDownloading:
      return { icon: '↓', label: t('status.readiness.downloading'), color: '#1976d2' };
    case LaneReadiness.ModelLoading:
      return { icon: '⟳', label: t('status.readiness.loading'), color: '#1976d2' };
    case LaneReadiness.ModelFailed:
      return { icon: '✗', label: t('status.readiness.failed'), color: '#c00' };
    case LaneReadiness.Unknown:
    default:
      return { icon: '?', label: t('status.checking'), color: '#888' };
  }
}

/**
 * Check if a download snapshot is for a specific lane.
 *
 * P20C: Download operations are Quick-only. Deep lane never shows download
 * progress, even if the provider matches.
 */
function isDownloadForLane(download, provider, model, laneName) {
  if (!download) return false;
  // P20C: Deep lane never shows download progress
  if (laneName === 'Deep') return false;
  if (download.provider !== provider) return false;
  if (model && download.model && download.model !== model) return false;
  return true;
}

/**
 * Render download progress row.
 *
 * DownloadStatus uses Rust's default externally-tagged serde representation:
 * - Unit variants (Starting, Completed, AlreadyPresent) serialize as strings
 * - Tuple variants (Downloading(progress), Paused(progress)) serialize as {"downloading": {...}}
 * - Failed(error) serializes as {"failed": "..."}
 *
 * DownloadProgress fields: bytes_downloaded, total_bytes, bytes_per_second,
 * estimated_completion_seconds. The percentage() method is NOT serialized.
 */
function renderDownloadProgressRow(download, supportsProgress) {
  const status = download.status;
  let html = '<tr>';
  html += '<td></td>';
  html += '<td style="font-size:12px">';

  // Check for object variants first (Downloading, Paused, Failed)
  if (status && typeof status === 'object' && status.downloading) {
    const dl = status.downloading;
    const totalBytes = dl.total_bytes != null ? dl.total_bytes : null;
    const bytesDownloaded = dl.bytes_downloaded != null ? dl.bytes_downloaded : null;
    const pct = (bytesDownloaded != null && totalBytes && totalBytes > 0)
      ? Math.round(bytesDownloaded / totalBytes * 100)
      : null;
    const transferred = bytesDownloaded != null ? formatBytes(bytesDownloaded) : null;
    const total = totalBytes != null ? formatBytes(totalBytes) : null;

    html += '<div style="display:flex;align-items:center;gap:8px">';
    html += '<span style="color:#1976d2">↓ ' + escapeHtml(t('status.readiness.downloading')) + '</span>';
    if (pct != null) {
      html += '<span style="color:#555">' + pct + '%</span>';
      html += '<div style="flex:1;height:4px;background:#eee;border-radius:2px;max-width:100px">';
      html += '<div style="width:' + pct + '%;height:100%;background:#1976d2;border-radius:2px"></div>';
      html += '</div>';
    }
    if (transferred) {
      html += '<span style="color:#888">' + transferred;
      if (total) html += ' / ' + total;
      html += '</span>';
    }
    html += '</div>';
  } else if (status && typeof status === 'object' && status.paused) {
    const dl = status.paused;
    const totalBytes = dl.total_bytes != null ? dl.total_bytes : null;
    const bytesDownloaded = dl.bytes_downloaded != null ? dl.bytes_downloaded : null;
    const pct = (bytesDownloaded != null && totalBytes && totalBytes > 0)
      ? Math.round(bytesDownloaded / totalBytes * 100)
      : null;
    const transferred = bytesDownloaded != null ? formatBytes(bytesDownloaded) : null;
    const total = totalBytes != null ? formatBytes(totalBytes) : null;

    html += '<div style="display:flex;align-items:center;gap:8px">';
    html += '<span style="color:#f57c00">⏸ ' + escapeHtml(t('status.readiness.paused')) + '</span>';
    if (pct != null) {
      html += '<span style="color:#555">' + pct + '%</span>';
      html += '<div style="flex:1;height:4px;background:#eee;border-radius:2px;max-width:100px">';
      html += '<div style="width:' + pct + '%;height:100%;background:#f57c00;border-radius:2px"></div>';
      html += '</div>';
    }
    if (transferred) {
      html += '<span style="color:#888">' + transferred;
      if (total) html += ' / ' + total;
      html += '</span>';
    }
    html += '</div>';
  } else if (status && typeof status === 'object' && status.failed) {
    html += '<span style="color:#c00">✗ ' + escapeHtml(t('status.readiness.failed')) + '</span>';
  }
  // Unit variants serialize as plain strings
  else if (status === 'starting') {
    html += '<span style="color:#1976d2">⟳ ' + escapeHtml(t('status.readiness.starting')) + '</span>';
  } else if (status === 'completed') {
    html += '<span style="color:#2e7d32">✓ ' + escapeHtml(t('status.readiness.completed')) + '</span>';
  } else if (status === 'already_present') {
    html += '<span style="color:#2e7d32">✓ ' + escapeHtml(t('status.readiness.already.present')) + '</span>';
  } else {
    html += '<span style="color:#888">' + escapeHtml(t('status.checking')) + '</span>';
  }

  html += '</td>';
  html += '</tr>';
  return html;
}

/**
 * Render action button based on the backend's NextAction.
 *
 * The backend computes what the user should do next; the frontend just
 * renders the appropriate button.
 *
 * P20C: ResolveAuth is rendered as guidance text (no credential UX yet).
 * ConfigureModel navigates to the Models page.
 */
function renderNextActionButton(nextAction, provider, canDownload, canLoad) {
  switch (nextAction) {
    case NextAction.Download:
      if (canDownload) {
        return '<button class="btn readiness-action-btn" data-action="download" data-provider="' + provider + '" style="padding:4px 10px;font-size:11px">' + escapeHtml(t('status.readiness.action.download')) + '</button>';
      }
      return '';
    case NextAction.Load:
      if (canLoad) {
        return '<button class="btn readiness-action-btn" data-action="load" data-provider="' + provider + '" style="padding:4px 10px;font-size:11px">' + escapeHtml(t('status.readiness.action.load')) + '</button>';
      }
      return '';
    case NextAction.ResolveAuth:
      // No credential UX established yet. Show guidance, not a fake button.
      return '<span style="font-size:11px;color:#c00">' + escapeHtml(t('status.readiness.auth.required')) + '</span>';
    case NextAction.ConfigureModel:
      return '<button class="btn readiness-action-btn" data-action="configure-model" style="padding:4px 10px;font-size:11px">' + escapeHtml(t('status.readiness.action.configure.model')) + '</button>';
    case NextAction.Refresh:
      return '<button class="btn readiness-action-btn" data-action="refresh" style="padding:4px 10px;font-size:11px">' + escapeHtml(t('status.readiness.action.retry')) + '</button>';
    case NextAction.None:
    default:
      return '';
  }
}

/**
 * Format bytes to human-readable string.
 */
function formatBytes(bytes) {
  if (bytes == null) return '';
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
}

/**
 * Escape HTML special characters.
 */
function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Current readiness action callbacks.
 * Updated by attachReadinessActionHandlers without re-attaching listeners.
 */
let currentReadinessCallbacks = {
  onDownloadStarted: () => {},
  onLoadStarted: () => {},
  onRefreshCompleted: () => {},
  onConfigureModel: () => {},
};

/**
 * Attach event handlers for readiness action buttons using event delegation.
 *
 * Uses a single delegated listener on the readiness region container.
 * This ensures handlers survive readiness DOM replacement and prevents
 * duplicate listener accumulation.
 *
 * Accepts explicit callbacks for action orchestration instead of using
 * global events. This ensures single-owner semantics and prevents cycles.
 *
 * @param {object} callbacks - Action completion callbacks
 * @param {function} callbacks.onDownloadStarted - Called after download starts (before polling)
 * @param {function} callbacks.onLoadStarted - Called after load completes (direct reconciliation)
 * @param {function} callbacks.onRefreshCompleted - Called after manual refresh completes
 * @param {function} callbacks.onConfigureModel - Called when configure-model action triggered
 */
export function attachReadinessActionHandlers(callbacks = {}) {
  // Update the current callbacks (used by the delegated listener)
  currentReadinessCallbacks = {
    onDownloadStarted: callbacks.onDownloadStarted || (() => {}),
    onLoadStarted: callbacks.onLoadStarted || (() => {}),
    onRefreshCompleted: callbacks.onRefreshCompleted || (() => {}),
    onConfigureModel: callbacks.onConfigureModel || (() => {}),
  };

  // Find the readiness region container (stable root for delegation)
  const readinessRegion = document.getElementById('status-readiness-region');
  if (!readinessRegion) return;

  // Only attach the delegated listener once
  if (readinessRegion._readinessClickHandlerAttached) return;

  // Create the delegated click handler
  const clickHandler = async (e) => {
    const btn = e.target.closest('.readiness-action-btn');
    if (!btn) return;

    e.preventDefault();
    const action = btn.dataset.action;
    const provider = btn.dataset.provider;

    btn.disabled = true;
    btn.textContent = t('status.readiness.action.working');

    try {
      if (action === 'download') {
        await invoke('start_quick_model_download');
        // Start operation controller polling immediately, not refreshProviderReadiness
        currentReadinessCallbacks.onDownloadStarted();
      } else if (action === 'load') {
        // Load is synchronous: wait for completion, then reconcile readiness directly
        // Do NOT start operation polling; load_lmstudio_instance is not an operation
        await invoke('load_lmstudio_instance');
        // Load completed successfully; trigger exactly one readiness reconciliation
        await refreshProviderReadiness();
        currentReadinessCallbacks.onLoadStarted();
      } else if (action === 'refresh') {
        // Refresh readiness from backend
        await refreshProviderReadiness();
        // Notify owner to re-render via explicit callback
        currentReadinessCallbacks.onRefreshCompleted();
        return; // Exactly-once: refresh action completes here
      } else if (action === 'configure-model') {
        // Navigate to the Models page
        if (window.showSection) {
          window.showSection('models');
        }
        currentReadinessCallbacks.onConfigureModel();
        return; // Don't refresh readiness; models page has its own loading
      }
      // For download, do NOT immediately refreshProviderReadiness.
      // The operation controller will poll get_model_download_status instead.
      // Terminal state will trigger exactly one refreshProviderReadiness reconciliation.
      // For load, we already called refreshProviderReadiness above.
    } catch (e) {
      console.error('Readiness action failed:', e);
      btn.disabled = false;
      btn.textContent = t('status.readiness.action.failed');
    }
  };

  // Attach the delegated listener once
  readinessRegion.addEventListener('click', clickHandler);
  readinessRegion._readinessClickHandlerAttached = true;
}
