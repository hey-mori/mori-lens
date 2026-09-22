describe('Mori Lens Tauri E2E - Navigation', () => {
  it('navigates between sections', async () => {
    // Wait for app to load
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

    // Verify initial section (Status)
    const statusBtn = await $('button[data-section="status"]');
    await statusBtn.waitForExist({ timeout: 5000 });

    // Click on General/Settings
    const settingsBtn = await $('button[data-section="settings"]');
    await settingsBtn.click();

    // Wait for section to change
    await browser.pause(500);

    // Verify settings section is active
    const isActive = await settingsBtn.getAttribute('aria-current');
    expect(isActive).toBe('page');

    // Click on Library
    const libraryBtn = await $('button[data-section="library"]');
    await libraryBtn.click();

    await browser.pause(500);

    // Verify library section is active
    const isLibraryActive = await libraryBtn.getAttribute('aria-current');
    expect(isLibraryActive).toBe('page');
  });
});
