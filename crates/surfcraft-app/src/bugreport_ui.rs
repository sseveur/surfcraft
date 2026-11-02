//! Help > Report a bug: a non-modal window that builds a report zip and opens a prefilled GitHub issue or e-mail.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::SurfCraftApp;
use crate::bugreport::{self, Report, Since};
use crate::i18n::{tr, trf};

type Shot = Arc<Mutex<Option<Arc<egui::ColorImage>>>>;

const PREVIEW_BYTES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendBy {
    GitHub,
    Email,
}

pub struct BugReport {
    pub open: bool,
    pub title: String,
    pub description: String,
    pub email: String,
    pub since: Since,
    pub include_system: bool,
    pub include_board: bool,
    pub include_screenshot: bool,
    /// Log text for the current choice of [`Self::since`], refreshed when it changes.
    pub(crate) logs: Option<(Since, String)>,
    /// Folder for the zip instead of Downloads (tests).
    pub(crate) out_dir: Option<PathBuf>,
    /// The last zip written.
    pub(crate) saved: Option<PathBuf>,
    error: Option<String>,
    /// Waiting for the window screenshot before writing the zip.
    sending: Option<(SendBy, Shot, Instant)>,
}

impl Default for BugReport {
    fn default() -> Self {
        Self {
            open: true,
            title: String::new(),
            description: String::new(),
            email: String::new(),
            since: Since::default(),
            include_system: true,
            include_board: false,
            include_screenshot: false,
            logs: None,
            out_dir: None,
            saved: None,
            error: None,
            sending: None,
        }
    }
}

fn kb(bytes: usize) -> String {
    if bytes < 10_240 {
        format!("{:.1}", bytes as f64 / 1024.0)
    } else {
        format!("{}", bytes / 1024)
    }
}

fn png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    let buf = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)?;
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

impl SurfCraftApp {
    pub(crate) fn open_bug_report(&mut self) {
        match self.bug_report.as_mut() {
            Some(b) => b.open = true,
            None => self.bug_report = Some(BugReport::default()),
        }
        log::info!("bug report window opened");
    }

    /// Read the log for the chosen period when the choice changed.
    fn refresh_bug_report_logs(&mut self) {
        let Some(b) = self.bug_report.as_mut() else {
            return;
        };
        if b.logs.as_ref().is_none_or(|(s, _)| *s != b.since) {
            let text = self
                .data_dir
                .as_ref()
                .map(|d| {
                    let start = b.since.start(crate::logging::now().0);
                    crate::logging::collect(&crate::logging::log_dir(d), start)
                })
                .unwrap_or_default();
            b.logs = Some((b.since, text));
        }
    }

    /// Log text for the chosen period.
    pub(crate) fn bug_report_logs(&mut self) -> String {
        self.refresh_bug_report_logs();
        self.bug_report
            .as_ref()
            .and_then(|b| b.logs.as_ref())
            .map(|l| l.1.clone())
            .unwrap_or_default()
    }

    /// Version, system, graphics, display, language and units, home folder as `~`.
    pub(crate) fn system_info(&self, ctx: &egui::Context) -> String {
        let lang = self.set.language.unwrap_or(crate::i18n::language());
        let file = self
            .doc
            .path
            .as_deref()
            .map(crate::logging::path)
            .unwrap_or_else(|| "untitled".into());
        format!(
            "{}\nGraphics: {}\nDisplay scale: {}\nLocale: {}\nLanguage: {}\nUnits: {}\nBoard file: {file}\nUnsaved changes: {}\nDetailed logging: {}\n",
            crate::logging::app_line(),
            crate::logging::gpu(),
            ctx.pixels_per_point(),
            bugreport::locale(),
            lang.native_name(),
            surfcraft_core::LengthUnit::from(self.set.unit).label(),
            if self.doc.is_dirty() { "yes" } else { "no" },
            if self.set.detailed_logging {
                "on"
            } else {
                "off"
            },
        )
    }

    fn build_report(&mut self, ctx: &egui::Context, shot: Option<&egui::ColorImage>) -> Report {
        let logs = self.bug_report_logs();
        let system_info = self.system_info(ctx);
        let board = surfcraft_io::brd::write(&self.doc.board);
        let b = self.bug_report.as_ref().expect("bug report open");
        Report {
            title: b.title.clone(),
            description: b.description.clone(),
            email: b.email.clone(),
            system_info: b.include_system.then_some(system_info),
            logs,
            board: b.include_board.then_some(board),
            screenshot_png: shot.filter(|_| b.include_screenshot).and_then(png),
        }
    }

    /// Write the zip, show it in the file manager and open the issue page or e-mail.
    fn send_bug_report(
        &mut self,
        ctx: &egui::Context,
        by: SendBy,
        shot: Option<&egui::ColorImage>,
    ) {
        // Fresh log text, up to now.
        if let Some(b) = self.bug_report.as_mut() {
            b.logs = None;
        }
        let report = self.build_report(ctx, shot);
        let now = crate::logging::now().0;
        let Some(b) = self.bug_report.as_mut() else {
            return;
        };
        let dir = b
            .out_dir
            .clone()
            .or_else(|| bugreport::output_dir(self.data_dir.as_deref()));
        let Some(dir) = dir else {
            b.error = Some(tr("No folder to save the report in").to_owned());
            return;
        };
        match bugreport::write(&dir, &report, now) {
            Ok(zip) => {
                log::info!("bug report saved to {}", crate::logging::path(&zip));
                let url = match by {
                    SendBy::GitHub => report.issue_url(&zip),
                    SendBy::Email => report.mailto_url(&zip),
                };
                let _ = crate::platform::reveal(&zip);
                b.error = crate::platform::open_url(&url).err().map(|e| {
                    trf(
                        "Could not open {0}: {1}",
                        &[&url.split('?').next().unwrap_or(""), &e],
                    )
                });
                b.saved = Some(zip);
            }
            Err(e) => {
                log::error!(
                    "could not save the bug report in {}: {e}",
                    crate::logging::path(&dir)
                );
                b.error = Some(trf("Could not save {0}: {1}", &[&dir.display(), &e]));
            }
        }
    }

    /// The Report a bug window, when open. Called once per frame.
    pub(crate) fn bug_report_window(&mut self, ctx: &egui::Context) {
        let Some(b) = self.bug_report.as_mut() else {
            return;
        };
        // A send waiting for its screenshot.
        if let Some((by, shot, at)) = b.sending.take() {
            let img = shot.lock().unwrap().take();
            if img.is_some() || at.elapsed() > Duration::from_secs(3) {
                self.send_bug_report(ctx, by, img.as_deref());
            } else {
                b.sending = Some((by, shot, at));
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        self.refresh_bug_report_logs();
        let Some(b) = self.bug_report.as_mut() else {
            return;
        };
        if !b.open {
            return;
        }
        let logs: &str = b.logs.as_ref().map_or("", |l| l.1.as_str());
        // Laying out megabytes of text each frame would be slow: the preview shows the end.
        let preview = match logs.len().checked_sub(PREVIEW_BYTES) {
            Some(skip) => {
                let start = (skip..logs.len())
                    .find(|&i| logs.is_char_boundary(i))
                    .unwrap_or(logs.len());
                &logs[start..]
            }
            None => logs,
        };
        let mut open = true;
        let mut send = None;
        let mut copy = false;
        egui::Window::new(tr("Report a bug"))
            .id(egui::Id::unique("bug-report-window"))
            .open(&mut open)
            .default_width(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(tr("SurfCraft sends nothing by itself: it saves a zip file with the report and opens a new GitHub issue or e-mail for you to check and send."));
                ui.add_space(4.0);
                egui::Grid::new("bug-fields").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Title"));
                    ui.add(egui::TextEdit::singleline(&mut b.title).desired_width(400.0));
                    ui.end_row();
                    ui.label(tr("Your e-mail (optional)"));
                    ui.add(egui::TextEdit::singleline(&mut b.email).desired_width(400.0))
                        .on_hover_text(tr("Only written in report.txt, so we can ask you questions. GitHub issues are public."));
                    ui.end_row();
                });
                ui.label(tr("What happened / steps to reproduce"));
                ui.add(
                    egui::TextEdit::multiline(&mut b.description)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Include logs from the last:"));
                    egui::ComboBox::from_id_salt("bug-since")
                        .selected_text(b.since.label())
                        .show_ui(ui, |ui| {
                            for s in Since::ALL {
                                ui.selectable_value(&mut b.since, s, s.label());
                            }
                        });
                    let lines = logs.lines().count();
                    ui.weak(if lines == 1 {
                        trf("1 line, {0} KB", &[&kb(logs.len())])
                    } else {
                        trf("{0} lines, {1} KB", &[&lines, &kb(logs.len())])
                    });
                });
                egui::CollapsingHeader::new(tr("Preview"))
                    .id_salt("bug-preview")
                    .show(ui, |ui| {
                        egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
                            if preview.len() < logs.len() {
                                ui.weak(tr("Showing the end of the log"));
                            }
                            let mut text = preview;
                            ui.add(
                                egui::TextEdit::multiline(&mut text)
                                    .code_editor()
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    });
                ui.checkbox(&mut b.include_system, tr("System information"))
                    .on_hover_text(tr("SurfCraft version, operating system, graphics, display scale, locale, language and units"));
                ui.checkbox(&mut b.include_board, tr("The current board"))
                    .on_hover_text(tr("A copy of the open board as board.brd, with unsaved changes. Your own file is not touched."));
                ui.checkbox(&mut b.include_screenshot, tr("A screenshot of the window"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Send by:"));
                    let busy = b.sending.is_some();
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr("GitHub issue")))
                        .on_hover_text(tr("Saves the zip, then opens a new issue on github.com in your browser"))
                        .clicked()
                    {
                        send = Some(SendBy::GitHub);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr("E-mail")))
                        .on_hover_text(tr("Saves the zip, then opens a new message in your mail program"))
                        .clicked()
                    {
                        send = Some(SendBy::Email);
                    }
                    if busy {
                        ui.spinner();
                    }
                    ui.separator();
                    if ui.button(tr("Copy report text")).clicked() {
                        copy = true;
                    }
                });
                if let Some(p) = &b.saved {
                    ui.label(trf(
                        "Report saved to {0}. Attach it to the issue or e-mail.",
                        &[&crate::logging::path(p)],
                    ));
                }
                if let Some(e) = &b.error {
                    ui.colored_label(egui::Color32::from_rgb(230, 80, 60), e);
                }
            });
        b.open = open;
        if copy {
            let text = self.build_report(ctx, None).text(crate::logging::now().0);
            ctx.copy_text(text);
            self.notify(tr("Report text copied"));
        }
        if let Some(by) = send {
            let b = self.bug_report.as_mut().expect("open");
            b.error = None;
            if b.include_screenshot {
                let slot = Shot::default();
                let s2 = slot.clone();
                let repaint = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(move |img| {
                        *s2.lock().unwrap() = Some(img);
                        repaint.request_repaint();
                    }),
                ));
                b.sending = Some((by, slot, Instant::now()));
                ctx.request_repaint();
            } else {
                self.send_bug_report(ctx, by, None);
            }
        }
    }
}
