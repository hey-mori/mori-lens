/**
 * P19B E2E Test 01: Native Launch with Production DOM
 *
 * Verifies that the compiled Mori Lens Tauri app launches and the Control Center
 * WebView loads the actual production frontend with correct DOM structure.
 *
 * Evidence: APPLICATION-SHELL E2E PASS — PRODUCTION DOM
 */

describe('Mori Lens Tauri E2E - Launch', () => {
  it('launches the app and loads Control Center with production DOM', async () => {
    // Wait for the app to load
    await browser.waitUntil(
      async () => {
        const title = await browser.getTitle();
        // App title is localized; accept either English or Korean
        return title && (title.includes('Mori Lens') || title.includes('모리 렌즈'));
      },
      {
        timeout: 15000,
        timeoutMsg: 'Expected app title to be loaded'
      }
    );

    // Check if document is loaded and has content
    const hasNav = await browser.execute(() => {
      return document.querySelector('nav.sidebar') !== null;
    });
    expect(hasNav).toBe(true);

    const hasMainContent = await browser.execute(() => {
      return document.querySelector('#main-content') !== null;
    });
    expect(hasMainContent).toBe(true);

    const hasStatusSection = await browser.execute(() => {
      const section = document.querySelector('#section-status');
      return section !== null && section.classList.contains('active');
    });
    expect(hasStatusSection).toBe(true);

    // Verify navigation items exist
    const navItems = await browser.execute(() => {
      const items = document.querySelectorAll('.sidebar-item');
      return {
        status: Array.from(items).some(el => el.dataset.section === 'status'),
        general: Array.from(items).some(el => el.dataset.section === 'general'),
        library: Array.from(items).some(el => el.dataset.section === 'library'),
      };
    });

    expect(navItems.status).toBe(true);
    expect(navItems.general).toBe(true);
    expect(navItems.library).toBe(true);

    // Verify status nav item is active
    const statusNavActive = await browser.execute(() => {
      const nav = document.querySelector('.sidebar-item[data-section="status"]');
      return nav && nav.getAttribute('aria-current') === 'page' && nav.classList.contains('active');
    });
    expect(statusNavActive).toBe(true);
  });
});
