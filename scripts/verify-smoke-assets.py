#!/usr/bin/env python3
"""Verify structural validity of generated package smoke assets.

Checks PNG signatures/CRC/dimensions, ICO directory structure,
ICNS entry structure with embedded PNG validity, and MSIX asset sizes.

stdlib-only — no external packages required.
Exit 0 on success, non-zero on any failure.
"""

import struct
import zlib
import os
import sys


class VerifyError(Exception):
    pass


def _verify_png_chunks(data: bytes, expected_w: int, expected_h: int, label: str) -> None:
    """Verify PNG signature, chunk structure, CRC, and dimensions."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise VerifyError(f"{label}: invalid PNG signature")

    pos = 8
    found_ihdr = False
    found_iend = False

    while pos < len(data):
        if pos + 8 > len(data):
            raise VerifyError(f"{label}: truncated chunk header at offset {pos}")
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        chunk_type = data[pos + 4 : pos + 8]

        if pos + 12 + length > len(data):
            raise VerifyError(f"{label}: truncated chunk '{chunk_type}' at offset {pos}")

        chunk_data = data[pos + 8 : pos + 8 + length]
        crc_stored = struct.unpack(">I", data[pos + 8 + length : pos + 12 + length])[0]
        crc_computed = zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF

        if crc_stored != crc_computed:
            raise VerifyError(
                f"{label}: CRC mismatch in {chunk_type!r} chunk "
                f"(stored={crc_stored:#010x}, computed={crc_computed:#010x})"
            )

        if chunk_type == b"IHDR":
            if length < 13:
                raise VerifyError(f"{label}: IHDR too short")
            w, h = struct.unpack(">II", chunk_data[:8])
            if w != expected_w or h != expected_h:
                raise VerifyError(
                    f"{label}: IHDR dimensions {w}x{h} != expected {expected_w}x{expected_h}"
                )
            found_ihdr = True
        elif chunk_type == b"IEND":
            found_iend = True
            break

        pos += 12 + length

    if not found_ihdr:
        raise VerifyError(f"{label}: missing IHDR chunk")
    if not found_iend:
        raise VerifyError(f"{label}: missing IEND chunk")


def verify_png(filepath: str, expected_w: int, expected_h: int) -> None:
    """Verify a standalone PNG file."""
    if not os.path.isfile(filepath):
        raise VerifyError(f"missing: {filepath}")
    with open(filepath, "rb") as f:
        data = f.read()
    _verify_png_chunks(data, expected_w, expected_h, filepath)


def verify_ico(filepath: str) -> None:
    """Verify ICO directory structure."""
    if not os.path.isfile(filepath):
        raise VerifyError(f"missing: {filepath}")
    with open(filepath, "rb") as f:
        data = f.read()
    if len(data) < 6:
        raise VerifyError(f"{filepath}: too short for ICONDIR")
    reserved, img_type, count = struct.unpack("<HHH", data[:6])
    if reserved != 0:
        raise VerifyError(f"{filepath}: ICONDIR reserved != 0")
    if img_type != 1:
        raise VerifyError(f"{filepath}: ICONDIR type {img_type} != 1 (ICO)")
    if count < 1:
        raise VerifyError(f"{filepath}: ICONDIR count == 0")
    if len(data) < 6 + count * 16:
        raise VerifyError(f"{filepath}: truncated ICONDIRENTRY")


def verify_icns(filepath: str) -> None:
    """Verify ICNS container and embedded PNG validity."""
    if not os.path.isfile(filepath):
        raise VerifyError(f"missing: {filepath}")
    with open(filepath, "rb") as f:
        data = f.read()

    if len(data) < 8:
        raise VerifyError(f"{filepath}: too short for ICNS header")
    magic = data[:4]
    if magic != b"icns":
        raise VerifyError(f"{filepath}: magic {magic!r} != 'icns'")

    declared_len = struct.unpack(">I", data[4:8])[0]
    if declared_len != len(data):
        raise VerifyError(
            f"{filepath}: declared length {declared_len} != actual {len(data)}"
        )

    pos = 8
    entry_count = 0
    while pos < len(data):
        if pos + 8 > len(data):
            raise VerifyError(f"{filepath}: truncated entry header at offset {pos}")
        ostype = data[pos : pos + 4]
        entry_len = struct.unpack(">I", data[pos + 4 : pos + 8])[0]
        if entry_len < 8:
            raise VerifyError(f"{filepath}: entry {ostype!r} length {entry_len} < 8")
        if pos + entry_len > len(data):
            raise VerifyError(f"{filepath}: entry {ostype!r} extends past end of file")

        png_data = data[pos + 8 : pos + entry_len]
        if png_data[:4] != b"\x89PNG":
            raise VerifyError(f"{filepath}: entry {ostype!r} does not contain a PNG")

        # Determine expected dimensions from OSType
        dim_map = {
            b"icp4": (16, 16),
            b"icp5": (32, 32),
            b"icp6": (64, 64),
            b"ic07": (128, 128),
            b"ic08": (256, 256),
            b"ic09": (512, 512),
            b"ic10": (1024, 1024),
            b"ic14": (512, 512),
        }
        expected = dim_map.get(ostype)
        if expected:
            _verify_png_chunks(
                png_data, expected[0], expected[1],
                f"{filepath}[{ostype!r}]"
            )

        entry_count += 1
        pos += entry_len

    if entry_count == 0:
        raise VerifyError(f"{filepath}: no entries found")


def main() -> None:
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    errors = []

    # Tauri icons
    icon_png = os.path.join(project_root, "apps", "lens", "icons", "icon.png")
    icon_ico = os.path.join(project_root, "apps", "lens", "icons", "icon.ico")
    icon_icns = os.path.join(project_root, "apps", "lens", "icons", "icon.icns")

    for filepath, w, h in [
        (icon_png, 1024, 1024),
    ]:
        try:
            verify_png(filepath, w, h)
            print(f"  PNG OK: {os.path.relpath(filepath, project_root)} ({w}x{h})")
        except VerifyError as e:
            errors.append(str(e))
            print(f"  PNG FAIL: {e}")

    try:
        verify_ico(icon_ico)
        print(f"  ICO OK: {os.path.relpath(icon_ico, project_root)}")
    except VerifyError as e:
        errors.append(str(e))
        print(f"  ICO FAIL: {e}")

    try:
        verify_icns(icon_icns)
        print(f"  ICNS OK: {os.path.relpath(icon_icns, project_root)}")
    except VerifyError as e:
        errors.append(str(e))
        print(f"  ICNS FAIL: {e}")

    # MSIX assets
    msix_assets = {
        "StoreLogo.png": (50, 50),
        "Square44x44Logo.png": (44, 44),
        "Square150x150Logo.png": (150, 150),
        "Wide310x150Logo.png": (310, 150),
    }
    assets_dir = os.path.join(project_root, "packaging", "windows-msix", "Assets")
    for name, (w, h) in msix_assets.items():
        filepath = os.path.join(assets_dir, name)
        try:
            verify_png(filepath, w, h)
            print(f"  MSIX OK: Assets/{name} ({w}x{h})")
        except VerifyError as e:
            errors.append(str(e))
            print(f"  MSIX FAIL: {e}")

    if errors:
        print(f"\nFAILED: {len(errors)} error(s)")
        sys.exit(1)
    else:
        print("\nAll smoke assets verified.")
        sys.exit(0)


if __name__ == "__main__":
    main()
