//! File → New from measurements: type a board's tape measurements (or import them from CSV) and build a fair
//! board from them (see `surfcraft_session::measurements`).

use eframe::egui;
use surfcraft_io::templates::TEMPLATES;
use surfcraft_session::measurements::{self, BOTTOMS, Column, Problem, Row, Table, TailShape};
use surfcraft_session::shaping::{ContourKind, RAIL_PRESETS};

use crate::app::{Pending, SurfCraftApp};
use crate::document::write_with_backup;
use crate::i18n::{tr, trf};
use crate::settings::Settings;

/// Window state.
#[derive(Clone, Debug, Default)]
pub struct MeasureState {
    pub table: Table,
    pub problems: Vec<Problem>,
}

/// What the window asks the app to do.
pub enum MeasureAction {
    Create(Box<surfcraft_core::Board>, f64),
    ImportCsv,
    SaveCsv(String),
}

fn tail_label(t: TailShape) -> &'static str {
    match t {
        TailShape::Pin => tr("Pin tail"),
        TailShape::Round => tr("Round tail"),
        TailShape::Squash => tr("Squash tail"),
        TailShape::Square => tr("Square tail"),
        TailShape::Swallow => tr("Swallow tail"),
    }
}

fn bottom_label(k: ContourKind) -> &'static str {
    match k {
        ContourKind::Flat => tr("Flat"),
        ContourKind::SingleConcave => tr("Single concave"),
        ContourKind::DoubleConcave => tr("Double concave"),
        ContourKind::Vee => tr("Vee"),
        ContourKind::Channels => tr("Channels"),
    }
}

fn column(c: Column) -> &'static str {
    match c {
        Column::Length => tr("length"),
        Column::Position => tr("position"),
        Column::Width => tr("width"),
        Column::Rocker => tr("rocker"),
        Column::Thickness => tr("thickness"),
        Column::Depth => tr("depth"),
    }
}

/// The problem in the interface language.
pub fn problem_text(p: &Problem) -> String {
    match p {
        Problem::Unreadable {
            row: 0,
            column: Column::Depth,
            text,
        } => trf("Cannot read the bottom depth \"{0}\".", &[text]),
        Problem::Unreadable { row: 0, text, .. } => trf("Cannot read the length \"{0}\".", &[text]),
        Problem::Unreadable {
            row,
            column: c,
            text,
        } => trf(
            "Row {0}: cannot read the {1} \"{2}\".",
            &[row, &column(*c), text],
        ),
        Problem::NoLength => tr("Enter the board length.").into(),
        Problem::LengthOutOfRange => tr("The length must be between 3' and 15'.").into(),
        Problem::OffBoard { row } => trf("Row {0}: the station is not on the board.", &[row]),
        Problem::Duplicate { row, other } => {
            trf("Row {0}: same station as row {1}.", &[row, other])
        }
        Problem::NotPositive { row, column: c } => trf(
            "Row {0}: the {1} must be more than zero.",
            &[row, &column(*c)],
        ),
        Problem::TooLarge { row, column: c } => trf(
            "Row {0}: the {1} is too large for a surfboard.",
            &[row, &column(*c)],
        ),
        Problem::NegativeRocker { row } => trf(
            "Row {0}: rocker is the height of the bottom above a flat surface and cannot be negative.",
            &[row],
        ),
        Problem::TooFewWidths => {
            tr("Give at least two widths between the tail and the nose.").into()
        }
        Problem::NoThickness => tr("Give at least one thickness.").into(),
        Problem::TailBlockTooWide => {
            tr("The tail block is wider than the board ahead of it.").into()
        }
        Problem::DepthOutOfRange => {
            tr("The bottom depth must be more than zero and at most 1\".").into()
        }
        Problem::Shaping(_) => {
            tr("The bottom or rail shape could not be applied to this board.").into()
        }
        Problem::CsvLine { line, text } => trf("Line {0}: cannot read \"{1}\".", &[line, text]),
    }
}

/// Show the window while `state` is set.
pub fn window(
    ctx: &egui::Context,
    state: &mut Option<MeasureState>,
    set: &Settings,
) -> Option<MeasureAction> {
    let mut open = state.is_some();
    let s = state.as_mut()?;
    let mut action = None;
    let mut close = false;
    egui::Window::new(tr("Board from measurements"))
        .id(egui::Id::unique("board-from-measurements"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.weak(tr(
                "Measure a board you like with a tape and type the numbers: SurfCraft fits fair curves through them.",
            ));
            let t = &mut s.table;
            egui::Grid::new("measure-settings").num_columns(2).show(ui, |ui| {
                ui.label(tr("Length"));
                ui.add(egui::TextEdit::singleline(&mut t.length).desired_width(110.0).hint_text("6'2\""));
                ui.end_row();
                ui.label(tr("Tail shape"));
                egui::ComboBox::from_id_salt("measure-tail")
                    .selected_text(tail_label(t.tail))
                    .show_ui(ui, |ui| {
                        for shape in TailShape::ALL {
                            ui.selectable_value(&mut t.tail, shape, tail_label(shape));
                        }
                    });
                ui.end_row();
                ui.label(tr("Shape between stations"))
                    .on_hover_text(tr("The template whose shape and cross-sections fill in between your measurements"));
                let name = t.template.map_or(tr("Closest in length"), |i| tr(TEMPLATES[i].name));
                egui::ComboBox::from_id_salt("measure-template")
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.template, None, tr("Closest in length"));
                        for (i, tp) in TEMPLATES.iter().enumerate() {
                            ui.selectable_value(&mut t.template, Some(i), tr(tp.name));
                        }
                    });
                ui.end_row();
                ui.label(tr("Bottom"))
                    .on_hover_text(tr("Shaped after the curves are fitted: the rail line stays and foam is cut, so a concave or vee makes the stringer thinner"));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("measure-bottom")
                        .selected_text(t.bottom.map_or(tr("As the template"), bottom_label))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut t.bottom, None, tr("As the template"));
                            for k in BOTTOMS {
                                ui.selectable_value(&mut t.bottom, Some(k), bottom_label(k));
                            }
                        });
                    if t.bottom.is_some_and(|k| k != ContourKind::Flat) {
                        ui.label(tr("Depth"));
                        ui.add(
                            egui::TextEdit::singleline(&mut t.bottom_depth)
                                .desired_width(70.0)
                                .hint_text(tr("usual")),
                        );
                    }
                });
                ui.end_row();
                ui.label(tr("Rails"));
                egui::ComboBox::from_id_salt("measure-rail")
                    .selected_text(t.rail.map_or(tr("As the template"), |i| RAIL_PRESETS[i].0))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.rail, None, tr("As the template"));
                        for (i, p) in RAIL_PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut t.rail, Some(i), p.0);
                        }
                    });
                ui.end_row();
            });
            ui.separator();
            let help = tr(
                "Position: distance from the tail (36\"), or tail 12\", nose 12\", centre. The tail 0 row is the tail end (tail block width, tail lift), nose 0 the nose (nose lift). Rocker is the height of the bottom above a flat surface. Values accept units: 18 1/2\", 47cm.",
            );
            let mut remove = None;
            egui::Grid::new("measure-table").num_columns(5).striped(true).show(ui, |ui| {
                ui.strong(tr("Position")).on_hover_text(help);
                ui.strong(tr("Width"));
                ui.strong(tr("Rocker"));
                ui.strong(tr("Thickness"));
                ui.label("");
                ui.end_row();
                for (i, r) in t.rows.iter_mut().enumerate() {
                    for (f, w) in [
                        (&mut r.position, 90.0),
                        (&mut r.width, 80.0),
                        (&mut r.rocker, 80.0),
                        (&mut r.thickness, 80.0),
                    ] {
                        ui.add_sized([w, 20.0], egui::TextEdit::singleline(f));
                    }
                    if ui.small_button(tr("Remove")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                t.rows.remove(i);
            }
            let csv_help = tr(
                "CSV columns: position, width, rocker, thickness (any order, named in a header line). Optional lines: length, tail (pin, round, squash, square, swallow), template, bottom (template, flat, single_concave, double_concave, vee), bottom depth, rail (template, 50/50, 60/40, 70/30, 80/20). Comma or semicolon separated; bare numbers are in the display unit.",
            );
            ui.horizontal(|ui| {
                if ui.button(tr("Add row")).clicked() {
                    t.rows.push(Row::default());
                }
                if ui.button(tr("Import CSV…")).on_hover_text(csv_help).clicked() {
                    action = Some(MeasureAction::ImportCsv);
                }
                if ui.button(tr("Save CSV…")).on_hover_text(csv_help).clicked() {
                    action = Some(MeasureAction::SaveCsv(t.to_csv()));
                }
            });
            for p in &s.problems {
                ui.colored_label(egui::Color32::from_rgb(230, 80, 60), problem_text(p));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("Create")).clicked() {
                    match measurements::parse(&s.table, set.length_unit()) {
                        Ok(m) => {
                            match measurements::build(&m) {
                                Ok((b, report)) => {
                                    s.problems.clear();
                                    action =
                                        Some(MeasureAction::Create(Box::new(b), report.worst()));
                                    close = true;
                                }
                                Err(p) => s.problems = vec![p],
                            }
                        }
                        Err(ps) => s.problems = ps,
                    }
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    if !open || close {
        *state = None;
    }
    action
}

impl SurfCraftApp {
    /// Carry out what the measurements window asked for.
    pub(crate) fn measure_action(&mut self, action: MeasureAction) {
        match action {
            MeasureAction::Create(board, worst) => {
                self.guarded(Pending::New(board));
                let fit = self.set.fmt(worst);
                self.notify(trf(
                    "Board created; it is within {0} of your measurements.",
                    &[&fit],
                ));
            }
            MeasureAction::ImportCsv => {
                let Some(p) = self.pick_file("CSV", &["csv", "txt"]) else {
                    return;
                };
                let text = match std::fs::read_to_string(&p) {
                    Ok(t) => t,
                    Err(e) => {
                        self.notify(format!("{}: {e}", p.display()));
                        return;
                    }
                };
                let result = Table::from_csv(&text);
                let state = self.dialogs.measure.get_or_insert_with(Default::default);
                match result {
                    Ok(t) => {
                        state.table = t;
                        state.problems.clear();
                    }
                    Err(e) => state.problems = vec![e],
                }
            }
            MeasureAction::SaveCsv(text) => {
                if let Some(p) = self.save_file("CSV", "csv", Some("measurements")) {
                    match write_with_backup(&p, text.as_bytes()) {
                        Ok(()) => self.notify(trf("Saved {0}", &[&p.display()])),
                        Err(e) => self.notify(format!("{}: {e}", p.display())),
                    }
                }
            }
        }
    }
}
