describe('Mori Lens Tauri E2E - Locale Switching', () => {
  it('switches interface language from English to Korean', async () => {
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

    // Navigate to Settings
    const settingsBtn = await $('button[data-section="settings"]');
    await settingsBtn.click();
    await browser.pause(500);

    // Find the language selector
    const languageSelect = await $('#f-interface-lang');
    await languageSelect.waitForExist({ timeout: 5000 });

    // Get current value
    const currentValue = await languageSelect.getValue();
    console.log(`Initial language: ${currentValue}`);

    // Switch to Korean if not already
    if (currentValue !== 'ko') {
      await languageSelect.selectByAttribute('value', 'ko');
      await browser.pause(1000);

      // Verify the value changed
      const newValue = await languageSelect.getValue();
      expect(newValue).toBe('ko');
    }

    // Switch back to English
    await languageSelect.selectByAttribute('value', 'en');
    await browser.pause(1000);

    // Verify it changed back
    const finalValue = await languageSelect.getValue();
    expect(finalValue).toBe('en');
  });
});
