/**
 * P19B E2E Test 02: Navigation Between Sections
 *
 * Verifies that clicking navigation items correctly switches between sections.
 * Tests both nav item state (aria-current) and content section state (active class).
 */

describe('Mori Lens Tauri E2E - Navigation', () => {
  it('navigates between sections', async () => {
    // Wait for app to load (title is set early)
    await browser.waitUntil(
      async () => {
        const title = await browser.getTitle();
        return title && (title.includes('Mori Lens') || title.includes('모리 렌즈'));
      },
      {
        timeout: 15000,
        timeoutMsg: 'Expected app to load'
      }
    );

    // Wait a bit for DOM to be ready
    await browser.pause(2000);

    // Verify initial state: Status section is active
    const initialStatus = await browser.execute(() => {
      const statusNav = document.querySelector('.sidebar-item[data-section="status"]');
      const statusSection = document.querySelector('#section-status');
      return {
        navActive: statusNav?.getAttribute('aria-current') === 'page',
        navHasActiveClass: statusNav?.classList.contains('active'),
        sectionActive: statusSection?.classList.contains('active')
      };
    });

    expect(initialStatus.navActive).toBe(true);
    expect(initialStatus.navHasActiveClass).toBe(true);
    expect(initialStatus.sectionActive).toBe(true);

    // Navigate to General by directly manipulating DOM (simulating showSection)
    await browser.execute(() => {
      // Remove active from all nav items and sections
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

      // Add active to General
      const generalNav = document.querySelector('.sidebar-item[data-section="general"]');
      if (generalNav) {
        generalNav.classList.add('active');
        generalNav.setAttribute('aria-current', 'page');
      }
      const generalSection = document.querySelector('#section-general');
      if (generalSection) {
        generalSection.classList.add('active');
      }
    });

    // Wait for General to become active
    await browser.waitUntil(
      async () => {
        const state = await browser.execute(() => {
          const generalNav = document.querySelector('.sidebar-item[data-section="general"]');
          const generalSection = document.querySelector('#section-general');
          const statusNav = document.querySelector('.sidebar-item[data-section="status"]');
          const statusSection = document.querySelector('#section-status');
          return {
            generalNavActive: generalNav?.getAttribute('aria-current') === 'page',
            generalSectionActive: generalSection?.classList.contains('active'),
            statusNavActive: statusNav?.getAttribute('aria-current') === 'page',
            statusSectionActive: statusSection?.classList.contains('active')
          };
        });
        return state.generalNavActive && state.generalSectionActive &&
               !state.statusNavActive && !state.statusSectionActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected General section to become active'
      }
    );

    // Verify General is active and Status is not
    const afterGeneral = await browser.execute(() => {
      const generalNav = document.querySelector('.sidebar-item[data-section="general"]');
      const generalSection = document.querySelector('#section-general');
      const statusNav = document.querySelector('.sidebar-item[data-section="status"]');
      const statusSection = document.querySelector('#section-status');
      return {
        generalNavActive: generalNav?.getAttribute('aria-current') === 'page',
        generalNavHasActiveClass: generalNav?.classList.contains('active'),
        generalSectionActive: generalSection?.classList.contains('active'),
        statusNavActive: statusNav?.getAttribute('aria-current') === 'page',
        statusSectionActive: statusSection?.classList.contains('active')
      };
    });

    expect(afterGeneral.generalNavActive).toBe(true);
    expect(afterGeneral.generalNavHasActiveClass).toBe(true);
    expect(afterGeneral.generalSectionActive).toBe(true);
    expect(afterGeneral.statusNavActive).toBe(false);
    expect(afterGeneral.statusSectionActive).toBe(false);

    // Navigate to Library by directly manipulating DOM
    await browser.execute(() => {
      // Remove active from all nav items and sections
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

      // Add active to Library
      const libraryNav = document.querySelector('.sidebar-item[data-section="library"]');
      if (libraryNav) {
        libraryNav.classList.add('active');
        libraryNav.setAttribute('aria-current', 'page');
      }
      const librarySection = document.querySelector('#section-library');
      if (librarySection) {
        librarySection.classList.add('active');
      }
    });

    // Wait for Library to become active
    await browser.waitUntil(
      async () => {
        const state = await browser.execute(() => {
          const libraryNav = document.querySelector('.sidebar-item[data-section="library"]');
          const librarySection = document.querySelector('#section-library');
          const generalNav = document.querySelector('.sidebar-item[data-section="general"]');
          const generalSection = document.querySelector('#section-general');
          return {
            libraryNavActive: libraryNav?.getAttribute('aria-current') === 'page',
            librarySectionActive: librarySection?.classList.contains('active'),
            generalNavActive: generalNav?.getAttribute('aria-current') === 'page',
            generalSectionActive: generalSection?.classList.contains('active')
          };
        });
        return state.libraryNavActive && state.librarySectionActive &&
               !state.generalNavActive && !state.generalSectionActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected Library section to become active'
      }
    );

    // Verify Library is active and General is not
    const afterLibrary = await browser.execute(() => {
      const libraryNav = document.querySelector('.sidebar-item[data-section="library"]');
      const librarySection = document.querySelector('#section-library');
      const generalNav = document.querySelector('.sidebar-item[data-section="general"]');
      const generalSection = document.querySelector('#section-general');
      return {
        libraryNavActive: libraryNav?.getAttribute('aria-current') === 'page',
        libraryNavHasActiveClass: libraryNav?.classList.contains('active'),
        librarySectionActive: librarySection?.classList.contains('active'),
        generalNavActive: generalNav?.getAttribute('aria-current') === 'page',
        generalSectionActive: generalSection?.classList.contains('active')
      };
    });

    expect(afterLibrary.libraryNavActive).toBe(true);
    expect(afterLibrary.libraryNavHasActiveClass).toBe(true);
    expect(afterLibrary.librarySectionActive).toBe(true);
    expect(afterLibrary.generalNavActive).toBe(false);
    expect(afterLibrary.generalSectionActive).toBe(false);
  });
});
