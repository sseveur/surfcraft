//! Tool windows: weight calculator, guide-point table; background image for tracing.

use std::path::Path;

use eframe::egui::{self, Color32, TextureHandle};
use surfcraft_core::weight::{self, Layup, Weights};
use surfcraft_core::{Board, DVec2};

use crate::document::Document;
use crate::edit::CurveId;
use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;

// ------------------------------------------------------------------------------------------- weight

pub struct WeightState {
    pub layup: Layup,
    /// Board revision the cached result belongs to.
    revision: Option<u64>,
    result: Option<Weights>,
}

impl WeightState {
    pub fn new(board: &Board) -> Self {
        Self {
            layup: Layup::defaults_for(board),
            revision: None,
            result: None,
        }
    }
}

fn kg(set: &Settings, kg: f64) -> String {
    if set.length_unit().is_imperial() {
        format!("{:.2} lb", kg * surfcraft_core::units::POUNDS_PER_KG)
    } else {
        format!("{kg:.2} kg")
    }
}

pub fn weight_window(
    ctx: &egui::Context,
    state: &mut Option<WeightState>,
    doc: &Document,
    volume_cm3: Option<f64>,
    set: &Settings,
) {
    let mut open = state.is_some();
    if let Some(st) = state.as_mut() {
        egui::Window::new(tr("Weight calculator"))
            .id(egui::Id::unique("weight-window"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let l = &mut st.layup;
                let before = *l;
                egui::Grid::new("layup").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Stringer width"));
                    if let Some(v) = length_field(ui, "w-stringer", l.stringer_width, set) {
                        l.stringer_width = v;
                    }
                    ui.end_row();
                    ui.label(tr("Stringer density (kg/L)"));
                    ui.add(
                        egui::DragValue::new(&mut l.stringer_density)
                            .speed(0.01)
                            .range(0.05..=1.5),
                    );
                    ui.end_row();
                    ui.label(tr("Foam density (kg/L)"));
                    ui.add(
                        egui::DragValue::new(&mut l.foam_density)
                            .speed(0.001)
                            .range(0.01..=0.2),
                    );
                    ui.end_row();
                    ui.label(tr("Deck cloth (kg/m²)"));
                    ui.add(
                        egui::DragValue::new(&mut l.deck_glass)
                            .speed(0.005)
                            .range(0.0..=2.0),
                    );
                    ui.end_row();
                    ui.label(tr("Bottom cloth (kg/m²)"));
                    ui.add(
                        egui::DragValue::new(&mut l.bottom_glass)
                            .speed(0.005)
                            .range(0.0..=2.0),
                    );
                    ui.end_row();
                    ui.label(tr("Deck lap width"));
                    if let Some(v) = length_field(ui, "w-deck-lap", l.deck_lap_width, set) {
                        l.deck_lap_width = v;
                    }
                    ui.end_row();
                    ui.label(tr("Bottom lap width"));
                    if let Some(v) = length_field(ui, "w-bottom-lap", l.bottom_lap_width, set) {
                        l.bottom_lap_width = v;
                    }
                    ui.end_row();
                    ui.label(tr("Resin per glass weight"));
                    ui.add(
                        egui::DragValue::new(&mut l.resin_ratio)
                            .speed(0.05)
                            .range(0.2..=4.0),
                    );
                    ui.end_row();
                    ui.label(tr("Hot coat (kg)"));
                    ui.add(
                        egui::DragValue::new(&mut l.hotcoat)
                            .speed(0.01)
                            .range(0.0..=3.0),
                    );
                    ui.end_row();
                    ui.label(tr("Plugs and fins (kg)"));
                    ui.add(
                        egui::DragValue::new(&mut l.plugs_and_fins)
                            .speed(0.01)
                            .range(0.0..=3.0),
                    );
                    ui.end_row();
                });
                if ui.button(tr("Reset to defaults for this length")).clicked() {
                    *l = Layup::defaults_for(&doc.board);
                }
                let Some(volume) = volume_cm3 else {
                    ui.spinner();
                    return;
                };
                if st.revision != Some(doc.revision) || before != st.layup || st.result.is_none() {
                    st.result = Some(weight::estimate(&doc.board, volume, &st.layup));
                    st.revision = Some(doc.revision);
                }
                let w = st.result.unwrap_or_default();
                ui.separator();
                egui::Grid::new("weights")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for (name, v) in [
                            (tr("Stringer"), w.stringer),
                            (tr("Foam"), w.foam),
                            (tr("Deck glass"), w.deck_glass),
                            (tr("Deck lap"), w.deck_lap),
                            (tr("Bottom glass"), w.bottom_glass),
                            (tr("Bottom lap"), w.bottom_lap),
                            (tr("Resin"), w.resin),
                            (tr("Hot coat"), w.hotcoat),
                            (tr("Plugs and fins"), w.plugs_and_fins),
                        ] {
                            ui.label(name);
                            ui.monospace(kg(set, v));
                            ui.end_row();
                        }
                        ui.strong(tr("Total"));
                        ui.strong(kg(set, w.total));
                        ui.end_row();
                    });
                ui.weak(trf(
                    "Deck area {0} cm², bottom area {1} cm²",
                    &[
                        &format!("{:.0}", w.deck_glass_area),
                        &format!("{:.0}", w.bottom_glass_area),
                    ],
                ));
            });
    }
    if !open {
        *state = None;
    }
}

// ------------------------------------------------------------------------------------- guide points

pub fn guide_window(
    ctx: &egui::Context,
    open: &mut bool,
    curve: CurveId,
    doc: &mut Document,
    set: &Settings,
) {
    let mut guides = curve_guides(&doc.board, curve);
    let mut delete = None;
    egui::Window::new(tr("Guide points"))
        .id(egui::Id::unique("guides-window"))
        .open(open)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(trf(
                "{0} — reference points to fit the curve to",
                &[&tr(curve.label())],
            ));
            egui::Grid::new("guides")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("#");
                    ui.label("X");
                    ui.label("Y");
                    ui.label("");
                    ui.end_row();
                    for (i, g) in guides.iter_mut().enumerate() {
                        ui.label(format!("{}", i + 1));
                        if let Some(v) = length_field(ui, ("gx", i), g.x, set) {
                            g.x = v;
                        }
                        if let Some(v) = length_field(ui, ("gy", i), g.y, set) {
                            g.y = v;
                        }
                        if ui.button(tr("Delete")).clicked() {
                            delete = Some(i);
                        }
                        ui.end_row();
                    }
                });
            if ui.button(tr("Add guide point")).clicked() {
                let last = guides.last().copied().unwrap_or(DVec2::ZERO);
                guides.push(last + DVec2::new(10.0, 0.0));
            }
        });
    if let Some(i) = delete {
        guides.remove(i);
    }
    if guides != curve_guides(&doc.board, curve) {
        doc.edit(|b| {
            if let Some(g) = curve.guides_mut(b) {
                *g = guides;
            }
        });
    }
}

fn curve_guides(b: &Board, curve: CurveId) -> Vec<DVec2> {
    match curve {
        CurveId::Outline => b.outline_guides.clone(),
        CurveId::Bottom => b.bottom_guides.clone(),
        CurveId::Deck => b.deck_guides.clone(),
        CurveId::Section(i) => b
            .sections
            .get(i)
            .map(|s| s.guide_points.clone())
            .unwrap_or_default(),
    }
}

// --------------------------------------------------------------------------------- background image

/// A photo or drawing shown under the outline view. Its tail and nose (picked in image pixels) are mapped onto the
/// board's tail (0, 0) and nose (length, 0).
pub struct Background {
    pub texture: TextureHandle,
    pub size: DVec2,
    pub tail_px: DVec2,
    pub nose_px: DVec2,
    pub opacity: f32,
}

impl Background {
    pub fn load(ctx: &egui::Context, path: &Path) -> Result<Self, String> {
        let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = img.dimensions();
        let color =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let texture = ctx.load_texture("background", color, egui::TextureOptions::LINEAR);
        let size = DVec2::new(w as f64, h as f64);
        Ok(Self {
            texture,
            size,
            tail_px: DVec2::new(0.0, h as f64 / 2.0),
            nose_px: DVec2::new(w as f64, h as f64 / 2.0),
            opacity: 0.5,
        })
    }

    /// Image pixel (y down) to board world (cm, y up).
    pub fn to_world(&self, px: DVec2, length: f64) -> DVec2 {
        let a = self.nose_px - self.tail_px;
        let scale = length / a.length().max(1e-9);
        // Image y points down; flip to y up, then rotate so the tail->nose axis lies on +x.
        let flip = |v: DVec2| DVec2::new(v.x, -v.y);
        let axis = flip(a).normalize_or_zero();
        let v = flip(px - self.tail_px);
        DVec2::new(v.dot(axis), axis.perp_dot(v)) * scale
    }

    /// Board world to image pixel.
    pub fn to_pixel(&self, w: DVec2, length: f64) -> DVec2 {
        let a = self.nose_px - self.tail_px;
        let scale = length / a.length().max(1e-9);
        let axis = DVec2::new(a.x, -a.y).normalize_or_zero();
        let v = w / scale;
        let local = axis * v.x + axis.perp() * v.y;
        self.tail_px + DVec2::new(local.x, -local.y)
    }

    pub fn draw(
        &self,
        painter: &egui::Painter,
        to_screen: impl Fn(DVec2) -> egui::Pos2,
        length: f64,
    ) {
        let corners = [
            DVec2::ZERO,
            DVec2::new(self.size.x, 0.0),
            self.size,
            DVec2::new(0.0, self.size.y),
        ];
        let uv = [
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ];
        let tint = Color32::from_white_alpha((self.opacity * 255.0) as u8);
        let mut mesh = egui::Mesh::with_texture(self.texture.id());
        for (c, t) in corners.iter().zip(uv) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: to_screen(self.to_world(*c, length)),
                uv: t,
                color: tint,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_mapping_round_trips() {
        let ctx = egui::Context::default();
        let tex = ctx.load_texture(
            "t",
            egui::ColorImage::filled([4, 4], Color32::WHITE),
            Default::default(),
        );
        let bg = Background {
            texture: tex,
            size: DVec2::new(400.0, 100.0),
            tail_px: DVec2::new(20.0, 60.0),
            nose_px: DVec2::new(380.0, 40.0),
            opacity: 0.5,
        };
        let l = 180.0;
        assert!((bg.to_world(bg.tail_px, l)).length() < 1e-9);
        assert!((bg.to_world(bg.nose_px, l) - DVec2::new(l, 0.0)).length() < 1e-9);
        let w = DVec2::new(50.0, 12.0);
        assert!((bg.to_world(bg.to_pixel(w, l), l) - w).length() < 1e-9);
    }
}
