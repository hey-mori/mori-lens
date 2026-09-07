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
    """Generate a minimal valid PNG file (8-bit RGBA, solid grey)."""

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


def make_ico(filepath: str) -> None:
    """Generate a minimal valid ICO file (16x16, 32-bit RGBA).
    
    ICO format: ICONDIR + ICONDIRENTRY + image data (BITMAPINFOHEADER + pixels + AND mask)
    """
    width, height = 16, 16
    bpp = 32
    
    # BITMAPINFOHEADER (40 bytes)
    bmp_header = struct.pack("<IiiHHIIiiII",
        40,              # biSize
        width,           # biWidth
        height * 2,      # biHeight (doubled for ICO - includes AND mask)
        1,               # biPlanes
        bpp,             # biBitCount
        0,               # biCompression (BI_RGB)
        0,               # biSizeImage (can be 0 for BI_RGB)
        0, 0,            # biXPPM, biYPPM
        0, 0             # biClrUsed, biClrImportant
    )
    
    # Pixel data (BGRA, bottom-up)
    pixels = b""
    for _ in range(height):
        for _ in range(width):
            pixels += b"\x80\x80\x80\xff"  # BGRA: grey, fully opaque
    
    # AND mask (1bpp, all zeros = fully opaque)
    and_mask = b"\x00" * ((width + 31) // 32 * 4 * height)
    
    # Image data
    image_data = bmp_header + pixels + and_mask
    
    # ICONDIRENTRY (16 bytes)
    icon_entry = struct.pack("<BBBBHHII",
        width if width < 256 else 0,   # bWidth (0 = 256)
        height if height < 256 else 0, # bHeight (0 = 256)
        0,               # bColorCount
        0,               # bReserved
        1,               # wPlanes
        bpp,             # wBitCount
        len(image_data), # dwBytesInRes
        22               # dwImageOffset (6 + 16 = ICONDIR + 1 ICONDIRENTRY)
    )
    
    # ICONDIR (6 bytes)
    icon_dir = struct.pack("<HHH",
        0,    # idReserved
        1,    # idType (1 = ICO)
        1     # idCount
    )
    
    with open(filepath, "wb") as f:
        f.write(icon_dir + icon_entry + image_data)


def main() -> None:
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    # Tauri icons (1024x1024 PNG for macOS .app bundling, 16x16 ICO for Windows)
    icon_png_path = os.path.join(project_root, "apps", "lens", "icons", "icon.png")
    icon_ico_path = os.path.join(project_root, "apps", "lens", "icons", "icon.ico")
    make_png(1024, 1024, icon_png_path)
    print(f"Generated: {icon_png_path}")
    make_ico(icon_ico_path)
    print(f"Generated: {icon_ico_path}")

    # MSIX Store assets — each at its required size for MakeAppx validation
    assets_dir = os.path.join(project_root, "packaging", "windows-msix", "Assets")
    asset_sizes = {
        "StoreLogo.png": 50,
        "Square44x44Logo.png": 44,
        "Square150x150Logo.png": 150,
        "Wide310x150Logo.png": 310,  # width; height is 150
    }
    for name, size in asset_sizes.items():
        path = os.path.join(assets_dir, name)
        if name == "Wide310x150Logo.png":
            make_png(310, 150, path)
        else:
            make_png(size, size, path)
        print(f"Generated: {path}")

    print("All smoke assets generated successfully.")


if __name__ == "__main__":
    main()
