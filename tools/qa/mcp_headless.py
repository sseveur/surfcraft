"""Headless QA of surfcraft-mcp over stdio: every tool from tools/list with valid and invalid arguments, schema
sanity, JSON-RPC edge cases. No SurfCraft window is involved (discovery points at a missing file).

    python3 -I tools/qa/mcp_headless.py --mcp target/release/surfcraft-mcp --out <scratch dir>

Exit code 0 when no check failed; findings that are not hard failures are listed as "notes".
"""

import argparse
import json
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import Mcp, McpError  # noqa: E402

FAILS = []
NOTES = []


def fail(msg):
    FAILS.append(msg)
    print("FAIL:", msg)


def note(msg):
    NOTES.append(msg)
    print("NOTE:", msg)


def check(cond, msg):
    if not cond:
        fail(msg)
    return cond


# ------------------------------------------------------------------------------------------------ schemas

JSON_TYPES = {"object", "array", "string", "number", "integer", "boolean", "null"}


def check_schema(tool):
    name = tool.get("name")
    s = tool.get("inputSchema")
    check(isinstance(tool.get("description"), str) and tool["description"], f"{name}: description")
    if not check(isinstance(s, dict), f"{name}: inputSchema is an object"):
        return
    check(s.get("type") == "object", f"{name}: inputSchema.type == object")
    props = s.get("properties", {})
    check(isinstance(props, dict), f"{name}: properties is an object")
    req = s.get("required", [])
    check(isinstance(req, list), f"{name}: required is a list")
    missing = [r for r in req if r not in props]
    check(not missing, f"{name}: required {missing} not in properties")
    if req == []:
        note(f"{name}: 'required: []' (invalid in JSON Schema draft-04 clients)") if name == "board_summary" else None
    for pname, p in props.items():
        if not check(isinstance(p, dict), f"{name}.{pname}: property schema is an object"):
            continue
        t = p.get("type")
        types = t if isinstance(t, list) else [t]
        if t is None and "enum" not in p:
            fail(f"{name}.{pname}: no type")
        for ty in types:
            if ty is not None and ty not in JSON_TYPES:
                fail(f"{name}.{pname}: bad type {ty!r}")
        if isinstance(t, list):
            NOTES_UNION.add(f"{name}.{pname}")
        if "enum" in p:
            check(isinstance(p["enum"], list) and p["enum"], f"{name}.{pname}: enum list")
        if ty == "array":
            check(isinstance(p.get("items"), dict), f"{name}.{pname}: array items schema")
        d = p.get("description")
        if isinstance(d, str) and d.startswith(" ("):
            NOTES_EMPTY_DESC.add(f"{name}.{pname}")


NOTES_UNION = set()
NOTES_EMPTY_DESC = set()


# ------------------------------------------------------------------------------------------------ file checks

MAGIC = {
    "pdf": lambda b: b.startswith(b"%PDF"),
    "svg": lambda b: b"<svg" in b[:500],
    "dxf": lambda b: b.lstrip().startswith(b"0") and b"SECTION" in b[:200],
    "stl": lambda b: b.startswith(b"solid") or len(b) > 84,
    "obj": lambda b: b"\nv " in b or b.startswith(b"v ") or b"\nv " in b[:2000] or b.startswith(b"#"),
    "step": lambda b: b.startswith(b"ISO-10303-21"),
    "nc": lambda b: b"G21" in b and b.rstrip().endswith(b"M2"),
}


def check_file(path, fmt, what):
    try:
        data = open(path, "rb").read()
    except OSError as e:
        fail(f"{what}: {path} not written ({e})")
        return 0
    check(len(data) > 100, f"{what}: {path} is {len(data)} bytes")
    check(MAGIC[fmt](data), f"{what}: {path} does not look like {fmt}")
    if fmt in ("svg", "dxf", "obj", "step", "nc", "stl") and data[:5] != b"\x00\x00\x00\x00\x00":
        low = data[:2_000_000].lower()
        if b"nan" in low.replace(b"nanotech", b"") and fmt != "pdf":
            # crude, but NaN never belongs in geometry text
            if fmt in ("dxf", "obj", "nc") or b" nan" in low or b",nan" in low or b"(nan" in low:
                fail(f"{what}: NaN in {path}")
    return len(data)


# ------------------------------------------------------------------------------------------------ main


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcp", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--skip-hang-probe", action="store_true")
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    env = {
        "SURFCRAFT_AUTOMATION_FILE": os.path.join(out, "no-endpoint.json"),
        "SURFCRAFT_APP": os.path.join(out, "no-such-app"),
    }
    m = Mcp(os.path.abspath(a.mcp), env)
    init = m.initialize()
    res = init["result"]
    check(res["serverInfo"]["name"] == "surfcraft", "serverInfo.name")
    check(res["protocolVersion"] == "2025-06-18", "protocol version echoed")

    # Protocol edge cases.
    r = m.request("initialize", {"protocolVersion": "1999-01-01", "capabilities": {}})
    if r.get("result", {}).get("protocolVersion") == "1999-01-01":
        note("initialize echoes any protocolVersion, even an unsupported one (1999-01-01)")
    m.send_raw("{not json")
    r = m.read_line(5)
    check(r is not None and r.get("error", {}).get("code") == -32700, f"parse error reply: {r}")
    m.send_raw(json.dumps([{"jsonrpc": "2.0", "id": 99, "method": "ping"}]))
    r = m.read_line(2)
    if r is None:
        note("a JSON-RPC batch (array) gets no reply at all (batching was removed in 2025-06-18, but an error reply is expected)")
    m.send_raw(json.dumps({"jsonrpc": "2.0", "id": "str-id", "method": "ping"}))
    r = m.read_line(5)
    check(r is not None and r.get("id") == "str-id", f"string ids echoed: {r}")
    r = m.request("tools/call", {"arguments": {}})
    check(r.get("error", {}).get("code") == -32602, f"tools/call without name: {r}")
    r = m.request("tools/call", {"name": "board_summary", "arguments": "not an object"})
    note(f"tools/call with arguments as a string: {'error' if 'error' in r or r['result'].get('isError') else 'accepted'}")
    r = m.request("resources/list", {})
    check(r.get("error", {}).get("code") == -32601, f"resources/list -> method not found: {r}")

    tools = m.request("tools/list", {})["result"]["tools"]
    names = [t["name"] for t in tools]
    print(f"{len(tools)} tools: {', '.join(names)}")
    check(len(set(names)) == len(names), "tool names unique")
    for t in tools:
        check_schema(t)
    if NOTES_UNION:
        note("union types ['number','string'] in " + ", ".join(sorted(NOTES_UNION)[:6]) + f" ... ({len(NOTES_UNION)} properties; some function-calling clients reject type arrays)")
    if NOTES_EMPTY_DESC:
        note("descriptions starting with ' (cm, ...' (empty label) in " + ", ".join(sorted(NOTES_EMPTY_DESC)))

    called = set()
    results = []

    def call(name, args, expect_ok=True, label=""):
        called.add(name)
        try:
            ok, body, raw = m.call(name, args)
        except McpError as e:
            fail(f"{name} {label}: {e}")
            return None
        results.append({"tool": name, "args": args, "ok": ok, "label": label})
        if expect_ok is True and not ok:
            fail(f"{name} {label} {json.dumps(args)[:200]} -> error {str(body)[:300]}")
        if expect_ok is False and ok:
            fail(f"{name} {label} {json.dumps(args)[:200]} accepted, expected an error: {str(body)[:200]}")
        return body if ok else ("ERR", body)

    # --- valid calls, in a realistic order
    s = call("connection_status", {})
    check(isinstance(s, dict) and s.get("mode") == "headless", f"headless mode: {s}")
    s = call("board_summary", {})
    # The default board is the built-in shortboard; its volume as computed by the reference program.
    golden = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../tests/fixtures/shortboard.golden.json")
    with open(golden) as f:
        litres = json.load(f)["slinear"]["volume"] / 1000.0
    check(abs(s["volume_litres"] - litres) < 0.01, f"default template volume {s['volume_litres']} L, reference {litres:.2f} L")
    # A size different from the template's (72" x 19 1/4"), so the scaling is exercised.
    s = call("new_board", {"template": "shortboard", "length": "6'4\"", "width": "20\"", "volume_litres": 28.5})
    check(abs(s["volume_litres"] - 28.5) < 0.1, f"new_board volume {s['volume_litres']}")
    check(abs(s["length"]["cm"] - 193.04) < 0.01, f"new_board length {s['length']}")
    check(abs(s["max_width"]["cm"] - 50.8) < 0.05, f"new_board width {s['max_width']}")
    base = s
    call("sample_board", {})
    sb = call("sample_board", {"stations": [10, "3'", "100cm"]})
    check(isinstance(sb, list) and len(sb) == 3, "sample_board stations")
    for curve in ("outline", "deck", "bottom", "section"):
        c = call("get_curve", {"curve": curve})
        check(isinstance(c, dict) and len(c.get("knots", [])) >= 2, f"get_curve {curve}")
    k = call("get_curve", {"curve": "outline"})["knots"][1]
    r = call("set_knot", {"curve": "outline", "index": 1, "x": k["point"][0], "y": k["point"][1] + 0.5})
    check(abs(r["point"][1] - (k["point"][1] + 0.5)) < 1e-6, f"set_knot moved y: {r}")
    r = call("move_knot", {"curve": "outline", "index": 1, "dy": -0.5})
    check(abs(r["point"][1] - k["point"][1]) < 1e-6, f"move_knot back: {r}")
    r = call("move_knot", {"curve": "deck", "index": 1, "handle": "next", "dx": "1cm"})
    call("set_smooth", {"curve": "deck", "index": 1, "smooth": False})
    call("set_smooth", {"curve": "deck", "index": 1, "smooth": True})
    r = call("add_knot", {"curve": "bottom", "x": 100, "y": 0.5})
    new_idx = r["index"] if isinstance(r, dict) else None
    if new_idx is not None:
        call("delete_knot", {"curve": "bottom", "index": new_idx})
    si = call("section_info", {})
    check("release_angle_degrees" in si, "section_info fields")
    r = call("add_section", {"position": 50})
    sec = r["section_index"]
    r = call("move_section", {"section_index": sec, "position": "60cm"})
    sec = r["section_index"]
    s = call("board_summary", {})
    check(any(abs(x["position"]["cm"] - 60.0) < 1e-6 for x in s["sections"]), "section moved to 60 cm")
    call("remove_section", {"section_index": sec})
    s = call("scale_board", {"length": 185, "volume_litres": 29})
    check(abs(s["length"]["cm"] - 185) < 0.01 and abs(s["volume_litres"] - 29) < 0.1, f"scale_board: {s['length']} {s['volume_litres']}")
    call("set_info", {"name": "QA board", "designer": "QA", "comments": "line1\nline2 (paren)"})
    s = call("board_summary", {})
    check(s["name"] == "QA board", "set_info name")
    r = call("set_fins", {"side_rear_x": 30, "side_rear_y": "11 1/2\"", "splay_degrees": 4})
    check(r["fins"][0] == 30 and abs(r["fins"][1] - 29.21) < 0.01 and r["fins"][8] == 4, f"set_fins {r}")
    w = call("estimate_weight", {})
    check(2.0 < w["total_kg"] < 6.0, f"estimate_weight total {w['total_kg']}")
    w2 = call("estimate_weight", {"foam_density": 30})
    check(w2["foam_kg"] > w["foam_kg"], "estimate_weight foam density override")
    for model in ("control_point", "sblend"):
        r = call("set_surface_model", {"model": model})
        check(r["volume_litres"] > 0, f"set_surface_model {model}")
    before = call("board_summary", {})
    call("flip_board", {})
    call("flip_board", {})
    after = call("board_summary", {})
    check(abs(before["volume_litres"] - after["volume_litres"]) < 0.01, "flip twice keeps volume")
    for u in ("ft_in", "ft_in_decimal", "mm", "cm", "m"):
        r = call("set_units", {"units": u})
        s = call("board_summary", {})
        print(f"  units {u}: length {s['length']['text']}")
    call("set_units", {"units": "cm"})

    # Exports: every 'what' with each format it supports.
    export_schema = next(t for t in tools if t["name"] == "export")["inputSchema"]["properties"]
    whats = export_schema["what"]["enum"]
    sizes = {}
    for what in whats:
        fmts = ["stl", "obj", "step"] if what == "model" else ["pdf", "svg", "dxf"]
        for fmt in fmts:
            path = os.path.join(out, f"export-{what}.{fmt}")
            r = call("export", {"what": what, "format": fmt, "path": path, "paper": "A3" if fmt == "pdf" else "A4"}, label=f"{what}/{fmt}")
            if isinstance(r, dict):
                sizes[f"{what}.{fmt}"] = check_file(path, fmt, f"export {what}/{fmt}")
    print(f"  exported {len(sizes)} files")
    # Overwrite keeps a backup.
    p = os.path.join(out, "export-outline.svg")
    call("export", {"what": "outline", "format": "svg", "path": p})
    check(os.path.exists(p + ".bak"), "export over an existing file keeps .bak")

    # Toolpaths.
    for job in ("deck", "bottom", "hotwire_profile", "hotwire_outline"):
        path = os.path.join(out, f"tp-{job}.nc")
        t0 = time.time()
        r = call("generate_toolpath", {"job": job, "path": path, "lengthwise_resolution": 2}, label=job)
        if isinstance(r, dict):
            check_file(path, "nc", f"generate_toolpath {job}")
            print(f"  toolpath {job}: {r['passes']} passes, {r['moves']} moves, {r['cut_length_m']} m, {r['estimated_minutes']} min ({time.time() - t0:.2f} s)")
    r = call("generate_toolpath", {"job": "deck", "holding": "end_clamps", "deck_cuts": 5, "cutter_diameter": "1\"", "corner_radius": 0.3, "cutting_speed_mm_s": 100})
    r = call("generate_toolpath", {"job": "bottom", "bottom_cuts": 0, "bottom_rail_cuts": 0})

    # Save / open round trip.
    bpath = os.path.join(out, "qa.brd")
    call("save_board", {"path": bpath})
    call("save_board", {})
    check(os.path.exists(bpath + ".bak"), "save over existing keeps .bak")
    s1 = call("board_summary", {})
    call("new_board", {"template": "longboard"})
    s2 = call("open_board", {"path": bpath})
    check(abs(s1["volume_litres"] - s2["volume_litres"]) < 0.01, "open_board round trip volume")
    check(s2["name"] == "QA board", "open_board round trip name")
    stp = os.path.join(out, "export-model.step")
    s3 = call("open_board", {"path": stp}, expect_ok=None, label="open STEP export")
    if isinstance(s3, dict):
        check(abs(s3["volume_litres"] - s1["volume_litres"]) < 0.5, f"STEP re-import volume {s3['volume_litres']} vs {s1['volume_litres']}")
        r = call("save_board", {}, expect_ok=False, label="save imported STEP without path")
    call("open_board", {"path": bpath})

    # Undo / redo.
    call("set_info", {"name": "changed"})
    r = call("undo", {})
    check(r.get("undone") is True, "undo")
    check(call("board_summary", {})["name"] == "QA board", "undo restored name")
    r = call("redo", {})
    check(call("board_summary", {})["name"] == "changed", "redo")
    for _ in range(250):
        r = m.call("undo", {})[1]
        if not r.get("undone"):
            break
    r = call("undo", {})
    check(r.get("undone") is False, "undo at the start reports false")

    # App-only tools headless.
    for name, args in (("screenshot", {}), ("set_view", {"view": "3d"}), ("select_section", {"section_index": 1})):
        r = call(name, args, expect_ok=False, label="headless")
        if isinstance(r, tuple):
            check("launch_app" in str(r[1]), f"{name} headless error mentions launch_app: {r[1]}")
    r = call("launch_app", {}, expect_ok=False, label="no app binary")

    # --- invalid arguments: must be errors (or at least not crash)
    invalid = [
        ("get_curve", {}),
        ("get_curve", {"curve": "banana"}),
        ("get_curve", {"curve": "section", "section_index": 0}),
        ("get_curve", {"curve": "section", "section_index": -1}),
        ("get_curve", {"curve": "section", "section_index": 999}),
        ("set_knot", {"curve": "outline", "index": 1, "x": "abc", "y": 1}),
        ("set_knot", {"curve": "outline", "index": 1.5, "x": 1, "y": 1}),
        ("set_knot", {"curve": "outline", "index": 1, "x": 1}),
        ("set_knot", {"curve": "outline", "index": 1, "handle": "middle", "x": 1, "y": 1}),
        ("set_knot", {"curve": "outline", "index": 1, "x": True, "y": 1}),
        ("move_knot", {"curve": "deck", "index": 99}),
        ("set_smooth", {"curve": "deck", "index": 1}),
        ("set_smooth", {"curve": "deck", "index": 1, "smooth": "yes"}),
        ("add_knot", {"curve": "outline", "x": 1}),
        ("delete_knot", {"curve": "outline", "index": 0}),
        ("delete_knot", {"curve": "outline", "index": 99}),
        ("section_info", {"section_index": 0}),
        ("section_info", {"section_index": "1"}),
        ("add_section", {"position": -5}),
        ("add_section", {"position": 1e9}),
        ("add_section", {}),
        ("move_section", {"section_index": 0, "position": 50}),
        ("move_section", {"section_index": 1}),
        ("remove_section", {"section_index": 0}),
        ("remove_section", {"section_index": 999}),
        ("scale_board", {"length": 5}),
        ("scale_board", {"length": "tall"}),
        ("scale_board", {"volume_litres": 0}),
        ("scale_board", {"volume_litres": 1e9}),
        ("new_board", {}),
        ("new_board", {"template": "fish"}),
        ("new_board", {"template": "shortboard", "width": 0.1}),
        ("open_board", {}),
        ("open_board", {"path": os.path.join(out, "does-not-exist.brd")}),
        ("open_board", {"path": out}),
        ("open_board", {"path": os.path.join(out, "export-outline.pdf")}),
        ("open_board", {"path": os.path.join(out, "tp-deck.nc")}),
        ("export", {"what": "outline", "format": "pdf"}),
        ("export", {"what": "outline", "format": "stl", "path": os.path.join(out, "x.stl")}),
        ("export", {"what": "model", "format": "pdf", "path": os.path.join(out, "x.pdf")}),
        ("export", {"what": "nonsense", "format": "pdf", "path": os.path.join(out, "x.pdf")}),
        ("export", {"what": "outline", "format": "pdf", "path": os.path.join(out, "no-such-dir", "x.pdf")}),
        ("generate_toolpath", {}),
        ("generate_toolpath", {"job": "laser"}),
        ("generate_toolpath", {"job": "deck", "holding": "vacuum"}),
        ("generate_toolpath", {"job": "deck", "deck_cuts": -1}),
        ("generate_toolpath", {"job": "deck", "lengthwise_resolution": "fine"}),
        ("generate_toolpath", {"job": "deck", "path": os.path.join(out, "no-such-dir", "x.nc")}),
        ("set_surface_model", {"model": "nurbs"}),
        ("set_surface_model", {}),
        ("set_units", {"units": "furlongs"}),
        ("set_units", {}),
        ("sample_board", {"stations": ["six feet"]}),
        ("sample_board", {"stations": [{}]}),
    ]
    for name, args in invalid:
        call(name, args, expect_ok=False, label="invalid")
    check(m.alive(), "server alive after invalid arguments")

    # Wrong types for optional properties: silently ignored?
    ignored = []
    probes = [
        ("generate_toolpath", {"job": "deck", "cutting_speed_mm_s": "fast", "lengthwise_resolution": 5}),
        ("set_fins", {"splay_degrees": "wide"}),
        ("scale_board", {"volume_litres": "30"}),
        ("new_board", {"template": "shortboard", "volume_litres": "30"}),
        ("estimate_weight", {"foam_density": "heavy"}),
        ("set_info", {"name": 42}),
        ("sample_board", {"stations": "everywhere"}),
        ("scale_board", {"scale_fins": "no"}),
    ]
    for name, args in probes:
        ok, body, _ = m.call(name, args)
        called.add(name)
        if ok:
            ignored.append(f"{name} {json.dumps(args)}")
    if ignored:
        note("wrong-typed optional arguments silently ignored (no error): " + "; ".join(ignored))

    # Out-of-range values that are accepted.
    accepted = []
    for name, args in (
        ("set_fins", {"side_rear_x": -500, "centre_depth": -3}),
        ("estimate_weight", {"foam_density": -30, "resin_ratio": -1}),
        ("generate_toolpath", {"job": "deck", "cutter_diameter": 0.4, "lengthwise_resolution": 10}),
        ("sample_board", {"stations": [-1000, 1e9]}),
    ):
        ok, body, _ = m.call(name, args)
        if ok:
            accepted.append(f"{name} {json.dumps(args)} -> {json.dumps(body)[:160]}")
    for x in accepted:
        note("accepted out-of-range input: " + x)

    # Degenerate geometry edits.
    s0 = m.call("board_summary", {})[1]
    ok, r, _ = m.call("set_knot", {"curve": "outline", "index": 1, "x": 1e6, "y": 30})
    try:
        s = m.call("board_summary", {}, timeout=15)[1]
        note(f"outline knot at x = 1e6 cm accepted; summary volume {s.get('volume_litres')!r}")
        m.call("undo", {})
    except McpError:
        fail("set_knot outline[1] to x = 1e6 cm is accepted, then board_summary does not answer within 15 s (server stuck); restarting the server")
        m.proc.kill()
        m.proc.wait()
        m = Mcp(os.path.abspath(a.mcp), env)
        m.initialize()
        s0 = m.call("board_summary", {})[1]
    check(m.alive(), "alive after the far knot")
    sec = s0["sections"]
    if len(sec) >= 2:
        a_pos = sec[0]["position"]["cm"]
        ok, r, _ = m.call("move_section", {"section_index": sec[1]["index"], "position": a_pos})
        s = m.call("board_summary", {})[1]
        pos = [x["position"]["cm"] for x in s["sections"]]
        if ok and len(set(pos)) < len(pos):
            note(f"move_section onto another section's position is accepted: duplicate stations {pos}; volume {s['volume_litres']}")
            ok3, r3, _ = m.call("generate_toolpath", {"job": "deck", "lengthwise_resolution": 5})
            note(f"  toolpath after that: {'ok' if ok3 else 'error ' + str(r3)[:120]}")
        m.call("undo", {})
    check(m.alive(), "alive after section edits")

    not_called = [n for n in names if n not in called]
    check(not not_called, f"tools never called: {not_called}")

    # Hang probe last: coarse resolution on a short board never terminates (see qa_cam.rs).
    if not a.skip_hang_probe:
        m.call("new_board", {"template": "shortboard", "length": 170})
        rid = m.next_id
        m.next_id += 1
        m.send_raw(json.dumps({"jsonrpc": "2.0", "id": rid, "method": "tools/call", "params": {"name": "generate_toolpath", "arguments": {"job": "deck", "lengthwise_resolution": 20}}}))
        r = m.read_line(2.0)
        if r is None:
            fail("generate_toolpath deck at lengthwise_resolution 20 on a 170 cm board: no reply in 2 s (server stuck, memory growing); killed the server")
            m.proc.kill()
        else:
            print("  hang probe answered:", str(r)[:200])
    m.close()
    slow = sorted(m.log, key=lambda e: -e["seconds"])[:5]
    print("slowest calls:", ", ".join(f"{e['tool']} {e['seconds']}s" for e in slow))
    summary = {"fails": FAILS, "notes": NOTES, "tools": names, "calls": len(m.log)}
    with open(os.path.join(out, "mcp_headless_report.json"), "w") as f:
        json.dump(summary, f, indent=2)
    print(f"\n{len(FAILS)} failures, {len(NOTES)} notes, {len(m.log)} calls")
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
