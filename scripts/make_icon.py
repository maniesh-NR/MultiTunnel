#!/usr/bin/env python3
"""Generate assets/icon.ico — the MultiTunnel tray icon.

Design (pure geometry, no text): a dark-navy rounded square with two
parallel link bars (the bonded links) — sky blue over emerald. Two sizes
are embedded: 32x32 (primary, tray display) and 16x16 (small-icon slot).

Both images are stored as classic 32-bpp BMP-format ICO entries:

    ICONDIR       (6 bytes)   reserved=0, type=1 (icon), count=2
    ICONDIRENTRY  (16 bytes)  per image, pointing at its RT_ICON bytes
    image         BITMAPINFOHEADER (biHeight = 2*h: XOR mask + AND mask),
                  bottom-up BGRA rows, then a 1-bpp AND mask (rows padded
                  to 4 bytes; bit set where the pixel is fully transparent)

`CreateIconFromResourceEx` consumes exactly those RT_ICON bytes (the image
at the entry offset — NOT the ICO directory header), which is what
src/tray.rs parses out at runtime.

Usage:  python3 scripts/make_icon.py  [output path, default assets/icon.ico]
Deterministic output; stdlib only.
"""

import os
import struct
import sys

# ---- palette ---------------------------------------------------------------

NAVY = (0x0D, 0x1B, 0x2A, 0xFF)   # dark navy background  #0D1B2A
SKY = (0x4F, 0xC3, 0xF7, 0xFF)    # link bar 1 (sky)      #4FC3F7
EMERALD = (0x4A, 0xDE, 0x80, 0xFF)  # link bar 2 (emerald) #4ADE80


# ---- tiny raster helpers ---------------------------------------------------

def rounded_rect(x0, y0, x1, y1, r):
    """Mask of a rounded rectangle, half-open [x0,x1) x [y0,y1), corner
    radius r (classic quadrant test: the corner center is inset by r)."""

    def inside(x, y):
        if x < x0 or x >= x1 or y < y0 or y >= y1:
            return False
        dx = max(x0 + r - x - 1, x - (x1 - r), 0)
        dy = max(y0 + r - y - 1, y - (y1 - r), 0)
        return dx * dx + dy * dy <= r * r

    return inside


def render(size, layers):
    """Composite `layers` (color, mask) onto a transparent size x size RGBA
    buffer; later layers overwrite earlier pixels."""
    px = [[(0, 0, 0, 0)] * size for _ in range(size)]
    for color, mask in layers:
        for y in range(size):
            for x in range(size):
                if mask(x, y):
                    px[y][x] = color
    return px


def ico_image(size, px):
    """Encode one size x size RGBA raster as RT_ICON bytes (BITMAPINFOHEADER
    with doubled height, bottom-up 32-bpp XOR rows, 1-bpp AND mask)."""
    xor = b"".join(
        bytes(c for x in range(size) for c in (px[y][x][2], px[y][x][1], px[y][x][0], px[y][x][3]))
        for y in range(size - 1, -1, -1)  # bottom-up
    )
    # AND mask: 1-bpp, bottom-up, each row padded to a 4-byte multiple.
    row_bytes = (size + 31) // 32 * 4
    and_mask = bytearray(row_bytes * size)
    for i, y in enumerate(range(size - 1, -1, -1)):
        row = bytearray(row_bytes)
        for x in range(size):
            if px[y][x][3] == 0:  # fully transparent -> mask bit set
                row[x // 8] |= 0x80 >> (x % 8)
        and_mask[i * row_bytes : (i + 1) * row_bytes] = row
    total = 40 + len(xor) + len(and_mask)
    header = struct.pack(
        "<IiiHHIIiiII",
        40,        # biSize
        size,      # biWidth
        size * 2,  # biHeight (XOR + AND masks)
        1,         # biPlanes
        32,        # biBitCount
        0,         # biCompression = BI_RGB
        total,     # biSizeImage
        0, 0, 0, 0,
    )
    return header + xor + bytes(and_mask)


def write_ico(path, images):
    """Write an ICO file from [(size, rt_icon_bytes), ...]."""
    count = len(images)
    out = bytearray(struct.pack("<HHH", 0, 1, count))  # reserved, type=icon, count
    offset = 6 + 16 * count
    for size, data in images:
        out += struct.pack("<BBBBHHII", size & 0xFF, size & 0xFF, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data in images:
        out += data
    with open(path, "wb") as f:
        f.write(out)
    return len(out)


# ---- the MultiTunnel motif -------------------------------------------------

def motif_32():
    return render(
        32,
        [
            (NAVY, rounded_rect(1, 1, 31, 31, 7)),
            (SKY, rounded_rect(6, 9, 26, 14, 2)),
            (EMERALD, rounded_rect(6, 18, 26, 23, 2)),
        ],
    )


def motif_16():
    return render(
        16,
        [
            (NAVY, rounded_rect(0, 0, 16, 16, 4)),
            (SKY, rounded_rect(3, 4, 13, 7, 1)),
            (EMERALD, rounded_rect(3, 9, 13, 12, 1)),
        ],
    )


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "icon.ico")
    images = [(32, ico_image(32, motif_32())), (16, ico_image(16, motif_16()))]
    n = write_ico(out, images)
    print(f"wrote {out}: {n} bytes (32x32 + 16x16, BMP-format entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
