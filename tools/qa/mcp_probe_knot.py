"""Probe: how far can a knot be moved before board_summary stops answering? (headless surfcraft-mcp)

    python3 -I tools/qa/mcp_probe_knot.py --mcp target/release/surfcraft-mcp --out <scratch dir>
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import Mcp, McpError  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcp", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--timeout", type=float, default=15)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    env = {
        "SURFCRAFT_AUTOMATION_FILE": os.path.join(os.path.abspath(a.out), "no-endpoint.json"),
        "SURFCRAFT_APP": os.path.join(os.path.abspath(a.out), "no-such-app"),
    }
    worst = None
    for curve, idx, x, y in (
        ("outline", 1, 1000.0, 30.0),
        ("outline", 1, 1e6, 30.0),
        ("outline", 1, 90.0, 1e6),
        ("outline", 1, 1e12, 1e12),
        ("outline", 1, 1e308, 1e308),
        ("deck", 1, 90.0, 1e6),
        ("bottom", 1, -1e6, 0.0),
        ("section", 1, 1e6, 1e6),
    ):
        m = Mcp(os.path.abspath(a.mcp), env)
        m.initialize()
        ok, r, _ = m.call("set_knot", {"curve": curve, "index": idx, "x": x, "y": y})
        try:
            ok2, s, _ = m.call("board_summary", {}, timeout=a.timeout)
            res = f"summary ok={ok2} volume={s.get('volume_litres') if isinstance(s, dict) else s!r}"
        except McpError as e:
            res = f"NO ANSWER ({e})"
            worst = (curve, x, y)
        print(f"set_knot {curve}[{idx}] = ({x:g}, {y:g}): set ok={ok}; {res}")
        m.proc.kill()
        m.proc.wait()
    return 1 if worst else 0


if __name__ == "__main__":
    sys.exit(main())
