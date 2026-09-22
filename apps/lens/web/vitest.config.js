import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['js/**/*.vitest.test.js'],
    exclude: ['node_modules/**', 'js/i18n.test.js'],
  },
});
