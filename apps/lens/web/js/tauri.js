// Tauri API wrapper
// Provides invoke, listen, emit to all modules

const { invoke: _invoke } = window.__TAURI__.core;
const { listen: _listen } = window.__TAURI__.event;

export function invoke(cmd, args) {
  return _invoke(cmd, args);
}

export function listen(event, handler) {
  return _listen(event, handler);
}
