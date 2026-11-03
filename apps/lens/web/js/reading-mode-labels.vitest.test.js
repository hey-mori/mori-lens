// Labels for the reading modes (allow, modifier, deny).
//
// The modifier mode was labelled "Modifier" in English and "수정자" in Korean.
// "수정자" is the grammatical or editing sense of the word, not the keyboard
// one, so the Korean settings screen named the mode with the same wrong sense
// a model once produced when it rendered "not reading here without the
// modifier" as "제시문에 조사 없이 읽지 마세요". The per-application selector
// also had no translation keys, so it stayed in English in Korean.
import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { initI18n, setLocale, t } from './i18n.js';

const store = {};
Object.defineProperty(globalThis, 'localStorage', {
  value: {
    getItem: (key) => store[key] ?? null,
    setItem: (key, value) => { store[key] = value; },
    clear: () => { for (const key of Object.keys(store)) delete store[key]; },
  },
});
Object.defineProperty(globalThis, 'navigator', { value: { language: 'en-US' }, writable: true });
Object.defineProperty(globalThis, 'document', { value: { dispatchEvent: () => {} }, writable: true });

// Vitest runs from apps/lens, where vitest.config.js lives.
const html = readFileSync(resolve(process.cwd(), 'web/index.html'), 'utf8');
const modes = ['allow', 'modifier', 'deny'];

describe('reading mode labels', () => {
  beforeEach(() => {
    localStorage.clear();
    initI18n();
  });

  it('describe what each mode does in English', () => {
    setLocale('en');
    expect(t('general.default.tier.allow')).toBe('Always read');
    expect(t('general.default.tier.modifier')).toBe('Only while the modifier key is held');
    expect(t('general.default.tier.deny')).toBe('Never read');
  });

  it('use the keyboard sense of "modifier" in Korean', () => {
    setLocale('ko');
    const label = t('general.default.tier.modifier');
    expect(label).toContain('보조 키');
    expect(label).not.toContain('수정자');
  });

  it('give every mode option in the page a translation key', () => {
    for (const mode of modes) {
      const options = html.match(new RegExp(`<option value="${mode}"[^>]*>`, 'g')) ?? [];
      expect(options.length).toBeGreaterThan(0);
      for (const option of options) {
        expect(option).toContain(`data-i18n="general.default.tier.${mode}"`);
      }
    }
  });
});
