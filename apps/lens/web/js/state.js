// Shared application state
// Isolated to avoid circular imports

export const state = {
  currentSettings: null,
  appsDirty: false,
  statusRefreshInterval: null,
  cachedLocalAiStatus: null,
  localAiLoadState: 'NOT_LOADED',
  // P20C: provider readiness (replaces cachedLocalAiStatus for readiness UI)
  cachedProviderReadiness: null,
  providerReadinessLoadState: 'NOT_LOADED',
};
