/**
 * P19B E2E Test 01: Minimal Native Launch
 * 
 * This test verifies that the compiled Mori Lens Tauri app can be launched
 * and the Control Center WebView loads the production frontend.
 * 
 * Evidence: APPLICATION-SHELL E2E PASS — MINIMAL
 */

describe('Mori Lens Tauri E2E - Launch', () => {
  it('launches the app and loads Control Center', async () => {
    // The Tauri service automatically launches the app
    // We just need to verify the WebView loaded
    
    // Wait for the main window to be available
    await browser.waitUntil(
      async () => {
        const title = await browser.getTitle();
        return title && title.length > 0;
      },
      {
        timeout: 10000,
        timeoutMsg: 'Expected app to load within 10s'
      }
    );
    
    // Verify we're on the Control Center by checking for a production element
    // The nav element with id="nav" is unique to the production Control Center
    const nav = await $('#nav');
    await nav.waitForExist({ timeout: 5000 });
    
    // Verify navigation buttons exist (these are from production index.html)
    const statusBtn = await $('button[data-section="status"]');
    const libraryBtn = await $('button[data-section="library"]');
    const settingsBtn = await $('button[data-section="settings"]');
    
    expect(await statusBtn.isExisting()).toBe(true);
    expect(await libraryBtn.isExisting()).toBe(true);
    expect(await settingsBtn.isExisting()).toBe(true);
  });
});
