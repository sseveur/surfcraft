"""Generate the SurfCraft test boards (tests/fixtures/*.brd) from a few design parameters.

These boards are original SurfCraft designs: each one is described by its length, widths, thicknesses and rocker,
and the control points below are derived from those numbers. Run: python3 -I make_boards.py <out_dir>
"""

import math
import sys

IN = 2.54  # cm


def bez(p0, p1, p2, p3, t):
    u = 1 - t
    return tuple(u**3 * a + 3 * u * u * t * b + 3 * u * t * t * c + t**3 * d for a, b, c, d in zip(p0, p1, p2, p3))


class Knot:
    def __init__(self, p, prev=None, nxt=None, smooth=True):
        self.p, self.prev, self.next, self.smooth = p, prev or p, nxt or p, smooth

    def brd(self):
        v = [self.p[0], self.p[1], self.prev[0], self.prev[1], self.next[0], self.next[1]]
        return "(cp [%s] %s false)" % (",".join(repr(float(x)) for x in v), "true" if self.smooth else "false")


def curve_y_at(knots, x):
    """y of a curve at x (curves here run left to right in x)."""
    for a, b in zip(knots, knots[1:]):
        if a.p[0] <= x <= b.p[0] and b.p[0] > a.p[0]:
            lo, hi = 0.0, 1.0
            for _ in range(60):
                mid = (lo + hi) / 2
                if bez(a.p, a.next, b.prev, b.p, mid)[0] < x:
                    lo = mid
                else:
                    hi = mid
            return bez(a.p, a.next, b.prev, b.p, (lo + hi) / 2)[1]
    return 0.0


def design(d):
    L, W = d["length"], d["width"]
    hw, wp = W / 2, d["wide_point"] * L
    tail_hw, tail_x = d["tail_width"] / 2, d["tail_block"]
    # Outline (half width), tail centre to nose tip.
    outline = [Knot((0.0, 0.0), nxt=(0.0, tail_hw * 0.55), smooth=False)]
    if tail_x > 0:
        outline.append(Knot((tail_x, tail_hw), prev=(tail_x * 0.4, tail_hw * 0.9),
                            nxt=(tail_x + L * 0.16, tail_hw + (hw - tail_hw) * 0.8), smooth=False))
    outline.append(Knot((wp, hw), prev=(wp - L * d["hips"], hw), nxt=(wp + L * d["shoulders"], hw)))
    tip = d["nose_tip"]
    outline.append(Knot((L, 0.0), prev=(L - L * tip[0], hw * tip[1]), nxt=(L - L * tip[0], -hw * tip[1])))

    tr, nr = d["tail_rocker"], d["nose_rocker"]
    flat = d["flat_spot"] * L
    bottom = [
        Knot((0.0, tr), nxt=(L * 0.12, tr * 0.45)),
        Knot((flat, 0.0), prev=(flat - L * 0.22, 0.0), nxt=(flat + L * 0.24, 0.0)),
        Knot((L, nr), prev=(L - L * d["nose_kick"], nr * 0.28), nxt=(L + 3.0, nr + 6.0)),
    ]
    T, tt, nt = d["thickness"], d["tail_thickness"], d["nose_thickness"]
    tx = d["thick_point"] * L
    tb = curve_y_at(bottom, tx)
    deck = [
        Knot((0.0, tr), nxt=(0.0, tr + tt * 0.5), smooth=False),
        Knot((0.0, tr + tt), prev=(0.0, tr + tt * 0.6), nxt=(L * 0.16, tr * 0.25 + tt + (T - tt) * 0.85), smooth=False),
        Knot((tx, tb + T), prev=(tx - L * 0.24, tb + T), nxt=(tx + L * 0.24, tb + T)),
        Knot((L, nr + nt), prev=(L - L * 0.2, nr * 0.35 + nt + (T - nt) * 0.6), nxt=(L, nr + nt * 0.6), smooth=False),
        Knot((L, nr), prev=(L, nr + nt * 0.4), smooth=False),
    ]

    sections = []
    for frac, vee in d["sections"]:
        # Fraction of the length, a distance from the tail (cm), or (negative) a distance from the nose.
        x = L + frac if frac < 0 else frac * L if frac <= 1 else frac
        w = curve_y_at(outline, x)
        h = curve_y_at(deck, x) - curve_y_at(bottom, x)
        sections.append((x, rail(w, h, d["rail"], vee)))
    return outline, bottom, deck, sections


def rail(w, h, r, vee):
    """Half cross-section from the bottom centre around the rail to the deck centre. r: 0 = boxy, 1 = soft."""
    apex = h * (0.28 + 0.1 * r)
    edge = (w - 0.4 - 0.6 * r, h * 0.03)
    return [
        Knot((0.0, 0.0), prev=(-w * 0.3, 0.0), nxt=(w * 0.3, vee)),
        Knot(edge, prev=(w * 0.62, vee * 0.5), nxt=(w - 0.05, edge[1] + h * 0.04)),
        Knot((w, apex), prev=(w, apex - h * 0.12), nxt=(w, apex + h * (0.18 + 0.08 * r))),
        Knot((w * (0.78 - 0.1 * r), h * 0.93), prev=(w * 0.94, h * 0.84), nxt=(w * 0.5, h)),
        Knot((0.0, h), prev=(w * 0.28, h), nxt=(-w * 0.28, h)),
    ]


BOARDS = {
    "shortboard": dict(name="SurfCraft shortboard", length=72 * IN, width=19.25 * IN, wide_point=0.49,
                       tail_width=10.5 * IN, tail_block=1.6, hips=0.25, shoulders=0.21, nose_tip=(0.13, 0.42),
                       tail_rocker=5.4, nose_rocker=12.8, flat_spot=0.46, nose_kick=0.2,
                       thickness=2.4 * IN, tail_thickness=1.3, nose_thickness=1.2, thick_point=0.47,
                       rail=0.35, sections=[(30.48, 0.15), (0.3, 0.1), (0.5, 0.0), (0.72, 0.1), (-30.48, 0.2)]),
    "funboard": dict(name="SurfCraft mid-length", length=90 * IN, width=21.5 * IN, wide_point=0.5,
                     tail_width=14.5 * IN, tail_block=2.2, hips=0.24, shoulders=0.24, nose_tip=(0.09, 0.62),
                     tail_rocker=6.2, nose_rocker=14.5, flat_spot=0.45, nose_kick=0.18,
                     thickness=2.75 * IN, tail_thickness=1.8, nose_thickness=1.7, thick_point=0.48,
                     rail=0.6, sections=[(30.48, 0.1), (0.3, 0.05), (0.5, 0.0), (0.75, 0.0), (-30.48, 0.15)]),
    "longboard": dict(name="SurfCraft longboard", length=110 * IN, width=22.75 * IN, wide_point=0.52,
                      tail_width=15.5 * IN, tail_block=2.5, hips=0.22, shoulders=0.25, nose_tip=(0.06, 0.78),
                      tail_rocker=7.0, nose_rocker=13.0, flat_spot=0.5, nose_kick=0.16,
                      thickness=3.0 * IN, tail_thickness=2.2, nose_thickness=2.0, thick_point=0.5,
                      rail=0.85, sections=[(30.48, 0.0), (0.33, 0.0), (0.5, 0.0), (0.8, 0.0), (-30.48, 0.0)]),
    # No end sections: readers must add the zero-size ones.
    "scan_template": dict(name="SurfCraft no end sections", length=76 * IN, width=20.5 * IN, wide_point=0.48,
                          tail_width=12.0 * IN, tail_block=0.0, hips=0.26, shoulders=0.22, nose_tip=(0.11, 0.5),
                          tail_rocker=5.0, nose_rocker=11.5, flat_spot=0.47, nose_kick=0.19,
                          thickness=2.5 * IN, tail_thickness=1.4, nose_thickness=1.3, thick_point=0.47,
                          rail=0.5, sections=[(0.25, 0.1), (0.5, 0.0), (0.75, 0.05)], no_end_sections=True),
}


def write(name, d, out):
    outline, bottom, deck, sections = design(d)
    L = d["length"]
    secs = sections
    lines = [
        "p01 : %r" % L, "p03 : %r" % d["thickness"], "p04 : %r" % d["width"],
        "p07 : V4.4", "p08 : %s" % d["name"], "p09 : SurfCraft",
    ]
    for key, knots in (("p32", outline), ("p33", bottom), ("p34", deck)):
        lines.append("%s : (" % key)
        lines += [k.brd() for k in knots]
        lines.append(")")
    lines.append("p35 : (")
    zero = "(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)"
    if not d.get("no_end_sections"):
        lines += ["(p36 0.0", zero, ")"]
    for x, knots in secs:
        lines.append("(p36 %r" % x)
        lines += [k.brd() for k in knots]
        lines.append(")")
    if not d.get("no_end_sections"):
        lines += ["(p36 %r" % L, zero, ")"]
    lines.append(")")
    with open("%s/%s.brd" % (out, name), "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    for name, d in BOARDS.items():
        write(name, d, sys.argv[1])
        print("wrote", name)
