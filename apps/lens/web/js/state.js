// Shared application state
// Isolated to avoid circular imports

export const state = {
  currentSettings: null,
  appsDirty: false,
  statusRefreshInterval: null,
  cachedLocalAiStatus: null,
  localAiLoadState: 'NOT_LOADED',
};
