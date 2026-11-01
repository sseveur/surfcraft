//! Talk to the real `surfcraft-mcp` binary over stdio, headless (no SurfCraft window).

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Mcp {
    fn start() -> Self {
        let dir = std::env::temp_dir().join(format!("surfcraft-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_surfcraft-mcp"))
            // No window: point discovery at a file that doesn't exist.
            .env("SURFCRAFT_AUTOMATION_FILE", dir.join("none.json"))
            .env("SURFCRAFT_APP", dir.join("no-such-app"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], id);
        v
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, Value) {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        let res = &r["result"];
        let text = res["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (res["isError"] == false, body)
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn headless_session_over_stdio() {
    let mut m = Mcp::start();
    let init = m.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "surfcraft");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    m.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let tools = m.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "board_summary",
        "set_knot",
        "export",
        "launch_app",
        "screenshot",
        "estimate_weight",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for t in tools["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }

    let (ok, status) = m.tool("connection_status", json!({}));
    assert!(ok);
    assert_eq!(status["mode"], "headless");

    let (ok, s) = m.tool("board_summary", json!({}));
    assert!(ok);
    // The default board is the built-in shortboard; its volume as computed by the reference program.
    let golden: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/shortboard.golden.json"
    ))
    .unwrap();
    let litres = golden["slinear"]["volume"].as_f64().unwrap() / 1000.0;
    assert!((s["volume_litres"].as_f64().unwrap() - litres).abs() < 0.01);

    let (ok, s) = m.tool(
        "new_board",
        json!({"template": "funboard", "length": "7'2\"", "volume_litres": 45}),
    );
    assert!(ok, "{s}");
    assert!((s["volume_litres"].as_f64().unwrap() - 45.0).abs() < 0.1);

    let out = std::env::temp_dir()
        .join(format!("surfcraft-mcp-test-{}", std::process::id()))
        .join("board.stl");
    let (ok, r) = m.tool(
        "export",
        json!({"what": "model", "format": "stl", "path": out}),
    );
    assert!(ok, "{r}");
    assert!(std::fs::metadata(&out).unwrap().len() > 10_000);

    let (ok, err) = m.tool("screenshot", json!({}));
    assert!(!ok);
    assert!(err.as_str().unwrap().contains("launch_app"));

    let (ok, err) = m.tool(
        "set_knot",
        json!({"curve": "outline", "index": 999, "x": 1, "y": 1}),
    );
    assert!(!ok, "{err}");

    let r = m.request(
        "tools/call",
        json!({"name": "no_such_tool", "arguments": {}}),
    );
    assert_eq!(r["error"]["code"], -32602);
    let r = m.request("bogus/method", json!({}));
    assert_eq!(r["error"]["code"], -32601);
    assert!(m.request("ping", json!({}))["result"].is_object());
}
