// Navigation: section switching, sidebar events, tray navigation

/**
 * Show a section and optionally move focus.
 * @param {string} sectionName - Section to show
 * @param {Object} options - Focus behavior options
 * @param {boolean} options.skipFocusMove - If true, don't move focus to heading (for keyboard nav)
 */
export function showSection(sectionName, options = {}) {
  const tabs = document.querySelectorAll('.sidebar-item[role="tab"]');
  const panels = document.querySelectorAll('.section[role="tabpanel"]');

  tabs.forEach(i => {
    i.classList.remove('active');
    i.setAttribute('aria-selected', 'false');
    i.setAttribute('tabindex', '-1');
  });
  panels.forEach(s => s.classList.remove('active'));

  const item = document.querySelector(`.sidebar-item[data-section="${sectionName}"]`);
  if (item) {
    item.classList.add('active');
    item.setAttribute('aria-selected', 'true');
    item.setAttribute('tabindex', '0');
    const panel = document.getElementById('section-' + sectionName);
    panel.classList.add('active');

    // Move focus to the section heading for screen reader context
    // Skip this during keyboard arrow navigation to keep focus in tablist
    if (!options.skipFocusMove) {
      const heading = panel.querySelector('h1');
      if (heading) {
        heading.setAttribute('tabindex', '-1');
        heading.focus({ preventScroll: true });
      }
    }

    // Dynamically import section modules to load their data
    if (sectionName === 'status') {
      import('./models.js').then(m => m.refreshLocalAiStatus())
        .then(() => import('./status.js').then(s => s.loadStatus()));
    }
    else if (sectionName === 'library') import('./library.js').then(m => m.loadLibrary());
    else if (sectionName === 'review') import('./review.js').then(m => m.loadReview());
    else if (sectionName === 'recent') import('./recent.js').then(m => m.loadRecent());
    else if (sectionName === 'history') import('./history.js').then(m => m.loadHistory());
    else if (sectionName === 'general') import('./settings.js').then(m => m.loadSettings());
    else if (sectionName === 'models') {
      import('./settings.js').then(m => m.loadSettings());
      import('./models.js').then(m => m.refreshModels());
    }
    else if (sectionName === 'apps') import('./settings.js').then(m => m.loadSettings());
    else if (sectionName === 'about') import('./diagnostics.js').then(m => m.loadAbout());
  }
}

export function initNavigation() {
  const tabs = Array.from(document.querySelectorAll('.sidebar-item[role="tab"]'));

  tabs.forEach(item => {
    item.addEventListener('click', () => showSection(item.dataset.section));
  });

  // Keyboard navigation: arrow keys, Home, End (WAI-ARIA tablist pattern)
  document.querySelector('[role="tablist"]').addEventListener('keydown', (e) => {
    const currentIndex = tabs.indexOf(document.activeElement);
    if (currentIndex === -1) return;

    let newIndex;
    switch (e.key) {
      case 'ArrowDown':
      case 'ArrowRight':
        newIndex = (currentIndex + 1) % tabs.length;
        break;
      case 'ArrowUp':
      case 'ArrowLeft':
        newIndex = (currentIndex - 1 + tabs.length) % tabs.length;
        break;
      case 'Home':
        newIndex = 0;
        break;
      case 'End':
        newIndex = tabs.length - 1;
        break;
      default:
        return;
    }

    e.preventDefault();
    tabs[newIndex].focus();
    showSection(tabs[newIndex].dataset.section, { skipFocusMove: true });
  });
}
