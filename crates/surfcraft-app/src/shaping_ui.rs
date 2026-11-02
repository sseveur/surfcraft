//! Shape menu window: bottom contours (concaves, vee, channels), applied to every cross-section in a
//! range. The geometry lives in `surfcraft_session::shaping`; these windows edit the parameters, preview the
//! result (in the window and as a dashed overlay in the Section view) and apply it as one undo step.

use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};
use surfcraft_core::section::{APEX_ANGLE, TUCK_UNDER_ANGLE};
use surfcraft_core::{Board, CrossSection, DVec2};
use surfcraft_session::shaping::{
    self, ContourKind, ContourZone, DEFAULT_SPACING, RunOut, ShapeReport,
};

use crate::document::Document;
use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;
use crate::view2d::{Canvas, ViewState, ident, sample};
use crate::views::EditorState;

/// Last used parameters, kept in the settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapingPrefs {
    pub zones: Vec<ContourZone>,
    /// Largest gap between sections inside a contour zone (cm).
    pub spacing: f64,
}

impl Default for ShapingPrefs {
    fn default() -> Self {
        Self {
            zones: vec![ContourZone::default()],
            spacing: DEFAULT_SPACING,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Contours,
}

/// A computed preview: the board it was computed from (revision) and the result.
struct Preview {
    tool: Tool,
    revision: u64,
    board: Board,
    report: ShapeReport,
}

#[derive(Default)]
pub struct ShapingState {
    pub contours_open: bool,
    preview: Option<Preview>,
    /// Range of "Remove contours" (cm from tail); `None` until the window first opens.
    remove_range: Option<(f64, f64)>,
    contour_view: ViewState,
    error: Option<String>,
}

impl ShapingState {
    pub fn open_contours(&mut self) {
        self.contours_open = true;
        self.contour_view.needs_fit = true;
    }
}

fn kind_label(k: ContourKind) -> &'static str {
    match k {
        ContourKind::Flat => tr("Flat"),
        ContourKind::SingleConcave => tr("Single concave"),
        ContourKind::DoubleConcave => tr("Double concave (inside a single)"),
        ContourKind::Vee => tr("Vee"),
        ContourKind::Channels => tr("Channels"),
    }
}

/// The section of `board` at station `x`: the section placed there, else the interpolated one.
pub fn section_at(board: &Board, x: f64) -> Option<CrossSection> {
    board
        .sections
        .iter()
        .find(|s| (s.position - x).abs() < 1e-9 && !s.is_end_section())
        .cloned()
        .or_else(|| board.interpolated_section(x))
}

fn litres(cm3: f64) -> String {
    format!("{:.2}", cm3 / 1000.0)
}

fn report_text(r: &ShapeReport) -> String {
    let mut s = trf(
        "Volume {0} L > {1} L ({2} L)",
        &[
            &litres(r.volume_before),
            &litres(r.volume_after),
            &format!("{:+.2}", (r.volume_after - r.volume_before) / 1000.0),
        ],
    );
    s.push_str("  ·  ");
    s.push_str(&trf(
        "{0} sections changed, {1} added",
        &[&r.sections_changed, &r.sections_added],
    ));
    if !r.skipped.is_empty() {
        s.push_str("  ·  ");
        s.push_str(&trf(
            "{0} sections skipped (too small for this shape)",
            &[&r.skipped.len()],
        ));
    }
    s
}

/// Show the Shape windows. Returns a status message after an apply.
pub fn windows(
    ctx: &egui::Context,
    st: &mut ShapingState,
    doc: &mut Document,
    ed: &mut EditorState,
    set: &mut Settings,
) -> Option<String> {
    // A preview of an older board, or of a closed window, is gone.
    if let Some(p) = &st.preview {
        let open = match p.tool {
            Tool::Contours => st.contours_open,
        };
        if p.revision != doc.revision || !open {
            st.preview = None;
        }
    }
    let mut msg = None;
    if st.contours_open {
        msg = contours_window(ctx, st, doc, ed, set).or(msg);
    }
    ed.shaping_preview = st.preview.as_ref().map(|p| p.board.clone());
    msg
}

/// Draw the current Section-view section before (grey) and after (accent) the previewed change.
fn preview_canvas(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    doc: &Document,
    ed: &EditorState,
    preview: Option<&Preview>,
    set: &Settings,
) {
    let board = &doc.board;
    let i = ed.current_section.clamp(
        board.real_sections().start,
        board.real_sections().end.saturating_sub(1).max(1),
    );
    let Some(before) = board.sections.get(i).filter(|s| !s.is_end_section()) else {
        return;
    };
    let after = preview.and_then(|p| section_at(&p.board, before.position));
    ui.label(trf(
        "Cross-section at {0} from tail (choose it in the Section view)",
        &[&set.fmt(before.position)],
    ));
    let half = before.spline.max_x().max(1.0);
    let top = before.spline.max_y().max(1.0);
    // Half section (stringer on the left) at a larger scale; the canvas can be zoomed and panned.
    let bounds = (DVec2::new(-0.5, -1.0), DVec2::new(half + 0.5, top + 0.5));
    let dark = ui.visuals().dark_mode;
    let bg = if dark {
        Color32::from_rgb(24, 26, 30)
    } else {
        Color32::from_rgb(250, 250, 248)
    };
    let pal = crate::views::Palette::new(dark);
    ui.allocate_ui(egui::vec2(ui.available_width(), 170.0), |ui| {
        let canvas = Canvas::show(ui, view, bounds, bg);
        let grey = Stroke::new(1.5, pal.inactive);
        canvas.polyline(sample(&before.spline, 24, ident), grey);
        if let Some(a) = &after {
            let st = Stroke::new(2.0, pal.accent);
            canvas.polyline(sample(&a.spline, 24, ident), st);
            // The same rail markers as the Section view.
            let apex = a.point_at_s(a.spline.s_by_normal_reverse(APEX_ANGLE.to_radians(), true));
            let tuck = a.point_at_s(
                a.spline
                    .s_by_normal_reverse(TUCK_UNDER_ANGLE.to_radians(), true),
            );
            for (p, c) in [(apex, pal.accent), (tuck, pal.curve_alt)] {
                canvas
                    .painter
                    .circle_stroke(canvas.xf.to_screen(p), 4.0, Stroke::new(1.2, c));
            }
        }
    });
}

/// Length field on a grid row; returns whether the value changed.
fn len_row(
    ui: &mut egui::Ui,
    label: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
    v: &mut f64,
    min: f64,
    set: &Settings,
) -> bool {
    ui.label(label);
    let changed = if let Some(n) = length_field(ui, id, *v, set) {
        *v = n.max(min);
        true
    } else {
        false
    };
    ui.end_row();
    changed
}

fn zone_ui(ui: &mut egui::Ui, i: usize, z: &mut ContourZone, set: &Settings) -> bool {
    let mut changed = false;
    egui::Grid::new(("contour-zone", i))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr("Type"));
            egui::ComboBox::from_id_salt(("contour-kind", i))
                .selected_text(kind_label(z.kind))
                .show_ui(ui, |ui| {
                    for k in ContourKind::ALL {
                        changed |= ui.selectable_value(&mut z.kind, k, kind_label(k)).changed();
                    }
                });
            ui.end_row();
            if z.kind != ContourKind::Flat {
                let label = if z.kind == ContourKind::Channels {
                    tr("Channel depth")
                } else {
                    tr("Depth")
                };
                changed |= len_row(ui, label, ("c-depth", i), &mut z.depth, 0.0, set);
            }
            if z.kind == ContourKind::DoubleConcave {
                changed |= len_row(
                    ui,
                    tr("Inner concave depth"),
                    ("c-inner", i),
                    &mut z.inner_depth,
                    0.0,
                    set,
                );
            }
            if z.kind == ContourKind::Channels {
                ui.label(tr("Channels per side"));
                changed |= ui
                    .add(egui::DragValue::new(&mut z.channel_count).range(1..=6))
                    .changed();
                ui.end_row();
                changed |= len_row(
                    ui,
                    tr("Channel width"),
                    ("c-cw", i),
                    &mut z.channel_width,
                    0.3,
                    set,
                );
            }
            changed |= len_row(
                ui,
                tr("Start (from tail)"),
                ("c-start", i),
                &mut z.start,
                0.0,
                set,
            );
            changed |= len_row(
                ui,
                tr("End (from tail)"),
                ("c-end", i),
                &mut z.end,
                0.0,
                set,
            );
            changed |= len_row(ui, tr("Fade in"), ("c-fin", i), &mut z.fade_in, 0.0, set);
            changed |= len_row(ui, tr("Fade out"), ("c-fout", i), &mut z.fade_out, 0.0, set);
            ui.label(tr("Runs out"));
            ui.horizontal(|ui| {
                let from_rail = matches!(z.run_out, RunOut::FromRail(_));
                if ui.radio(from_rail, tr("in from the rail")).clicked() && !from_rail {
                    z.run_out = RunOut::FromRail(2.54);
                    changed = true;
                }
                if ui.radio(!from_rail, tr("fraction of half-width")).clicked() && from_rail {
                    z.run_out = RunOut::Fraction(0.9);
                    changed = true;
                }
            });
            ui.end_row();
            match &mut z.run_out {
                RunOut::FromRail(d) => {
                    changed |= len_row(ui, tr("Distance from rail"), ("c-ro", i), d, 0.0, set);
                }
                RunOut::Fraction(f) => {
                    ui.label(tr("Fraction of half-width"));
                    changed |= ui
                        .add(egui::Slider::new(f, 0.1..=1.0).fixed_decimals(2))
                        .changed();
                    ui.end_row();
                }
            }
        });
    changed
}

fn contours_window(
    ctx: &egui::Context,
    st: &mut ShapingState,
    doc: &mut Document,
    ed: &EditorState,
    set: &mut Settings,
) -> Option<String> {
    let mut open = true;
    let mut msg = None;
    let l = doc.board.length();
    let (mut r0, mut r1) = *st.remove_range.get_or_insert((0.0, l));
    let mut prefs = set.shaping.clone();
    egui::Window::new(tr("Bottom contours"))
        .id(egui::Id::unique("contours-window"))
        .open(&mut open)
        .default_size([420.0, 640.0])
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.weak(tr(
                    "Depth is measured with a straight edge across the run-out points. The rocker on the \
                     stringer is kept, so a concave lowers the bottom towards the rails (a little more volume) \
                     and a vee raises it.",
                ));
                let mut changed = false;
                let mut remove = None;
                let n = prefs.zones.len();
                for (i, z) in prefs.zones.iter_mut().enumerate() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(trf("Zone {0}", &[&(i + 1)]));
                        if n > 1 && ui.small_button(tr("Remove zone")).clicked() {
                            remove = Some(i);
                        }
                    });
                    changed |= zone_ui(ui, i, z, set);
                }
                if let Some(i) = remove {
                    prefs.zones.remove(i);
                    changed = true;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Add zone"))
                        .on_hover_text(tr(
                            "Zones are applied in order; where they overlap, the later one blends over the earlier one.",
                        ))
                        .clicked()
                    {
                        let mut z = ContourZone::for_board(ContourKind::DoubleConcave, &doc.board);
                        z.inner_depth = 0.159;
                        prefs.zones.push(z);
                        changed = true;
                    }
                    ui.label(tr("Section spacing"));
                    if let Some(v) = length_field(ui, "c-spacing", prefs.spacing, set) {
                        prefs.spacing = v.clamp(2.0, 100.0);
                        changed = true;
                    }
                });
                if changed {
                    st.preview = None;
                    st.error = None;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Preview"))
                        .on_hover_text(tr("Shows the result as a dashed curve in the Section view and below."))
                        .clicked()
                    {
                        let mut b = doc.board.clone();
                        match shaping::apply_contours(&mut b, &prefs.zones, prefs.spacing) {
                            Ok(report) => {
                                st.preview = Some(Preview {
                                    tool: Tool::Contours,
                                    revision: doc.revision,
                                    board: b,
                                    report,
                                });
                                st.error = None;
                            }
                            Err(e) => st.error = Some(e),
                        }
                    }
                    if ui
                        .button(tr("Apply"))
                        .on_hover_text(tr("Changes the board as one undo step."))
                        .clicked()
                    {
                        let mut b = doc.board.clone();
                        match shaping::apply_contours(&mut b, &prefs.zones, prefs.spacing) {
                            Ok(r) => {
                                doc.edit(|d| *d = b);
                                st.preview = None;
                                st.error = None;
                                msg = Some(report_text(&r));
                            }
                            Err(e) => st.error = Some(e),
                        }
                    }
                });
                if let Some(e) = &st.error {
                    ui.colored_label(Color32::from_rgb(230, 80, 60), e);
                }
                if let Some(p) = st.preview.as_ref().filter(|p| p.tool == Tool::Contours) {
                    ui.label(report_text(&p.report));
                }
                let preview = st.preview.as_ref().filter(|p| p.tool == Tool::Contours);
                preview_canvas(ui, &mut st.contour_view, doc, ed, preview, set);
                ui.separator();
                ui.strong(tr("Remove contours"));
                egui::Grid::new("remove-contours").num_columns(2).show(ui, |ui| {
                    len_row(ui, tr("Start (from tail)"), "rm-start", &mut r0, 0.0, set);
                    len_row(ui, tr("End (from tail)"), "rm-end", &mut r1, 0.0, set);
                });
                if ui
                    .button(tr("Flatten the bottom"))
                    .on_hover_text(tr(
                        "Makes the bottom straight and level from the stringer to the first zone's run-out in every cross-section of this range (one undo step).",
                    ))
                    .clicked()
                {
                    let run_out = prefs.zones.first().map_or(RunOut::FromRail(2.54), |z| z.run_out);




                    let mut b = doc.board.clone();
                    match shaping::remove_contours(&mut b, r0, r1.max(r0 + 1.0), run_out) {
                        Ok(r) => {
                            doc.edit(|d| *d = b);
                            st.preview = None;
                            msg = Some(report_text(&r));
                        }
                        Err(e) => st.error = Some(e),
                    }
                }
            });
        });
    st.remove_range = Some((r0, r1));
    set.shaping = prefs;
    if !open {
        st.contours_open = false;
    }
    msg
}
