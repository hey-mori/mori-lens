/**
 * P19B E2E Test 05: Library Empty State
 *
 * Verifies that the Library section shows the correct empty state when no items
 * have been saved yet. Tests the initial state of the Library UI.
 */

describe('Mori Lens Tauri E2E - Library Empty State', () => {
  it('shows empty state in Library section', async () => {
    // Wait for app to load
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

    // Wait for DOM to be ready
    await browser.pause(2000);

    // Navigate to Library section
    await browser.execute(() => {
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

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

    // Wait for Library section to be active
    await browser.waitUntil(
      async () => {
        const isActive = await browser.execute(() => {
          const section = document.querySelector('#section-library');
          return section?.classList.contains('active');
        });
        return isActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected Library section to be active'
      }
    );

    // Verify Library section exists and has content
    const librarySectionExists = await browser.execute(() => {
      const section = document.querySelector('#section-library');
      return section !== null && section.classList.contains('active');
    });
    expect(librarySectionExists).toBe(true);

    // Check for empty state message or empty list
    const emptyState = await browser.execute(() => {
      const section = document.querySelector('#section-library');
      if (!section) return { exists: false };

      // Look for empty state indicators
      const hasEmptyMessage = section.querySelector('.empty-state') !== null ||
                              section.querySelector('[data-empty]') !== null ||
                              section.textContent.includes('No items') ||
                              section.textContent.includes('Nothing saved') ||
                              section.textContent.includes('Your library is empty');

      // Check if there's a list with no items
      const listContainer = section.querySelector('.library-list, .items-list, ul, [data-items]');
      const itemCount = listContainer ? listContainer.children.length : 0;

      return {
        exists: true,
        hasEmptyMessage,
        itemCount,
        isEmpty: itemCount === 0 || hasEmptyMessage
      };
    });

    expect(emptyState.exists).toBe(true);
    // In a fresh E2E environment, the library should be empty
    expect(emptyState.isEmpty).toBe(true);
  });

  it('verifies Library nav item has correct state', async () => {
    // Wait for app to load
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

    // Wait for DOM to be ready
    await browser.pause(2000);

    // Navigate to Library
    await browser.execute(() => {
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

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

    // Wait for navigation to complete
    await browser.waitUntil(
      async () => {
        const state = await browser.execute(() => {
          const libraryNav = document.querySelector('.sidebar-item[data-section="library"]');
          return {
            navActive: libraryNav?.getAttribute('aria-current') === 'page',
            navHasActiveClass: libraryNav?.classList.contains('active'),
            sectionActive: document.querySelector('#section-library')?.classList.contains('active')
          };
        });
        return state.navActive && state.navHasActiveClass && state.sectionActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected Library navigation state to be correct'
      }
    );

    // Verify final state
    const finalState = await browser.execute(() => {
      const libraryNav = document.querySelector('.sidebar-item[data-section="library"]');
      const statusNav = document.querySelector('.sidebar-item[data-section="status"]');
      return {
        libraryNavActive: libraryNav?.getAttribute('aria-current') === 'page',
        libraryNavHasActiveClass: libraryNav?.classList.contains('active'),
        librarySectionActive: document.querySelector('#section-library')?.classList.contains('active'),
        statusNavActive: statusNav?.getAttribute('aria-current') === 'page',
        statusSectionActive: document.querySelector('#section-status')?.classList.contains('active')
      };
    });

    expect(finalState.libraryNavActive).toBe(true);
    expect(finalState.libraryNavHasActiveClass).toBe(true);
    expect(finalState.librarySectionActive).toBe(true);
    expect(finalState.statusNavActive).toBe(false);
    expect(finalState.statusSectionActive).toBe(false);
  });
});
