// Operation controller: single-owner polling for download/load operations.
//
// This module owns the operation polling lifecycle. It uses get_model_download_status
// (the lightweight OperationManager snapshot) instead of get_provider_readiness
// (the heavyweight provider observation) during active operations.
//
// Invariants:
// - At most one active poll timer
// - At most one active operation observer
// - Polling uses get_model_download_status, not get_provider_readiness
// - Terminal state stops polling
// - Paused remains non-terminal (continues polling)
// - Navigation away stops polling; navigation back reconnects

import { invoke } from './tauri.js';
import { state } from './state.js';

const ACTIVE_POLL_INTERVAL_MS = 2000;
const PAUSED_POLL_INTERVAL_MS = 15000;

let pollTimerId = null;
let isPolling = false;
let lastOperationSnapshot = null;
let onOperationUpdate = null;

/**
 * Start polling for operation status updates.
 *
 * Uses setTimeout chaining to prevent duplicate timers.
 * Each poll calls get_model_download_status (lightweight).
 *
 * Exported for explicit start from action handlers.
 */
export function startOperationPolling(updateCallback) {
  onOperationUpdate = updateCallback;
  startPolling();
}

/**
 * Start observing an existing operation (if any).
 *
 * Called when Status page loads. If an operation is active, begins polling.
 * Does not start a new operation; only observes existing ones.
 */
export function observeExisting(updateCallback) {
  onOperationUpdate = updateCallback;

  // Check if there's already an active operation in cached readiness
  const cachedReadiness = state.cachedProviderReadiness;
  if (cachedReadiness && cachedReadiness.download && cachedReadiness.download.active) {
    startPolling();
  }
}

/**
 * Start polling for operation status updates.
 *
 * Uses setTimeout chaining to prevent duplicate timers.
 * Each poll calls get_model_download_status (lightweight).
 */
function startPolling() {
  if (isPolling) return;
  isPolling = true;
  scheduleNextPoll();
}

function scheduleNextPoll() {
  if (!isPolling) return;

  const interval = lastOperationSnapshot && lastOperationSnapshot.status &&
                   typeof lastOperationSnapshot.status === 'object' &&
                   lastOperationSnapshot.status.paused
    ? PAUSED_POLL_INTERVAL_MS
    : ACTIVE_POLL_INTERVAL_MS;

  pollTimerId = setTimeout(pollOperation, interval);
}

async function pollOperation() {
  if (!isPolling) return;

  try {
    const snapshot = await invoke('get_model_download_status');

    if (snapshot) {
      lastOperationSnapshot = snapshot;

      // Notify the Status page to update the operation display
      if (onOperationUpdate) {
        onOperationUpdate(snapshot);
      }

      // Check if operation reached terminal state
      if (!snapshot.active) {
        stopPolling();
        // Trigger one final readiness reconciliation
        if (onOperationUpdate) {
          onOperationUpdate(null, true); // null snapshot, terminal flag
        }
        return;
      }
    } else {
      // No operation found; stop polling
      stopPolling();
      return;
    }

    // Schedule next poll
    scheduleNextPoll();
  } catch (e) {
    console.error('Operation poll failed:', e);
    // Stop polling on error; user can manually refresh
    stopPolling();
  }
}

/**
 * Stop polling for operation updates.
 *
 * Called when:
 * - Operation reaches terminal state
 * - User navigates away from Status
 * - Explicit stop request
 */
export function stopPolling() {
  isPolling = false;
  if (pollTimerId !== null) {
    clearTimeout(pollTimerId);
    pollTimerId = null;
  }
  lastOperationSnapshot = null;
}

/**
 * Dispose the controller completely.
 *
 * Called when Status page is unmounted or app is shutting down.
 */
export function dispose() {
  stopPolling();
  onOperationUpdate = null;
}

/**
 * Check if currently polling.
 *
 * Used by tests to verify timer cardinality.
 */
export function isCurrentlyPolling() {
  return isPolling;
}

/**
 * Get the last observed operation snapshot.
 *
 * Used by tests to verify polling behavior.
 */
export function getLastSnapshot() {
  return lastOperationSnapshot;
}
