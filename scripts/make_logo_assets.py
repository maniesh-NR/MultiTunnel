#!/usr/bin/env python3
"""Generate the MultiTunnel logo assets from the user's source image.

The source (assets/logo.png, kept verbatim) is the logo the user attached:
an emblem (a mark) above a wordmark, drawn on a uniform light-blue canvas.
This script decodes it with the stdlib only (zlib + per-scanline unfilter
— the repo carries no image crate), splits the two segments, and emits:

    assets/logo.png     verbatim copy of the source (reference/repro)
    assets/logo_24.rgba titlebar emblem, 24x24 RGBA8, TRANSPARENT
                        background (S16: flood-fill key-out)
    assets/logo_64.rgba window icon,    64x64 RGBA8 (window::Settings icon)
    assets/icon.ico     tray icon (connected state): BMP-format entries
                        16/32/48 (emblem, tight crop keeps the mark
                        legible) + 256 (full lockup) — the same file
                        src/tray.rs (legacy tray) and the mt-app tray parse
                        at runtime.
    assets/icon_mono.ico  tray icon (not-connected state): same entry
                        shape, monochrome + transparent (S16).
    assets/logo_transparent.png  full lockup, transparent background.
    assets/banner.png    1280x360 README hero banner (S16): the lockup
                        composited on a canvas matching the logo
                        background.

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


def resize_area(img, dw, dh):
    """Area-averaging (box) resample — REQUIRED for large downscales
    (S16): bilinear samples only 4 of ~400 source pixels at 20:1, which
    punched holes through the transparent emblem. Alpha is premultiplied
    before averaging and unpremultiplied after, so transparent
    surroundings never tint the surviving colors."""
    w, h, buf = img
    if (w, h) == (dw, dh):
        return img
    out = bytearray(dw * dh * 4)
    for dy in range(dh):
        sy0, sy1 = dy * h / dh, (dy + 1) * h / dh
        for dx in range(dw):
            sx0, sx1 = dx * w / dw, (dx + 1) * w / dw
            r = g = b = a = 0.0
            for y in range(int(sy0), min(int(sy1) + 1, h)):
                wy = min(sy1, y + 1) - max(sy0, y)
                if wy <= 0:
                    continue
                for x in range(int(sx0), min(int(sx1) + 1, w)):
                    wx = min(sx1, x + 1) - max(sx0, x)
                    if wx <= 0:
                        continue
                    wgt = wy * wx
                    o = (y * w + x) * 4
                    al = buf[o + 3] / 255.0
                    r += buf[o] * al * wgt
                    g += buf[o + 1] * al * wgt
                    b += buf[o + 2] * al * wgt
                    a += al * wgt
            o = (dy * dw + dx) * 4
            if a > 0:
                out[o] = int(round(r / a))
                out[o + 1] = int(round(g / a))
                out[o + 2] = int(round(b / a))
                out[o + 3] = int(round(min(a / (w / dw * h / dh), 1.0) * 255))
    return (dw, dh, bytes(out))


def smart_resize(img, dw, dh):
    """Bilinear for upscales/mild rescales, area-average when the source
    is more than 2x the destination in either axis."""
    w, h, _ = img
    if dw * 2 <= w or dh * 2 <= h:
        return resize_area(img, dw, dh)
    return resize_bilinear(img, dw, dh)


def square_on_bg(img, size, bg, margin=0.08):
    """Scale img to fit a size x size canvas of `bg` (4-tuple: alpha is
    honored — (0,0,0,0) yields a transparent canvas), centered."""
    w, h, _ = img
    inner = size * (1.0 - 2 * margin)
    scale = min(inner / w, inner / h)
    tw, th = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
    scaled = smart_resize(img, tw, th)
    ox, oy = (size - tw) // 2, (size - th) // 2
    out = bytearray(bytes(bg[:3]) + bytes((bg[3],)) * 1) * (size * size)
    for y in range(th):
        d = ((oy + y) * size + ox) * 4
        out[d : d + tw * 4] = scaled[2][(y * tw) * 4 : (y * tw + tw) * 4]
    return (size, size, bytes(out))


# ---- S16: background key-out / monochrome / compositing / PNG encode --------


def key_out_background(img, local_tol=14, global_tol=25):
    """Flood-fill the border-connected background to alpha=0.

    Two gates keep the mark intact: a pixel joins the background only if
    it is (a) adjacent to an already-classified background pixel with a
    per-channel diff <= local_tol (handles the source's soft gradient)
    and (b) within global_tol of the average border color (so a pale
    interior region of the mark can never be eaten via a soft edge).
    Enclosed background-colored pockets stay opaque (they read as part of
    the mark, like the counters of letters)."""
    w, h, buf = img
    # Average border color (the canvas the mark was drawn on).
    border = []
    for x in range(w):
        border.append((0, x))
        border.append((h - 1, x))
    for y in range(1, h - 1):
        border.append((y, 0))
        border.append((y, w - 1))
    n = len(border)
    avg = [sum(px(img, x, y)[c] for y, x in border) // n for c in range(3)]
    out = bytearray(buf)
    seen = bytearray(w * h)
    stack = []
    for y, x in border:
        if dist(px(img, x, y), avg) <= global_tol:
            stack.append((y, x))
            seen[y * w + x] = 1
    while stack:
        y, x = stack.pop()
        o = (y * w + x) * 4
        out[o + 3] = 0
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and not seen[ny * w + nx]:
                seen[ny * w + nx] = 1
                c = px(img, nx, ny)
                cur = (buf[o], buf[o + 1], buf[o + 2])
                if (
                    max(abs(c[k] - cur[k]) for k in range(3)) <= local_tol
                    and dist(c, avg) <= global_tol
                ):
                    stack.append((ny, nx))
    return (w, h, bytes(out))


def desaturate(img, lo=64, hi=216):
    """Monochrome: luminance grayscale with contrast normalization to
    [lo, hi] — mid-tones that stay legible on light AND dark taskbars."""
    w, h, buf = img
    out = bytearray(buf)
    lums = []
    for i in range(w * h):
        o = i * 4
        if buf[o + 3] > 0:
            lums.append(int(0.299 * buf[o] + 0.587 * buf[o + 1] + 0.114 * buf[o + 2]))
    if not lums:
        return img
    lums.sort()
    p2, p98 = lums[len(lums) // 50], lums[49 * len(lums) // 50]
    span = max(p98 - p2, 1)
    for i in range(w * h):
        o = i * 4
        if buf[o + 3] > 0:
            lum = int(0.299 * buf[o] + 0.587 * buf[o + 1] + 0.114 * buf[o + 2])
            g = int(round(lo + (min(max(lum, p2), p98) - p2) * (hi - lo) / span))
            out[o] = out[o + 1] = out[o + 2] = g
    return (w, h, bytes(out))


def composite_on(img, size_wh, bg, margin=0.14):
    """Alpha-composite a transparent RGBA raster centered on a solid
    `bg` canvas of size_wh (w, h), with a symmetric margin."""
    w, h, buf = img
    cw, ch = size_wh
    aw, ah = cw * (1.0 - 2 * margin), ch * (1.0 - 2 * margin)
    scale = min(aw / w, ah / h)
    tw, th = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
    scaled = smart_resize(img, tw, th)
    ox, oy = (cw - tw) // 2, (ch - th) // 2
    out = bytearray(bytes(bg[:3]) + b"\xff" * 1) * (cw * ch)
    for y in range(th):
        for x in range(tw):
            s = (y * tw + x) * 4
            a = scaled[2][s + 3]
            if a == 0:
                continue
            d = ((oy + y) * cw + ox + x) * 4
            if a == 255:
                out[d : d + 3] = scaled[2][s : s + 3]
            else:
                for c in range(3):
                    out[d + c] = (scaled[2][s + c] * a + out[d + c] * (255 - a)) // 255
    return (cw, ch, bytes(out))


def encode_png(img):
    """Minimal deterministic PNG writer (RGBA8, filter 0, zlib level 9)."""
    w, h, buf = img
    raw = b"".join(b"\x00" + bytes(buf[y * w * 4 : (y + 1) * w * 4]) for y in range(h))

    def chunk(typ, body):
        return (
            struct.pack(">I", len(body))
            + typ
            + body
            + struct.pack(">I", zlib.crc32(typ + body) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


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

    # S16: transparent variants (flood-fill key-out on the tight crops).
    emblem_t = key_out_background(emblem_img)
    lockup_t = key_out_background(lockup_img)
    opaque = sum(1 for i in range(len(emblem_t[2])) if i % 4 == 3 and emblem_t[2][i] > 0)
    print(f"transparent emblem: {opaque} opaque px of {emblem_t[0]*emblem_t[1]}")

    # titlebar emblem: TRANSPARENT background (S16 ask), padded to a square
    # so the mark stays centered at 20x20 draw size.
    sq24 = square_on_bg(emblem_t, 24, (0, 0, 0, 0))
    with open(os.path.join(outdir, "logo_24.rgba"), "wb") as f:
        f.write(sq24[2])
    print(f"logo_24.rgba: {len(sq24[2])} bytes (24x24, transparent bg)")

    # window icon: colored square on the logo's own canvas (unchanged look)
    sq64 = square_on_bg(emblem_img, 64, bg)
    with open(os.path.join(outdir, "logo_64.rgba"), "wb") as f:
        f.write(sq64[2])
    print(f"logo_64.rgba: {len(sq64[2])} bytes (64x64)")

    # transparent lockup (PNG) — README + compositing source
    png = encode_png(lockup_t)
    with open(os.path.join(outdir, "logo_transparent.png"), "wb") as f:
        f.write(png)
    print(f"logo_transparent.png: {len(png)} bytes ({lockup_t[0]}x{lockup_t[1]})")

    # README hero banner: lockup composited on a canvas-colored 1280x360
    banner = composite_on(lockup_t, (1280, 360), bg)
    banner_png = encode_png(banner)
    with open(os.path.join(outdir, "banner.png"), "wb") as f:
        f.write(banner_png)
    print(f"banner.png: {len(banner_png)} bytes (1280x360)")

    # tray icon (connected): 16/32/48 emblem squares + 256 full lockup
    entries = []
    for size in (16, 32, 48, 256):
        sq = square_on_bg(emblem_img if size < 64 else lockup_img, size, bg)
        entries.append((size, ico_image(sq)))
    n = write_ico(os.path.join(outdir, "icon.ico"), entries)
    print(f"icon.ico: {n} bytes (16/32/48 emblem + 256 lockup, BMP entries)")

    # tray icon (not connected): monochrome + transparent (S16)
    mono_emblem = desaturate(emblem_t)
    mono_lockup = desaturate(lockup_t)
    entries = []
    for size in (16, 32, 48, 256):
        sq = square_on_bg(mono_emblem if size < 64 else mono_lockup, size, (0, 0, 0, 0))
        entries.append((size, ico_image(sq)))
    n = write_ico(os.path.join(outdir, "icon_mono.ico"), entries)
    print(f"icon_mono.ico: {n} bytes (16/32/48 emblem + 256 lockup, mono)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
