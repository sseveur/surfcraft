//! Autosave in the main window: the File menu setting, the per-frame tick and the recovery window shown at startup
//! when a previous run left unsaved work behind.

use std::time::Instant;

use eframe::egui;

use crate::app::{Pending, SurfCraftApp};
use crate::autosave::{self, Autosave, Entry};
use crate::i18n::{tr, trf};

/// Autosaves offered for recovery.
#[derive(Default)]
pub struct Recovery {
    pub open: bool,
    pub entries: Vec<Entry>,
}

enum Action {
    Recover(usize),
    SaveAs(usize),
    Discard(usize),
    Later,
}

/// "5 min ago", "3 h ago", "2 days ago".
fn age(saved_at: i64) -> String {
    let s = (crate::logging::now().0 - saved_at).max(0);
    if s < 3600 {
        trf("{0} min ago", &[&(s / 60)])
    } else if s < 48 * 3600 {
        trf("{0} h ago", &[&(s / 3600)])
    } else {
        trf("{0} days ago", &[&(s / 86_400)])
    }
}

impl SurfCraftApp {
    /// Start autosaving into `<data dir>/autosave`; with `recover`, first look for work left by a crash.
    pub(crate) fn init_autosave(&mut self, recover: bool) {
        let Some(base) = self.data_dir.clone() else {
            return;
        };
        let dir = autosave::autosave_dir(&base);
        if recover {
            let entries = autosave::scan(&dir);
            if !entries.is_empty() {
                log::info!(
                    "{} autosaved boards found from an earlier run",
                    entries.len()
                );
                self.recovery = Recovery {
                    open: true,
                    entries,
                };
            }
        }
        self.autosave = Some(Autosave::start(dir, Instant::now()));
    }

    /// File menu: the autosave interval.
    pub(crate) fn autosave_menu(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Autosave every"));
            ui.add(
                egui::DragValue::new(&mut self.set.autosave_minutes)
                    .range(0..=60)
                    .suffix(" min"),
            );
        })
        .response
        .on_hover_text(tr("While the board has unsaved changes, a copy is kept in SurfCraft's own folder (never in your file) and offered back if SurfCraft closes unexpectedly. 0 turns it off."));
    }

    /// The open document is being closed or replaced: its autosave goes.
    pub(crate) fn autosave_document_closed(&mut self) {
        if let Some(a) = self.autosave.as_mut() {
            a.new_document(Instant::now());
        }
    }

    /// Once per frame: autosave when due, and the recovery window.
    pub(crate) fn autosave_frame(&mut self, ctx: &egui::Context) {
        if let Some(a) = self.autosave.as_mut()
            && let Some(wait) = a.tick(&self.doc, self.set.autosave_minutes, Instant::now())
        {
            ctx.request_repaint_after(wait);
        }
        if self.recovery.open {
            self.recovery_window(ctx);
        }
    }

    /// Normal exit.
    pub(crate) fn autosave_shutdown(&mut self) {
        if let Some(a) = self.autosave.as_mut() {
            a.shutdown();
        }
    }

    /// Open a recovered board in place of the current one: unsaved, with its original path so Save goes there.
    pub(crate) fn recover(&mut self, entry: Entry) {
        let Some(board) = entry.board.clone() else {
            return;
        };
        let original = entry.meta.original();
        log::info!(
            "recovered {} (autosave {}, original {})",
            entry.meta.name,
            entry.id,
            original
                .as_deref()
                .map(crate::logging::path)
                .unwrap_or_else(|| "untitled".into())
        );
        self.replace_document(board, original);
        self.doc.mark_unsaved();
        if let Some(a) = self.autosave.as_mut() {
            a.adopt(&entry.id, &self.doc, Instant::now());
        }
        self.notify(trf("Recovered {0}", &[&entry.meta.name]));
    }

    fn recovery_dir(&self) -> Option<std::path::PathBuf> {
        self.autosave
            .as_ref()
            .map(|a| a.dir.clone())
            .or_else(|| self.data_dir.as_deref().map(autosave::autosave_dir))
    }

    fn recovery_window(&mut self, ctx: &egui::Context) {
        let mut action = None;
        let set = &self.set;
        let entries = &self.recovery.entries;
        egui::Window::new(tr("Recover unsaved boards"))
            .id(egui::Id::unique("recovery-window"))
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(tr("SurfCraft did not close normally last time. These boards had unsaved changes:"));
                ui.separator();
                for (i, e) in entries.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        let name = if e.meta.name.is_empty() {
                            tr("Untitled").to_owned()
                        } else {
                            e.meta.name.clone()
                        };
                        ui.strong(name);
                        let file = e
                            .meta
                            .original()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| tr("Never saved").to_owned());
                        ui.label(trf("File: {0}", &[&file]));
                        let mut when = trf("Autosaved {0}", &[&age(e.meta.saved_at)]);
                        if let Some(b) = &e.board {
                            when += &format!(" · {} {}", tr("Length"), set.fmt(b.length()));
                        }
                        ui.weak(when);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(e.board.is_some(), egui::Button::new(tr("Recover")))
                                .on_hover_text(tr("Open it as an unsaved board; Save writes to its file (keeping a .bak)"))
                                .clicked()
                            {
                                action = Some(Action::Recover(i));
                            }
                            if ui
                                .add_enabled(e.board.is_some(), egui::Button::new(tr("Save as…")))
                                .clicked()
                            {
                                action = Some(Action::SaveAs(i));
                            }
                            if ui
                                .button(tr("Discard"))
                                .on_hover_text(tr("Delete this autosave"))
                                .clicked()
                            {
                                action = Some(Action::Discard(i));
                            }
                        });
                        ui.separator();
                    });
                }
                if ui
                    .button(tr("Decide later"))
                    .on_hover_text(tr("Keep the autosaves; they are offered again next time SurfCraft starts"))
                    .clicked()
                {
                    action = Some(Action::Later);
                }
            });
        let dir = self.recovery_dir();
        match action {
            Some(Action::Recover(i)) => {
                let entry = self.recovery.entries.remove(i);
                self.guarded(Pending::Recover(Box::new(entry)));
            }
            Some(Action::SaveAs(i)) => {
                let e = self.recovery.entries[i].clone();
                let (Some(board), Some(dir)) = (e.board.clone(), dir) else {
                    return;
                };
                let name = format!(
                    "{}.brd",
                    if e.meta.name.is_empty() {
                        "board"
                    } else {
                        &e.meta.name
                    }
                );
                if let Some(p) = self.save_file(tr("BoardCAD board"), "brd", Some(&name)) {
                    let text = surfcraft_io::brd::write(&board);
                    match crate::document::write_with_backup(&p, text.as_bytes()) {
                        Ok(()) => {
                            log::info!(
                                "recovered autosave {} saved as {}",
                                e.id,
                                crate::logging::path(&p)
                            );
                            autosave::remove_entry(&dir, &e.id);
                            self.recovery.entries.remove(i);
                            self.notify(trf("Saved {0}", &[&p.display()]));
                        }
                        Err(err) => {
                            log::error!("could not save {}: {err}", crate::logging::path(&p));
                            self.notify(trf("Could not save {0}: {1}", &[&p.display(), &err]));
                        }
                    }
                }
            }
            Some(Action::Discard(i)) => {
                let e = self.recovery.entries.remove(i);
                if let Some(dir) = dir {
                    autosave::remove_entry(&dir, &e.id);
                }
                log::info!("autosave {} ({}) discarded", e.id, e.meta.name);
            }
            Some(Action::Later) => {
                log::info!("recovery postponed");
                self.recovery.open = false;
            }
            None => {}
        }
        if self.recovery.entries.is_empty() {
            self.recovery.open = false;
        }
    }
}
