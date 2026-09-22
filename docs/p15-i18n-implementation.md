# P15 i18n Implementation Summary

**Date:** 2026-09-22  
**Status:** Phase 1 Complete (Core i18n Infrastructure + UI Integration)  

## Overview

Implemented a minimal, dependency-free internationalization (i18n) system for the Control Center UI. This Phase 1 establishes the foundation for full localization without requiring a heavy framework.

## Architecture

### Core Design Principles

1. **Minimal footprint** - No external dependencies, pure JavaScript
2. **Deterministic fallback** - Always falls back to English if key missing
3. **Persistent locale** - Stores user preference in localStorage
4. **Simple interpolation** - Supports `{param}` replacement
5. **Test-driven** - All functionality covered by unit tests

### File Structure

```
apps/lens/web/js/
├── i18n.js          # Core i18n module (exports: initI18n, getLocale, setLocale, t, hasKey, getKeys)
└── i18n.test.js     # Unit tests (8 tests, all passing)
```

### API

```javascript
// Initialize i18n (detects locale from localStorage or system)
initI18n()

// Get current locale
getLocale() // Returns: 'en' | 'ko'

// Set locale (persists to localStorage, triggers 'locale-changed' event)
setLocale(locale)

// Translate key with optional interpolation
t(key, params) // Returns: translated string

// Check if key exists
hasKey(key) // Returns: boolean

// Get all keys for a locale (useful for testing)
getKeys(locale) // Returns: string[]
```

### Translation Coverage

The i18n module covers all UI sections:
- **app.*** - Application-level strings (title, skip link)
- **nav.*** - Navigation menu items
- **status.*** - Status page strings
- **library.*** - Library page strings (title, filters, buttons)
- **review.*** - Review page strings
- **recent.*** - Recent page strings
- **history.*** - History page strings
- **general.*** - General settings (theme, OCR, profile, history settings)
- **shortcuts.*** - Keyboard shortcuts page
- **models.*** - Models/settings page (engines, endpoints, test)
- **apps.*** - Per-application rules page
- **about.*** - About page and danger zone
- **common.*** - Shared UI elements (buttons, labels)

Total: ~100 translation keys per language (en + ko)

## HTML Integration

All UI sections in `index.html` use `data-i18n` attributes for text content,
`data-i18n-aria` for ARIA labels, and `data-i18n-title` for title attributes.
The inline module script initializes i18n, applies translations on load, and
re-applies them on `locale-changed` events.

## Testing

Run tests:
```bash
cd apps/lens/web
node js/i18n.test.js
```

## Design Decisions

### Why Not Use a Framework?

1. **Bundle size** - No external dependencies means faster load times
2. **Simplicity** - Easy to understand and maintain
3. **Control** - Full control over behavior and edge cases
4. **Sufficiency** - Current needs don't require advanced features (pluralization, etc.)

### Why localStorage?

1. **Persistence** - User preference survives page reloads
2. **Simplicity** - No backend required
3. **Performance** - Instant access, no network requests

### Why Only English and Korean?

1. **Current user base** - Primary languages for Mori Lens users
2. **Incremental approach** - Easy to add more languages later
3. **Testing scope** - Manageable for initial implementation

## Pending (Future Work)

- Add language selector dropdown to the UI
- Integrate i18n into JavaScript modules (status.js, library.js, etc.)
  for dynamically generated strings
- Accessibility audit with screen readers
- Keyboard shortcut documentation strings
