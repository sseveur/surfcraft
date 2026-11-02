//! Lets an external client (the SurfCraft MCP server) drive this window: commands run on the UI thread as normal,
//! undoable edits, so the user sees every change live.

use std::sync::{Arc, Mutex};

use eframe::egui;
use serde_json::{Value, json};
use surfcraft_session::automation::{Pending, Server};
use surfcraft_session::commands::{self, Context};

use crate::app::{SurfCraftApp, Tab};
use crate::i18n::{tr, trf};
use crate::settings::Unit;

type Shot = Arc<Mutex<Option<Arc<egui::ColorImage>>>>;

pub struct Automation {
    pub server: Server,
    shots: Vec<(Pending, Shot)>,
}

// App-only commands (described for clients in the MCP server): status, screenshot, set_view, set_units,
// select_section. Everything else goes to `surfcraft_session::commands::execute`.

fn encode_png(img: &egui::ColorImage, max_width: u32) -> Result<String, String> {
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    let buf = image::RgbaImage::from_raw(w, h, rgba).ok_or("bad screenshot size")?;
    let buf = if w > max_width {
        let nh = (h as f64 * max_width as f64 / w as f64).round() as u32;
        image::imageops::resize(
            &buf,
            max_width,
            nh.max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        buf
    };
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(base64(&out.into_inner()))
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

impl SurfCraftApp {
    pub(crate) fn set_automation(&mut self, ctx: &egui::Context, on: bool) {
        if on == self.automation.is_some() {
            return;
        }
        if on {
            let repaint = ctx.clone();
            match Server::start(move || repaint.request_repaint()) {
                Ok(server) => {
                    log::info!(
                        "AI assistant control on, listening on 127.0.0.1:{}",
                        server.endpoint.port
                    );
                    self.notify(trf(
                        "AI assistant control on (127.0.0.1:{0})",
                        &[&server.endpoint.port],
                    ));
                    self.automation = Some(Automation {
                        server,
                        shots: Vec::new(),
                    });
                }
                Err(e) => {
                    log::error!("could not start automation: {e}");
                    self.notify(trf("Could not start automation: {0}", &[&e]));
                }
            }
        } else {
            self.automation = None; // dropping withdraws the endpoint file
            log::info!("AI assistant control off");
            self.notify(tr("AI assistant control off"));
        }
    }

    /// Answer queued automation requests. Called once per frame.
    pub(crate) fn handle_automation(&mut self, ctx: &egui::Context) {
        let Some(auto) = self.automation.as_mut() else {
            return;
        };
        // Finished screenshots.
        let mut i = 0;
        while i < auto.shots.len() {
            let ready = auto.shots[i].1.lock().unwrap().take();
            if let Some(img) = ready {
                let (p, _) = auto.shots.remove(i);
                p.respond(encode_png(&img, 1600).map(
                    |b64| json!({"png_base64": b64, "width": img.size[0], "height": img.size[1]}),
                ));
            } else {
                i += 1;
            }
        }
        let incoming: Vec<Pending> = auto.server.requests.try_iter().collect();
        for p in incoming {
            log::debug!("automation command {}", p.cmd);
            if p.cmd == "screenshot" {
                let slot = Shot::default();
                let s2 = slot.clone();
                let repaint = ctx.clone();
                let cb = egui::ScreenshotCallback::new(move |img| {
                    *s2.lock().unwrap() = Some(img);
                    repaint.request_repaint();
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
                ctx.request_repaint();
                if let Some(a) = self.automation.as_mut() {
                    a.shots.push((p, slot));
                }
                continue;
            }
            let result = self.run_command(&p.cmd, &p.args);
            if let Err(e) = &result {
                log::info!("automation command {} failed: {e}", p.cmd);
            }
            p.respond(result);
        }
    }

    fn run_command(&mut self, cmd: &str, args: &Value) -> Result<Value, String> {
        match cmd {
            "status" => Ok(json!({
                "title": self.doc.title(),
                "path": self.doc.path.as_ref().map(|p| p.display().to_string()),
                "unsaved_changes": self.doc.is_dirty(),
                "view": format!("{:?}", self.tab),
                "units": surfcraft_core::LengthUnit::from(self.set.unit).label(),
                "current_section": self.ed.current_section,
                "can_undo": self.doc.can_undo(),
                "can_redo": self.doc.can_redo(),
            })),
            "set_view" => {
                self.tab = match args["view"].as_str().unwrap_or("") {
                    "overview" => Tab::Overview,
                    "outline" => Tab::Outline,
                    "rocker" | "profile" => Tab::Profile,
                    "section" => Tab::Section,
                    "3d" => Tab::ThreeD,
                    v => return Err(format!("unknown view {v:?}")),
                };
                if args["fit"].as_bool().unwrap_or(true) {
                    self.ed.fit_all();
                }
                Ok(json!({"view": format!("{:?}", self.tab)}))
            }
            "set_units" => {
                self.set.unit = match args["units"].as_str().unwrap_or("") {
                    "ft_in" => Unit::FeetInchesFraction,
                    "ft_in_decimal" => Unit::FeetInchesDecimal,
                    "mm" => Unit::Millimetres,
                    "cm" => Unit::Centimetres,
                    "m" => Unit::Metres,
                    u => return Err(format!("unknown units {u:?}")),
                };
                Ok(json!({"units": surfcraft_core::LengthUnit::from(self.set.unit).label()}))
            }
            "select_section" => {
                let i = args["section_index"]
                    .as_u64()
                    .ok_or("missing section_index")? as usize;
                if !self.doc.board.real_sections().contains(&i) {
                    return Err(format!(
                        "section_index {i} is not a real section ({:?})",
                        self.doc.board.real_sections()
                    ));
                }
                self.ed.current_section = i;
                self.ed.selection = None;
                Ok(json!({"section_index": i}))
            }
            _ => {
                let mut c = Context {
                    unit: self.set.length_unit(),
                    fraction: self.set.fraction,
                    current_section: self.ed.current_section,
                };
                let replaced = matches!(cmd, "new_board" | "open_board");
                if replaced
                    && self.doc.is_dirty()
                    && !args["discard_unsaved"].as_bool().unwrap_or(false)
                {
                    return Err("the open board has unsaved changes: save it first or pass discard_unsaved: true".into());
                }
                let r = commands::execute(&mut self.doc, &mut c, cmd, args);
                self.ed.current_section = c.current_section;
                if replaced && r.is_ok() {
                    self.autosave_document_closed();
                    let cur = self.ed.current_section;
                    self.ed.reset_for(&self.doc.board);
                    self.ed.current_section = cur;
                    self.results = None;
                    if let Some(p) = self.doc.path.clone() {
                        self.set.add_recent(p);
                    }
                }
                if r.is_ok() && cmd != "board_summary" {
                    self.ed.clamp_section(&self.doc.board);
                }
                r
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
