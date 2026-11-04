#!/usr/bin/env python3
"""
Generate the Mori Lens application icons.

Design: the Mori tile, the same rounded square and purple gradient as the Mori
app icon, carrying a white lens: a ring with a handle, and the Mori star inside
the ring. The tile and star say which family Lens belongs to, the lens says
which member it is. docs/assets/lens-mark.svg is the same drawing as a vector,
for the README.

Small sizes are drawn, not shrunk. At 32 pixels and below the star inside the ring
reads as a plus sign, a zoom-in button, so the 16 to 32 pixel icons carry
the ring and handle alone, drawn heavier.

Every size is rendered at 8x and reduced with a Lanczos filter, so curves are
anti-aliased at every size Windows asks for.

Output:
- apps/lens/icons/icon.png (1024x1024, with the macOS safe-area margin)
- apps/lens/icons/icon.ico (16 to 256, near full bleed, as Windows draws them)
- apps/lens/icons/icon.icns (from icon.png)
- packaging/windows-msix/Assets/*.png

Usage:
    python scripts/generate-product-assets.py

Deterministic for a given Pillow version, which scripts/requirements-assets.txt
pins.
"""

import math
import struct
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

try:
    from PIL import Image, ImageDraw
except ImportError:
    print("ERROR: Pillow is required. Install with: pip install Pillow", file=sys.stderr)
    sys.exit(1)

REPO_ROOT = Path(__file__).parent.parent.absolute()

# Rendering scale. Every icon is drawn this many times larger, then reduced.
SUPERSAMPLE = 8

# The Mori tile gradient, top left to bottom right, from public/mori-mark.svg
# in hey-mori/mori.
GRADIENT = [(0.0, (0x5C, 0x52, 0xF4)), (0.55, (0x71, 0x5D, 0xEB)), (1.0, (0xAA, 0x86, 0xF5))]

# Tile corner radius in the 1024 design box, as in the Mori mark.
TILE_RADIUS = 152

# Margins, as a fraction of the canvas on each side. macOS icons sit inside
# Apple's grid; Windows draws its icons close to full bleed.
MARGIN_MACOS = 0.098
MARGIN_WINDOWS = 0.03

# The lens, in the 1024 design box. The full drawing, for 40 pixels and up.
FULL = {
    "centre": 468,
    "ring_radius": 235,  # to the middle of the stroke
    "ring_width": 74,
    "handle_end": 800,
    "handle_width": 96,
    "star_scale": 0.46,
}
# The small drawing, for 32 pixels and below: no star, heavier strokes.
SMALL = {
    "centre": 452,
    "ring_radius": 248,
    "ring_width": 104,
    "handle_end": 820,
    "handle_width": 124,
    "star_scale": 0.0,
}
SMALL_UP_TO = 32

# The Mori star, as the cubic segments of public/mori-mark.svg in hey-mori/mori.
STAR_SEGMENTS = [
    ((512, 226), (552, 226), (566, 263), (584, 337)),
    ((584, 337), (606, 428), (638, 462), (722, 485)),
    ((722, 485), (787, 502), (806, 517), (806, 536)),
    ((806, 536), (806, 561), (775, 572), (714, 588)),
    ((714, 588), (630, 610), (601, 643), (579, 734)),
    ((579, 734), (561, 808), (552, 836), (512, 836)),
    ((512, 836), (472, 836), (463, 808), (445, 734)),
    ((445, 734), (423, 643), (394, 610), (310, 588)),
    ((310, 588), (249, 572), (218, 561), (218, 536)),
    ((218, 536), (218, 517), (237, 502), (302, 485)),
    ((302, 485), (386, 462), (418, 428), (440, 337)),
    ((440, 337), (458, 263), (472, 226), (512, 226)),
]
STAR_CENTRE = (512, 531)
WHITE = (255, 255, 255, 255)


def star_points(steps=32):
    points = []
    for p0, p1, p2, p3 in STAR_SEGMENTS:
        for i in range(steps):
            t = i / steps
            u = 1 - t
            points.append(
                (
                    u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
                    u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1],
                )
            )
    return points


def gradient(size):
    """The diagonal tile gradient at `size`, computed small and smoothly enlarged."""
    base = 256
    img = Image.new("RGB", (base, base))
    pixels = img.load()
    for y in range(base):
        for x in range(base):
            t = (x + y) / (2 * (base - 1))
            for (o0, c0), (o1, c1) in zip(GRADIENT, GRADIENT[1:]):
                if o0 <= t <= o1:
                    f = (t - o0) / (o1 - o0)
                    pixels[x, y] = tuple(round(c0[i] + (c1[i] - c0[i]) * f) for i in range(3))
                    break
    return img.resize((size, size), Image.BICUBIC)


def render(size, margin):
    """The icon at `size` pixels square, tile inset by `margin` on each side."""
    lens = SMALL if size <= SMALL_UP_TO else FULL
    canvas = size * SUPERSAMPLE
    inset = margin * canvas
    tile = canvas - 2 * inset
    unit = tile / 1024

    def at(x, y):
        return (inset + x * unit, inset + y * unit)

    img = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    mask = Image.new("L", (canvas, canvas), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [inset, inset, canvas - inset - 1, canvas - inset - 1],
        radius=TILE_RADIUS * unit,
        fill=255,
    )
    img.paste(gradient(canvas), (0, 0), mask)

    draw = ImageDraw.Draw(img)
    centre = lens["centre"]
    radius = lens["ring_radius"]

    # The handle first, from inside the ring so the two overlap, down the
    # diagonal to a rounded end.
    start = centre + radius * math.cos(math.pi / 4)
    end = lens["handle_end"]
    half = lens["handle_width"] * unit / 2
    draw.line([at(start, start), at(end, end)], fill=WHITE, width=round(2 * half))
    ex, ey = at(end, end)
    draw.ellipse([ex - half, ey - half, ex + half, ey + half], fill=WHITE)

    outer = radius + lens["ring_width"] / 2
    x0, y0 = at(centre - outer, centre - outer)
    x1, y1 = at(centre + outer, centre + outer)
    draw.ellipse([x0, y0, x1, y1], outline=WHITE, width=round(lens["ring_width"] * unit))

    scale = lens["star_scale"]
    if scale:
        draw.polygon(
            [
                at(centre + (x - STAR_CENTRE[0]) * scale, centre + (y - STAR_CENTRE[1]) * scale)
                for x, y in star_points()
            ],
            fill=WHITE,
        )

    return img.resize((size, size), Image.LANCZOS)


def write_icns(png_path, icns_path):
    """A minimal ICNS holding the 1024 PNG as its ic10 entry."""
    png = png_path.read_bytes()
    entry = b"ic10" + struct.pack(">I", 8 + len(png)) + png
    icns_path.write_bytes(b"icns" + struct.pack(">I", 8 + len(entry)) + entry)


def generate_application_icons():
    icons = REPO_ROOT / "apps" / "lens" / "icons"
    icons.mkdir(parents=True, exist_ok=True)

    png = icons / "icon.png"
    print(f"  {png.relative_to(REPO_ROOT)} (1024x1024)")
    render(1024, MARGIN_MACOS).save(png, "PNG")

    ico = icons / "icon.ico"
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    print(f"  {ico.relative_to(REPO_ROOT)} ({', '.join(map(str, sizes))})")
    images = [render(size, MARGIN_WINDOWS) for size in sizes]
    images[-1].save(
        ico,
        format="ICO",
        sizes=[(size, size) for size in sizes],
        append_images=images[:-1],
    )

    icns = icons / "icon.icns"
    print(f"  {icns.relative_to(REPO_ROOT)}")
    write_icns(png, icns)


def generate_msix_assets():
    assets = REPO_ROOT / "packaging" / "windows-msix" / "Assets"
    assets.mkdir(parents=True, exist_ok=True)

    for name, size in {
        "StoreLogo.png": 50,
        "Square44x44Logo.png": 44,
        "Square150x150Logo.png": 150,
    }.items():
        print(f"  {(assets / name).relative_to(REPO_ROOT)} ({size}x{size})")
        render(size, MARGIN_WINDOWS).save(assets / name, "PNG")

    # The wide tile: the icon centred on a transparent banner.
    width, height = 310, 150
    icon_size = 110
    wide = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    icon = render(icon_size, MARGIN_WINDOWS)
    wide.paste(icon, ((width - icon_size) // 2, (height - icon_size) // 2), icon)
    print(f"  {(assets / 'Wide310x150Logo.png').relative_to(REPO_ROOT)} ({width}x{height})")
    wide.save(assets / "Wide310x150Logo.png", "PNG")


def main():
    print("Generating Mori Lens icons")
    generate_application_icons()
    generate_msix_assets()
    print("Done")


if __name__ == "__main__":
    main()
