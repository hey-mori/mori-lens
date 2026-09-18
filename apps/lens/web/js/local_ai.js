// Local AI status: shared refresh helper for Models and Status pages

import { invoke } from './tauri.js';
import { state } from './state.js';

/**
 * Refresh the cached Local AI status from the backend.
 *
 * This is the single owner of the refresh operation. Both Models and Status
 * pages call this function to update state.cachedLocalAiStatus and
 * state.localAiLoadState.
 *
 * Rendering remains with the pages that render; this function only owns
 * the data fetch and cache update.
 */
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
