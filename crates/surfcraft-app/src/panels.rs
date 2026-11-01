//! Side panel: board specifications and the numeric knot editor.

use eframe::egui::{self, RichText};
use surfcraft_core::{DVec2, Handle, units::FOOT};

use crate::compute::Results;
use crate::document::Document;
use crate::edit::{self, CurveId};
use crate::i18n::{tr, trf};
use crate::settings::Settings;
use crate::views::EditorState;

pub fn specs(
    ui: &mut egui::Ui,
    doc: &Document,
    results: Option<&Results>,
    stale: bool,
    set: &Settings,
) {
    let b = &doc.board;
    let length = b.length();
    ui.heading(tr("Specs"));
    let f = |cm: f64| set.fmt(cm);
    fn row_styled(ui: &mut egui::Ui, name: RichText, value: RichText) {
        ui.label(name);
        ui.label(value.monospace());
        ui.end_row();
    }
    egui::Grid::new("specs")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, name: &str, value: String| {
                row_styled(ui, RichText::new(name), RichText::new(value))
            };
            row(ui, tr("Length"), f(length));
            row(ui, tr("Over bottom curve"), f(b.length_over_curve()));
            row(
                ui,
                tr("Wide point"),
                format!("{} @ {}", f(b.max_width()), f(b.max_width_pos())),
            );
            row(ui, tr("Width centre"), f(b.center_width()));
            row(ui, tr("Nose 1 ft"), f(b.width_at(length - FOOT)));
            row(ui, tr("Tail 1 ft"), f(b.width_at(FOOT)));
            // Values from the background worker: greyed with "…" while they are being recomputed.
            let computed = |ui: &mut egui::Ui, name: &str, value: String, strong: bool| {
                let mut text = RichText::new(if stale { format!("{value} …") } else { value });
                let mut label = RichText::new(name);
                if stale {
                    text = text.weak();
                } else if strong {
                    text = text.strong();
                    label = label.strong();
                }
                row_styled(ui, label, text);
            };
            match results {
                Some(r) => computed(
                    ui,
                    tr("Thickest"),
                    format!(
                        "{} @ {}",
                        f(r.specs.max_thickness),
                        f(r.specs.max_thickness_pos)
                    ),
                    false,
                ),
                None => row_styled(ui, RichText::new(tr("Thickest")), RichText::new("…")),
            }
            row(ui, tr("Thickness centre"), f(b.center_thickness()));
            row(ui, tr("Nose rocker"), f(b.rocker_at(length)));
            row(ui, tr("Tail rocker"), f(b.rocker_at(0.0)));
            row(ui, tr("Nose rocker 1 ft"), f(b.rocker_at(length - FOOT)));
            row(ui, tr("Tail rocker 1 ft"), f(b.rocker_at(FOOT)));
            match results {
                Some(r) => {
                    let area = r.specs.area;
                    let area_text = if set.length_unit().is_imperial() {
                        format!("{:.2} ft²", area / (30.48 * 30.48))
                    } else {
                        format!("{:.0} cm²", area)
                    };
                    computed(ui, tr("Planform area"), area_text, false);
                    computed(
                        ui,
                        tr("Centre of mass"),
                        trf("{0} from tail", &[&f(r.center_of_mass)]),
                        false,
                    );
                    computed(
                        ui,
                        tr("Volume"),
                        format!("{:.2} L", r.specs.volume / 1000.0),
                        true,
                    );
                }
                None => row_styled(
                    ui,
                    RichText::new(tr("Volume")),
                    RichText::new(tr("computing…")),
                ),
            }
        });
}

/// Numeric editing of the selected knot.
pub fn knot_inspector(ui: &mut egui::Ui, doc: &mut Document, ed: &mut EditorState, set: &Settings) {
    let Some(sel) = ed.selection.clone() else {
        ui.weak(tr("Click a control point to edit it numerically."));
        return;
    };
    let Some(k) = sel
        .primary()
        .and_then(|i| sel.curve.spline(&doc.board).and_then(|s| s.knots().get(i)))
        .cloned()
    else {
        ed.selection = None;
        return;
    };
    let index = sel.primary().unwrap_or(0);
    let mut heading = trf("{0} knot {1}", &[&tr(sel.curve.label()), &(index + 1)]);
    if sel.knots.len() > 1 {
        heading.push(' ');
        heading.push_str(&trf("(+{0} more)", &[&(sel.knots.len() - 1)]));
    }
    ui.label(heading);
    let (xl, yl) = match sel.curve {
        CurveId::Outline => (tr("from tail"), tr("half width")),
        CurveId::Bottom | CurveId::Deck => (tr("from tail"), tr("height")),
        CurveId::Section(_) => (tr("from stringer"), tr("height")),
    };
    egui::Grid::new("knot").num_columns(3).show(ui, |ui| {
        ui.label("");
        ui.label(xl);
        ui.label(yl);
        ui.end_row();
        for (name, which) in [
            (tr("Point"), Handle::Point),
            (tr("Tangent (prev)"), Handle::Prev),
            (tr("Tangent (next)"), Handle::Next),
        ] {
            let p = k.get(which);
            let active = sel.handle == which;
            ui.label(if active {
                RichText::new(name).strong()
            } else {
                RichText::new(name)
            });
            let nx = length_field(ui, ("kx", which as usize, index), p.x, set);
            let ny = length_field(ui, ("ky", which as usize, index), p.y, set);
            ui.end_row();
            if nx.is_some() || ny.is_some() {
                let target = DVec2::new(nx.unwrap_or(p.x), ny.unwrap_or(p.y));
                let curve = sel.curve;
                doc.edit(|b| edit::set_handle(b, curve, index, which, target));
            }
        }
    });
    let mut cont = k.continuous;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut cont, tr("Continuous (smooth)")).changed() {
            let curve = sel.curve;
            doc.edit(|b| edit::set_continuous(b, curve, index, cont));
        }
        let angle = (k.next() - k.point())
            .y
            .atan2((k.next() - k.point()).x)
            .to_degrees();
        ui.weak(trf("tangent {0}°", &[&format!("{angle:.1}")]));
    });
}

/// A length text field: shows the formatted value, returns a new value (cm) when the user commits an edit.
pub fn length_field(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: f64,
    set: &Settings,
) -> Option<f64> {
    let id = ui.scope_id().with(id);
    let mut text: String = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| set.fmt(value));
    let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(118.0));
    let mut result = None;
    if resp.has_focus() || resp.lost_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if resp.lost_focus() {
        // Commit only real edits: Enter on unchanged text must not round the value to the display precision.
        // Esc cancels.
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if !escape && text.trim() != set.fmt(value) {
            result = set
                .length_unit()
                .parse(&text)
                .filter(|v| v.is_finite() && (v - value).abs() > 1e-9);
        }
        ui.data_mut(|d| d.remove::<String>(id));
    }
    result
}
