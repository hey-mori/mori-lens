// i18n.test.js - Tests for i18n module
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

// Tests
console.log('Running i18n tests...\n');

// Test 1: Default locale
localStorageMock.clear();
initI18n();
console.assert(getLocale() === 'en', 'Default locale should be en');
console.log('✓ Test 1: Default locale is en');

// Test 2: Set locale
setLocale('ko');
console.assert(getLocale() === 'ko', 'Locale should be ko after setLocale');
console.assert(localStorageMock.getItem('lens-locale') === 'ko', 'Locale should be persisted');
console.log('✓ Test 2: setLocale works and persists');

// Test 3: Translation lookup
setLocale('en');
console.assert(t('nav.status') === 'Status', 'English translation should work');
setLocale('ko');
console.assert(t('nav.status') === '상태', 'Korean translation should work');
console.log('✓ Test 3: Translation lookup works');

// Test 4: Fallback to English for missing keys
setLocale('ko');
console.assert(t('nonexistent.key') === 'nonexistent.key', 'Missing key should return key itself');
console.log('✓ Test 4: Missing key returns key itself');

// Test 5: Interpolation
setLocale('en');
const result = t('status.health.failed', { error: 'timeout' });
console.assert(result === 'Health check failed: timeout', 'Interpolation should work');
console.log('✓ Test 5: Interpolation works');

// Test 6: hasKey
setLocale('en');
console.assert(hasKey('nav.status') === true, 'hasKey should return true for existing key');
console.assert(hasKey('nonexistent.key') === false, 'hasKey should return false for missing key');
console.log('✓ Test 6: hasKey works');

// Test 7: getKeys
const keys = getKeys('en');
console.assert(keys.includes('nav.status'), 'getKeys should return all keys');
console.assert(keys.length > 0, 'getKeys should return non-empty array');
console.log('✓ Test 7: getKeys works');

// Test 8: Invalid locale fallback
setLocale('invalid');
console.assert(getLocale() === 'en', 'Invalid locale should fall back to en');
console.log('✓ Test 8: Invalid locale falls back to en');

console.log('\n✅ All i18n tests passed!');
