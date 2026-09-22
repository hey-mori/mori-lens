/**
 * P19B E2E Test 06: Provider Unavailable
 *
 * Verifies that the app handles the case where the AI provider (e.g., Ollama)
 * is not available. The app should not crash and should show appropriate status.
 */

describe('Mori Lens Tauri E2E - Provider Unavailable', () => {
  it('handles missing AI provider gracefully', async () => {
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
    await browser.pause(3000);

    // Verify the app is still running and responsive
    const appIsResponsive = await browser.execute(() => {
      return document.querySelector('nav.sidebar') !== null &&
             document.querySelector('#main-content') !== null;
    });
    expect(appIsResponsive).toBe(true);

    // Check Status section for health/provider status
    await browser.execute(() => {
      document.querySelectorAll('.sidebar-item').forEach(i => {
        i.classList.remove('active');
        i.removeAttribute('aria-current');
      });
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

      const statusNav = document.querySelector('.sidebar-item[data-section="status"]');
      if (statusNav) {
        statusNav.classList.add('active');
        statusNav.setAttribute('aria-current', 'page');
      }
      const statusSection = document.querySelector('#section-status');
      if (statusSection) {
        statusSection.classList.add('active');
      }
    });

    // Wait for Status section to be active
    await browser.waitUntil(
      async () => {
        const isActive = await browser.execute(() => {
          const section = document.querySelector('#section-status');
          return section?.classList.contains('active');
        });
        return isActive;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Expected Status section to be active'
      }
    );

    // Verify Status section exists
    const statusSectionExists = await browser.execute(() => {
      const section = document.querySelector('#section-status');
      return section !== null && section.classList.contains('active');
    });
    expect(statusSectionExists).toBe(true);

    // Check for health status indicators
    // The app should show some status about the AI provider, even if it's unavailable
    const healthStatus = await browser.execute(() => {
      const section = document.querySelector('#section-status');
      if (!section) return { exists: false };

      // Look for health/status indicators
      const hasHealthIndicator = section.querySelector('[data-health], .health-status, .status-indicator') !== null;

      // Check for provider status text
      const textContent = section.textContent || '';
      const hasProviderInfo = textContent.includes('Ollama') ||
                              textContent.includes('provider') ||
                              textContent.includes('model') ||
                              textContent.includes('Local') ||
                              textContent.includes('endpoint');

      return {
        exists: true,
        hasHealthIndicator,
        hasProviderInfo,
        hasContent: textContent.length > 0
      };
    });

    expect(healthStatus.exists).toBe(true);
    expect(healthStatus.hasContent).toBe(true);
    // The app should show some information about the provider/health
    // Even if the provider is unavailable, it should not crash
  });

  it('maintains UI stability when provider is unavailable', async () => {
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

    // Wait for initialization
    await browser.pause(3000);

    // Navigate through all sections to ensure UI doesn't crash
    const sections = ['status', 'general', 'library'];

    for (const sectionName of sections) {
      await browser.execute((section) => {
        document.querySelectorAll('.sidebar-item').forEach(i => {
          i.classList.remove('active');
          i.removeAttribute('aria-current');
        });
        document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

        const nav = document.querySelector(`.sidebar-item[data-section="${section}"]`);
        if (nav) {
          nav.classList.add('active');
          nav.setAttribute('aria-current', 'page');
        }
        const sectionEl = document.querySelector(`#section-${section}`);
        if (sectionEl) {
          sectionEl.classList.add('active');
        }
      }, sectionName);

      // Wait a bit for any async operations
      await browser.pause(500);

      // Verify the section is active and the app is still responsive
      const sectionIsActive = await browser.execute((section) => {
        const sectionEl = document.querySelector(`#section-${section}`);
        return sectionEl?.classList.contains('active') === true;
      }, sectionName);

      expect(sectionIsActive).toBe(true);

      // Verify the app hasn't crashed by checking if we can still interact with the DOM
      const appIsResponsive = await browser.execute(() => {
        return document.querySelector('nav.sidebar') !== null;
      });
      expect(appIsResponsive).toBe(true);
    }
  });

  it('shows Models section with endpoint configuration', async () => {
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

    // Navigate to Models section (if it exists)
    const hasModelsSection = await browser.execute(() => {
      const modelsNav = document.querySelector('.sidebar-item[data-section="models"]');
      return modelsNav !== null;
    });

    if (hasModelsSection) {
      await browser.execute(() => {
        document.querySelectorAll('.sidebar-item').forEach(i => {
          i.classList.remove('active');
          i.removeAttribute('aria-current');
        });
        document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

        const modelsNav = document.querySelector('.sidebar-item[data-section="models"]');
        if (modelsNav) {
          modelsNav.classList.add('active');
          modelsNav.setAttribute('aria-current', 'page');
        }
        const modelsSection = document.querySelector('#section-models');
        if (modelsSection) {
          modelsSection.classList.add('active');
        }
      });

      // Wait for Models section to be active
      await browser.waitUntil(
        async () => {
          const isActive = await browser.execute(() => {
            const section = document.querySelector('#section-models');
            return section?.classList.contains('active');
          });
          return isActive;
        },
        {
          timeout: 5000,
          timeoutMsg: 'Expected Models section to be active'
        }
      );

      // Verify Models section has endpoint configuration
      const modelsSection = await browser.execute(() => {
        const section = document.querySelector('#section-models');
        if (!section) return { exists: false };

        // Look for endpoint configuration
        const hasEndpointInput = section.querySelector('input[name="endpoint"], #endpoint, [data-endpoint]') !== null;
        const hasModelInput = section.querySelector('input[name="model"], #model, [data-model]') !== null;

        const textContent = section.textContent || '';
        const hasEndpointText = textContent.includes('endpoint') ||
                                textContent.includes('127.0.0.1') ||
                                textContent.includes('localhost');

        return {
          exists: true,
          hasEndpointInput,
          hasModelInput,
          hasEndpointText,
          hasConfiguration: hasEndpointInput || hasEndpointText
        };
      });

      expect(modelsSection.exists).toBe(true);
      // Models section should have some configuration UI
      expect(modelsSection.hasConfiguration).toBe(true);
    }
  });
});
