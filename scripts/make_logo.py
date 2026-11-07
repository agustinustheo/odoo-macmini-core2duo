#!/usr/bin/env python3
"""Generate assets/logo.svg: a Ferris-style crab holding an "odoo" sign, next
to an "odoo-cli" wordmark. Everything is plain geometry (no fonts), so the SVG
renders the same everywhere.

Ferris the crab is public domain (CC0, by Karen Rustad Tolva). The "odoo"
lettering is our own drawing in Odoo-like colours, not Odoo's logo artwork.

Usage: python3 scripts/make_logo.py  (writes assets/logo.svg)
"""
import math
from pathlib import Path

W, H = 1280, 420

# Palette
BG_TOP, BG_BOTTOM = "#241b2f", "#14111c"
CRAB, CRAB_DARK, CRAB_LIGHT = "#F74C00", "#B83800", "#FF7A3D"
PLUM, PLUM_LIGHT = "#714B67", "#C08AB5"
TEAL = "#2EC4CC"
SIGN, SIGN_EDGE = "#FFF8F2", "#E8D9CF"


def f(x):
    return f"{x:.1f}".rstrip("0").rstrip(".")


# ------------------------------------------------------------- lettering
class Pen:
    """Monoline geometric letters. Returns SVG elements; advances x."""

    def __init__(self, x, baseline, xh, sw, color):
        self.x, self.b, self.xh, self.sw, self.color = x, baseline, xh, sw, color
        self.r = xh / 2 - sw / 2
        self.asc = xh * 1.62
        self.gap = sw * 0.9
        self.out = []

    def _stroke(self, d):
        self.out.append(
            f'<path d="{d}" fill="none" stroke="{self.color}" stroke-width="{f(self.sw)}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>'
        )

    def _circle(self, cx, cy):
        self.out.append(
            f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(self.r)}" fill="none" '
            f'stroke="{self.color}" stroke-width="{f(self.sw)}"/>'
        )

    def o(self):
        cx = self.x + self.r + self.sw / 2
        self._circle(cx, self.b - self.xh / 2)
        self.x += 2 * self.r + self.sw + self.gap

    def d(self):
        cx = self.x + self.r + self.sw / 2
        self._circle(cx, self.b - self.xh / 2)
        sx = cx + self.r
        self._stroke(f"M{f(sx)} {f(self.b - self.asc)} L{f(sx)} {f(self.b - self.sw / 2)}")
        self.x += 2 * self.r + self.sw + self.gap

    def c(self):
        cx = self.x + self.r + self.sw / 2
        cy = self.b - self.xh / 2
        a0, a1 = math.radians(-50), math.radians(50)  # open on the right
        x0, y0 = cx + self.r * math.cos(a0), cy + self.r * math.sin(a0)
        x1, y1 = cx + self.r * math.cos(a1), cy + self.r * math.sin(a1)
        self._stroke(f"M{f(x0)} {f(y0)} A{f(self.r)} {f(self.r)} 0 1 0 {f(x1)} {f(y1)}")
        self.x += 2 * self.r + self.sw + self.gap * 0.6

    def l(self):
        sx = self.x + self.sw / 2
        self._stroke(f"M{f(sx)} {f(self.b - self.asc)} L{f(sx)} {f(self.b - self.sw / 2)}")
        self.x += self.sw + self.gap * 1.3

    def i(self):
        sx = self.x + self.sw / 2
        self._stroke(f"M{f(sx)} {f(self.b - self.xh + self.sw / 2)} L{f(sx)} {f(self.b - self.sw / 2)}")
        self.out.append(
            f'<circle cx="{f(sx)}" cy="{f(self.b - self.xh - self.sw * 1.6)}" r="{f(self.sw * 0.62)}" fill="{self.color}"/>'
        )
        self.x += self.sw + self.gap

    def hyphen(self):
        y = self.b - self.xh / 2
        self._stroke(f"M{f(self.x + self.sw / 2)} {f(y)} L{f(self.x + self.xh * 0.62)} {f(y)}")
        self.x += self.xh * 0.62 + self.gap * 1.4

    def write(self, text):
        for ch in text:
            {"o": self.o, "d": self.d, "c": self.c, "l": self.l, "i": self.i, "-": self.hyphen}[ch]()
        return self


def word_width(text, xh, sw):
    p = Pen(0, 0, xh, sw, "#000").write(text)
    return p.x - p.gap


# ------------------------------------------------------------- crab
def crab(cx, cy):
    """Ferris-style crab, body centred at (cx, cy), claws raised to a sign."""
    out = []
    # legs (behind the body)
    for side in (-1, 1):
        for k in range(3):
            x0 = cx + side * (100 - k * 22)
            y0 = cy + 40 + k * 10
            knee_x = cx + side * (166 - k * 20)
            knee_y = cy + 34 + k * 20
            x2 = cx + side * (186 - k * 26)
            y2 = cy + 92 + k * 14
            out.append(
                f'<path d="M{f(x0)} {f(y0)} L{f(knee_x)} {f(knee_y)} L{f(x2)} {f(y2)}" fill="none" '
                f'stroke="{CRAB_DARK}" stroke-width="12" stroke-linecap="round" stroke-linejoin="round"/>'
            )
    # arms up to the sign
    for side in (-1, 1):
        sx, sy = cx + side * 104, cy - 12
        ex, ey = cx + side * 128, cy - 132
        out.append(
            f'<path d="M{f(sx)} {f(sy)} Q{f(cx + side * 168)} {f(cy - 60)} {f(ex)} {f(ey)}" fill="none" '
            f'stroke="{CRAB_DARK}" stroke-width="17" stroke-linecap="round"/>'
        )
    # shell with a spiky top edge
    top = cy - 52
    pts = []
    n = 9
    for k in range(n + 1):
        t = k / n
        x = cx - 140 + 280 * t
        bump = 15 if k % 2 == 0 else 0
        y = top + 26 * (2 * t - 1) ** 2 * 1.4 - bump
        pts.append((x, y))
    d = f"M{f(cx - 150)} {f(cy + 8)} "
    d += f"L{f(pts[0][0])} {f(pts[0][1] + 10)} "
    for (x, y) in pts:
        d += f"L{f(x)} {f(y)} "
    d += f"L{f(cx + 150)} {f(cy + 8)} "
    d += f"Q{f(cx + 150)} {f(cy + 70)} {f(cx)} {f(cy + 72)} Q{f(cx - 150)} {f(cy + 70)} {f(cx - 150)} {f(cy + 8)} Z"
    out.append(f'<path d="{d}" fill="url(#shell)" stroke="{CRAB_DARK}" stroke-width="6" stroke-linejoin="round"/>')
    # cheek blush + eyes + smile
    for side in (-1, 1):
        out.append(f'<ellipse cx="{f(cx + side * 88)}" cy="{f(cy + 26)}" rx="20" ry="11" fill="{CRAB_LIGHT}" opacity="0.75"/>')
        ex = cx + side * 40
        out.append(f'<circle cx="{f(ex)}" cy="{f(cy - 6)}" r="17" fill="#1b1420"/>')
        out.append(f'<circle cx="{f(ex + 6)}" cy="{f(cy - 12)}" r="6" fill="#ffffff"/>')
    out.append(
        f'<path d="M{f(cx - 26)} {f(cy + 26)} Q{f(cx)} {f(cy + 50)} {f(cx + 26)} {f(cy + 26)}" fill="none" '
        f'stroke="#1b1420" stroke-width="7" stroke-linecap="round"/>'
    )
    return out


def claw(x, y, side):
    """A pincer gripping the sign edge at (x, y)."""
    rot = -18 * side
    r = 30
    # circle with a V-shaped bite taken out of the top: the pincer
    a = math.radians(32)
    xl, yl = -r * math.sin(a), -r * math.cos(a)
    xr, yr = r * math.sin(a), -r * math.cos(a)
    d = (f"M{f(xr)} {f(yr)} A{r} {r} 0 1 1 {f(xl)} {f(yl)} L0 -4 Z")
    return (
        f'<g transform="translate({f(x)} {f(y)}) rotate({rot})">'
        f'<path d="{d}" fill="url(#shell)" stroke="{CRAB_DARK}" stroke-width="6" stroke-linejoin="round"/>'
        f'<ellipse cx="{f(-9 * side)}" cy="8" rx="8" ry="5" fill="{CRAB_LIGHT}" opacity="0.8"/>'
        f"</g>"
    )


# ------------------------------------------------------------- compose
def build():
    el = []
    el.append(
        f'<defs>'
        f'<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG_TOP}"/><stop offset="1" stop-color="{BG_BOTTOM}"/></linearGradient>'
        f'<linearGradient id="shell" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{CRAB_LIGHT}"/><stop offset="0.55" stop-color="{CRAB}"/><stop offset="1" stop-color="#D84200"/></linearGradient>'
        f'<radialGradient id="glow" cx="0.27" cy="0.5" r="0.45"><stop offset="0" stop-color="{PLUM}" stop-opacity="0.55"/><stop offset="1" stop-color="{PLUM}" stop-opacity="0"/></radialGradient>'
        f'</defs>'
    )
    el.append(f'<rect width="{W}" height="{H}" rx="36" fill="url(#bg)"/>')
    el.append(f'<rect width="{W}" height="{H}" rx="36" fill="url(#glow)"/>')

    # crab + sign
    cx, cy = 320, 282
    el += crab(cx, cy)
    sw_, sh = 300, 112
    sx, sy = cx - sw_ / 2, cy - 132 - sh + 18
    el.append(f'<rect x="{f(sx + 6)}" y="{f(sy + 8)}" width="{sw_}" height="{sh}" rx="22" fill="#000" opacity="0.25"/>')
    el.append(f'<rect x="{f(sx)}" y="{f(sy)}" width="{sw_}" height="{sh}" rx="22" fill="{SIGN}" stroke="{SIGN_EDGE}" stroke-width="5"/>')
    xh, sw = 50, 13
    ww = word_width("odoo", xh, sw)
    pen = Pen(cx - ww / 2, sy + sh / 2 + xh / 2 + 6, xh, sw, PLUM).write("odoo")
    el += pen.out
    el.append(claw(cx - 128, sy + sh - 2, -1))
    el.append(claw(cx + 128, sy + sh - 2, 1))

    # wordmark
    x0 = 600
    avail = W - x0 - 70
    xh = 96
    while word_width("odoo-cli", xh, xh * 0.23) > avail:
        xh -= 1
    sw = xh * 0.23
    base = H / 2 + xh * 0.62
    p1 = Pen(x0, base, xh, sw, PLUM_LIGHT).write("odoo")
    p2 = Pen(p1.x, base, xh, sw, TEAL).write("-cli")
    el += p1.out + p2.out
    # terminal prompt underline: "›_" accent under the wordmark
    y = base + 52
    el.append(
        f'<path d="M{x0 + 4} {y - 14} L{x0 + 22} {y} L{x0 + 4} {y + 14}" fill="none" stroke="{TEAL}" '
        f'stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>'
    )
    el.append(f'<rect x="{x0 + 34}" y="{y + 8}" width="34" height="8" rx="4" fill="{PLUM_LIGHT}"/>')
    el.append(f'<rect x="{x0 + 80}" y="{y - 2}" width="{f(p2.x - p2.gap - x0 - 80)}" height="6" rx="3" fill="#ffffff" opacity="0.12"/>')

    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" '
        f'role="img" aria-label="odoo-cli: a Rust crab holding an odoo sign">'
        f"<title>odoo-cli</title>" + "".join(el) + "</svg>\n"
    )
    out = Path(__file__).resolve().parent.parent / "assets" / "logo.svg"
    out.write_text(svg)
    print(out)


if __name__ == "__main__":
    build()
