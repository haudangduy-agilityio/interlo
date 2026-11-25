"""Tests of shot_check.py. Run: python3 -I -m unittest harness/shot_check_test.py (from bench/M0b-06)."""
import os
import struct
import sys
import tempfile
import unittest
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shot_check as sc  # noqa: E402

PANEL = (230, 0, 126)
PAGE = (11, 61, 145)
OTHER = (120, 100, 80)


def make_png(width, height, pixel_at, colour_type=2, filters=None):
    """PNG bytes; pixel_at(x, y) gives an (r, g, b) tuple. `filters` picks the filter per row."""
    channels = 3 if colour_type == 2 else 4
    raw = bytearray()
    prev = bytearray(width * channels)
    for y in range(height):
        row = bytearray()
        for x in range(width):
            row.extend(pixel_at(x, y))
            if channels == 4:
                row.append(255)
        ftype = (filters[y % len(filters)] if filters else 0)
        out = bytearray()
        for i, value in enumerate(row):
            left = row[i - channels] if i >= channels else 0
            up = prev[i]
            upleft = prev[i - channels] if i >= channels else 0
            if ftype == 0:
                pred = 0
            elif ftype == 1:
                pred = left
            elif ftype == 2:
                pred = up
            elif ftype == 3:
                pred = (left + up) // 2
            else:
                p = left + up - upleft
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - upleft)
                pred = left if pa <= pb and pa <= pc else (up if pb <= pc else upleft)
            out.append((value - pred) & 255)
        raw.append(ftype)
        raw.extend(out)
        prev = row

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, colour_type, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b"")


def write(png):
    handle, path = tempfile.mkstemp(suffix=".png")
    os.write(handle, png)
    os.close(handle)
    return path


RECT = (40, 60, 100, 20)  # left, top, width, height in a 200 by 120 image


def screen(panel_visible=True, page_visible=True):
    left, top, w, h = RECT

    def pixel(x, y):
        if left <= x < left + w and top <= y < top + h:
            return PANEL if panel_visible else (PAGE if page_visible else OTHER)
        return PAGE if page_visible else OTHER

    return pixel


class ReadPng(unittest.TestCase):
    def test_reads_rgb_and_rgba_with_every_filter(self):
        for colour_type in (2, 6):
            png = make_png(20, 10, lambda x, y: (x * 10 % 256, y * 20 % 256, (x + y) % 256), colour_type, [0, 1, 2, 3, 4])
            width, height, get = sc.read_png(write(png))
            self.assertEqual((width, height), (20, 10))
            self.assertEqual(get(3, 4), (30, 80, 7))
            self.assertEqual(get(19, 9), (190, 180, 28))

    def test_refuses_what_is_not_a_png(self):
        with self.assertRaises(ValueError):
            sc.read_png(write(b"not a png"))


class ParseHex(unittest.TestCase):
    def test_parses_with_and_without_hash(self):
        self.assertEqual(sc.parse_hex("#E6007E"), PANEL)
        self.assertEqual(sc.parse_hex("0b3d91"), PAGE)

    def test_refuses_bad_text(self):
        for bad in ("", "#12345", "zzzzzz", "#1234567"):
            with self.assertRaises(ValueError):
                sc.parse_hex(bad)


class Fractions(unittest.TestCase):
    def setUp(self):
        self.img = sc.read_png(write(make_png(200, 120, screen())))

    def test_fraction_near_is_one_for_the_panel_rectangle_and_zero_for_the_page(self):
        self.assertEqual(sc.fraction_near(self.img, RECT, PANEL, inset=2), 1.0)
        self.assertEqual(sc.fraction_near(self.img, RECT, PAGE, inset=2), 0.0)

    def test_fraction_near_tolerates_a_small_difference_per_channel(self):
        near = (PANEL[0] - 8, PANEL[1] + 8, PANEL[2])
        img = sc.read_png(write(make_png(200, 120, lambda x, y: near if RECT[0] <= x < 140 and 60 <= y < 80 else PAGE)))
        self.assertEqual(sc.fraction_near(img, RECT, PANEL, inset=0), 1.0)
        far = (PANEL[0] - 9, PANEL[1], PANEL[2])
        img = sc.read_png(write(make_png(200, 120, lambda x, y: far if RECT[0] <= x < 140 and 60 <= y < 80 else PAGE)))
        self.assertEqual(sc.fraction_near(img, RECT, PANEL, inset=0), 0.0)

    def test_fraction_outside_near_ignores_the_rectangle(self):
        self.assertGreater(sc.fraction_outside_near(self.img, RECT, PAGE), 0.99)

    def test_a_rectangle_outside_the_image_counts_as_no_pixels_matching(self):
        self.assertEqual(sc.fraction_near(self.img, (500, 500, 10, 10), PANEL), 0.0)


class Checks(unittest.TestCase):
    def verdict(self, mode, pixel):
        img = sc.read_png(write(make_png(200, 120, pixel)))
        return sc.check(mode, img, RECT, PANEL, PAGE)

    def test_panel_mode_passes_when_the_bar_is_over_the_page(self):
        self.assertEqual(self.verdict("panel", screen())[0], "PASS")

    def test_panel_mode_says_no_when_the_page_is_visible_but_the_bar_is_not(self):
        self.assertEqual(self.verdict("panel", screen(panel_visible=False))[0], "NO")

    def test_panel_mode_is_blocked_when_the_page_is_not_in_the_shot(self):
        self.assertEqual(self.verdict("panel", screen(page_visible=False))[0], "BLOCKED")
        self.assertEqual(self.verdict("panel", lambda x, y: (0, 0, 0))[0], "BLOCKED")

    def test_absent_mode_passes_for_a_shot_before_the_show(self):
        self.assertEqual(self.verdict("absent", screen(panel_visible=False))[0], "PASS")

    def test_absent_mode_says_no_when_the_bar_is_there(self):
        self.assertEqual(self.verdict("absent", screen())[0], "NO")

    def test_unknown_mode_is_an_error(self):
        img = sc.read_png(write(make_png(20, 20, screen())))
        with self.assertRaises(ValueError):
            sc.check("sideways", img, RECT, PANEL, PAGE)


if __name__ == "__main__":
    unittest.main()
