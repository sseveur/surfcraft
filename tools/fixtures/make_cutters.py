"""Generate the test cutter mesh tests/fixtures/cutters/bullnose.stl (ASCII STL, millimetres).

A bullnose cutter: radius 15.5, height 10 (tip at z = 0), flat bottom with a 4 mm corner radius, closed at the top.
Run: python3 -I make_cutters.py <out_file>
"""

import math
import sys

R, H, RC = 15.5, 10.0, 4.0
AROUND, CORNER = 48, 8


def profile():
    """(radius, z) from the tip centre, round the corner, up the side, to the top centre."""
    pts = [(0.0, 0.0)]
    for i in range(CORNER + 1):
        a = -math.pi / 2 + (math.pi / 2) * i / CORNER
        pts.append((R - RC + RC * math.cos(a), RC + RC * math.sin(a)))
    pts.append((R, H))
    pts.append((0.0, H))
    return pts


def facet(out, a, b, c):
    u = [b[i] - a[i] for i in range(3)]
    v = [c[i] - a[i] for i in range(3)]
    n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    length = math.sqrt(sum(x * x for x in n))
    if length < 1e-12:
        return
    n = [x / length for x in n]
    out.append("  facet normal %r %r %r\n    outer loop\n" % tuple(n))
    for p in (a, b, c):
        out.append("      vertex %r %r %r\n" % tuple(p))
    out.append("    endloop\n  endfacet\n")


def main(path):
    prof = profile()
    ring = lambda r, z, k: (r * math.cos(2 * math.pi * k / AROUND), r * math.sin(2 * math.pi * k / AROUND), z)
    out = ["solid bullnose\n"]
    for (r0, z0), (r1, z1) in zip(prof, prof[1:]):
        for k in range(AROUND):
            a, b = ring(r0, z0, k), ring(r0, z0, k + 1)
            c, d = ring(r1, z1, k), ring(r1, z1, k + 1)
            # Outward-facing triangles (counter-clockwise seen from outside).
            facet(out, a, c, b)
            facet(out, b, c, d)
    out.append("endsolid bullnose\n")
    with open(path, "w") as f:
        f.write("".join(out))


if __name__ == "__main__":
    main(sys.argv[1])
