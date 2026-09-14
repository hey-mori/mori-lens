// Tauri IPC wrapper
// Provides access to Tauri core APIs

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

export { invoke, listen };
