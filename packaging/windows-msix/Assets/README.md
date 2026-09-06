# Store Assets

This directory holds icon assets for MSIX Store packaging.

## Required Assets

| File | Size | Status |
|---|---|---|
| `StoreLogo.png` | 50×50 | ASSET BLOCKER — needs real design |
| `Square44x44Logo.png` | 44×44 | ASSET BLOCKER |
| `Square150x150Logo.png` | 150×150 | ASSET BLOCKER |
| `Wide310x150Logo.png` | 310×150 | ASSET BLOCKER |

## Notes

- These are required for Microsoft Store submission
- The current product uses `icon.png` as a placeholder icon
- Real `.icns` (macOS) and Store-ready PNG assets are tracked as ASSET BLOCKER in the handoff
- Until real assets exist, MSIX packaging produces structurally valid but visually placeholder packages
