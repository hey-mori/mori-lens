// Application bootstrap
// Main entrypoint for the Control Center

import { listen } from './tauri.js';
import { initNavigation, showSection } from './navigation.js';
import { state } from './state.js';

// Re-export state for backward compatibility
export { state } from './state.js';

async function loadAll() {
  try {
    const statusModule = await import('./status.js');
    const settingsModule = await import('./settings.js');
    const diagnosticsModule = await import('./diagnostics.js');
    await Promise.all([
      statusModule.loadStatus(),
      settingsModule.loadSettings(),
      diagnosticsModule.loadAbout(),
    ]);
    // Asynchronously refresh Local AI after initial load
    import('./models.js').then(m => m.refreshLocalAiStatus()).then(() => {
      statusModule.loadStatus();
    });
  } catch (e) {
    console.error('loadAll failed:', e);
  }
}

function startStatusRefresh() {
  if (state.statusRefreshInterval) return;
  state.statusRefreshInterval = setInterval(async () => {
    const statusSection = document.getElementById('section-status');
    if (statusSection && statusSection.classList.contains('active')) {
      const statusModule = await import('./status.js');
      await statusModule.loadStatus();
    }
  }, 1000);
}

function stopStatusRefresh() {
  if (state.statusRefreshInterval) {
    clearInterval(state.statusRefreshInterval);
    state.statusRefreshInterval = null;
  }
}

async function init() {
  // Initialize navigation
  initNavigation();

  // Initialize section-specific handlers
  const libraryModule = await import('./library.js');
  libraryModule.initLibraryHandlers();

  const reviewModule = await import('./review.js');
  reviewModule.initReviewHandlers();

  const historyModule = await import('./history.js');
  historyModule.initHistoryHandlers();

  // Wire up buttons with IDs
  wireStaticButtons();

  // Listen for section navigation events from tray menu
  listen('navigate-section', (event) => {
    showSection(event.payload);
  });

  // Start/stop status refresh on focus
  window.addEventListener('focus', startStatusRefresh);
  window.addEventListener('blur', stopStatusRefresh);

  // Load initial data
  await loadAll();
}

function wireStaticButtons() {
  // General section
  const generalSaveBtn = document.getElementById('general-save-btn');
  if (generalSaveBtn) generalSaveBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.saveSettings());
  });
  const generalResetBtn = document.getElementById('general-reset-btn');
  if (generalResetBtn) generalResetBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.loadSettings());
  });

  // Models section
  const modelsRefreshBtn = document.getElementById('models-refresh-btn');
  if (modelsRefreshBtn) modelsRefreshBtn.addEventListener('click', () => {
    import('./models.js').then(m => m.refreshModels());
  });
  const testModelBtn = document.getElementById('test-btn');
  if (testModelBtn) testModelBtn.addEventListener('click', () => {
    import('./models.js').then(m => m.testModel());
  });
  const modelsSaveBtn = document.getElementById('models-save-btn');
  if (modelsSaveBtn) modelsSaveBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.saveSettings());
  });
  const modelsResetBtn = document.getElementById('models-reset-btn');
  if (modelsResetBtn) modelsResetBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.loadSettings());
  });

  // Apps section
  const addAppBtn = document.getElementById('add-app-btn');
  if (addAppBtn) addAppBtn.addEventListener('click', () => {
    import('./apps.js').then(m => m.addApp());
  });
  const appsSaveBtn = document.getElementById('apps-save-btn');
  if (appsSaveBtn) appsSaveBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.saveSettings());
  });
  const appsResetBtn = document.getElementById('apps-reset-btn');
  if (appsResetBtn) appsResetBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.loadSettings());
  });

  // Shortcuts section
  const shortcutsSaveBtn = document.getElementById('shortcuts-save-btn');
  if (shortcutsSaveBtn) shortcutsSaveBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.saveSettings());
  });
  const shortcutsRevertBtn = document.getElementById('shortcuts-revert-btn');
  if (shortcutsRevertBtn) shortcutsRevertBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.loadSettings());
  });
  const shortcutsDefaultsBtn = document.getElementById('shortcuts-defaults-btn');
  if (shortcutsDefaultsBtn) shortcutsDefaultsBtn.addEventListener('click', () => {
    import('./settings.js').then(m => m.restoreShortcutDefaults());
  });

  // About section
  const secureWipeBtn = document.getElementById('secure-wipe-btn');
  if (secureWipeBtn) secureWipeBtn.addEventListener('click', () => {
    import('./diagnostics.js').then(m => m.secureWipe());
  });

  // History section
  const historyPruneBtn = document.getElementById('history-prune-btn');
  if (historyPruneBtn) historyPruneBtn.addEventListener('click', () => {
    import('./history.js').then(m => m.pruneHistory());
  });
}

// Auto-initialize when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
