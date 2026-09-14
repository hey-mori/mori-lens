// Shared UI helpers
// Common utility functions used across multiple sections

/**
 * Escape HTML special characters to prevent XSS
 */
export function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Format byte size to human-readable string
 */
export function formatSize(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

/**
 * Set status message in the active section
 */
export function setStatus(msg, persistent = false) {
  const active = document.querySelector('.section.active');
  const statusEl = active.querySelector('[id$="-status"]');
  if (statusEl) {
    statusEl.textContent = msg;
    if (!persistent) {
      setTimeout(() => { statusEl.textContent = ''; }, 3000);
    }
  }
}
