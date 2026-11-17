#!/usr/bin/env python3
"""Turn a square, full-bleed PNG into a macOS .icns in the macOS icon layout.

    python3 make_icns.py SOURCE.png OUT.icns [--png OUT_1024.png]
                         [--body 824] [--radius 185] [--no-shadow]

macOS does not mask icons: whatever is in the file is what the Dock shows. So
the artwork is placed in a rounded square (Big Sur grid: 824 of 1024 px, corner
radius ~185) on a transparent canvas, with a soft drop shadow. Needs Pillow and
macOS `iconutil`.
"""
import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

CANVAS = 1024
SUPERSAMPLE = 4  # draw the mask big, shrink it, for smooth corners


def layout(source: Path, body: int, radius: int, shadow: bool) -> Image.Image:
    art = Image.open(source).convert("RGBA")
    if art.width != art.height:
        sys.exit(f"{source} is {art.width}x{art.height}; the source must be square.")
    if art.width < CANVAS:
        print(f"warning: source is {art.width}px, under {CANVAS}px; the icon will be soft", file=sys.stderr)
    art = art.resize((body, body), Image.LANCZOS)

    mask = Image.new("L", (body * SUPERSAMPLE,) * 2, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, body * SUPERSAMPLE - 1, body * SUPERSAMPLE - 1), radius * SUPERSAMPLE, fill=255
    )
    mask = mask.resize((body, body), Image.LANCZOS)

    at = ((CANVAS - body) // 2,) * 2
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    if shadow:
        glow = Image.new("L", (CANVAS, CANVAS), 0)
        glow.paste(mask, (at[0], at[1] + 12))
        glow = glow.filter(ImageFilter.GaussianBlur(14)).point(lambda v: int(v * 0.30))
        canvas.paste(Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 255)), (0, 0), glow)
    art.putalpha(mask)
    canvas.alpha_composite(art, at)
    return canvas


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--png", type=Path, help="also write the 1024px laid-out PNG here")
    ap.add_argument("--body", type=int, default=824)
    ap.add_argument("--radius", type=int, default=185)
    ap.add_argument("--no-shadow", action="store_true")
    a = ap.parse_args()

    canvas = layout(a.source, a.body, a.radius, not a.no_shadow)
    if a.png:
        canvas.save(a.png)
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            canvas.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
            canvas.resize((size * 2, size * 2), Image.LANCZOS).save(iconset / f"icon_{size}x{size}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(a.out)], check=True)
    print(f"wrote {a.out}")


if __name__ == "__main__":
    main()
