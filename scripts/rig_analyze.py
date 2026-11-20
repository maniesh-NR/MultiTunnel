#!/usr/bin/env python3
"""Pixel-cluster analysis for the S15 rig screenshots (pure stdlib PNG)."""
import struct
import sys
import zlib


def load(path):
    d = open(path, "rb").read()
    pos, idat = 8, b""
    while pos < len(d):
        (ln,) = struct.unpack(">I", d[pos : pos + 4])
        typ = d[pos + 4 : pos + 8]
        if typ == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", d[pos + 8 : pos + 18])
        elif typ == b"IDAT":
            idat += d[pos + 8 : pos + 8 + ln]
        elif typ == b"IEND":
            break
        pos += 12 + ln
    raw = zlib.decompress(idat)
    nch = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    stride = w * nch
    out = bytearray(w * h * 3)
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
            o = (y * w + x) * 3
            s = x * nch
            if ctype in (2, 6):
                out[o : o + 3] = line[s : s + 3]
            elif ctype == 0:
                g = line[s]
                out[o : o + 3] = bytes((g, g, g))
            elif ctype == 4:
                g = line[s]
                out[o : o + 3] = bytes((g, g, g))
        prev = line
    return w, h, out


def px(img, x, y):
    w, h, b = img
    o = (y * w + x) * 3
    return (b[o], b[o + 1], b[o + 2])


def clusters_in_row_band(img, y0, y1, bg, tol=18, gap=6, x0=0, x1=None):
    """x-clusters of non-bg pixels in a horizontal band (rows y0..y1)."""
    w, h, _ = img
    x1 = x1 or w
    cols = set()
    for y in range(y0, y1):
        for x in range(x0, x1):
            p = px(img, x, y)
            if max(abs(p[c] - bg[c]) for c in range(3)) > tol:
                cols.add(x)
    out = []
    for x in sorted(cols):
        if out and x - out[-1][1] <= gap:
            out[-1][1] = x
        else:
            out.append([x, x])
    return out


if __name__ == "__main__":
    img = load(sys.argv[1])
    w, h, _ = img
    print(f"{sys.argv[1]}: {w}x{h}")
    corners = [px(img, 2, 2), px(img, w - 3, 2), px(img, 2, h - 3), px(img, w - 3, h - 3)]
    print("corners:", corners)
    for y0, y1, label in [
        (0, 40, "titlebar-band"),
        (0, 4, "top-edge"),
        (40, 120, "below-titlebar"),
    ]:
        cl = clusters_in_row_band(img, y0, y1, corners[0])
        print(f"{label} (y{y0}-{y1}) clusters vs corner-bg:", cl[:12], "count:", len(cl))
