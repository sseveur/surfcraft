//! Logging in the main window: Help menu items (detailed logging, log folder) and the per-frame debug trace.

use eframe::egui;

use crate::app::SurfCraftApp;
use crate::i18n::{tr, trf};

/// Last values reported by the debug trace.
#[derive(Default)]
pub(crate) struct Trace {
    last: Vec<String>,
}

impl SurfCraftApp {
    /// Help menu: bug report, detailed logging and the log folder.
    pub(crate) fn log_menu_items(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(tr("Report a bug…"))
            .on_hover_text(tr("Saves a zip with the recent log and opens a new GitHub issue or e-mail for you to send"))
            .clicked()
        {
            self.open_bug_report();
            ui.close();
        }
        ui.checkbox(&mut self.set.detailed_logging, tr("Detailed logging"))
            .on_hover_text(tr("Also records every edit, click and view change in the log. To help with a bug: turn it on, reproduce the problem, then use Report a bug."));
        if ui
            .button(tr("Open log folder"))
            .on_hover_text(tr(
                "The log files SurfCraft keeps for the last 14 days, one per day",
            ))
            .clicked()
        {
            self.open_log_folder();
            ui.close();
        }
    }

    fn open_log_folder(&mut self) {
        let Some(base) = self.data_dir.clone() else {
            self.notify(tr("This system has no folder for the log"));
            return;
        };
        let dir = crate::logging::log_dir(&base);
        let r = std::fs::create_dir_all(&dir).and_then(|()| crate::platform::open_folder(&dir));
        if let Err(e) = r {
            self.notify(trf("Could not open {0}: {1}", &[&dir.display(), &e]));
        }
    }

    /// Once per frame, after the UI is laid out: apply the setting, and with detailed logging record what the
    /// user clicked and what changed.
    pub(crate) fn log_frame(&mut self, ctx: &egui::Context) {
        crate::logging::set_detailed(self.set.detailed_logging);
        if !log::log_enabled!(log::Level::Debug) {
            return;
        }
        ctx.output(|o| {
            for e in &o.events {
                if let egui::output::OutputEvent::Clicked(info) = e {
                    // The label only: never the value of a text field.
                    log::debug!(
                        "click {:?} {:?}",
                        info.role,
                        info.label.as_deref().unwrap_or("")
                    );
                }
            }
        });
        let d = &self.dialogs;
        let open: Vec<&str> = [
            (d.new_board.is_some(), "new board"),
            (d.scale.is_some(), "scale"),
            (d.info, "board info"),
            (d.fins, "fins"),
            (d.about, "about"),
            (d.export, "print & export"),
            (d.weight.is_some(), "weight"),
            (d.guides, "guide points"),
            (d.cam, "machine"),
            (self.tour.open, "tour"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        let now = [
            format!("view {:?}", self.tab),
            format!("tool {:?}", self.ed.tool),
            format!("cross-section {}", self.ed.current_section),
            format!("profile target {:?}", self.ed.profile_target),
            format!("units {:?}", self.set.unit),
            format!("windows {open:?}"),
            format!(
                "selection {:?}",
                self.ed.selection.as_ref().map(|s| (s.curve, s.knots.len()))
            ),
        ];
        let trace = &mut self.trace;
        trace.last.resize(now.len(), String::new());
        for (last, now) in trace.last.iter_mut().zip(now) {
            if *last != now {
                log::debug!("{now}");
                *last = now;
            }
        }
    }
}
