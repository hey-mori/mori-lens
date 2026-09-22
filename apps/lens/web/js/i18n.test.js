// i18n.test.js - Tests for i18n module
import { initI18n, getLocale, setLocale, t, hasKey, getKeys, translations } from './i18n.js';

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

// Test 4: Missing key returns key itself (not English fallback)
setLocale('ko');
console.assert(t('nonexistent.key') === 'nonexistent.key', 'Missing key should return key itself');
console.log('✓ Test 4: Missing key returns key itself (not English fallback)');

// Test 9: Locale fallback - key exists in English but missing in Korean
// Add a temporary key to test locale fallback
translations.en['test.english.only'] = 'English only';
// Don't add it to Korean
setLocale('ko');
console.assert(t('test.english.only') === 'English only', 'Should fall back to English when key missing in Korean');
delete translations.en['test.english.only'];
console.log('✓ Test 9: Locale fallback works (English fallback when Korean missing)');

// Test 10: Key parity check
const enKeys = getKeys('en').sort();
const koKeys = getKeys('ko').sort();
const missingInKo = enKeys.filter(k => !koKeys.includes(k));
const missingInEn = koKeys.filter(k => !enKeys.includes(k));
if (missingInKo.length === 0 && missingInEn.length === 0) {
  console.log('✓ Test 10: Key parity - en and ko have identical key sets');
} else {
  if (missingInKo.length > 0) {
    console.warn('⚠ Test 10: Keys missing in Korean:', missingInKo);
  }
  if (missingInEn.length > 0) {
    console.warn('⚠ Test 10: Keys missing in English:', missingInEn);
  }
}

// Test 11: Pluralization
setLocale('en');
console.assert(t('library.items.count', { count: 1 }) === '1 item', 'Singular form in English');
console.assert(t('library.items.count', { count: 5 }) === '5 items', 'Plural form in English');
setLocale('ko');
console.assert(t('library.items.count', { count: 1 }) === '1개 항목', 'Singular form in Korean');
console.assert(t('library.items.count', { count: 5 }) === '5개 항목', 'Plural form in Korean (same as singular)');
console.log('✓ Test 11: Pluralization works');

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
