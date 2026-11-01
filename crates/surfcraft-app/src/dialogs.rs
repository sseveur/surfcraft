//! Modal-ish tool windows: new board, scale, board info, fins.

use eframe::egui;
use surfcraft_core::Board;
use surfcraft_io::templates::TEMPLATES;

use crate::document::Document;
use crate::i18n::tr;
use crate::panels::length_field;
use crate::settings::Settings;

#[derive(Default)]
pub struct Dialogs {
    pub new_board: Option<NewBoard>,
    pub scale: Option<Scale>,
    pub info: bool,
    pub fins: bool,
    pub about: bool,
    pub export: bool,
    pub export_state: crate::export_ui::ExportState,
    pub weight: Option<crate::tools::WeightState>,
    pub guides: bool,
    pub cam: bool,
    pub cam_state: crate::cam_ui::CamState,
}

/// Scale `board` to the given overall size.
pub fn scale_board(board: &mut Board, length: f64, width: f64, thickness: f64, scale_fins: bool) {
    let (l0, w0) = (board.length(), board.max_width());
    board.scale(length, width, thickness);
    if scale_fins {
        board.scale_fins(length / l0, width / w0);
    }
}

/// Adjust thickness (keeping length and width) until the volume is `litres`. Returns the reached volume.
pub fn solve_volume(board: &mut Board, litres: f64) -> f64 {
    let target = litres * 1000.0;
    let (l, w) = (board.length(), board.max_width());
    let mut t = board.max_thickness();
    let mut v = board.volume();
    for _ in 0..6 {
        if (v - target).abs() / target < 0.001 || v <= 0.0 {
            break;
        }
        // Volume is close to proportional to thickness.
        t *= target / v;
        board.scale(l, w, t);
        v = board.volume();
    }
    v / 1000.0
}

pub struct NewBoard {
    template: usize,
    length: Option<f64>,
    width: Option<f64>,
    thickness: Option<f64>,
    litres: String,
}

impl NewBoard {
    pub fn new() -> Self {
        Self {
            template: 0,
            length: None,
            width: None,
            thickness: None,
            litres: String::new(),
        }
    }
}

pub struct Scale {
    length: f64,
    width: f64,
    thickness: f64,
    keep_proportions: bool,
    scale_fins: bool,
    /// Optional target volume in litres; thickness is solved after scaling.
    litres: String,
}

impl Scale {
    pub fn of(board: &Board) -> Self {
        Self {
            length: board.length(),
            width: board.max_width(),
            thickness: board.max_thickness(),
            keep_proportions: false,
            scale_fins: true,
            litres: String::new(),
        }
    }
}

/// Returns a board to open when the user creates one.
pub fn new_board_window(
    ctx: &egui::Context,
    state: &mut Option<NewBoard>,
    set: &Settings,
) -> Option<Board> {
    let mut open = state.is_some();
    let mut result = None;
    let mut close = false;
    if let Some(nb) = state.as_mut() {
        egui::Window::new(tr("New board"))
            .id(egui::Id::unique("new-board-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(tr("Start from:"));
                for (i, t) in TEMPLATES.iter().enumerate() {
                    ui.radio_value(
                        &mut nb.template,
                        i,
                        format!("{} — {}", tr(t.name), tr(t.description)),
                    );
                }
                ui.separator();
                ui.label(tr("Optional target size (blank keeps the template's):"));
                let base = TEMPLATES[nb.template].board();
                egui::Grid::new("nb").num_columns(2).show(ui, |ui| {
                    for (i, (label, slot, current)) in [
                        (tr("Length"), &mut nb.length, base.length()),
                        (tr("Width"), &mut nb.width, base.max_width()),
                        (tr("Thickness"), &mut nb.thickness, base.max_thickness()),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.label(label);
                        if let Some(v) = length_field(ui, ("nb", i), slot.unwrap_or(current), set) {
                            *slot = Some(v);
                        }
                        ui.end_row();
                    }
                    ui.label(tr("Volume (L)"));
                    ui.add(
                        egui::TextEdit::singleline(&mut nb.litres)
                            .desired_width(96.0)
                            .hint_text(tr("e.g. 32.5")),
                    );
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(tr("Create")).clicked() {
                        let mut b = base.clone();
                        let (l, w, t) = (
                            nb.length.unwrap_or(b.length()),
                            nb.width.unwrap_or(b.max_width()),
                            nb.thickness.unwrap_or(b.max_thickness()),
                        );
                        if nb.length.is_some() || nb.width.is_some() || nb.thickness.is_some() {
                            scale_board(&mut b, l, w, t, true);
                        }
                        if let Ok(litres) = nb.litres.trim().replace(',', ".").parse::<f64>()
                            && litres > 1.0
                        {
                            solve_volume(&mut b, litres);
                        }
                        b.info.name = String::new();
                        result = Some(b);
                        close = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
    }
    if !open || close {
        *state = None;
    }
    result
}

pub fn scale_window(
    ctx: &egui::Context,
    state: &mut Option<Scale>,
    doc: &mut Document,
    set: &Settings,
) {
    let mut open = state.is_some();
    let mut close = false;
    if let Some(s) = state.as_mut() {
        egui::Window::new(tr("Scale board"))
            .id(egui::Id::unique("scale-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let (l0, w0, t0) = (
                    doc.board.length(),
                    doc.board.max_width(),
                    doc.board.max_thickness(),
                );
                egui::Grid::new("scale").num_columns(3).show(ui, |ui| {
                    ui.label("");
                    ui.label(tr("now"));
                    ui.label(tr("new"));
                    ui.end_row();
                    // Factor of whichever field the user just changed, relative to the current board.
                    let mut changed = None;
                    for (i, (label, now, slot)) in [
                        (tr("Length"), l0, &mut s.length),
                        (tr("Width"), w0, &mut s.width),
                        (tr("Thickness"), t0, &mut s.thickness),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.label(label);
                        ui.label(set.fmt(now));
                        if let Some(v) = length_field(ui, ("scale", i), *slot, set) {
                            changed = Some(v / now);
                            *slot = v;
                        }
                        ui.end_row();
                    }
                    if s.keep_proportions
                        && let Some(f) = changed
                    {
                        s.length = l0 * f;
                        s.width = w0 * f;
                        s.thickness = t0 * f;
                    }
                });
                ui.checkbox(&mut s.keep_proportions, tr("Keep proportions"));
                ui.checkbox(&mut s.scale_fins, tr("Move fins with the board"));
                ui.horizontal(|ui| {
                    ui.label(tr("Target volume (L)"));
                    ui.add(
                        egui::TextEdit::singleline(&mut s.litres)
                            .desired_width(70.0)
                            .hint_text(tr("optional")),
                    );
                });
                ui.weak(tr(
                    "With a target volume, thickness is adjusted after scaling to reach it.",
                ));
                let valid = [s.length, s.width, s.thickness]
                    .iter()
                    .all(|v| v.is_finite())
                    && s.length >= 30.0
                    && s.width >= 5.0
                    && s.thickness >= 1.0;
                if !valid {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 80, 60),
                        tr("Too small: minimum length 30 cm, width 5 cm, thickness 1 cm."),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(valid, egui::Button::new(tr("Apply")))
                        .clicked()
                    {
                        let (l, w, t, f) = (s.length, s.width, s.thickness, s.scale_fins);
                        let litres = s
                            .litres
                            .trim()
                            .replace(',', ".")
                            .parse::<f64>()
                            .ok()
                            .filter(|v| *v > 1.0);
                        doc.edit(|b| {
                            scale_board(b, l, w, t, f);
                            if let Some(v) = litres {
                                solve_volume(b, v);
                            }
                        });
                        close = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
    }
    if !open || close {
        *state = None;
    }
}

pub fn info_window(ctx: &egui::Context, open: &mut bool, doc: &mut Document) {
    let mut info = doc.board.info.clone();
    egui::Window::new(tr("Board info"))
        .id(egui::Id::unique("info-window"))
        .open(open)
        .resizable(true)
        .show(ctx, |ui| {
            egui::Grid::new("info").num_columns(2).show(ui, |ui| {
                for (label, field) in [
                    (tr("Name"), &mut info.name),
                    (tr("Model"), &mut info.model),
                    (tr("Designer"), &mut info.designer),
                    (tr("Author"), &mut info.author),
                    (tr("Surfer"), &mut info.surfer),
                    (tr("Fin setup"), &mut info.fin_type),
                ] {
                    ui.label(label);
                    ui.text_edit_singleline(field);
                    ui.end_row();
                }
            });
            ui.label(tr("Description"));
            ui.text_edit_multiline(&mut info.description);
            ui.label(tr("Comments"));
            ui.text_edit_multiline(&mut info.comments);
        });
    if info != doc.board.info {
        // One undo step per field edit, not per keystroke (closed by the app when focus leaves the field).
        doc.edit_grouped(|b| b.info = info);
    }
}

pub fn fins_window(ctx: &egui::Context, open: &mut bool, doc: &mut Document, set: &Settings) {
    let mut fins = doc.board.fins;
    egui::Window::new(tr("Fins"))
        .id(egui::Id::unique("fins-window"))
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(tr("Positions from the tail and out from the stringer."));
            egui::Grid::new("fins").num_columns(2).show(ui, |ui| {
                let labels = [
                    tr("Side fin rear, from tail"),
                    tr("Side fin rear, from stringer"),
                    tr("Side fin front, from tail"),
                    tr("Side fin front, from stringer"),
                    tr("Centre fin rear, from tail"),
                    tr("Centre fin front, from tail"),
                    tr("Centre fin depth"),
                    tr("Side fin depth"),
                ];
                for (i, label) in labels.iter().enumerate() {
                    ui.label(*label);
                    if let Some(v) = length_field(ui, ("fin", i), fins[i], set) {
                        fins[i] = v;
                    }
                    ui.end_row();
                }
                ui.label(tr("Splay angle (°)"));
                ui.add(
                    egui::DragValue::new(&mut fins[8])
                        .speed(0.1)
                        .range(-20.0..=20.0),
                );
                ui.end_row();
            });
        });
    if fins != doc.board.fins {
        doc.edit_grouped(|b| b.fins = fins);
    }
}

pub fn about_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(tr("About SurfCraft"))
        .id(egui::Id::unique("about-window"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(format!("SurfCraft {}", env!("CARGO_PKG_VERSION")));
            ui.label(tr("Surfboard design for macOS, Windows and Linux."));
            ui.label(tr("Opens and saves BoardCAD .brd files."));
            ui.separator();
            ui.label(tr("Free software under the MIT or Apache 2.0 licence, at your choice."));
        });
}
