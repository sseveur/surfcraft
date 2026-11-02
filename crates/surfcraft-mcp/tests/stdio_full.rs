//! QA: every MCP tool over stdio against the real binary, headless (no window).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

fn dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("qa-mcp");
    std::fs::create_dir_all(&d).unwrap();
    d
}

impl Mcp {
    fn start() -> Self {
        let d = dir();
        let mut child = Command::new(env!("CARGO_BIN_EXE_surfcraft-mcp"))
            .env("SURFCRAFT_AUTOMATION_FILE", d.join("none.json"))
            .env("SURFCRAFT_APP", d.join("no-such-app"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut m = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        m.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "qa", "version": "0"}}),
        );
        m.send_raw(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
        m
    }

    fn send_raw(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad reply {line:?}: {e}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
        let v = self.read();
        assert_eq!(v["id"], id, "{v}");
        assert_eq!(v["jsonrpc"], "2.0");
        v
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, Value) {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        let res = &r["result"];
        assert!(res.is_object(), "{name}: {r}");
        let text = res["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (res["isError"] == false, body)
    }

    #[track_caller]
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (ok, body) = self.tool(name, args.clone());
        assert!(ok, "{name} {args}: {body}");
        body
    }

    #[track_caller]
    fn fails(&mut self, name: &str, args: Value) -> String {
        let (ok, body) = self.tool(name, args.clone());
        assert!(!ok, "{name} {args} should fail: {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn every_tool_listed_with_schema() {
    let mut m = Mcp::start();
    let tools = m.request("tools/list", json!({}));
    let list = tools["result"]["tools"].as_array().unwrap();
    let commands = surfcraft_session::commands::tool_specs().len();
    assert_eq!(list.len(), commands + 6, "session commands + 6 app tools");
    let mut names: Vec<&str> = list.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n, "duplicate tool names");
    for t in list {
        assert!(!t["description"].as_str().unwrap().is_empty());
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }
}

#[test]
fn every_tool_runs_headless() {
    let mut m = Mcp::start();
    let d = dir();
    let s = m.ok("board_summary", json!({}));
    assert_eq!(s["length"]["text"], "6' 0\"");
    m.ok("sample_board", json!({"stations": ["1'", 60, "47cm"]}));
    for c in ["outline", "deck", "bottom", "section"] {
        let r = m.ok("get_curve", json!({"curve": c}));
        assert!(r["knots"].as_array().unwrap().len() >= 2, "{c}");
    }
    m.ok(
        "set_knot",
        json!({"curve": "outline", "index": 2, "x": 90, "y": "9 1/4\""}),
    );
    m.ok(
        "move_knot",
        json!({"curve": "deck", "index": 2, "dy": "1/8\""}),
    );
    m.ok(
        "set_smooth",
        json!({"curve": "bottom", "index": 1, "smooth": false}),
    );
    let s = m.ok("smooth_curve", json!({"curve": "outline", "indices": [1]}));
    assert_eq!(s["smoothed"], 1, "the tail corner is smoothed when picked");
    let k = m.ok("add_knot", json!({"curve": "outline", "x": 150, "y": 15}));
    m.ok(
        "delete_knot",
        json!({"curve": "outline", "index": k["index"]}),
    );
    m.ok("section_info", json!({}));
    let s = m.ok("add_section", json!({"position": "4'"}));
    let i = s["section_index"].as_u64().unwrap();
    let mv = m.ok(
        "move_section",
        json!({"section_index": i, "position": "4' 2\""}),
    );
    m.ok(
        "remove_section",
        json!({"section_index": mv["section_index"]}),
    );
    let s = m.ok(
        "scale_board",
        json!({"length": "6'4\"", "volume_litres": 30}),
    );
    assert!((s["volume_litres"].as_f64().unwrap() - 30.0).abs() < 0.1);
    m.ok("set_info", json!({"name": "QA", "designer": "Tester"}));
    m.ok(
        "set_fins",
        json!({"side_rear_x": "4 1/2\"", "splay_degrees": 4}),
    );
    let f = m.ok(
        "set_fins",
        json!({"preset": "5 fins", "box_system": "FCS II"}),
    );
    assert_eq!(f["setup"]["fins"].as_array().unwrap().len(), 5, "{f}");
    let w = m.ok("estimate_weight", json!({}));
    assert!(w["total_kg"].as_f64().unwrap() > 1.0);
    m.ok("set_surface_model", json!({"model": "control_point"}));
    m.ok("set_surface_model", json!({"model": "sblend"}));
    m.ok("flip_board", json!({}));
    m.ok("flip_board", json!({}));
    assert_eq!(m.ok("undo", json!({}))["undone"], true);
    assert_eq!(m.ok("redo", json!({}))["redone"], true);
    for (what, format) in [
        ("outline", "pdf"),
        ("spin", "svg"),
        ("profile", "dxf"),
        ("sections", "pdf"),
        ("outline_full", "svg"),
        ("spec_sheet", "pdf"),
        ("model", "stl"),
        ("model", "obj"),
        ("model", "step"),
    ] {
        let p = d.join(format!("mcp-{what}.{format}"));
        let r = m.ok(
            "export",
            json!({"what": what, "format": format, "path": p, "paper": "Letter"}),
        );
        assert!(r["bytes"].as_u64().unwrap() > 100);
        assert!(p.exists());
    }
    let brd = d.join("mcp.brd");
    m.ok("save_board", json!({"path": brd}));
    m.ok("save_board", json!({}));
    let s = m.ok("new_board", json!({"template": "longboard"}));
    assert_eq!(s["length"]["text"], "9' 2\"");
    let s = m.ok("open_board", json!({"path": brd}));
    assert_eq!(s["name"], "QA");
    let s = m.ok("open_board", json!({"path": d.join("mcp-model.step")}));
    assert!(s["length"]["cm"].as_f64().unwrap() > 190.0);
    // Units affect formatted results headlessly.
    m.ok("set_units", json!({"units": "cm"}));
    let s = m.ok("board_summary", json!({}));
    assert!(
        s["length"]["text"].as_str().unwrap().ends_with(" cm"),
        "{s}"
    );
    m.ok("set_units", json!({"units": "mm"}));
    let s = m.ok("board_summary", json!({}));
    assert!(s["length"]["text"].as_str().unwrap().ends_with(" mm"));
    m.fails("set_units", json!({"units": "furlongs"}));
    // App-only tools explain how to get a window.
    let st = m.ok("connection_status", json!({}));
    assert_eq!(st["mode"], "headless");
    for t in ["screenshot", "set_view", "select_section"] {
        let e = m.fails(t, json!({"view": "outline", "section_index": 2}));
        assert!(e.contains("launch_app"), "{t}: {e}");
    }
    let e = m.fails("launch_app", json!({}));
    assert!(
        e.contains("could not start") || e.contains("not found"),
        "{e}"
    );
    // Still headless and usable after the failed launch.
    m.ok("board_summary", json!({}));
}

#[test]
fn every_tool_rejects_garbage_arguments() {
    let mut m = Mcp::start();
    let tools = m.request("tools/list", json!({}));
    let names: Vec<String> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    for name in &names {
        if name == "launch_app" {
            continue;
        }
        for args in [
            json!({"curve": "rail", "index": -3, "path": 5, "what": 1}),
            json!(null),
            json!([1, 2]),
        ] {
            let r = m.request("tools/call", json!({"name": name, "arguments": args}));
            // Either a tool result (possibly isError) or a JSON-RPC error; never a dead server.
            assert!(
                r["result"].is_object() || r["error"].is_object(),
                "{name}: {r}"
            );
        }
    }
    // Still alive.
    assert!(m.request("ping", json!({}))["result"].is_object());
}

#[test]
fn protocol_edge_cases() {
    let mut m = Mcp::start();
    // Malformed JSON gets a parse error with id null.
    m.send_raw("{this is not json");
    let v = m.read();
    assert_eq!(v["error"]["code"], -32700, "{v}");
    assert!(v["id"].is_null());
    // Blank lines are ignored; notifications get no reply.
    m.send_raw("");
    m.send_raw(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {}}).to_string(),
    );
    // String ids are echoed.
    m.send_raw(&json!({"jsonrpc": "2.0", "id": "abc", "method": "ping"}).to_string());
    let v = m.read();
    assert_eq!(v["id"], "abc");
    // tools/call without arguments.
    let r = m.request("tools/call", json!({"name": "board_summary"}));
    assert_eq!(r["result"]["isError"], false);
    // Unknown tool, unknown method.
    assert_eq!(
        m.request("tools/call", json!({"name": "nope"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        m.request("resources/list", json!({}))["error"]["code"],
        -32601
    );
    // initialize echoes the client's version.
    let init = m.request("initialize", json!({"protocolVersion": "2024-11-05"}));
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
}

#[test]
fn headless_errors_leave_board_unchanged() {
    let mut m = Mcp::start();
    let before = m.ok("board_summary", json!({}));
    m.fails(
        "set_knot",
        json!({"curve": "outline", "index": 99, "x": 1, "y": 1}),
    );
    m.fails("scale_board", json!({"length": "banana"}));
    m.fails("open_board", json!({"path": "/no/such/file.brd"}));
    m.fails("save_board", json!({}));
    m.fails(
        "export",
        json!({"what": "model", "format": "pdf", "path": dir().join("x.pdf")}),
    );
    let after = m.ok("board_summary", json!({}));
    assert_eq!(before, after);
    assert_eq!(m.ok("undo", json!({}))["undone"], false);
}

#[test]
fn headless_new_board_guards_unsaved_changes() {
    let mut m = Mcp::start();
    m.ok("set_info", json!({"name": "unsaved work"}));
    m.fails("new_board", json!({"template": "funboard"}));
}
