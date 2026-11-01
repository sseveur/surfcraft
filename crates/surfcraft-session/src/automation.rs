//! Local automation bridge between a running SurfCraft window and an external client (the MCP server).
//!
//! Security model:
//! - Disabled unless the user enables it (setting or `--automation` flag).
//! - Listens on 127.0.0.1 only, on an OS-chosen port.
//! - Every request must carry a random token. Port and token are written to `automation.json` in the user's
//!   config directory with owner-only permissions; anyone who can read that file is already the user.
//!
//! Wire format: one JSON object per line. Request `{"token", "cmd", "args"}`, reply `{"ok": true, "result"}` or
//! `{"ok": false, "error"}`.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// `<config dir>/automation.json`, e.g. `~/Library/Application Support/org.surfcraft.SurfCraft/automation.json`.
/// `SURFCRAFT_AUTOMATION_FILE` overrides the location (tests, multiple instances).
pub fn endpoint_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SURFCRAFT_AUTOMATION_FILE") {
        return Some(PathBuf::from(p));
    }
    directories::ProjectDirs::from("org", "surfcraft", "SurfCraft")
        .map(|d| d.config_dir().join("automation.json"))
}

fn random_token() -> String {
    // RandomState keys come from the OS random generator.
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i ^ std::process::id() as u64);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn write_private(path: &PathBuf, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
    }
    std::fs::rename(&tmp, path)
}

/// Read the endpoint file of a running app, if any.
pub fn discover() -> Option<Endpoint> {
    let text = std::fs::read_to_string(endpoint_path()?).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(Endpoint {
        port: v["port"].as_u64()? as u16,
        token: v["token"].as_str()?.to_owned(),
        pid: v["pid"].as_u64().unwrap_or(0) as u32,
    })
}

/// A request waiting for the app to answer it.
pub struct Pending {
    pub cmd: String,
    pub args: Value,
    reply: Sender<Result<Value, String>>,
}

impl Pending {
    pub fn respond(self, result: Result<Value, String>) {
        let _ = self.reply.send(result);
    }
}

/// Listener owned by the app. Requests arrive on [`Server::requests`]; the app answers them on its UI thread.
pub struct Server {
    pub endpoint: Endpoint,
    pub requests: Receiver<Pending>,
}

impl Server {
    /// Start listening and publish the endpoint file. `wake` is called when a request arrives (repaint the UI).
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> std::io::Result<Server> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: random_token(),
            pid: std::process::id(),
        };
        if let Some(path) = endpoint_path() {
            write_private(
                &path,
                &json!({"port": endpoint.port, "token": endpoint.token, "pid": endpoint.pid})
                    .to_string(),
            )?;
        }
        let (tx, rx) = mpsc::channel::<Pending>();
        let token = endpoint.token.clone();
        let wake = std::sync::Arc::new(wake);
        std::thread::Builder::new()
            .name("automation".into())
            .spawn(move || {
                let open = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
                for stream in listener.incoming().flatten() {
                    use std::sync::atomic::Ordering;
                    if open.load(Ordering::Acquire) >= MAX_CONNECTIONS {
                        continue; // dropped: closes the socket
                    }
                    open.fetch_add(1, Ordering::AcqRel);
                    let tx = tx.clone();
                    let token = token.clone();
                    let wake = wake.clone();
                    let open = open.clone();
                    std::thread::spawn(move || {
                        serve(stream, &token, &tx, &*wake);
                        open.fetch_sub(1, Ordering::AcqRel);
                    });
                }
            })?;
        Ok(Server {
            endpoint,
            requests: rx,
        })
    }

    /// Remove the endpoint file (on shutdown or when automation is switched off).
    pub fn withdraw(&self) {
        if let Some(path) = endpoint_path()
            && discover().is_some_and(|e| e.pid == self.endpoint.pid)
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.withdraw();
    }
}

/// Limits for unauthenticated local clients.
const MAX_CONNECTIONS: usize = 8;
const MAX_LINE: u64 = 1 << 20;
const MAX_BAD_TOKENS: usize = 3;

fn serve(stream: TcpStream, token: &str, tx: &Sender<Pending>, wake: &(dyn Fn() + Send + Sync)) {
    use std::io::Read;
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let mut bad_tokens = 0;
    loop {
        // Read one line, at most MAX_LINE bytes (a client cannot make us buffer unbounded input).
        let mut buf = Vec::new();
        match (&mut reader).take(MAX_LINE).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') && buf.len() as u64 >= MAX_LINE {
            let _ = writeln!(
                writer,
                "{}",
                json!({"ok": false, "error": "request too large"})
            );
            return;
        }
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => Err(format!("bad request: {e}")),
            Ok(req) if req["token"].as_str() != Some(token) => {
                bad_tokens += 1;
                if bad_tokens >= MAX_BAD_TOKENS {
                    let _ = writeln!(writer, "{}", json!({"ok": false, "error": "invalid token"}));
                    return;
                }
                Err("invalid token".into())
            }
            Ok(req) => {
                let (rtx, rrx) = mpsc::channel();
                let pending = Pending {
                    cmd: req["cmd"].as_str().unwrap_or_default().to_owned(),
                    args: req.get("args").cloned().unwrap_or(Value::Null),
                    reply: rtx,
                };
                if tx.send(pending).is_err() {
                    return;
                }
                wake();
                rrx.recv_timeout(Duration::from_secs(120))
                    .unwrap_or_else(|_| Err("SurfCraft did not answer in time".into()))
            }
        };
        let msg = match reply {
            Ok(v) => json!({"ok": true, "result": v}),
            Err(e) => json!({"ok": false, "error": e}),
        };
        if writeln!(writer, "{msg}").is_err() {
            return;
        }
    }
}

/// Client connection to a running app.
pub struct Client {
    token: String,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Client {
    pub fn connect(ep: &Endpoint) -> std::io::Result<Client> {
        let stream = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], ep.port)),
            Duration::from_secs(2),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(130)))?;
        Ok(Client {
            token: ep.token.clone(),
            writer: stream.try_clone()?,
            reader: BufReader::new(stream),
        })
    }

    /// How long to wait for a reply.
    pub fn set_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
        self.reader.get_ref().set_read_timeout(Some(timeout))
    }

    pub fn call(&mut self, cmd: &str, args: &Value) -> Result<Value, String> {
        let req = json!({"token": self.token, "cmd": cmd, "args": args});
        writeln!(self.writer, "{req}").map_err(|e| format!("connection lost: {e}"))?;
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("connection lost: {e}"))?;
        if line.is_empty() {
            return Err("connection closed".into());
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))?;
        if v["ok"].as_bool() == Some(true) {
            Ok(v["result"].clone())
        } else {
            Err(v["error"].as_str().unwrap_or("unknown error").to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_token_check() {
        let file = std::env::temp_dir().join(format!(
            "surfcraft-automation-test-{}.json",
            std::process::id()
        ));
        // SAFETY: only this test touches this variable.
        unsafe { std::env::set_var("SURFCRAFT_AUTOMATION_FILE", &file) };
        let server = Server::start(|| {}).unwrap();
        let ep = server.endpoint.clone();
        assert_eq!(discover(), Some(ep.clone()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // App side: answer requests on another thread.
        std::thread::spawn(move || {
            for p in server.requests.iter() {
                let r = if p.cmd == "echo" {
                    Ok(p.args.clone())
                } else {
                    Err("nope".into())
                };
                p.respond(r);
            }
        });
        let mut c = Client::connect(&ep).unwrap();
        assert_eq!(c.call("echo", &json!({"a": 1})).unwrap(), json!({"a": 1}));
        assert_eq!(c.call("other", &json!({})).unwrap_err(), "nope");
        let mut bad = Client::connect(&Endpoint {
            token: "wrong".into(),
            ..ep.clone()
        })
        .unwrap();
        assert_eq!(bad.call("echo", &json!({})).unwrap_err(), "invalid token");
        assert_ne!(random_token(), random_token());
        assert_eq!(random_token().len(), 32);
    }
}
