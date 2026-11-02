//! Fit in blank (Board menu): choose a foam blank from the library or your own, place the board in it (shift,
//! tilt, height, or Auto fit) and see whether it fits; the blank is drawn behind the board in the outline and
//! rocker views, with the places where the board sticks out in red. Geometry in `surfcraft_core::blank`.

use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Stroke};
use serde::{Deserialize, Serialize};
use surfcraft_core::blank::{
    self, Blank, BlankSetup, BlankShape, BoardSamples, FitReport, FitView, Measure, Placement,
};
use surfcraft_core::units::{FOOT, INCH};
use surfcraft_core::{Board, DVec2};

use crate::document::Document;
use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;
use crate::view2d::Canvas;

/// Saved with the settings: the user's own blanks and whether the chosen blank is drawn in the views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlankPrefs {
    /// User blanks in the one-line form of the `.brd` field (cm).
    pub user: Vec<String>,
    /// Draw the board's blank behind the outline and the rocker.
    pub show: bool,
}

impl Default for BlankPrefs {
    fn default() -> Self {
        Self {
            user: Vec::new(),
            show: true,
        }
    }
}

impl BlankPrefs {
    pub fn user_blanks(&self) -> Vec<Blank> {
        self.user
            .iter()
            .filter_map(|s| Blank::from_compact(s))
            .collect()
    }

    /// Add a blank, replacing the one called `replace` (or one with the same name).
    pub fn save(&mut self, b: &Blank, replace: Option<&str>) {
        let names = [Some(b.name.as_str()), replace];
        self.user.retain(|s| {
            Blank::from_compact(s).is_none_or(|u| !names.contains(&Some(u.name.as_str())))
        });
        self.user.push(b.to_compact());
    }

    pub fn delete(&mut self, name: &str) {
        self.user
            .retain(|s| Blank::from_compact(s).is_none_or(|u| u.name != name));
    }

    pub fn is_user(&self, name: &str) -> bool {
        self.user_blanks().iter().any(|b| b.name == name)
    }
}

/// The Fit in blank window.
#[derive(Default)]
pub struct BlankWindow {
    pub open: bool,
    editor: Option<Editor>,
}

/// New / edit blank form.
struct Editor {
    blank: Blank,
    /// Name of the user blank being edited (replaced on save).
    replaces: Option<String>,
    error: Option<String>,
}

// ------------------------------------------------------------------------------------------- fit cache

struct Cached {
    revision: u64,
    key: String,
    shape: Arc<BlankShape>,
    report: Arc<FitReport>,
}

impl Clone for Cached {
    fn clone(&self) -> Self {
        Self {
            revision: self.revision,
            key: self.key.clone(),
            shape: self.shape.clone(),
            report: self.report.clone(),
        }
    }
}

/// The board's blank shape and fit report, recomputed only when the board changes.
pub fn fit_of(ctx: &egui::Context, doc: &Document) -> Option<(Arc<BlankShape>, Arc<FitReport>)> {
    let setup = doc.board.blank.as_ref()?;
    let id = egui::Id::unique("blank-fit-cache");
    let key = setup.to_compact();
    if let Some(c) = ctx.data(|d| d.get_temp::<Cached>(id))
        && c.revision == doc.revision
        && c.key == key
    {
        return Some((c.shape, c.report));
    }
    let shape = BlankShape::new(&setup.blank);
    let report = blank::fit_samples(&BoardSamples::new(&doc.board), &shape, &setup.placement);
    let c = Cached {
        revision: doc.revision,
        key,
        shape: Arc::new(shape),
        report: Arc::new(report),
    };
    ctx.data_mut(|d| d.insert_temp(id, c.clone()));
    Some((c.shape, c.report))
}

// ------------------------------------------------------------------------------------------- window

fn good() -> Color32 {
    Color32::from_rgb(60, 170, 90)
}

fn bad() -> Color32 {
    Color32::from_rgb(225, 60, 60)
}

fn view_name(v: FitView) -> &'static str {
    match v {
        FitView::Outline => tr("Outline (per side)"),
        FitView::Deck => tr("Deck"),
        FitView::Bottom => tr("Bottom"),
        FitView::Length => tr("Length"),
    }
}

/// The Fit in blank window (non-modal).
pub fn window(ctx: &egui::Context, st: &mut BlankWindow, doc: &mut Document, set: &mut Settings) {
    if !st.open {
        return;
    }
    let mut open = st.open;
    egui::Window::new(tr("Fit in blank"))
        .id(egui::Id::unique("blank-fit-window"))
        .open(&mut open)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| body(ui, st, doc, set));
    st.open = open;
    editor_window(ctx, st, doc, set);
}

fn body(ui: &mut egui::Ui, st: &mut BlankWindow, doc: &mut Document, set: &mut Settings) {
    let current = doc.board.blank.as_ref().map(|s| s.blank.name.clone());
    let user = set.blanks.user_blanks();
    let mut choose: Option<Blank> = None;
    ui.horizontal(|ui| {
        ui.label(tr("Blank"));
        egui::ComboBox::from_id_salt("blank-picker")
            .width(300.0)
            .selected_text(
                current
                    .clone()
                    .unwrap_or_else(|| tr("Choose a blank").into()),
            )
            .show_ui(ui, |ui| {
                ui.weak(tr("Library"));
                for b in blank::library() {
                    let on = current.as_deref() == Some(b.name.as_str());
                    if ui.selectable_label(on, &b.name).clicked() {
                        choose = Some(b);
                    }
                }
                if !user.is_empty() {
                    ui.separator();
                    ui.weak(tr("My blanks"));
                    for b in &user {
                        let on = current.as_deref() == Some(b.name.as_str());
                        if ui.selectable_label(on, &b.name).clicked() {
                            choose = Some(b.clone());
                        }
                    }
                }
            });
    });
    if let Some(b) = choose {
        doc.edit(|board| {
            board.blank = Some(BlankSetup {
                blank: b,
                placement: Placement::default(),
            })
        });
    }
    ui.horizontal(|ui| {
        if ui
            .button(tr("New blank…"))
            .on_hover_text(tr(
                "Enter a blank's dimensions from a catalogue or a blank you have",
            ))
            .clicked()
        {
            st.editor = Some(Editor {
                blank: new_blank_for(&doc.board),
                replaces: None,
                error: None,
            });
        }
        let setup = doc.board.blank.clone();
        if ui
            .add_enabled(setup.is_some(), egui::Button::new(tr("Edit…")))
            .on_hover_text(tr("Change the dimensions; saved as one of your blanks"))
            .clicked()
            && let Some(s) = &setup
        {
            let mine = set.blanks.is_user(&s.blank.name);
            st.editor = Some(Editor {
                blank: s.blank.clone(),
                replaces: mine.then(|| s.blank.name.clone()),
                error: None,
            });
        }
        let mine = current.as_deref().is_some_and(|n| set.blanks.is_user(n));
        if ui
            .add_enabled(mine, egui::Button::new(tr("Delete")))
            .on_hover_text(tr(
                "Delete this blank from your blanks (the board keeps it)",
            ))
            .clicked()
            && let Some(n) = &current
        {
            set.blanks.delete(n);
        }
        if ui
            .add_enabled(setup.is_some(), egui::Button::new(tr("Remove from board")))
            .clicked()
        {
            doc.edit(|b| b.blank = None);
        }
    });
    ui.checkbox(
        &mut set.blanks.show,
        tr("Show the blank in the outline and rocker views"),
    );
    let Some(setup) = doc.board.blank.clone() else {
        ui.separator();
        ui.weak(tr(
            "Choose a blank to check that the board fits inside it before you buy it.",
        ));
        return;
    };
    let Some((_, r)) = fit_of(ui.ctx(), doc) else {
        return;
    };
    let b = &setup.blank;
    ui.weak(trf(
        "{0} long, {1} wide, {2} thick; rocker {3} nose, {4} tail",
        &[
            &set.fmt(b.length),
            &set.fmt(b.width),
            &set.fmt(b.thickness),
            &set.fmt(b.nose_rocker),
            &set.fmt(b.tail_rocker),
        ],
    ));
    ui.separator();
    placement_ui(ui, doc, set, &setup, &r);
    ui.separator();
    results_ui(ui, set, &r);
    ui.weak(tr(
        "The blank between its published measurements is estimated: keep a few millimetres of margin.",
    ));
}

fn placement_ui(
    ui: &mut egui::Ui,
    doc: &mut Document,
    set: &Settings,
    setup: &BlankSetup,
    r: &FitReport,
) {
    let p = setup.placement;
    let set_p = |doc: &mut Document, p: Placement| {
        doc.edit(|b| {
            if let Some(s) = &mut b.blank {
                s.placement = p;
            }
        })
    };
    egui::Grid::new("blank-placement")
        .num_columns(3)
        .show(ui, |ui| {
            ui.label(tr("Board tail from blank tail"));
            if let Some(v) = length_field(ui, "blank-shift", r.tail_offset, set) {
                set_p(
                    doc,
                    Placement {
                        tail_offset: Some(v),
                        ..p
                    },
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(p.tail_offset.is_some(), egui::Button::new(tr("Centre")))
                    .on_hover_text(tr("Same spare foam at both ends"))
                    .clicked()
                {
                    set_p(
                        doc,
                        Placement {
                            tail_offset: None,
                            ..p
                        },
                    );
                }
                if ui.button(tr("Align tails")).clicked() {
                    set_p(
                        doc,
                        Placement {
                            tail_offset: Some(0.0),
                            ..p
                        },
                    );
                }
            });
            ui.end_row();
            ui.label(tr("Tilt"));
            let mut t = p.tilt;
            if ui
                .add(
                    egui::DragValue::new(&mut t)
                        .speed(0.05)
                        .range(-10.0..=10.0)
                        .fixed_decimals(2)
                        .suffix("°"),
                )
                .on_hover_text(tr(
                    "Rotates the board in the rocker view; positive lifts the nose",
                ))
                .changed()
            {
                doc.edit_grouped(|b| {
                    if let Some(s) = &mut b.blank {
                        s.placement.tilt = t;
                    }
                });
            }
            if ui
                .add_enabled(p.tilt != 0.0, egui::Button::new(tr("Level")))
                .clicked()
            {
                set_p(doc, Placement { tilt: 0.0, ..p });
            }
            ui.end_row();
            ui.label(tr("Height above the blank's bottom"));
            if let Some(v) = length_field(ui, "blank-lift", r.lift, set) {
                set_p(doc, Placement { lift: Some(v), ..p });
            }
            if ui
                .add_enabled(p.lift.is_some(), egui::Button::new(tr("Centre height")))
                .on_hover_text(tr("Same smallest foam above and below the board"))
                .clicked()
            {
                set_p(doc, Placement { lift: None, ..p });
            }
            ui.end_row();
        });
    if ui
        .button(tr("Auto fit"))
        .on_hover_text(tr(
            "Finds the shift along the blank and the tilt that leave the most foam everywhere",
        ))
        .clicked()
    {
        let auto = blank::auto_fit(&doc.board, &setup.blank);
        set_p(doc, auto);
    }
}

fn results_ui(ui: &mut egui::Ui, set: &Settings, r: &FitReport) {
    if r.fits {
        ui.label(RichText::new(tr("Fits")).color(good()).strong().size(18.0));
        ui.label(trf(
            "Smallest clearance: {0}",
            &[&set.fmt(r.min_clearance())],
        ));
    } else {
        ui.label(
            RichText::new(tr("Too big"))
                .color(bad())
                .strong()
                .size(18.0),
        );
        for p in &r.problems {
            let text = match p.view {
                FitView::Length if p.at <= 0.0 => trf(
                    "The tail is {0} past the blank's tail",
                    &[&set.fmt(p.depth)],
                ),
                FitView::Length => trf(
                    "The nose is {0} past the blank's nose",
                    &[&set.fmt(p.depth)],
                ),
                v => trf(
                    "{0}: sticks out by {1} from {2} to {3} (worst at {4})",
                    &[
                        &view_name(v),
                        &set.fmt(p.depth),
                        &set.fmt(p.from),
                        &set.fmt(p.to),
                        &set.fmt(p.at),
                    ],
                ),
            };
            ui.label(RichText::new(text).color(bad()));
        }
    }
    let normal = ui.visuals().text_color();
    let color = move |v: f64| if v < 0.0 { bad() } else { normal };
    egui::Grid::new("blank-clearances")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.strong(tr("Foam left"));
            ui.strong(tr("Smallest"));
            ui.strong(tr("Tightest at (from the tail)"));
            ui.end_row();
            for (view, c) in [
                (FitView::Outline, r.outline),
                (FitView::Deck, r.deck),
                (FitView::Bottom, r.bottom),
            ] {
                ui.label(view_name(view));
                ui.label(RichText::new(set.fmt(c.min)).color(color(c.min)));
                ui.label(set.fmt(c.at));
                ui.end_row();
            }
            for (name, v) in [
                (tr("At the tail"), r.tail_spare),
                (tr("At the nose"), r.nose_spare),
            ] {
                ui.label(name);
                ui.label(RichText::new(set.fmt(v)).color(color(v)));
                ui.label("");
                ui.end_row();
            }
        });
    if r.tilt != 0.0 || r.tail_offset != 0.0 {
        ui.weak(trf(
            "Board placed {0} from the blank's tail, tilted {1}°, {2} above the blank's bottom",
            &[
                &set.fmt(r.tail_offset),
                &format!("{:.2}", r.tilt),
                &set.fmt(r.lift),
            ],
        ));
    }
}

/// A starting point for a new blank: the board's size with a little foam all round.
fn new_blank_for(board: &Board) -> Blank {
    let mut b = blank::blank_around(board, 0.5 * INCH, 0.5 * INCH, 0.25 * INCH);
    b.stations.retain(|s| (s.dist - FOOT).abs() < 1e-6);
    b.name = if board.info.name.trim().is_empty() {
        tr("My blank").to_string()
    } else {
        trf("Blank for {0}", &[&board.info.name.trim()])
    };
    b
}

/// A length field that may be empty (not measured). Returns `Some(new value)` when an edit is committed.
fn opt_length_field(
    ui: &mut egui::Ui,
    id: &str,
    value: Option<f64>,
    set: &Settings,
) -> Option<Option<f64>> {
    let id = ui.make_persistent_id(id);
    let shown = value.map(|v| set.fmt(v)).unwrap_or_default();
    let mut text: String = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or(shown.clone());
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .desired_width(118.0)
            .hint_text(tr("not measured")),
    );
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if resp.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        if text.trim() == shown {
            return None;
        }
        if text.trim().is_empty() {
            return Some(None);
        }
        return set
            .length_unit()
            .parse(&text)
            .filter(|v| v.is_finite())
            .map(Some);
    }
    None
}

fn editor_window(
    ctx: &egui::Context,
    st: &mut BlankWindow,
    doc: &mut Document,
    set: &mut Settings,
) {
    let Some(ed) = st.editor.as_mut() else {
        return;
    };
    let mut open = true;
    let mut done = false;
    egui::Window::new(tr("Blank dimensions"))
        .id(egui::Id::unique("blank-editor-window"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let b = &mut ed.blank;
            egui::Grid::new("blank-editor")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr("Name"));
                    ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(240.0));
                    ui.end_row();
                    let main: [(&str, &str, &mut f64); 6] = [
                        (tr("Length (tip to tip)"), "be-length", &mut b.length),
                        (tr("Widest width"), "be-width", &mut b.width),
                        (tr("Thickness at the middle"), "be-thick", &mut b.thickness),
                        (tr("Nose rocker"), "be-nr", &mut b.nose_rocker),
                        (tr("Tail rocker"), "be-tr", &mut b.tail_rocker),
                        (
                            tr("Wide point from the middle (+ toward the nose)"),
                            "be-wp",
                            &mut b.wide_point,
                        ),
                    ];
                    for (label, id, v) in main {
                        ui.label(label);
                        if let Some(n) = length_field(ui, id, *v, set) {
                            *v = n;
                        }
                        ui.end_row();
                    }
                    let foot = set.fmt(FOOT);
                    for (label, id, nose, m) in [
                        (
                            trf("Width {0} from the nose", &[&foot]),
                            "be-nw",
                            true,
                            Measure::Width,
                        ),
                        (
                            trf("Width {0} from the tail", &[&foot]),
                            "be-tw",
                            false,
                            Measure::Width,
                        ),
                        (
                            trf("Thickness {0} from the nose", &[&foot]),
                            "be-nt",
                            true,
                            Measure::Thickness,
                        ),
                        (
                            trf("Thickness {0} from the tail", &[&foot]),
                            "be-tt",
                            false,
                            Measure::Thickness,
                        ),
                        (
                            trf("Rocker {0} from the nose", &[&foot]),
                            "be-nr12",
                            true,
                            Measure::Rocker,
                        ),
                        (
                            trf("Rocker {0} from the tail", &[&foot]),
                            "be-tr12",
                            false,
                            Measure::Rocker,
                        ),
                    ] {
                        ui.label(label);
                        let v = b.station(nose, FOOT, m);
                        if let Some(n) = opt_length_field(ui, id, v, set) {
                            b.set_station(nose, FOOT, m, n);
                        }
                        ui.end_row();
                    }
                });
            let others = b
                .stations
                .iter()
                .filter(|s| (s.dist - FOOT).abs() > 1e-6)
                .count();
            if others > 0 {
                ui.weak(trf(
                    "Plus {0} more measured stations from the catalogue drawing (kept).",
                    &[&others],
                ));
            }
            if let Some(e) = &ed.error {
                ui.colored_label(bad(), e);
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Save and use")).clicked() {
                    ed.blank.name = ed.blank.name.trim().to_string();
                    if ed.blank.name.is_empty() {
                        ed.error = Some(tr("Give the blank a name.").into());
                    } else if let Err(e) = ed.blank.validate() {
                        ed.error = Some(trf("Check the numbers: {0}", &[&e]));
                    } else {
                        set.blanks.save(&ed.blank, ed.replaces.as_deref());
                        let blank = ed.blank.clone();
                        doc.edit(|board| {
                            let placement = board
                                .blank
                                .as_ref()
                                .map(|s| s.placement)
                                .unwrap_or_default();
                            board.blank = Some(BlankSetup { blank, placement });
                        });
                        done = true;
                    }
                }
                if ui.button(tr("Cancel")).clicked() {
                    done = true;
                }
            });
        });
    if done || !open {
        st.editor = None;
    }
}

// ------------------------------------------------------------------------------------------- drawing

fn fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgba_unmultiplied(235, 205, 140, 38)
    } else {
        Color32::from_rgba_unmultiplied(205, 160, 70, 45)
    }
}

fn edge(dark: bool) -> Stroke {
    let c = if dark {
        Color32::from_rgba_unmultiplied(235, 205, 140, 120)
    } else {
        Color32::from_rgba_unmultiplied(170, 120, 40, 140)
    };
    Stroke::new(1.0, c)
}

/// Fill the band between two polylines (same length) with a triangle strip.
fn band(canvas: &Canvas, a: &[DVec2], b: &[DVec2], color: Color32) {
    let mut mesh = egui::Mesh::default();
    for (p, q) in a.iter().zip(b) {
        mesh.colored_vertex(canvas.xf.to_screen(*p), color);
        mesh.colored_vertex(canvas.xf.to_screen(*q), color);
    }
    let n = a.len().min(b.len()) as u32;
    for i in 0..n.saturating_sub(1) {
        let k = 2 * i;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    canvas.painter.add(egui::Shape::mesh(mesh));
}

/// Runs of samples where `bad` holds, as point lists from `at`.
fn runs(
    r: &FitReport,
    bad: impl Fn(&blank::FitSample) -> bool,
    at: impl Fn(f64) -> DVec2,
) -> Vec<Vec<DVec2>> {
    let mut out: Vec<Vec<DVec2>> = Vec::new();
    let mut open = false;
    for s in &r.samples {
        if bad(s) {
            if !open {
                out.push(Vec::new());
                open = true;
            }
            if let Some(run) = out.last_mut() {
                run.push(at(s.x));
            }
        } else {
            open = false;
        }
    }
    out
}

fn visible(doc: &Document, set: &Settings) -> bool {
    set.blanks.show && doc.board.blank.is_some()
}

/// The blank behind the board in the outline view, and the board's outline in red where it sticks out.
pub fn draw_outline(canvas: &Canvas, doc: &Document, set: &Settings) {
    if !visible(doc, set) {
        return;
    }
    let Some((shape, r)) = fit_of(canvas.painter.ctx(), doc) else {
        return;
    };
    let dark = canvas.painter.ctx().global_style().visuals.dark_mode;
    let pts: Vec<DVec2> = shape
        .outline_points()
        .iter()
        .step_by(2)
        .map(|p| DVec2::new(r.to_board(DVec2::new(p.x, 0.0)).x, p.y))
        .collect();
    let lower: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.x, -p.y)).collect();
    band(canvas, &pts, &lower, fill(dark));
    canvas.polyline(pts.iter().copied(), edge(dark));
    canvas.polyline(lower.iter().copied(), edge(dark));
    let board = &doc.board;
    let red = Stroke::new(3.0, bad());
    for side in [1.0, -1.0] {
        for run in runs(
            &r,
            |s| s.outline < -1e-6,
            |x| DVec2::new(x, side * board.width_at(x) / 2.0),
        ) {
            canvas.polyline(run, red);
        }
    }
}

/// The blank's deck and bottom behind the board in the rocker view (`ys` = vertical exaggeration), and the
/// board's deck / bottom in red where they stick out.
pub fn draw_profile(canvas: &Canvas, doc: &Document, set: &Settings, ys: f64) {
    if !visible(doc, set) {
        return;
    }
    let Some((shape, r)) = fit_of(canvas.painter.ctx(), doc) else {
        return;
    };
    let dark = canvas.painter.ctx().global_style().visuals.dark_mode;
    let disp = |p: DVec2| {
        let q = r.to_board(p);
        DVec2::new(q.x, q.y * ys)
    };
    let bottom: Vec<DVec2> = shape
        .bottom_points()
        .into_iter()
        .step_by(2)
        .map(disp)
        .collect();
    let deck: Vec<DVec2> = shape
        .deck_points()
        .into_iter()
        .step_by(2)
        .map(disp)
        .collect();
    band(canvas, &bottom, &deck, fill(dark));
    canvas.polyline(bottom.iter().copied(), edge(dark));
    canvas.polyline(deck.iter().copied(), edge(dark));
    let board = &doc.board;
    let red = Stroke::new(3.0, bad());
    for run in runs(
        &r,
        |s| s.deck < -1e-6,
        |x| DVec2::new(x, board.deck_at(x) * ys),
    ) {
        canvas.polyline(run, red);
    }
    for run in runs(
        &r,
        |s| s.bottom < -1e-6,
        |x| DVec2::new(x, board.rocker_at(x) * ys),
    ) {
        canvas.polyline(run, red);
    }
}
