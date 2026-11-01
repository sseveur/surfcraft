"""Live QA of surfcraft-mcp driving a real SurfCraft window, plus security probes of the automation endpoint.

    python3 -I tools/qa/mcp_live.py --mcp target/release/surfcraft-mcp --app target/release/surfcraft --out <scratch dir>

Isolation: SURFCRAFT_AUTOMATION_FILE, HOME and TMPDIR point into <out>, so the user's settings, recent files and
endpoint file are not touched. The app is launched by the MCP `launch_app` tool. At the end the app is killed
(SIGKILL) to check that a stale endpoint file is handled; normal-exit cleanup is not reachable without UI access.
Screenshots are written to <out>/shots/.
"""

import argparse
import base64
import json
import os
import signal
import socket
import stat
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import Mcp, McpError  # noqa: E402

FAILS = []
NOTES = []


def fail(msg):
    FAILS.append(msg)
    print("FAIL:", msg, flush=True)


def note(msg):
    NOTES.append(msg)
    print("NOTE:", msg, flush=True)


def check(cond, msg):
    if not cond:
        fail(msg)
    return cond


def rss_kb(pid):
    try:
        return int(subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0)
    except ValueError:
        return 0


def threads(pid):
    out = subprocess.run(["ps", "-M", "-p", str(pid)], capture_output=True, text=True).stdout
    return max(0, len(out.strip().splitlines()) - 1)


def raw_call(port, msg, timeout=10):
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as s:
        s.sendall((msg if isinstance(msg, str) else json.dumps(msg)).encode() + b"\n")
        f = s.makefile("rb")
        line = f.readline()
        return json.loads(line) if line else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcp", required=True)
    ap.add_argument("--app", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    shots = os.path.join(out, "shots")
    home = os.path.join(out, "home")
    tmp = os.path.join(out, "tmp")
    for d in (shots, home, tmp):
        os.makedirs(d, exist_ok=True)
    endpoint = os.path.join(out, "endpoint.json")
    if os.path.exists(endpoint):
        os.remove(endpoint)
    env = {
        "SURFCRAFT_AUTOMATION_FILE": endpoint,
        "SURFCRAFT_APP": os.path.abspath(a.app),
        "HOME": home,
        "TMPDIR": tmp + "/",
    }
    m = Mcp(os.path.abspath(a.mcp), env)
    m.initialize()
    shot_no = [0]

    def call(name, args=None, expect_ok=True, timeout=180):
        try:
            ok, body, _ = m.call(name, args or {}, timeout=timeout)
        except McpError as e:
            fail(f"{name} {json.dumps(args)[:120]}: {e}")
            return None
        if expect_ok and not ok:
            fail(f"{name} {json.dumps(args)[:160]} -> {str(body)[:300]}")
        if expect_ok is False and ok:
            fail(f"{name} {json.dumps(args)[:160]} accepted, expected an error")
        return body if ok else ("ERR", body)

    def screenshot(label):
        t0 = time.time()
        r = call("screenshot", {}, timeout=150)
        dt = time.time() - t0
        if isinstance(r, dict) and r.get("type") == "image":
            data = base64.b64decode(r["data"])
            check(data.startswith(b"\x89PNG"), f"screenshot {label} is a PNG")
            shot_no[0] += 1
            path = os.path.join(shots, f"{shot_no[0]:02d}-{label}.png")
            with open(path, "wb") as f:
                f.write(data)
            print(f"  screenshot {label}: {len(data)} bytes in {dt:.1f} s -> {path}", flush=True)
            return path
        fail(f"screenshot {label}: {str(r)[:200]}")
        return None

    def status():
        r = call("connection_status", {})
        return r.get("window", {}) if isinstance(r, dict) else {}

    s = call("connection_status")
    check(s.get("mode") == "headless", f"starts headless: {s}")
    # Headless work before the window opens is handed over to it.
    call("set_info", {"name": "Handoff test"})
    t0 = time.time()
    r = call("launch_app", {}, timeout=60)
    print(f"launch_app: {r} in {time.time() - t0:.1f} s", flush=True)
    if not isinstance(r, dict):
        return finish(m, out)
    st = status()
    print("window status:", st, flush=True)
    check(st.get("title", "").startswith("Handoff test") or "surfcraft-mcp-" in (st.get("path") or ""), f"headless board handed over: {st}")
    if st.get("path") and "surfcraft-mcp-" in st["path"]:
        note(f"after launch_app the window's document is the hand-off temp file {st['path']} (title {st.get('title')!r}); Save writes there silently")
    r = call("launch_app", {})
    check(isinstance(r, dict) and r.get("already_running"), f"second launch_app reuses the window: {r}")

    # ---- endpoint file and listener
    try:
        ep = json.load(open(endpoint))
    except (OSError, ValueError) as e:
        fail(f"endpoint file: {e}")
        return finish(m, out)
    mode = stat.S_IMODE(os.stat(endpoint).st_mode)
    check(mode == 0o600, f"endpoint file mode {oct(mode)}")
    print(f"endpoint: port {ep['port']}, pid {ep['pid']}, token length {len(ep['token'])}, mode {oct(mode)}", flush=True)
    check(len(ep["token"]) == 32 and int(ep["token"], 16) >= 0, "token is 128-bit hex")
    pid = ep["pid"]
    lsof = subprocess.run(["lsof", "-nP", "-a", "-p", str(pid), "-iTCP", "-sTCP:LISTEN"], capture_output=True, text=True).stdout
    print("lsof:\n" + lsof.strip(), flush=True)
    listen = [l for l in lsof.splitlines()[1:]]
    check(listen and all("127.0.0.1:" in l for l in listen), "app listens on 127.0.0.1 only")
    check(any(f":{ep['port']}" in l for l in listen), "listening port matches the endpoint file")

    # ---- token checks over a raw socket
    r = raw_call(ep["port"], {"token": "0" * 32, "cmd": "status", "args": {}})
    check(r == {"ok": False, "error": "invalid token"}, f"wrong token rejected: {r}")
    r = raw_call(ep["port"], {"cmd": "status"})
    check(r and r.get("ok") is False, f"missing token rejected: {r}")
    r = raw_call(ep["port"], {"token": ep["token"][:-1] + ("0" if ep["token"][-1] != "0" else "1"), "cmd": "set_info", "args": {"name": "pwned"}})
    check(r and r.get("ok") is False, f"near-miss token rejected: {r}")
    r = raw_call(ep["port"], "not json")
    check(r and r.get("ok") is False, f"garbage rejected: {r}")
    # Many guesses on one connection are allowed (no lockout): note it.
    with socket.create_connection(("127.0.0.1", ep["port"]), timeout=10) as s:
        f = s.makefile("rb")
        n = 2000
        t0 = time.time()
        for i in range(n):
            s.sendall(json.dumps({"token": f"{i:032x}", "cmd": "status"}).encode() + b"\n")
            f.readline()
        rate = n / (time.time() - t0)
    note(f"no rate limit or disconnect on wrong tokens: {rate:.0f} guesses/s on one connection (128-bit token, so not exploitable in practice)")
    r = raw_call(ep["port"], {"token": ep["token"], "cmd": "status"})
    check(r and r.get("ok") is True, f"right token accepted over a raw socket: {str(r)[:120]}")

    # ---- pre-authentication resource use
    before = rss_kb(pid)
    with socket.create_connection(("127.0.0.1", ep["port"]), timeout=30) as s:
        chunk = b"a" * (1 << 20)
        for _ in range(64):
            s.sendall(chunk)
        s.sendall(b"\n")
        s.settimeout(60)
        try:
            s.makefile("rb").readline()
        except OSError:
            pass
    time.sleep(1)
    after = rss_kb(pid)
    grow = (after - before) / 1024
    print(f"64 MB unauthenticated line: app RSS {before / 1024:.0f} -> {after / 1024:.0f} MB", flush=True)
    if grow > 32:
        note(f"an unauthenticated client can make the app buffer an unbounded line (64 MB line grew RSS by {grow:.0f} MB); lines are read before the token is checked")
    t_before = threads(pid)
    socks = []
    for _ in range(200):
        try:
            socks.append(socket.create_connection(("127.0.0.1", ep["port"]), timeout=5))
        except OSError as e:
            note(f"connection refused after {len(socks)} sockets: {e}")
            break
    time.sleep(1)
    t_after = threads(pid)
    print(f"200 idle connections: app threads {t_before} -> {t_after}", flush=True)
    if t_after - t_before >= 150:
        note(f"every connection gets its own thread with no limit (200 idle sockets -> +{t_after - t_before} threads), pre-authentication")
    ok_while = status()
    check(bool(ok_while), "app answers while 200 sockets are open")
    for s in socks:
        s.close()

    # ---- realistic design session
    screenshot("launched")
    r = call("new_board", {"template": "funboard", "length": "7'6\"", "width": "21 1/2\"", "volume_litres": 52}, expect_ok=None)
    if isinstance(r, tuple):
        check("unsaved" in str(r[1]), f"new_board over unsaved changes refused with a clear message: {r[1]}")
        r = call("new_board", {"template": "funboard", "length": "7'6\"", "width": "21 1/2\"", "volume_litres": 52, "discard_unsaved": True})
    if isinstance(r, dict):
        check(abs(r["volume_litres"] - 52) < 0.1 and abs(r["length"]["cm"] - 228.6) < 0.01, f"new funboard: {r['length']} {r['volume_litres']}")
    st = status()
    check(st.get("unsaved_changes") is False and st.get("can_undo") is False, f"fresh board state: {st}")
    call("set_view", {"view": "overview"})
    screenshot("new-funboard")
    outline = call("get_curve", {"curve": "outline"})
    deck = call("get_curve", {"curve": "deck"})
    print(f"  outline knots {len(outline['knots'])}, deck knots {len(deck['knots'])}", flush=True)
    wide = max(outline["knots"], key=lambda k: k["point"][1])
    s0 = call("board_summary")
    r = call("move_knot", {"curve": "outline", "index": wide["index"], "dy": "1/2\""})
    s = call("board_summary")
    print(f"  after widening: width {s['max_width']['text']}, volume {s['volume_litres']} L", flush=True)
    st = status()
    check(st.get("can_undo") is True and st.get("unsaved_changes") is True and st.get("title", "").endswith("*"), f"window shows the edit as unsaved and undoable: {st}")
    call("set_view", {"view": "outline"})
    screenshot("outline-widened")
    r = call("undo")
    s2 = call("board_summary")
    # The outline is the half width: +1/2" on the knot is +1" of width.
    check(abs(s["max_width"]["cm"] - s0["max_width"]["cm"] - 2.54) < 0.05, f"widening by 1/2\" per side: {s0['max_width']} -> {s['max_width']}")
    check(abs(s2["max_width"]["cm"] - s0["max_width"]["cm"]) < 1e-6 and s2["volume_litres"] == s0["volume_litres"], f"undo in the window reverts the MCP edit: {s2['max_width']} vs {s0['max_width']}")
    call("redo")
    r = call("move_knot", {"curve": "deck", "index": 1, "dy": 0.3})
    r = call("add_section", {"position": "4'"})
    sec = r["section_index"] if isinstance(r, dict) else 2
    call("select_section", {"section_index": sec})
    call("set_view", {"view": "section"})
    screenshot("section-added")
    r = call("move_section", {"section_index": sec, "position": 130})
    sec = r["section_index"] if isinstance(r, dict) else sec
    info = call("section_info", {"section_index": sec})
    print(f"  section {sec}: {json.dumps(info)[:160]}", flush=True)
    call("remove_section", {"section_index": sec})
    r = call("select_section", {"section_index": 0}, expect_ok=False)
    r = call("scale_board", {"length": "7'8\"", "volume_litres": 55})
    if isinstance(r, dict):
        check(abs(r["volume_litres"] - 55) < 0.1, f"scale to 55 L: {r['volume_litres']}")
    call("set_fins", {"side_rear_x": "11\"", "side_rear_y": "8 1/2\"", "side_front_x": "15 1/2\"", "side_front_y": "9 1/4\"", "centre_rear_x": "3\"", "centre_front_x": "8\"", "splay_degrees": 4})
    call("set_info", {"name": "QA Mid", "designer": "QA", "surfer": "Tester", "fin_type": "thruster"})
    w = call("estimate_weight")
    print(f"  weight: {w.get('total_kg') if isinstance(w, dict) else w} kg", flush=True)
    call("set_view", {"view": "3d"})
    screenshot("3d")
    for view in ("overview", "outline", "rocker", "section", "3d"):
        r = call("set_view", {"view": view, "fit": True})
        check(status().get("view", "").lower().startswith({"overview": "overview", "outline": "outline", "rocker": "profile", "section": "section", "3d": "three"}[view]), f"set_view {view}")
        if view in ("rocker", "overview"):
            screenshot(f"view-{view}")
    for units in ("ft_in", "ft_in_decimal", "mm", "cm", "m"):
        call("set_units", {"units": units})
        st = status()
        s = call("board_summary")
        print(f"  units {units}: window {st.get('units')!r}, length {s['length']['text']}", flush=True)
    call("set_units", {"units": "cm"})
    call("set_view", {"view": "overview"})
    screenshot("units-cm")
    # Exports, one of each format, and toolpaths.
    for what, fmt in (("outline", "pdf"), ("profile", "svg"), ("sections", "dxf"), ("spec_sheet", "pdf"), ("model", "stl"), ("model", "obj"), ("model", "step"), ("hws_ribs", "dxf")):
        p = os.path.join(out, f"live-{what}.{fmt}")
        r = call("export", {"what": what, "format": fmt, "path": p})
        check(os.path.exists(p) and os.path.getsize(p) > 100, f"export {what}.{fmt} written")
    for job in ("deck", "bottom", "hotwire_profile", "hotwire_outline"):
        p = os.path.join(out, f"live-{job}.nc")
        t0 = time.time()
        r = call("generate_toolpath", {"job": job, "path": p})
        if isinstance(r, dict):
            print(f"  toolpath {job}: {r['moves']} moves, {r['cut_length_m']} m, {r['estimated_minutes']} min in {time.time() - t0:.2f} s", flush=True)
    # Save: where does a save without a path go?
    r = call("save_board", {}, expect_ok=None)
    print(f"  save_board without path: {r}", flush=True)
    if isinstance(r, dict) and "surfcraft-mcp-" in r.get("saved", ""):
        note(f"save_board without a path after launch_app saves into the hand-off temp file {r['saved']}")
    bp = os.path.join(out, "live-session.brd")
    call("save_board", {"path": bp})
    st = status()
    check(st.get("unsaved_changes") is False and st.get("path") == bp, f"saved state: {st}")
    # Undo all the way and redo.
    n = 0
    while n < 300:
        r = call("undo")
        if not isinstance(r, dict) or not r.get("undone"):
            break
        n += 1
    print(f"  undid {n} steps", flush=True)
    s = call("board_summary")
    check(abs(s["volume_litres"] - 52) < 0.1, f"undo back to the new board: {s['volume_litres']} L")
    screenshot("undone")
    for _ in range(n):
        call("redo")
    s = call("board_summary")
    check(abs(s["volume_litres"] - 55) < 0.1 and s["name"] == "QA Mid", f"redo all: {s['volume_litres']} {s['name']}")
    r = call("open_board", {"path": os.path.join(out, "live-model.step")}, expect_ok=None)
    print(f"  open STEP in the window: {str(r)[:150]}", flush=True)
    call("set_view", {"view": "3d"})
    screenshot("step-reimport")

    # ---- the window dies; the assistant asks for it again straight away
    os.kill(pid, signal.SIGKILL)
    time.sleep(1)
    check(os.path.exists(endpoint), "endpoint file left behind after SIGKILL (expected)")
    ok, r, _ = m.call("launch_app", {"path": bp}, timeout=60)
    if not ok:
        fail(f"launch_app right after the window died fails with {r!r} (the MCP still holds the dead connection); a second call is needed")
        r = call("launch_app", {"path": bp}, timeout=60)
    st = status()
    check(st.get("path") == bp, f"relaunch opens the file: {st}")
    screenshot("relaunched")
    ep2 = json.load(open(endpoint))
    check(ep2["pid"] != pid and ep2["token"] != ep["token"], "new endpoint with a new token after relaunch")
    pid = ep2["pid"]

    # ---- stale endpoint: kill the app hard, then use the MCP again
    os.kill(pid, signal.SIGKILL)
    time.sleep(1)
    t0 = time.time()
    s = call("connection_status")
    dt = time.time() - t0
    check(isinstance(s, dict) and s.get("mode") == "headless", f"MCP falls back to headless with a stale endpoint: {s}")
    print(f"  connection_status with stale endpoint: {s.get('mode') if isinstance(s, dict) else s} in {dt:.2f} s", flush=True)
    r = call("board_summary")
    note(f"after the window died, headless board is {r.get('name')!r} / {r.get('volume_litres')} L (window edits are not carried back)") if isinstance(r, dict) else None
    # Stale endpoint pointing at a listener that is not SurfCraft.
    probe_foreign_listener(m, endpoint, call)
    return finish(m, out)


def probe_foreign_listener(m, endpoint, call):
    """Endpoint file pointing at a port where something else listens."""
    # Garbage replier.
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    port = srv.getsockname()[1]

    def garbage():
        while True:
            try:
                c, _ = srv.accept()
            except OSError:
                return
            c.recv(65536)
            c.sendall(b"HTTP/1.1 400 Bad Request\r\n\r\n")
            c.close()

    threading.Thread(target=garbage, daemon=True).start()
    with open(endpoint, "w") as f:
        json.dump({"port": port, "token": "x" * 32, "pid": 1}, f)
    t0 = time.time()
    ok, body, _ = m.call("board_summary", {}, timeout=60)
    print(f"  stale endpoint -> garbage listener: ok={ok} {str(body)[:120]} in {time.time() - t0:.1f} s", flush=True)
    if not ok:
        note(f"an endpoint file pointing at another service makes every MCP tool fail ({str(body)[:80]}) instead of falling back to headless")
    srv.close()
    # Silent listener: accepts, never answers.
    srv2 = socket.socket()
    srv2.bind(("127.0.0.1", 0))
    srv2.listen(5)
    port2 = srv2.getsockname()[1]
    with open(endpoint, "w") as f:
        json.dump({"port": port2, "token": "x" * 32, "pid": 1}, f)
    t0 = time.time()
    try:
        ok, body, _ = m.call("board_summary", {}, timeout=20)
        print(f"  stale endpoint -> silent listener: ok={ok} in {time.time() - t0:.1f} s", flush=True)
    except McpError:
        note("an endpoint file pointing at a listener that never answers blocks each MCP call for the 130 s read timeout (no answer within 20 s)")
        # Let the pending call finish so the session stays usable.
        srv2.close()
        r = m.read_line(150)
        print(f"  ... late reply after {time.time() - t0:.0f} s: {str(r)[:120]}", flush=True)
    srv2.close()
    try:
        os.remove(endpoint)
    except OSError:
        pass


def finish(m, out):
    m.close()
    report = {"fails": FAILS, "notes": NOTES, "calls": m.log}
    with open(os.path.join(out, "mcp_live_report.json"), "w") as f:
        json.dump(report, f, indent=2)
    slow = sorted(m.log, key=lambda e: -e["seconds"])[:6]
    print("slowest calls:", ", ".join(f"{e['tool']} {e['seconds']}s" for e in slow))
    print(f"\n{len(FAILS)} failures, {len(NOTES)} notes, {len(m.log)} calls")
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
