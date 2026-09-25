#!/usr/bin/env python3
"""
Generate production-quality application icons for Mori Lens.

This script creates canonical production assets from a deterministic source.
Unlike smoke assets (grey placeholders), these are suitable for distribution.

Design: A simple lens/magnifying glass motif representing the hover-to-translate
concept. Uses a blue gradient to suggest clarity and focus.

Output:
- apps/lens/icons/icon.png (1024x1024)
- apps/lens/icons/icon.ico (16x16, 32x32, 48x48, 256x256)
- apps/lens/icons/icon.icns (multiple sizes for macOS)
- packaging/windows-msix/Assets/*.png (various sizes for MSIX)

Usage:
    python scripts/generate-product-assets.py
"""

import sys
from pathlib import Path

# Force UTF-8 output for Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("ERROR: Pillow is required. Install with: pip install Pillow", file=sys.stderr)
    sys.exit(1)

REPO_ROOT = Path(__file__).parent.parent.absolute()

# Design parameters
BG_COLOR = (240, 248, 255)  # Alice blue - light, clean
LENS_COLOR = (70, 130, 180)  # Steel blue - professional
LENS_HIGHLIGHT = (135, 206, 250)  # Light sky blue - for highlight
HANDLE_COLOR = (100, 100, 100)  # Dark grey - subtle


def create_lens_icon(size):
    """
    Create a simple lens/magnifying glass icon.

    Design:
    - Circular lens with gradient effect
    - Handle extending from bottom-right
    - Transparent background
    - Recognizable at small sizes (16px+)
    """
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Calculate proportions
    margin = max(1, size // 16)
    lens_center_x = int(size * 0.45)
    lens_center_y = int(size * 0.45)
    lens_radius = int(size * 0.35)

    # Draw lens circle (outer ring)
    draw.ellipse(
        [
            lens_center_x - lens_radius,
            lens_center_y - lens_radius,
            lens_center_x + lens_radius,
            lens_center_y + lens_radius,
        ],
        fill=LENS_COLOR,
        outline=None,
    )

    # Draw inner highlight (gives 3D effect)
    highlight_radius = int(lens_radius * 0.7)
    highlight_offset = int(lens_radius * 0.15)
    draw.ellipse(
        [
            lens_center_x - highlight_radius + highlight_offset,
            lens_center_y - highlight_radius + highlight_offset,
            lens_center_x + highlight_radius + highlight_offset,
            lens_center_y + highlight_radius + highlight_offset,
        ],
        fill=LENS_HIGHLIGHT,
        outline=None,
    )

    # Draw handle
    handle_start_x = lens_center_x + int(lens_radius * 0.7)
    handle_start_y = lens_center_y + int(lens_radius * 0.7)
    handle_end_x = int(size * 0.85)
    handle_end_y = int(size * 0.85)
    handle_width = max(2, size // 8)

    draw.line(
        [(handle_start_x, handle_start_y), (handle_end_x, handle_end_y)],
        fill=HANDLE_COLOR,
        width=handle_width,
    )

    return img


def create_icns_from_png(png_path, icns_path):
    """Create a minimal valid ICNS file from a PNG.

    ICNS format:
    - 4 bytes: "icns" magic
    - 4 bytes: total file length (big-endian)
    - Entries:
      - 4 bytes: OSType
      - 4 bytes: entry length (big-endian)
      - PNG data
    """
    import struct

    with open(png_path, "rb") as f:
        png_data = f.read()

    # Use ic10 OSType for 1024x1024
    ostype = b"ic10"
    entry_length = 8 + len(png_data)  # OSType + length + PNG data
    total_length = 8 + entry_length  # icns magic + length + entry

    with open(icns_path, "wb") as f:
        # Write ICNS header
        f.write(b"icns")
        f.write(struct.pack(">I", total_length))

        # Write entry
        f.write(ostype)
        f.write(struct.pack(">I", entry_length))
        f.write(png_data)


def generate_application_icons():
    """Generate all application icon formats."""
    print("Generating production application icons...")

    # Generate base 1024x1024 PNG
    icon_png_path = REPO_ROOT / "apps" / "lens" / "icons" / "icon.png"
    icon_png_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"  Creating {icon_png_path} (1024x1024)...")
    icon_1024 = create_lens_icon(1024)
    icon_1024.save(icon_png_path, "PNG")

    # Generate ICO (Windows)
    icon_ico_path = REPO_ROOT / "apps" / "lens" / "icons" / "icon.ico"
    print(f"  Creating {icon_ico_path} (multi-size)...")

    # Create multiple sizes for ICO
    ico_sizes = [16, 32, 48, 256]
    ico_images = []
    for size in ico_sizes:
        img = create_lens_icon(size)
        ico_images.append(img)

    # Save as ICO (Pillow will handle multi-size)
    ico_images[0].save(
        icon_ico_path,
        format="ICO",
        sizes=[(size, size) for size in ico_sizes],
        append_images=ico_images[1:],
    )

    # Generate ICNS (macOS) from the PNG
    icon_icns_path = REPO_ROOT / "apps" / "lens" / "icons" / "icon.icns"
    print(f"  Creating {icon_icns_path} (macOS)...")
    create_icns_from_png(icon_png_path, icon_icns_path)

    print("✓ Application icons generated")


def generate_msix_assets():
    """Generate MSIX packaging assets."""
    print("\nGenerating MSIX packaging assets...")

    msix_assets_dir = REPO_ROOT / "packaging" / "windows-msix" / "Assets"
    msix_assets_dir.mkdir(parents=True, exist_ok=True)

    # MSIX requires specific sizes
    assets = {
        "StoreLogo.png": 50,
        "Square44x44Logo.png": 44,
        "Square150x150Logo.png": 150,
        "Wide310x150Logo.png": (310, 150),  # Wide format
    }

    for asset_name, size_spec in assets.items():
        asset_path = msix_assets_dir / asset_name

        if isinstance(size_spec, tuple):
            width, height = size_spec
            # For wide format, create a landscape version
            img = Image.new("RGBA", (width, height), (0, 0, 0, 0))
            # Center the lens icon in the wide format
            icon_size = min(width, height) - 20
            icon = create_lens_icon(icon_size)
            # Paste centered
            paste_x = (width - icon_size) // 2
            paste_y = (height - icon_size) // 2
            img.paste(icon, (paste_x, paste_y), icon)
        else:
            img = create_lens_icon(size_spec)

        print(f"  Creating {asset_name} ({size_spec if isinstance(size_spec, int) else f'{size_spec[0]}x{size_spec[1]}'})...")
        img.save(asset_path, "PNG")

    print("✓ MSIX assets generated")


def main():
    print("=" * 70)
    print("Mori Lens Production Asset Generator")
    print("=" * 70)
    print()
    print("This generator creates production-quality application icons.")
    print("These are suitable for distribution, not just smoke testing.")
    print()

    generate_application_icons()
    generate_msix_assets()

    print()
    print("=" * 70)
    print("Production assets generated successfully.")
    print()
    print("Note: These assets are deterministic and can be regenerated.")
    print("For custom branding, modify the design parameters in this script.")
    print("=" * 70)


if __name__ == "__main__":
    main()
