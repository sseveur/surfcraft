//! "Compare with…" window: this board against another one (a file, or the ghost board), spec by spec and at
//! every foot (30 cm in metric) along the length, with the differences coloured. Non-modal: the table follows
//! edits to the board.

use eframe::egui::{self, Color32, RichText};
use surfcraft_core::Board;
use surfcraft_core::units::LengthUnit;
use surfcraft_session::compare::{self, Kind, Measure, Measures, Row};

use crate::app::{BOARD_EXTS, SurfCraftApp, read_board_file};
use crate::i18n::{tr, trf};
use crate::settings::Settings;

#[derive(Default)]
pub struct CompareState {
    pub open: bool,
    /// The other board and its display name.
    pub other: Option<(String, Board)>,
    /// Measurements cached until the board (or the station step) changes: they integrate volume and area.
    this_cache: Option<(Board, bool, Measures)>,
    other_cache: Option<(bool, Measures)>,
}

impl CompareState {
    pub fn set_other(&mut self, name: String, board: Board) {
        self.other = Some((name, board));
        self.other_cache = None;
        self.open = true;
    }

    /// The comparison rows for `this` board, in the station step of `imperial`.
    pub fn rows(&mut self, this: &Board, imperial: bool) -> Option<Vec<Row>> {
        let (_, other) = self.other.as_ref()?;
        let step = compare::station_step(imperial);
        if !matches!(&self.this_cache, Some((b, i, _)) if *i == imperial && b == this) {
            self.this_cache = Some((this.clone(), imperial, compare::measure(this, step)));
        }
        if !matches!(&self.other_cache, Some((i, _)) if *i == imperial) {
            self.other_cache = Some((imperial, compare::measure(other, step)));
        }
        let a = &self.this_cache.as_ref()?.2;
        let b = &self.other_cache.as_ref()?.1;
        Some(compare::rows(a, b, step))
    }
}

pub fn label(m: Measure, set: &Settings) -> String {
    let f = |cm: f64| set.fmt(cm);
    match m {
        Measure::Length => tr("Length").into(),
        Measure::MaxWidth => tr("Max width").into(),
        Measure::WidePointFromTail => tr("Wide point from tail").into(),
        Measure::WidePointFromCentre => tr("Wide point from centre (+ toward nose)").into(),
        Measure::NoseWidth1ft => tr("Nose 1 ft").into(),
        Measure::TailWidth1ft => tr("Tail 1 ft").into(),
        Measure::CentreWidth => tr("Width centre").into(),
        Measure::MaxThickness => tr("Max thickness").into(),
        Measure::ThickestFromTail => tr("Thickest point from tail").into(),
        Measure::CentreThickness => tr("Thickness centre").into(),
        Measure::NoseRocker => tr("Nose rocker").into(),
        Measure::TailRocker => tr("Tail rocker").into(),
        Measure::NoseRocker1ft => tr("Nose rocker 1 ft").into(),
        Measure::TailRocker1ft => tr("Tail rocker 1 ft").into(),
        Measure::NoseRocker2ft => tr("Nose rocker 2 ft").into(),
        Measure::TailRocker2ft => tr("Tail rocker 2 ft").into(),
        Measure::Area => tr("Planform area").into(),
        Measure::Volume => tr("Volume").into(),
        Measure::CentreOfMass => tr("Centre of mass from tail").into(),
        Measure::StationWidth(x) => trf("Width at {0}", &[&f(x)]),
        Measure::StationThickness(x) => trf("Thickness at {0}", &[&f(x)]),
        Measure::StationRocker(x) => trf("Rocker at {0}", &[&f(x)]),
    }
}

/// A value in the user's unit; `signed` adds "+" to positive differences.
pub fn value(kind: Kind, v: f64, signed: bool, unit: LengthUnit, fraction: u32) -> String {
    let plus = if signed && v > 0.0 { "+" } else { "" };
    let text = match kind {
        Kind::Length => unit.format(v, fraction),
        Kind::Area if unit.is_imperial() => format!("{:.2} ft²", v / (30.48 * 30.48)),
        Kind::Area => format!("{v:.0} cm²"),
        Kind::Volume => format!("{:.2} L", v / 1000.0),
    };
    format!("{plus}{text}")
}

/// Differences below display precision count as equal.
fn negligible(kind: Kind, d: f64) -> bool {
    d.abs()
        < match kind {
            Kind::Length => 0.01,
            Kind::Area => 1.0,
            Kind::Volume => 5.0,
        }
}

pub fn csv(rows: &[Row], set: &Settings) -> String {
    let unit = set.length_unit();
    compare::to_csv(
        [
            tr("Measure"),
            tr("This board"),
            tr("Other board"),
            tr("Difference"),
        ],
        rows,
        |m| label(m, set),
        |k, v, signed| {
            if signed && negligible(k, v) {
                value(k, 0.0, false, unit, set.fraction)
            } else {
                value(k, v, signed, unit, set.fraction)
            }
        },
    )
}

impl SurfCraftApp {
    /// File > Compare with…: compares with the ghost board when one is loaded, otherwise asks for a file.
    pub(crate) fn compare_with(&mut self) {
        match &self.ed.ghost {
            Some(g) => {
                let name = ghost_name(g);
                self.dialogs.compare.set_other(name, g.clone());
            }
            None => self.compare_open_file(),
        }
    }

    fn compare_open_file(&mut self) {
        let Some(p) = self.pick_file(tr("Boards (.brd, .s3d, .srf, .stp)"), BOARD_EXTS) else {
            return;
        };
        match read_board_file(&p) {
            Ok((mut b, _)) => {
                b.model = self.set.model.into();
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.dialogs.compare.set_other(name, b);
            }
            Err(e) => self.notify(trf("Could not open {0}: {1}", &[&p.display(), &e])),
        }
    }

    pub(crate) fn compare_window(&mut self, ctx: &egui::Context) {
        if !self.dialogs.compare.open {
            return;
        }
        enum Action {
            Open,
            UseGhost,
            ShowAsGhost,
            Copy(String),
        }
        let mut action = None;
        let mut open = true;
        let imperial = self.set.length_unit().is_imperial();
        let rows = self.dialogs.compare.rows(&self.doc.board, imperial);
        let other_name = self
            .dialogs
            .compare
            .other
            .as_ref()
            .map(|o| o.0.clone())
            .unwrap_or_default();
        let set = &self.set;
        let has_ghost = self.ed.ghost.is_some();
        let this_name = self
            .doc
            .title()
            .trim_end_matches(" *")
            .trim_end_matches(".brd")
            .to_string();
        egui::Window::new(tr("Compare boards"))
            .id(egui::Id::unique("compare-window"))
            .open(&mut open)
            .default_size([520.0, 560.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(tr("Open other board…"))
                        .on_hover_text(tr("Compare with a board file"))
                        .clicked()
                    {
                        action = Some(Action::Open);
                    }
                    if ui
                        .add_enabled(has_ghost, egui::Button::new(tr("Use ghost board")))
                        .on_hover_text(tr("Compare with the ghost board overlay"))
                        .clicked()
                    {
                        action = Some(Action::UseGhost);
                    }
                    if ui
                        .add_enabled(rows.is_some(), egui::Button::new(tr("Show as ghost")))
                        .on_hover_text(tr("Overlay the other board in the views"))
                        .clicked()
                    {
                        action = Some(Action::ShowAsGhost);
                    }
                    if let Some(rows) = &rows
                        && ui
                            .button(tr("Copy as CSV"))
                            .on_hover_text(tr("Copy the table for a spreadsheet"))
                            .clicked()
                    {
                        action = Some(Action::Copy(csv(rows, set)));
                    }
                });
                ui.separator();
                let Some(rows) = rows else {
                    ui.weak(tr("Open another board to compare with."));
                    return;
                };
                ui.weak(tr("Difference: this board minus the other board."));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    table(ui, &rows, set, &this_name, &other_name);
                });
            });
        if !open {
            self.dialogs.compare.open = false;
        }
        match action {
            Some(Action::Open) => self.compare_open_file(),
            Some(Action::UseGhost) => self.compare_with(),
            Some(Action::ShowAsGhost) => {
                if let Some((_, b)) = &self.dialogs.compare.other {
                    self.ed.ghost = Some(b.clone());
                    self.set.view.ghost = true;
                }
            }
            Some(Action::Copy(text)) => {
                ctx.copy_text(text);
                self.notify(tr("Comparison copied as CSV"));
            }
            None => {}
        }
    }
}

fn ghost_name(g: &Board) -> String {
    if g.info.name.trim().is_empty() {
        tr("Ghost board").into()
    } else {
        g.info.name.clone()
    }
}

fn diff_colour(ui: &egui::Ui, bigger: bool) -> Color32 {
    match (ui.visuals().dark_mode, bigger) {
        (true, true) => Color32::from_rgb(110, 190, 255),
        (true, false) => Color32::from_rgb(255, 150, 90),
        (false, true) => Color32::from_rgb(20, 90, 200),
        (false, false) => Color32::from_rgb(200, 70, 0),
    }
}

fn table(ui: &mut egui::Ui, rows: &[Row], set: &Settings, this_name: &str, other_name: &str) {
    let unit = set.length_unit();
    let v = |k: Kind, x: Option<f64>| {
        x.map(|x| value(k, x, false, unit, set.fraction))
            .unwrap_or_else(|| "-".into())
    };
    egui::Grid::new("compare-grid")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new(tr("Measure")).strong());
            ui.label(RichText::new(tr("This board")).strong())
                .on_hover_text(this_name);
            ui.label(RichText::new(tr("Other board")).strong())
                .on_hover_text(other_name);
            ui.label(RichText::new(tr("Difference")).strong())
                .on_hover_text(tr("Difference: this board minus the other board."));
            ui.end_row();
            let mut stations_started = false;
            for r in rows {
                if !stations_started && matches!(r.measure, Measure::StationWidth(_)) {
                    stations_started = true;
                    ui.label(
                        RichText::new(trf(
                            "Every {0} from the tail",
                            &[&set.fmt(compare::station_step(unit.is_imperial()))],
                        ))
                        .strong(),
                    );
                    ui.end_row();
                }
                let k = r.measure.kind();
                ui.label(label(r.measure, set));
                ui.label(RichText::new(v(k, r.this)).monospace());
                ui.label(RichText::new(v(k, r.other)).monospace());
                match r.diff() {
                    Some(d) if !negligible(k, d) => {
                        let c = diff_colour(ui, d > 0.0);
                        ui.label(
                            RichText::new(value(k, d, true, unit, set.fraction))
                                .monospace()
                                .color(c),
                        )
                        .on_hover_text(if d > 0.0 {
                            tr("This board is bigger")
                        } else {
                            tr("This board is smaller")
                        });
                    }
                    Some(_) => {
                        ui.label(RichText::new("=").monospace().weak());
                    }
                    None => {
                        ui.label("-");
                    }
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable as _;

    use super::*;
    use crate::gui_tests::{H, harness, snap, temp_dir};

    fn click(h: &mut H, label: &str) {
        h.get_by_label(label).click();
        h.run_steps(3);
    }

    fn menu(h: &mut H, top: &str, item: &str) {
        click(h, top);
        let n = h
            .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(item)))
            .next()
            .unwrap_or_else(|| panic!("no menu item {item:?}"))
            .rect()
            .center();
        h.hover_at(n);
        h.run_steps(1);
        h.get_all_by(|n| n.label().is_some_and(|l| l.starts_with(item)))
            .next()
            .unwrap()
            .click();
        h.run_steps(3);
    }

    #[test]
    fn value_formats() {
        let ft = LengthUnit::FeetInchesFraction;
        assert_eq!(value(Kind::Length, 2.54, true, ft, 16), "+1\"");
        assert_eq!(value(Kind::Length, -1.27, true, ft, 16), "-1/2\"");
        assert_eq!(value(Kind::Volume, 30_500.0, false, ft, 16), "30.50 L");
        assert_eq!(
            value(Kind::Area, 1000.0, false, LengthUnit::Centimetres, 16),
            "1000 cm²"
        );
    }

    #[test]
    fn compare_with_file_ghost_and_csv() {
        let mut h = harness();
        // A file: the built-in mid-length against the default shortboard.
        let p = temp_dir("qa-compare").join("mid.brd");
        std::fs::write(
            &p,
            surfcraft_io::brd::write(&surfcraft_io::templates::TEMPLATES[1].board()),
        )
        .unwrap();
        h.state_mut()
            .scripted_dialogs
            .as_mut()
            .unwrap()
            .push_back(p.clone());
        menu(&mut h, "File", "Compare with…");
        assert!(h.state().dialogs.compare.open);
        assert_eq!(h.state().dialogs.compare.other.as_ref().unwrap().0, "mid");
        assert!(h.query_by_label("Wide point from tail").is_some());
        assert!(h.query_by_label("Every 12\" from the tail").is_some());
        // The shortboard is shorter: negative length difference.
        let rows = {
            let app = h.state_mut();
            let board = app.doc.board.clone();
            app.dialogs.compare.rows(&board, true).unwrap()
        };
        let len = rows.iter().find(|r| r.measure == Measure::Length).unwrap();
        assert!(len.diff().unwrap() < -20.0);
        snap(&mut h, "compare-window");
        // CSV.
        click(&mut h, "Copy as CSV");
        assert!(h.state().status.as_ref().unwrap().0.contains("CSV"));
        let text = csv(&rows, &h.state().set);
        assert!(text.starts_with("Measure,This board,Other board,Difference\n"));
        assert_eq!(text.lines().count(), rows.len() + 1);
        // Show as ghost loads it as the overlay without touching the board.
        assert!(h.state().ed.ghost.is_none());
        click(&mut h, "Show as ghost");
        assert!(h.state().ed.ghost.is_some());
        assert!(h.state().set.view.ghost);
        assert!(!h.state().doc.is_dirty());
        // With a ghost loaded, Compare with… uses it directly (no file dialog).
        h.state_mut().dialogs.compare = CompareState::default();
        h.state_mut().ed.ghost.as_mut().unwrap().info.name = "Ghosty".into();
        menu(&mut h, "File", "Compare with…");
        assert_eq!(
            h.state().dialogs.compare.other.as_ref().unwrap().0,
            "Ghosty"
        );
        // Editing the board updates the table.
        let board = h.state().doc.board.clone();
        let before = h.state_mut().dialogs.compare.rows(&board, true);
        h.state_mut().doc.edit(|b| {
            let (l, w, t) = (b.length(), b.max_width(), b.max_thickness());
            b.scale(l + 10.0, w, t);
        });
        h.run_steps(3);
        let board = h.state().doc.board.clone();
        let after = h.state_mut().dialogs.compare.rows(&board, true);
        assert_ne!(before, after);
        // Closing the window.
        h.state_mut().dialogs.compare.open = false;
        h.run_steps(2);
        assert!(h.query_by_label("Copy as CSV").is_none());
    }
}
