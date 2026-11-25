#!/usr/bin/env python3
"""Pixel checks on a screenshot of one display (stdlib only).

Usage:
  shot_check.py panel   <png> <left> <top> <width> <height> <panel-hex> <page-hex>
  shot_check.py absent  <png> <left> <top> <width> <height> <panel-hex> <page-hex>
Prints one line `<VERDICT> <reason>` and exits 0 for PASS, 1 for NO, 3 for BLOCKED.
"""
import struct
import sys
import zlib

TOL = 8
INSET = 2
PANEL_MIN = 0.95
ABSENT_MAX = 0.05
PAGE_MIN = 0.60


def read_png(path):
    """Returns (width, height, get) where get(x, y) is an (r, g, b) tuple. 8-bit RGB or RGBA,
    not interlaced: the format `screencapture` writes."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG file")
    pos = 8
    header = None
    idat = bytearray()
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        chunk = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", chunk)
        elif kind == b"IDAT":
            idat.extend(chunk)
        elif kind == b"IEND":
            break
    if header is None:
        raise ValueError("PNG without a header")
    width, height, depth, colour_type, _, _, interlace = header
    if depth != 8 or colour_type not in (2, 6) or interlace != 0:
        raise ValueError("only 8-bit RGB or RGBA PNG files that are not interlaced are read")
    channels = 3 if colour_type == 2 else 4
    stride = width * channels
    raw = zlib.decompress(bytes(idat))
    if len(raw) != height * (stride + 1):
        raise ValueError("PNG data has the wrong length")
    rows = []
    prev = bytearray(stride)
    for y in range(height):
        base = y * (stride + 1)
        ftype = raw[base]
        row = bytearray(raw[base + 1 : base + 1 + stride])
        for i in range(stride):
            left = row[i - channels] if i >= channels else 0
            up = prev[i]
            upleft = prev[i - channels] if i >= channels else 0
            if ftype == 1:
                row[i] = (row[i] + left) & 255
            elif ftype == 2:
                row[i] = (row[i] + up) & 255
            elif ftype == 3:
                row[i] = (row[i] + (left + up) // 2) & 255
            elif ftype == 4:
                p = left + up - upleft
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - upleft)
                pred = left if pa <= pb and pa <= pc else (up if pb <= pc else upleft)
                row[i] = (row[i] + pred) & 255
            elif ftype != 0:
                raise ValueError("unknown PNG filter")
        rows.append(row)
        prev = row

    def get(x, y):
        o = x * channels
        row = rows[y]
        return (row[o], row[o + 1], row[o + 2])

    return width, height, get


def parse_hex(text):
    text = text.strip().lstrip("#")
    if len(text) != 6:
        raise ValueError("colour must be six hex digits")
    return (int(text[0:2], 16), int(text[2:4], 16), int(text[4:6], 16))


def _near(pixel, colour, tol):
    return all(abs(a - b) <= tol for a, b in zip(pixel, colour))


def fraction_near(img, rect, colour, tol=TOL, inset=0):
    """Fraction of the pixels of `rect` (shrunk by `inset` on each side) within `tol` of `colour`."""
    width, height, get = img
    left, top, w, h = rect
    left, top, w, h = left + inset, top + inset, w - 2 * inset, h - 2 * inset
    if w <= 0 or h <= 0:
        return 0.0
    hits = 0
    for y in range(max(top, 0), min(top + h, height)):
        for x in range(max(left, 0), min(left + w, width)):
            if _near(get(x, y), colour, tol):
                hits += 1
    return hits / (w * h)


def fraction_outside_near(img, rect, colour, tol=TOL, step=7):
    """Fraction of sampled pixels outside `rect` (every `step` pixels) within `tol` of `colour`."""
    width, height, get = img
    left, top, w, h = rect
    seen = hits = 0
    for y in range(0, height, step):
        for x in range(0, width, step):
            if left <= x < left + w and top <= y < top + h:
                continue
            seen += 1
            if _near(get(x, y), colour, tol):
                hits += 1
    return hits / seen if seen else 0.0


def check(mode, img, rect, panel, page):
    """Returns (verdict, reason) with the verdict PASS, NO or BLOCKED."""
    if mode not in ("panel", "absent"):
        raise ValueError("mode must be panel or absent")
    panel_f = fraction_near(img, rect, panel, inset=INSET)
    page_f = fraction_outside_near(img, rect, page)
    if page_f < PAGE_MIN:
        return "BLOCKED", "the test page colour covers only %.2f of the shot (need %.2f): blank shot, missing Screen Recording permission or the wrong Space" % (page_f, PAGE_MIN)
    if mode == "panel":
        if panel_f >= PANEL_MIN:
            return "PASS", "panel colour covers %.2f of the rectangle, page colour %.2f of the rest" % (panel_f, page_f)
        return "NO", "panel colour covers %.2f of the rectangle, need %.2f (page colour %.2f of the rest)" % (panel_f, PANEL_MIN, page_f)
    if panel_f <= ABSENT_MAX:
        return "PASS", "control: panel colour covers %.2f of the rectangle, page colour %.2f of the rest" % (panel_f, page_f)
    return "NO", "control: panel colour covers %.2f of the rectangle, expected at most %.2f" % (panel_f, ABSENT_MAX)


def main(argv):
    if len(argv) != 9:
        sys.stderr.write(__doc__)
        return 64
    mode, path = argv[1], argv[2]
    try:
        rect = tuple(int(v) for v in argv[3:7])
        verdict, reason = check(mode, read_png(path), rect, parse_hex(argv[7]), parse_hex(argv[8]))
    except (ValueError, OSError, zlib.error) as err:
        print("BLOCKED cannot read the shot: %s" % err)
        return 3
    print("%s %s" % (verdict, reason))
    return {"PASS": 0, "NO": 1}.get(verdict, 3)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
