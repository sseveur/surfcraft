//! SurfCraft MCP server (stdio).
//!
//! Tools run in the SurfCraft window when one is running with "Allow AI assistant control" (or launched by the
//! `launch_app` tool), so the user sees every edit and can undo it. Without a window, the same tools work headlessly
//! on an in-memory board that can be opened from and saved to .brd files.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use surfcraft_session::automation::{Client, discover};
use surfcraft_session::commands::{self, Context};
use surfcraft_session::document::Document;

const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "SurfCraft designs surfboards. The board is an outline (half width vs distance from tail), \
a rocker (bottom curve) and deck curve (heights vs distance from tail), and cross-sections at stations along the \
board, all cubic Bezier curves. Lengths are centimetres unless you pass a string with units (\"6'2\\\"\", \"18 1/2\\\"\", \
\"47cm\"). Start with board_summary or get_curve to read the shape, edit with set_knot/move_knot/scale_board, and check \
volume_litres after changes. Call launch_app to open the SurfCraft window so the user can watch, then screenshot to see it. \
Every edit is undoable (undo).";

/// Where tools are executed.
enum Backend {
    App(Client),
    Headless,
}

struct Server {
    backend: Backend,
    doc: Document,
    ctx: Context,
}

fn app_tools() -> Vec<Value> {
    vec![
        json!({"name": "connection_status", "description": "Whether tools run in a live SurfCraft window or headless, plus window state.", "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "launch_app", "description": "Open the SurfCraft window (with AI control enabled for this run) so the user can watch the design. Optionally opens a board file.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "Board file to open"}}}}),
        json!({"name": "screenshot", "description": "Image of the SurfCraft window. Requires the window (launch_app).", "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "set_view", "description": "Show a view in the window: overview (all views together), outline, rocker, section or 3d.", "inputSchema": {"type": "object", "properties": {"view": {"type": "string", "enum": ["overview", "outline", "rocker", "section", "3d"]}, "fit": {"type": "boolean"}}, "required": ["view"]}}),
        json!({"name": "set_units", "description": "Units used in the window and in formatted results.", "inputSchema": {"type": "object", "properties": {"units": {"type": "string", "enum": ["ft_in", "ft_in_decimal", "mm", "cm", "m"]}}, "required": ["units"]}}),
        json!({"name": "select_section", "description": "Make a cross-section current (shown in the window's cross-section view).", "inputSchema": {"type": "object", "properties": {"section_index": {"type": "integer"}}, "required": ["section_index"]}}),
    ]
}

fn tool_list() -> Vec<Value> {
    let mut tools: Vec<Value> = commands::tool_specs()
        .into_iter()
        .map(|t| {
            let mut schema = t.input_schema;
            if matches!(t.name, "new_board" | "open_board")
                && let Some(p) = schema["properties"].as_object_mut()
            {
                p.insert(
                    "discard_unsaved".into(),
                    json!({"type": "boolean", "description": "In the window: replace the board even if it has unsaved changes"}),
                );
            }
            json!({"name": t.name, "description": t.description, "inputSchema": schema})
        })
        .collect();
    tools.extend(app_tools());
    tools
}

/// Locate the SurfCraft app binary: next to this server, in the macOS app bundle, or on PATH.
fn app_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "surfcraft.exe"
    } else {
        "surfcraft"
    };
    if let Some(p) = std::env::var_os("SURFCRAFT_APP") {
        return Some(PathBuf::from(p));
    }
    if let Ok(me) = std::env::current_exe()
        && let Some(dir) = me.parent()
    {
        let sibling = dir.join(exe_name);
        if sibling.exists() {
            return Some(sibling);
        }
    }
    let bundle = PathBuf::from("/Applications/SurfCraft.app/Contents/MacOS/surfcraft");
    if cfg!(target_os = "macos") && bundle.exists() {
        return Some(bundle);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(exe_name))
            .find(|p| p.exists())
    })
}

impl Server {
    fn new() -> Self {
        Self {
            backend: Backend::Headless,
            doc: Document::new(surfcraft_io::templates::TEMPLATES[0].board(), None),
            ctx: Context::default(),
        }
    }

    /// Use the live window when one is reachable.
    fn connect(&mut self) {
        if matches!(self.backend, Backend::App(_)) {
            return;
        }
        if let Some(ep) = discover()
            && let Ok(mut c) = Client::connect(&ep)
        {
            // Handshake: a stale endpoint file may point at another program, or at nothing that answers.
            let _ = c.set_timeout(std::time::Duration::from_secs(3));
            if c.call("status", &json!({})).is_ok() {
                let _ = c.set_timeout(std::time::Duration::from_secs(130));
                self.backend = Backend::App(c);
            }
        }
    }

    fn call_app(&mut self, name: &str, args: &Value) -> Option<Result<Value, String>> {
        self.connect();
        let Backend::App(client) = &mut self.backend else {
            return None;
        };
        match client.call(name, args) {
            Err(e) if e.starts_with("connection") || e.starts_with("bad reply") => {
                // Window closed: fall back to headless (and retry once in case it restarted).
                self.backend = Backend::Headless;
                self.connect();
                match &mut self.backend {
                    Backend::App(c) => Some(c.call(name, args)),
                    Backend::Headless => None,
                }
            }
            r => Some(r),
        }
    }

    fn launch(&mut self, args: &Value) -> Result<Value, String> {
        // Make sure a remembered window is still alive before deciding it is "already running".
        if self
            .call_app("status", &json!({}))
            .is_none_or(|r| r.is_err())
        {
            self.backend = Backend::Headless;
        }
        self.connect();
        if let Backend::App(_) = self.backend {
            if let Some(p) = args["path"].as_str() {
                return self
                    .call_app("open_board", &json!({"path": p}))
                    .unwrap_or(Err("window went away".into()));
            }
            return Ok(json!({"already_running": true}));
        }
        let bin =
            app_binary().ok_or("SurfCraft app not found (set SURFCRAFT_APP to its executable)")?;
        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("--automation");
        // Hand over the headless board so work done so far continues in the window.
        let handoff =
            std::env::temp_dir().join(format!("surfcraft-mcp-{}.brd", std::process::id()));
        match args["path"].as_str() {
            Some(p) => {
                cmd.arg(p);
            }
            None => {
                std::fs::write(&handoff, surfcraft_io::brd::write(&self.doc.board))
                    .map_err(|e| e.to_string())?;
                // Open as an untitled board: Save must ask for a location, not write into the temp directory.
                cmd.arg("--untitled").arg(&handoff);
            }
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn()
            .map_err(|e| format!("could not start {}: {e}", bin.display()))?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(200));
            self.connect();
            if let Backend::App(_) = self.backend {
                return Ok(json!({"launched": bin.display().to_string()}));
            }
        }
        Err("SurfCraft started but did not open its automation link (is another copy running without AI control?)".into())
    }

    /// Run a tool; returns MCP `content` items.
    fn call(&mut self, name: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = |v: Value| {
            vec![
                json!({"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}),
            ]
        };
        match name {
            "connection_status" => {
                self.connect();
                let status = match self.call_app("status", &json!({})) {
                    Some(Ok(s)) => json!({"mode": "app", "window": s}),
                    _ => {
                        json!({"mode": "headless", "board": self.doc.title(), "hint": "call launch_app to open the SurfCraft window"})
                    }
                };
                Ok(text(status))
            }
            "launch_app" => self.launch(args).map(text),
            "screenshot" => match self.call_app("screenshot", args) {
                Some(Ok(v)) => Ok(vec![
                    json!({"type": "image", "data": v["png_base64"], "mimeType": "image/png"}),
                ]),
                Some(Err(e)) => Err(e),
                None => Err("no SurfCraft window: call launch_app first".into()),
            },
            "set_view" | "select_section" => match self.call_app(name, args) {
                Some(r) => r.map(text),
                None => Err("no SurfCraft window: call launch_app first".into()),
            },
            "set_units" => {
                let unit = match args["units"].as_str().unwrap_or("") {
                    "ft_in" => surfcraft_core::LengthUnit::FeetInchesFraction,
                    "ft_in_decimal" => surfcraft_core::LengthUnit::FeetInchesDecimal,
                    "mm" => surfcraft_core::LengthUnit::Millimetres,
                    "cm" => surfcraft_core::LengthUnit::Centimetres,
                    "m" => surfcraft_core::LengthUnit::Metres,
                    u => return Err(format!("unknown units {u:?}")),
                };
                self.ctx.unit = unit;
                match self.call_app(name, args) {
                    Some(r) => r.map(text),
                    None => Ok(text(json!({"units": unit.label()}))),
                }
            }
            _ => match self.call_app(name, args) {
                Some(r) => r.map(text),
                None => commands::execute(&mut self.doc, &mut self.ctx, name, args).map(text),
            },
        }
    }

    fn handle(&mut self, req: &Value) -> Option<Value> {
        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or_default();
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => Ok(json!({
                // Echo the client's version only if we speak it; otherwise offer ours.
                "protocolVersion": req["params"]["protocolVersion"]
                    .as_str()
                    .filter(|v| SUPPORTED_VERSIONS.contains(v))
                    .unwrap_or(PROTOCOL_VERSION),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "surfcraft", "title": "SurfCraft", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tool_list()})),
            "tools/call" => {
                let name = req["params"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let args = req["params"].get("arguments").cloned().unwrap_or(json!({}));
                if !tool_list().iter().any(|t| t["name"] == name) {
                    Err((-32602, format!("unknown tool {name:?}")))
                } else {
                    Ok(match self.call(&name, &args) {
                        Ok(content) => json!({"content": content, "isError": false}),
                        Err(e) => {
                            json!({"content": [{"type": "text", "text": e}], "isError": true})
                        }
                    })
                }
            }
            m if m.starts_with("notifications/") => return None,
            m => Err((-32601, format!("method not found: {m}"))),
        };
        let id = id?; // notifications get no reply
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, message)) => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
            }
        })
    }
}

fn main() {
    let mut server = Server::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            // MCP over stdio sends single messages; answer batches with one error instead of silence.
            Ok(Value::Array(_)) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "batch requests are not supported"}}),
            ),
            Ok(req) => server.handle(&req),
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
            ),
        };
        if let Some(r) = reply {
            if writeln!(stdout, "{r}").is_err() {
                break;
            }
            let _ = stdout.flush();
        }
    }
}
