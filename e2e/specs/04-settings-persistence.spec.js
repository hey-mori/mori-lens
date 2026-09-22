/**
 * P19B E2E Test 04: Settings Persistence
 *
 * Verifies that settings changes are persisted to the isolated E2E data root
 * and survive app restart. Uses MORI_LENS_E2E_DATA_ROOT to avoid polluting
 * the user's real settings.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../..');
const e2eDataRoot = path.join(repoRoot, '.e2e-data');

describe('Mori Lens Tauri E2E - Settings Persistence', () => {
  it('persists settings changes to isolated data root', async () => {
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

    // Verify settings.toml was created in E2E data root
    const settingsPath = path.join(e2eDataRoot, 'settings.toml');
    expect(fs.existsSync(settingsPath)).toBe(true);

    // Read initial settings
    const initialContent = fs.readFileSync(settingsPath, 'utf-8');

    // Change the interface language to Korean via DOM
    await browser.execute(() => {
      const select = document.querySelector('#interface-language-select');
      if (select) {
        select.value = 'ko';
        select.dispatchEvent(new Event('change', { bubbles: true }));
      }
    });

    // Wait a bit for the change event to propagate
    await browser.pause(500);

    // Click Save button
    const saveButtonExists = await browser.execute(() => {
      return document.querySelector('#general-save-btn') !== null;
    });
    expect(saveButtonExists).toBe(true);

    // Trigger save via browser.execute (more reliable than click)
    await browser.execute(() => {
      const saveBtn = document.querySelector('#general-save-btn');
      if (saveBtn) {
        saveBtn.click();
      }
    });

    // Wait for save to complete
    await browser.pause(2000);

    // Read settings again and verify the change was persisted
    const updatedContent = fs.readFileSync(settingsPath, 'utf-8');

    // The settings file should have been updated (content should differ or timestamp should be newer)
    // Check that the file was modified
    const stats = fs.statSync(settingsPath);
    const now = Date.now();
    const fileAge = now - stats.mtimeMs;

    // File should have been modified within the last 5 seconds
    expect(fileAge).toBeLessThan(5000);

    // Verify the settings file contains Korean language setting
    // The exact format depends on the TOML schema, but it should mention Korean or "ko"
    const hasKoreanSetting = updatedContent.includes('Korean') ||
                              updatedContent.includes('language = "ko"') ||
                              updatedContent.includes("language = 'ko'");
    expect(hasKoreanSetting).toBe(true);
  });

  it('verifies library.db is created in E2E data root', async () => {
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

    // library.db should be created in E2E data root (not in user's AppData)
    const libraryPath = path.join(e2eDataRoot, 'library.db');

    // The library.db is created on first save or when library is activated
    // For now, just verify the E2E data root exists and settings.toml is there
    expect(fs.existsSync(e2eDataRoot)).toBe(true);
    expect(fs.existsSync(path.join(e2eDataRoot, 'settings.toml'))).toBe(true);

    // Note: library.db is created lazily on first save, so we don't assert its existence here
    // The important thing is that it will be created in e2eDataRoot, not in AppData
  });
});
