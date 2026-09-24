// Navigation: section switching, sidebar events, tray navigation

let currentSection = null;

export function showSection(sectionName) {
  // Deactivate previous section if needed
  if (currentSection === 'status' && sectionName !== 'status') {
    import('./status.js').then(s => s.deactivateStatus());
  }

  document.querySelectorAll('.sidebar-item').forEach(i => {
    i.classList.remove('active');
    i.removeAttribute('aria-current');
  });
  document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));
  const item = document.querySelector(`.sidebar-item[data-section="${sectionName}"]`);
  if (item) {
    item.classList.add('active');
    item.setAttribute('aria-current', 'page');
    document.getElementById('section-' + sectionName).classList.add('active');
    // Dynamically import section modules to load their data
    if (sectionName === 'status') {
      import('./status.js').then(s => {
        s.refreshReadinessAndRender();
        s.activateStatus();
      });
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

    currentSection = sectionName;
  }
}

export function initNavigation() {
  // Sidebar click and keyboard handlers
  document.querySelectorAll('.sidebar-item').forEach(item => {
    item.addEventListener('click', () => showSection(item.dataset.section));
    item.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        showSection(item.dataset.section);
      }
    });
  });
}
