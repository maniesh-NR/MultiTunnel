#!/usr/bin/env python3
"""Generate the MultiTunnel logo assets from the user's source image.

The source (assets/logo.png, kept verbatim) is the logo the user attached:
an emblem (a mark) above a wordmark, drawn on a uniform light-blue canvas.
This script decodes it with the stdlib only (zlib + per-scanline unfilter
— the repo carries no image crate), splits the two segments, and emits:

    assets/logo.png     verbatim copy of the source (reference/repro)
    assets/logo_24.rgba titlebar emblem, 24x24 RGBA8 (iced image Handle)
    assets/logo_64.rgba window icon,    64x64 RGBA8 (window::Settings icon)
    assets/icon.ico     tray icon: BMP-format entries 16/32/48 (emblem,
                        tight crop keeps the mark legible) + 256 (full
                        lockup) — the same file src/tray.rs (legacy tray)
                        and the mt-app tray parse at runtime.

Small squares are the EMBLEM on the logo's own background color (the
source art is a solid-color canvas, so letterboxing is seamless); the
256 entry is the full lockup (emblem + wordmark). Deterministic: the
same input produces byte-identical outputs.

Usage: python3 scripts/make_logo_assets.py [src.png] [--out assets]
"""

import os
import struct
import sys
import zlib

# ---- PNG decode (stdlib) ---------------------------------------------------


def decode_png(data):
    """Minimal PNG reader: 8-bit depth, gray/RGB/RGBA/gray+alpha, no
    interlace. Returns (w, h, rgba_bytes)."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit("not a PNG")
    pos, idat, meta, palette, trns = 8, b"", None, None, None
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos : pos + 4])
        typ = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + ln]
        if typ == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
            if depth != 8:
                raise SystemExit("unsupported bit depth %d" % depth)
            if body[12] != 0:
                raise SystemExit("interlaced PNG unsupported")
            meta = (w, h, ctype)
        elif typ == b"PLTE":
            palette = body
        elif typ == b"tRNS":
            trns = body
        elif typ == b"IDAT":
            idat += body
        elif typ == b"IEND":
            break
        pos += 12 + ln
    if meta is None:
        raise SystemExit("missing IHDR")
    w, h, ctype = meta
    raw = zlib.decompress(idat)
    nch = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    stride = w * nch
    out = bytearray(w * h * 4)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        f = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        if f == 1:
            for x in range(nch, stride):
                line[x] = (line[x] + line[x - nch]) & 0xFF
        elif f == 2:
            for x in range(stride):
                line[x] = (line[x] + prev[x]) & 0xFF
        elif f == 3:
            for x in range(stride):
                a = line[x - nch] if x >= nch else 0
                line[x] = (line[x] + ((a + prev[x]) >> 1)) & 0xFF
        elif f == 4:
            for x in range(stride):
                a = line[x - nch] if x >= nch else 0
                b = prev[x]
                c = prev[x - nch] if x >= nch else 0
                q = a + b - c
                pa, pb, pc = abs(q - a), abs(q - b), abs(q - c)
                r = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[x] = (line[x] + r) & 0xFF
        for x in range(w):
            o = (y * w + x) * 4
            s = x * nch
            if ctype == 2:
                out[o : o + 3] = line[s : s + 3]
                out[o + 3] = 255
            elif ctype == 6:
                out[o : o + 4] = line[s : s + 4]
            elif ctype == 0:
                g = line[s]
                out[o : o + 3] = bytes((g, g, g))
                out[o + 3] = 255
            elif ctype == 4:
                g, a = line[s], line[s + 1]
                out[o : o + 3] = bytes((g, g, g))
                out[o + 3] = a
            elif ctype == 3:
                i = line[s]
                out[o : o + 3] = palette[i * 3 : i * 3 + 3]
                out[o + 3] = trns[i] if trns and i < len(trns) else 255
        prev = line
    return w, h, bytes(out)


# ---- raster helpers --------------------------------------------------------


def px(img, x, y):
    w, h, buf = img
    o = (y * w + x) * 4
    return (buf[o], buf[o + 1], buf[o + 2], buf[o + 3])


def dist(a, b):
    return max(abs(a[c] - b[c]) for c in range(3))


def content_bbox(img, bg, thresh=12):
    """Tight [x0, x1) x [y0, y1) box of pixels differing from bg."""
    w, h, _ = img
    minx, maxx, miny, maxy = w, -1, h, -1
    for y in range(h):
        for x in range(w):
            if dist(px(img, x, y), bg) >= thresh:
                if x < minx:
                    minx = x
                if x > maxx:
                    maxx = x
                if y < miny:
                    miny = y
                if y > maxy:
                    maxy = y
    if maxx < 0:
        raise SystemExit("image is uniform — no content found")
    return minx, miny, maxx + 1, maxy + 1


def row_has_content(img, y, x0, x1, bg, thresh=12):
    return any(dist(px(img, x, y), bg) >= thresh for x in range(x0, x1))


def split_lockup(img, bbox, bg):
    """Split emblem (top) from wordmark (bottom) at the widest empty row
    band inside the bbox. Returns (emblem_box, lockup_box_is_full)."""
    x0, y0, x1, y1 = bbox
    gap_start = None
    y = y1 - 2
    gap_rows = []
    while y > y0:
        if not row_has_content(img, y, x0, x1, bg):
            gap_rows.append(y)
            if gap_start is None:
                gap_start = y
            elif y != gap_rows[-2] - 1:
                # not contiguous — restart the band
                gap_rows = [y]
                gap_start = y
        elif gap_rows:
            break
        y -= 1
    if not gap_rows or len(gap_rows) < 3:
        # no clear split: treat everything as one emblem
        return bbox
    return (x0, y0, x1, gap_rows[0] + 1)


def crop(img, box):
    w, h, buf = img
    x0, y0, x1, y1 = box
    cw, ch = x1 - x0, y1 - y0
    out = bytearray(cw * ch * 4)
    for y in range(ch):
        s = ((y0 + y) * w + x0) * 4
        d = (y * cw) * 4
        out[d : d + cw * 4] = buf[s : s + cw * 4]
    return (cw, ch, bytes(out))


def resize_bilinear(img, dw, dh):
    """Bilinear resample of an RGBA raster to dw x dh."""
    w, h, buf = img
    if (w, h) == (dw, dh):
        return img
    out = bytearray(dw * dh * 4)
    xr = (w - 1) / max(dw - 1, 1)
    yr = (h - 1) / max(dh - 1, 1)
    for dy in range(dh):
        sy = dy * yr
        y0 = int(sy)
        y1 = min(y0 + 1, h - 1)
        fy = sy - y0
        for dx in range(dw):
            sx = dx * xr
            x0 = int(sx)
            x1 = min(x0 + 1, w - 1)
            fx = sx - x0
            for c in range(4):
                a = buf[(y0 * w + x0) * 4 + c]
                b = buf[(y0 * w + x1) * 4 + c]
                cc = buf[(y1 * w + x0) * 4 + c]
                d = buf[(y1 * w + x1) * 4 + c]
                top = a + (b - a) * fx
                bot = cc + (d - cc) * fx
                out[(dy * dw + dx) * 4 + c] = int(round(top + (bot - top) * fy))
    return (dw, dh, bytes(out))


def square_on_bg(img, size, bg, margin=0.08):
    """Scale img to fit a size x size canvas of `bg`, centered."""
    w, h, _ = img
    inner = size * (1.0 - 2 * margin)
    scale = min(inner / w, inner / h)
    tw, th = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
    scaled = resize_bilinear(img, tw, th)
    ox, oy = (size - tw) // 2, (size - th) // 2
    out = bytearray(bytes(bg[:3]) + b"\xff") * (size * size)
    for y in range(th):
        d = ((oy + y) * size + ox) * 4
        out[d : d + tw * 4] = scaled[2][(y * tw) * 4 : (y * tw + tw) * 4]
    return (size, size, bytes(out))


# ---- ICO writing (BMP-format entries, same layout as make_icon.py) ---------


def ico_image(img):
    """Encode a square RGBA raster as RT_ICON bytes (BITMAPINFOHEADER with
    doubled height, bottom-up 32-bpp XOR rows, 1-bpp AND mask)."""
    size, _, buf = img
    xor = b"".join(
        bytes(
            c
            for x in range(size)
            for c in (
                buf[((y * size + x) * 4) + 2],
                buf[((y * size + x) * 4) + 1],
                buf[(y * size + x) * 4],
                buf[((y * size + x) * 4) + 3],
            )
        )
        for y in range(size - 1, -1, -1)
    )
    row_bytes = (size + 31) // 32 * 4
    and_mask = bytearray(row_bytes * size)
    for i, y in enumerate(range(size - 1, -1, -1)):
        row = bytearray(row_bytes)
        for x in range(size):
            if buf[(y * size + x) * 4 + 3] == 0:
                row[x // 8] |= 0x80 >> (x % 8)
        and_mask[i * row_bytes : (i + 1) * row_bytes] = row
    total = 40 + len(xor) + len(and_mask)
    header = struct.pack(
        "<IiiHHIIiiII",
        40,
        size,
        size * 2,
        1,
        32,
        0,
        total,
        0,
        0,
        0,
        0,
    )
    return header + xor + bytes(and_mask)


def write_ico(path, images):
    count = len(images)
    out = bytearray(struct.pack("<HHH", 0, 1, count))
    offset = 6 + 16 * count
    for size, data in images:
        out += struct.pack(
            "<BBBBHHII", size & 0xFF, size & 0xFF, 0, 0, 1, 32, len(data), offset
        )
        offset += len(data)
    for _, data in images:
        out += data
    with open(path, "wb") as f:
        f.write(out)
    return len(out)


# ---- main -------------------------------------------------------------------


def main() -> int:
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "logo.png")
    outdir = os.path.join("assets", "")
    data = open(src, "rb").read()
    img = decode_png(data)
    w, h, _ = img
    bg = px(img, 0, 0)
    bbox = content_bbox(img, bg)
    emblem = split_lockup(img, bbox, bg)
    emblem_img = crop(img, emblem)
    lockup_img = crop(img, bbox)
    print(
        f"source {w}x{h}, bg rgb{bg[:3]}, "
        f"lockup {bbox[2]-bbox[0]}x{bbox[3]-bbox[1]}, "
        f"emblem {emblem[2]-emblem[0]}x{emblem[3]-emblem[1]}"
    )

    # keep the source verbatim
    with open(os.path.join(outdir, "logo.png"), "wb") as f:
        f.write(data)

    # titlebar emblem + window icon (square canvases on the logo's bg)
    for size in (24, 64):
        sq = square_on_bg(emblem_img, size, bg)
        with open(os.path.join(outdir, f"logo_{size}.rgba"), "wb") as f:
            f.write(sq[2])
        print(f"logo_{size}.rgba: {len(sq[2])} bytes ({size}x{size})")

    # tray icon: 16/32/48 emblem squares + 256 full lockup
    entries = []
    for size in (16, 32, 48, 256):
        sq = square_on_bg(emblem_img if size < 64 else lockup_img, size, bg)
        entries.append((size, ico_image(sq)))
    n = write_ico(os.path.join(outdir, "icon.ico"), entries)
    print(f"icon.ico: {n} bytes (16/32/48 emblem + 256 lockup, BMP entries)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
