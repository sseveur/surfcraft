//! Shaded 3D preview drawn with egui's painter (CPU shading, depth-sorted triangles).

use eframe::egui::{self, Color32, Pos2, Sense, epaint};
use surfcraft_core::{DVec3, Mesh};

use crate::i18n::tr;

pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub wireframe: bool,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: -0.6,
            pitch: 0.45,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            wireframe: false,
        }
    }
}

impl Orbit {
    pub fn reset(&mut self) {
        let wireframe = self.wireframe;
        *self = Self::default();
        self.wireframe = wireframe;
    }
}

/// Board coordinates -> view space (x right, y up, z toward viewer).
fn rotate(p: DVec3, yaw: f32, pitch: f32) -> DVec3 {
    // Board: x along, y across, z up. Start with length horizontal on screen.
    let (sy, cy) = (yaw as f64).sin_cos();
    let (sp, cp) = (pitch as f64).sin_cos();
    // yaw about the up axis (z)
    let x1 = p.x * cy - p.y * sy;
    let y1 = p.x * sy + p.y * cy;
    let z1 = p.z;
    // pitch about the screen x axis
    let up = z1 * cp - y1 * sp;
    let depth = z1 * sp + y1 * cp;
    DVec3::new(x1, up, depth)
}

pub fn show(
    ui: &mut egui::Ui,
    mesh: Option<&Mesh>,
    orbit: &mut Orbit,
    bg: Color32,
    base: Color32,
    busy: bool,
) {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, bg);
    if response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        orbit.yaw -= d.x * 0.01;
        orbit.pitch = (orbit.pitch + d.y * 0.01).clamp(-1.55, 1.55);
    }
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        orbit.pan += response.drag_delta();
    }
    if response.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta(), i.zoom_delta()));
        orbit.zoom = (orbit.zoom * zoom * (scroll.y * 0.0025).exp()).clamp(0.2, 20.0);
    }
    if response.double_clicked() {
        orbit.reset();
    }
    let Some(mesh) = mesh.filter(|m| !m.triangles.is_empty()) else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if busy {
                tr("Computing…")
            } else {
                tr("No 3D model")
            },
            egui::FontId::proportional(14.0),
            Color32::GRAY,
        );
        return;
    };

    let pts: Vec<DVec3> = mesh
        .positions
        .iter()
        .map(|&p| rotate(p, orbit.yaw, orbit.pitch))
        .collect();
    let (mut min, mut max) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for p in &pts {
        min = min.min(*p);
        max = max.max(*p);
    }
    let centre = (min + max) / 2.0;
    let extent = (max - min).max(DVec3::splat(1e-3));
    let scale = (rect.width() as f64 * 0.9 / extent.x).min(rect.height() as f64 * 0.9 / extent.y)
        * orbit.zoom as f64;
    let origin = rect.center() + orbit.pan;
    let screen = |p: DVec3| -> Pos2 {
        egui::pos2(
            origin.x + ((p.x - centre.x) * scale) as f32,
            origin.y - ((p.y - centre.y) * scale) as f32,
        )
    };

    let light = DVec3::new(-0.3, 0.6, 0.75).normalize();
    let mut order: Vec<(f64, usize)> = mesh
        .triangles
        .iter()
        .enumerate()
        .map(|(i, t)| (t.iter().map(|&v| pts[v as usize].z).sum::<f64>(), i))
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut out = epaint::Mesh::default();
    let mut lines = Vec::new();
    for (_, i) in order {
        let t = mesh.triangles[i];
        let [a, b, c] = t.map(|v| pts[v as usize]);
        let n = (b - a).cross(c - a).normalize_or_zero();
        // Two-sided lighting; slightly darker on faces pointing away.
        let facing = n.z;
        let lambert = n.dot(light).abs();
        let shade = (0.25 + 0.75 * lambert) * if facing < 0.0 { 0.85 } else { 1.0 };
        let col = Color32::from_rgb(
            (base.r() as f64 * shade) as u8,
            (base.g() as f64 * shade) as u8,
            (base.b() as f64 * shade) as u8,
        );
        let idx = out.vertices.len() as u32;
        for p in [a, b, c] {
            out.colored_vertex(screen(p), col);
        }
        out.add_triangle(idx, idx + 1, idx + 2);
        if orbit.wireframe {
            lines.push([screen(a), screen(b)]);
            lines.push([screen(b), screen(c)]);
        }
    }
    painter.add(egui::Shape::mesh(out));
    for l in lines {
        painter.line_segment(l, egui::Stroke::new(0.5, Color32::from_black_alpha(90)));
    }
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        tr("drag: rotate · right-drag: pan · wheel: zoom · double-click: reset"),
        egui::FontId::proportional(11.0),
        Color32::GRAY,
    );
}
