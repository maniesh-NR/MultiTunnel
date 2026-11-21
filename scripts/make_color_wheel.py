#!/usr/bin/env python3
"""Generate assets/color_wheel.rgba — the S18 Settings accent picker disc.

Geometry contract (MUST stay in sync with crates/mt-app/src/theme.rs):
  - 256x256 RGBA8, transparent outside the disc;
  - hue    = angle around the center, 0deg RED at 12 o'clock, CLOCKWISE;
  - chroma = distance from the center: 0 at the center (white), full at
    the rim;
  - value  = 1.0 (the M3 TonalSpot generator keys on hue+chroma).

The Rust side computes the exact same HSV(h, s, 1) for hit-testing, so the
asset and the picker agree by construction; the only freedom here is the
anti-aliased rim (the Rust side clamps to the rim at that angle, which the
AA edge rounds to the same color).

Usage:  python3 scripts/make_color_wheel.py  [output path, default
        assets/color_wheel.rgba]
Deterministic output; stdlib only.
"""

import math
import os
import sys

PX = 256


def hsv_to_rgb(h, s, v):
    """Mirror of the Rust helper (degrees / 0..1 floats / 0..255 ints)."""
    h = h % 360.0
    c = v * s
    x = c * (1.0 - abs((h / 60.0) % 2.0 - 1.0))
    m = v - c
    if h < 60:
        r, g, b = c, x, 0.0
    elif h < 120:
        r, g, b = x, c, 0.0
    elif h < 180:
        r, g, b = 0.0, c, x
    elif h < 240:
        r, g, b = 0.0, x, c
    elif h < 300:
        r, g, b = x, 0.0, c
    else:
        r, g, b = c, 0.0, x
    return (
        int(round((r + m) * 255)),
        int(round((g + m) * 255)),
        int(round((b + m) * 255)),
    )


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "color_wheel.rgba")
    cx = cy = (PX - 1) / 2.0
    radius = PX / 2.0
    buf = bytearray(PX * PX * 4)
    # 2x2 supersample per pixel for a smooth rim (deterministic grid).
    for y in range(PX):
        for x in range(PX):
            r_sum = g_sum = b_sum = a_sum = 0.0
            for sy in (0.25, 0.75):
                for sx in (0.25, 0.75):
                    dx = (x + sx) - 0.5 - cx
                    dy = (y + sy) - 0.5 - cy
                    dist = math.hypot(dx, dy)
                    if dist <= radius:
                        sat = dist / radius
                        hue = math.degrees(math.atan2(dy, dx)) + 90.0
                        r, g, b = hsv_to_rgb(hue, sat, 1.0)
                        r_sum += r
                        g_sum += g
                        b_sum += b
                        a_sum += 255.0
            o = (y * PX + x) * 4
            n = 4.0
            if a_sum > 0:
                buf[o] = int(round(r_sum / n))
                buf[o + 1] = int(round(g_sum / n))
                buf[o + 2] = int(round(b_sum / n))
                buf[o + 3] = int(round(a_sum / n))
    with open(out, "wb") as f:
        f.write(buf)
    opaque = sum(1 for i in range(PX * PX) if buf[i * 4 + 3] > 0)
    print(
        f"wrote {out}: {len(buf)} bytes ({PX}x{PX} RGBA, "
        f"{opaque} in-disc px, transparent rim corners)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
