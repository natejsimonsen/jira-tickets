#!/usr/bin/env python3
"""Draws the app icon: three fanned ticket cards on a blue rounded square.

Run from the repo root (stdlib only, ~30 s):
    python3 scripts/make-icon.py            # writes macos/AppIcon.png (1024) and macos/AppIcon-256.png
Then rebuild the .icns on macOS:
    python3 scripts/make-icon.py --icns     # also runs sips + iconutil -> macos/AppIcon.icns (16 px uses a simpler mark)

Shapes are signed distance functions; each pixel's coverage comes from its distance to the edge,
which anti-aliases without supersampling.
"""
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

SIZE = 1024
BODY, BODY_RADIUS = 824, 824 * 0.224  # Big Sur grid: 824 px body, ~22.4% corners
TOP, BOTTOM = (0x26, 0x84, 0xFF), (0x00, 0x52, 0xCC)
INK = (0x00, 0x52, 0xCC)
LINE = (0xB3, 0xC6, 0xE6)


def rounded_rect(cx, cy, w, h, r, angle=0.0):
    """Signed distance to a rounded rect centred at (cx, cy), rotated by `angle` degrees."""
    c, s = math.cos(math.radians(angle)), math.sin(math.radians(angle))
    hw, hh = w / 2 - r, h / 2 - r

    def sdf(x, y):
        dx, dy = x - cx, y - cy
        lx, ly = abs(dx * c + dy * s) - hw, abs(-dx * s + dy * c) - hh
        return math.hypot(max(lx, 0), max(ly, 0)) + min(max(lx, ly), 0) - r

    reach = math.hypot(w, h) / 2 + 2
    return sdf, (cx - reach, cy - reach, cx + reach, cy + reach)


def polyline(points, width):
    """Signed distance to a thick stroke with round caps and joins."""
    segs = list(zip(points, points[1:]))

    def sdf(x, y):
        best = 1e9
        for (ax, ay), (bx, by) in segs:
            vx, vy = bx - ax, by - ay
            t = max(0.0, min(1.0, ((x - ax) * vx + (y - ay) * vy) / (vx * vx + vy * vy)))
            best = min(best, math.hypot(x - ax - t * vx, y - ay - t * vy))
        return best - width / 2

    xs, ys = [p[0] for p in points], [p[1] for p in points]
    pad = width / 2 + 2
    return sdf, (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)


class Canvas:
    def __init__(self, size):
        self.size = size
        self.px = [[0.0, 0.0, 0.0, 0.0] for _ in range(size * size)]  # premultiplied RGBA, 0..1

    def fill(self, shape, color, alpha=1.0, soft=1.0, offset=(0, 0)):
        """Composites `shape` over the canvas. `color` is an RGB tuple or a function of y; `soft` widens the edge (blur)."""
        sdf, (x0, y0, x1, y1) = shape
        ox, oy = offset
        pad = soft
        for y in range(max(0, int(y0 + oy - pad)), min(self.size, int(y1 + oy + pad) + 1)):
            rgb = color(y) if callable(color) else color
            r, g, b = (v / 255 for v in rgb)
            for x in range(max(0, int(x0 + ox - pad)), min(self.size, int(x1 + ox + pad) + 1)):
                d = sdf(x + 0.5 - ox, y + 0.5 - oy)
                cov = min(1.0, max(0.0, 0.5 - d / soft))
                if soft > 1:
                    cov = cov * cov * (3 - 2 * cov)
                a = cov * alpha
                if a <= 0:
                    continue
                p = self.px[y * self.size + x]
                k = 1 - a
                p[0], p[1], p[2], p[3] = r * a + p[0] * k, g * a + p[1] * k, b * a + p[2] * k, a + p[3] * k

    def png(self, path, scale=1):
        """Writes the canvas, box-downsampled by `scale`, as an 8-bit RGBA PNG."""
        n = self.size // scale
        rows = []
        for y in range(n):
            row = bytearray(b"\0")
            for x in range(n):
                acc = [0.0] * 4
                for sy in range(scale):
                    base = (y * scale + sy) * self.size + x * scale
                    for p in self.px[base:base + scale]:
                        for i in range(4):
                            acc[i] += p[i]
                a = acc[3]
                rgb = [round(255 * min(1, c / a)) if a else 0 for c in acc[:3]]
                row += bytes(rgb + [round(255 * a / scale / scale)])
            rows.append(bytes(row))

        def chunk(tag, data):
            return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

        with open(path, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n")
            f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)))
            f.write(chunk(b"IDAT", zlib.compress(b"".join(rows), 9)))
            f.write(chunk(b"IEND", b""))


def draw(small=False):
    """Renders the icon; `small` drops the text lines and thickens the check so 16 px stays legible."""
    c = Canvas(SIZE)
    mid = SIZE / 2
    body = rounded_rect(mid, mid, BODY, BODY, BODY_RADIUS)
    c.fill(body, (0, 0, 0), alpha=0.30, soft=28, offset=(0, 12))  # drop shadow
    top_y, span = mid - BODY / 2, BODY

    def gradient(y):
        t = min(1.0, max(0.0, (y - top_y) / span))
        return tuple(a + (b - a) * t for a, b in zip(TOP, BOTTOM))

    c.fill(body, gradient)

    # Cards fan out from a pivot below the body; back cards are translucent white.
    w, h, r = 400, 470, 56
    cx, cy = mid + 40, mid + 30  # front card centre
    pivot = (cx, cy + 520)
    for angle, alpha in ((-16, 0.35), (-8, 0.6), (0, 1.0)):
        a = math.radians(angle)
        dx, dy = 0, cy - pivot[1]
        cx = pivot[0] + dx * math.cos(a) - dy * math.sin(a)
        cy = pivot[1] + dx * math.sin(a) + dy * math.cos(a)
        c.fill(rounded_rect(cx, cy, w, h, r, angle), (255, 255, 255), alpha=alpha)

    # Front card: a check over two short text lines.
    cx, cy = mid + 40, mid + 30
    if small:
        c.fill(polyline([(cx - 110, cy + 10), (cx - 30, cy + 90), (cx + 115, cy - 75)], 90), INK)
        return c
    c.fill(polyline([(cx - 92, cy - 70), (cx - 28, cy - 8), (cx + 96, cy - 132)], 54), INK)
    c.fill(polyline([(cx - 120, cy + 80), (cx + 120, cy + 80)], 34), LINE)
    c.fill(polyline([(cx - 120, cy + 148), (cx + 40, cy + 148)], 34), LINE)
    return c


def main():
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    c = draw()
    c.png("macos/AppIcon.png")
    c.png("macos/AppIcon-256.png", scale=4)
    if "--icns" in sys.argv:
        tmp = tempfile.mkdtemp()
        iconset = os.path.join(tmp, "AppIcon.iconset")
        os.mkdir(iconset)
        small = os.path.join(tmp, "small.png")
        draw(small=True).png(small)
        for px in (16, 32, 128, 256, 512):
            for mult, suffix in ((1, ""), (2, "@2x")):
                out = os.path.join(iconset, f"icon_{px}x{px}{suffix}.png")
                subprocess.run(["sips", "-z", str(px * mult), str(px * mult), small if px * mult == 16 else "macos/AppIcon.png", "--out", out], check=True, capture_output=True)
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", "macos/AppIcon.icns"], check=True)
        shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
