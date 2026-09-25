import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['web/js/**/*.vitest.test.js'],
    exclude: ['node_modules/**', 'web/js/i18n.test.js'],
  },
});
