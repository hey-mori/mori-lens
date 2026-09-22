// i18n.vitest.test.js - Vitest tests for i18n module
import { describe, it, expect, beforeEach } from 'vitest';
import { initI18n, getLocale, setLocale, t, hasKey, getKeys } from './i18n.js';

// Mock localStorage
const localStorageMock = (() => {
  let store = {};
  return {
    getItem: (key) => store[key] || null,
    setItem: (key, value) => { store[key] = value; },
    clear: () => { store = {}; },
  };
})();

Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });

// Mock navigator
Object.defineProperty(globalThis, 'navigator', {
  value: { language: 'en-US' },
  writable: true,
});

// Mock document
Object.defineProperty(globalThis, 'document', {
  value: {
    dispatchEvent: () => {},
  },
  writable: true,
});

describe('i18n module', () => {
  beforeEach(() => {
    localStorageMock.clear();
    initI18n();
  });

  it('defaults to en locale', () => {
    expect(getLocale()).toBe('en');
  });

  it('persists locale changes', () => {
    setLocale('ko');
    expect(getLocale()).toBe('ko');
    expect(localStorageMock.getItem('lens-locale')).toBe('ko');
  });

  it('translates keys correctly', () => {
    setLocale('en');
    expect(t('nav.status')).toBe('Status');

    setLocale('ko');
    expect(t('nav.status')).toBe('상태');
  });

  it('checks key existence', () => {
    expect(hasKey('nav.status')).toBe(true);
    expect(hasKey('nonexistent.key')).toBe(false);
  });

  it('returns all keys', () => {
    const keys = getKeys();
    expect(keys.length).toBeGreaterThan(0);
    expect(keys).toContain('nav.status');
  });
});
