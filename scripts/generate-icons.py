#!/usr/bin/env python3
"""Generate the InLook icon set from a single design specification.

Output (committed to the repo):
    assets/inlook.png         1024x1024 master raster
    assets/inlook.ico         multi-resolution Windows icon
    assets/icons/inlook-N.png Linux hicolor-theme sizes (16, 32, 48, 64, 128, 256, 512)

The macOS .icns is built on the macOS runner during release (scripts/build-dmg.sh)
because creating one cleanly requires iconutil from the macOS SDK.

Re-run this script if the icon design changes:
    python scripts/generate-icons.py
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
ICON_DIR = ASSETS / "icons"

# Design tokens — keep aligned with the HTML chrome rendered in src/render.rs
COLOR_BG = (44, 82, 130, 255)      # #2c5282 — accent blue
COLOR_FG = (255, 255, 255, 255)    # white monogram
RADIUS_RATIO = 0.195               # rounded-square corner radius / size
MONOGRAM = "IL"
MONOGRAM_RATIO = 0.55              # text height / size
MONOGRAM_Y_OFFSET = -0.02          # nudge up slightly for optical centering

FONT_CANDIDATES = [
    "C:/Windows/Fonts/seguisb.ttf",          # Segoe UI Semibold (Windows)
    "C:/Windows/Fonts/segoeuib.ttf",         # Segoe UI Bold
    "C:/Windows/Fonts/arialbd.ttf",          # Arial Bold
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
]

PNG_SIZES = [16, 32, 48, 64, 128, 256, 512, 1024]
ICO_SIZES = [16, 32, 48, 64, 128, 256]


def find_font(pixel_size: int) -> ImageFont.ImageFont:
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            return ImageFont.truetype(path, pixel_size)
    print("WARN: no bold TTF font found, falling back to default", file=sys.stderr)
    return ImageFont.load_default()


def draw_icon(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    radius = int(size * RADIUS_RATIO)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=COLOR_BG)

    font = find_font(int(size * MONOGRAM_RATIO))
    bbox = d.textbbox((0, 0), MONOGRAM, font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    x = (size - text_w) // 2 - bbox[0]
    y = (size - text_h) // 2 - bbox[1] + int(size * MONOGRAM_Y_OFFSET)
    d.text((x, y), MONOGRAM, fill=COLOR_FG, font=font)
    return img


def write_multi_ico(path: Path, frames: list[Image.Image]) -> None:
    """Write a Windows ICO containing the given frames, each as its own PNG.

    Modern Windows (Vista+) supports PNG-compressed entries inside an ICO
    container — the storage layout is the standard ICONDIR + ICONDIRENTRY
    headers, but each frame's payload is a PNG stream rather than a DIB.
    This keeps file size small while preserving crisp re-renders at each
    target size.
    """
    import io
    import struct

    payloads: list[bytes] = []
    entries: list[tuple[int, int]] = []  # (width, height) per frame
    for img in frames:
        buf = io.BytesIO()
        img.save(buf, format="PNG", optimize=True)
        payloads.append(buf.getvalue())
        entries.append(img.size)

    # ICONDIR header: reserved(2) | type(2) | count(2)
    header = struct.pack("<HHH", 0, 1, len(frames))

    # Each ICONDIRENTRY is 16 bytes. The image data follows the directory.
    dir_size = 6 + 16 * len(frames)
    offset = dir_size
    directory = bytearray()
    for (w, h), payload in zip(entries, payloads):
        # 0 represents 256 in the byte-sized width/height fields
        w_byte = 0 if w >= 256 else w
        h_byte = 0 if h >= 256 else h
        directory += struct.pack(
            "<BBBBHHII",
            w_byte, h_byte,
            0,        # color palette (0 for no palette)
            0,        # reserved
            1,        # color planes
            32,       # bits per pixel
            len(payload),
            offset,
        )
        offset += len(payload)

    with open(path, "wb") as f:
        f.write(header)
        f.write(bytes(directory))
        for payload in payloads:
            f.write(payload)


def main() -> int:
    ASSETS.mkdir(parents=True, exist_ok=True)
    ICON_DIR.mkdir(parents=True, exist_ok=True)

    rendered: dict[int, Image.Image] = {s: draw_icon(s) for s in PNG_SIZES}

    # Linux hicolor-theme PNGs
    for s in PNG_SIZES:
        out = ICON_DIR / f"inlook-{s}.png"
        rendered[s].save(out, "PNG", optimize=True)

    # Master raster (used by AppImage + macOS bundling)
    master = ASSETS / "inlook.png"
    rendered[1024].save(master, "PNG", optimize=True)

    # Multi-resolution Windows ICO. Build it by hand because Pillow's
    # ICO writer ignores `append_images` and uses one source image for all
    # sizes — that loses crispness at 16/32px where re-rendered glyphs are
    # noticeably sharper than a downsampled 1024px master.
    ico_path = ASSETS / "inlook.ico"
    write_multi_ico(ico_path, [rendered[s] for s in ICO_SIZES])

    # Print a small summary so the user can sanity-check sizes
    print("Generated:")
    for p in [master, ico_path] + sorted(ICON_DIR.iterdir()):
        print(f"  {p.relative_to(ROOT)}  ({p.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
