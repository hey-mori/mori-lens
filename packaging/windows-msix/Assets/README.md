# Store Assets

This directory holds icon assets for MSIX Store packaging.

## Required Assets

| File | Size | Status |
|---|---|---|
| `StoreLogo.png` | 50x50 | Generated from product icon |
| `Square44x44Logo.png` | 44x44 | Generated from product icon |
| `Square150x150Logo.png` | 150x150 | Generated from product icon |
| `Wide310x150Logo.png` | 310x150 | Generated from product icon |

## Notes

- These are required for Microsoft Store submission
- `scripts/generate-product-assets.py` draws each asset at its own size from the same geometry as `apps/lens/icons/icon.png`
- The MSIX packages structurally with these assets and they render correctly
