#!/usr/bin/env python3
"""Generate the InLook icon set from a single master image.

Design source (committed to the repo):
    assets/inlook-master.png  the master icon artwork (square, RGBA, transparent
                              outside the rounded badge). Highest available
                              resolution — sizes larger than the master are
                              upscaled, so a bigger master gives crisper large
                              icons.

Output (committed to the repo):
    assets/inlook.png         1024x1024 master raster (AppImage + macOS bundling)
    assets/inlook.ico         multi-resolution Windows icon
    assets/icons/inlook-N.png Linux hicolor-theme sizes (16, 32, 48, 64, 128, 256, 512, 1024)

The macOS .icns is built on the macOS runner during release (scripts/build-dmg.sh)
because creating one cleanly requires iconutil from the macOS SDK.

Re-run this script whenever assets/inlook-master.png changes:
    python scripts/generate-icons.py
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
ICON_DIR = ASSETS / "icons"
MASTER = ASSETS / "inlook-master.png"

PNG_SIZES = [16, 32, 48, 64, 128, 256, 512, 1024]
ICO_SIZES = [16, 32, 48, 64, 128, 256]


def load_master() -> Image.Image:
    if not MASTER.exists():
        print(f"ERROR: master image not found: {MASTER}", file=sys.stderr)
        sys.exit(1)
    img = Image.open(MASTER).convert("RGBA")
    if img.width != img.height:
        print(
            f"WARN: master is {img.width}x{img.height}, not square — "
            "output may look stretched",
            file=sys.stderr,
        )
    return img


def resample(master: Image.Image, size: int) -> Image.Image:
    """High-quality resize of the master to a square `size`. LANCZOS handles
    both down- and up-scaling; upscaling past the master's resolution is
    inherently soft."""
    return master.resize((size, size), Image.LANCZOS)


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

    master = load_master()
    rendered: dict[int, Image.Image] = {s: resample(master, s) for s in PNG_SIZES}

    # Linux hicolor-theme PNGs
    for s in PNG_SIZES:
        rendered[s].save(ICON_DIR / f"inlook-{s}.png", "PNG", optimize=True)

    # Master raster (used by AppImage + macOS bundling)
    master_out = ASSETS / "inlook.png"
    rendered[1024].save(master_out, "PNG", optimize=True)

    # Multi-resolution Windows ICO. Build it by hand because Pillow's ICO writer
    # ignores `append_images` and uses one source image for all sizes.
    ico_path = ASSETS / "inlook.ico"
    write_multi_ico(ico_path, [rendered[s] for s in ICO_SIZES])

    # Print a small summary so the user can sanity-check sizes
    print("Generated:")
    for p in [master_out, ico_path] + sorted(ICON_DIR.iterdir()):
        print(f"  {p.relative_to(ROOT)}  ({p.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
