#!/usr/bin/env python3
"""Generate minimal valid PNG assets for package smoke testing.

Creates:
  - apps/lens/icons/icon.png (32x32, for Tauri bundling)
  - packaging/windows-msix/Assets/*.png (50x50, for MSIX packaging)

These are structural smoke assets — valid PNGs that prove the packaging
pipeline works, not production design assets.
"""

import struct
import zlib
import os
import sys


def make_png(width: int, height: int, filepath: str) -> None:
    """Generate a minimal valid PNG file (8-bit RGB, solid grey)."""

    def chunk(chunk_type: bytes, data: bytes) -> bytes:
        c = chunk_type + data
        crc = struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
        return struct.pack(">I", len(data)) + c + crc

    # IHDR: width, height, bit depth 8, color type 6 (RGBA)
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    # IDAT: uncompressed rows with filter byte 0
    raw_data = b""
    for _ in range(height):
        raw_data += b"\x00" + b"\x80\x80\x80\xff" * width  # filter + grey+opaque RGBA
    compressed = zlib.compress(raw_data)

    os.makedirs(os.path.dirname(filepath) or ".", exist_ok=True)
    with open(filepath, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", ihdr_data))
        f.write(chunk(b"IDAT", compressed))
        f.write(chunk(b"IEND", b""))


def main() -> None:
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    # Tauri icon (1024x1024 for macOS .app bundling — Tauri needs large PNG)
    icon_path = os.path.join(project_root, "apps", "lens", "icons", "icon.png")
    make_png(1024, 1024, icon_path)
    print(f"Generated: {icon_path}")

    # MSIX Store assets (50x50 each)
    assets_dir = os.path.join(project_root, "packaging", "windows-msix", "Assets")
    for name in [
        "StoreLogo.png",
        "Square44x44Logo.png",
        "Square150x150Logo.png",
        "Wide310x150Logo.png",
    ]:
        path = os.path.join(assets_dir, name)
        make_png(50, 50, path)
        print(f"Generated: {path}")

    print("All smoke assets generated successfully.")


if __name__ == "__main__":
    main()
