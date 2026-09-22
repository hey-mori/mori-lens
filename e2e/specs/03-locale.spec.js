/**
 * P19B E2E Test 03: Interface Language Switching
 *
 * Verifies that switching the interface language actually changes visible labels,
 * not just the select value. Tests English → Korean → English round-trip.
 */

describe('Mori Lens Tauri E2E - Locale Switching', () => {
  it('switches interface language and verifies visible text changes', async () => {
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

    // Navigate to General section
    await browser.execute(() => {
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

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

    // Wait for General section to be active
    await browser.waitUntil(
      async () => {
        const isActive = await browser.execute(() => {
          const section = document.querySelector('#section-general');
          return section?.classList.contains('active');
        });
        return isActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected General section to be active'
      }
    );

    // Capture initial English labels (elements with data-i18n)
    const initialLabels = await browser.execute(() => {
      const getLabel = (selector) => {
        const el = document.querySelector(selector);
        return el?.textContent?.trim() || '';
      };

      return {
        generalNav: getLabel('.sidebar-item[data-section="general"]'),
        generalHeading: getLabel('#section-general h1'),
        interfaceLangLabel: getLabel('label[for="interface-language-select"]'),
        saveButton: getLabel('#general-save-btn')
      };
    });

    // Verify we have English labels
    expect(initialLabels.generalNav).toBeTruthy();
    expect(initialLabels.generalHeading).toBeTruthy();

    // Find the language selector
    const langSelectExists = await browser.execute(() => {
      return document.querySelector('#interface-language-select') !== null;
    });
    expect(langSelectExists).toBe(true);

    // Switch to Korean
    await browser.execute(() => {
      const select = document.querySelector('#interface-language-select');
      if (select) {
        select.value = 'ko';
        select.dispatchEvent(new Event('change', { bubbles: true }));
      }
    });

    // Wait for locale change to propagate
    await browser.pause(1000);

    // Capture Korean labels
    const koreanLabels = await browser.execute(() => {
      const getLabel = (selector) => {
        const el = document.querySelector(selector);
        return el?.textContent?.trim() || '';
      };

      return {
        generalNav: getLabel('.sidebar-item[data-section="general"]'),
        generalHeading: getLabel('#section-general h1'),
        interfaceLangLabel: getLabel('label[for="interface-language-select"]'),
        saveButton: getLabel('#general-save-btn')
      };
    });

    // Verify labels changed (Korean characters should be present)
    // "일반" is Korean for "General"
    expect(koreanLabels.generalNav).toMatch(/[일반국어]/);
    expect(koreanLabels.generalHeading).toMatch(/[일반국어]/);

    // Verify the select value changed
    const selectValue = await browser.execute(() => {
      const select = document.querySelector('#interface-language-select');
      return select?.value;
    });
    expect(selectValue).toBe('ko');

    // Switch back to English
    await browser.execute(() => {
      const select = document.querySelector('#interface-language-select');
      if (select) {
        select.value = 'en';
        select.dispatchEvent(new Event('change', { bubbles: true }));
      }
    });

    // Wait for locale change to propagate
    await browser.pause(1000);

    // Capture English labels again
    const finalLabels = await browser.execute(() => {
      const getLabel = (selector) => {
        const el = document.querySelector(selector);
        return el?.textContent?.trim() || '';
      };

      return {
        generalNav: getLabel('.sidebar-item[data-section="general"]'),
        generalHeading: getLabel('#section-general h1'),
        interfaceLangLabel: getLabel('label[for="interface-language-select"]'),
        saveButton: getLabel('#general-save-btn')
      };
    });

    // Verify labels returned to English
    expect(finalLabels.generalNav).toBe(initialLabels.generalNav);
    expect(finalLabels.generalHeading).toBe(initialLabels.generalHeading);

    // Verify the select value changed back
    const finalSelectValue = await browser.execute(() => {
      const select = document.querySelector('#interface-language-select');
      return select?.value;
    });
    expect(finalSelectValue).toBe('en');
  });
});
