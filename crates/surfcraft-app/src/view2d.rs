//! 2D canvas: world (cm, y up) to screen transform, zoom/pan, drawing helpers.

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2, pos2};
use surfcraft_core::{DVec2, Spline};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Screen position of world origin.
    pub origin: Pos2,
    /// Pixels per cm.
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            origin: Pos2::ZERO,
            scale: 1.0,
        }
    }
}

impl Transform {
    pub fn to_screen(self, p: DVec2) -> Pos2 {
        pos2(
            self.origin.x + p.x as f32 * self.scale,
            self.origin.y - p.y as f32 * self.scale,
        )
    }

    pub fn to_world(self, p: Pos2) -> DVec2 {
        DVec2::new(
            ((p.x - self.origin.x) / self.scale) as f64,
            ((self.origin.y - p.y) / self.scale) as f64,
        )
    }

    /// Screen pixels to world length.
    pub fn len_to_world(self, px: f32) -> f64 {
        (px / self.scale) as f64
    }

    pub fn fit(rect: Rect, min: DVec2, max: DVec2, margin: f32) -> Self {
        let size = (max - min).max(DVec2::splat(1e-3));
        let avail = (rect.size() - Vec2::splat(2.0 * margin)).max(Vec2::splat(10.0));
        let scale = (avail.x / size.x as f32).min(avail.y / size.y as f32);
        let centre = (min + max) / 2.0;
        let c = rect.center();
        Self {
            origin: pos2(c.x - centre.x as f32 * scale, c.y + centre.y as f32 * scale),
            scale,
        }
    }

    /// Zoom by `factor` keeping the world point under `anchor` fixed.
    pub fn zoom_about(&mut self, anchor: Pos2, factor: f32) {
        let factor = factor.clamp(0.2, 5.0);
        let new_scale = (self.scale * factor).clamp(0.05, 2000.0);
        let f = new_scale / self.scale;
        self.origin = anchor + (self.origin - anchor) * f;
        self.scale = new_scale;
    }
}

/// Per-view persistent state.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewState {
    pub xf: Transform,
    /// Fit to content on next frame.
    pub needs_fit: bool,
    pub initialised: bool,
    /// Screen rect the transform was last laid out for. The same view is shown in the overview and in its own
    /// tab, and the window can be resized, so the rect changes under a fitted transform.
    pub rect: Option<Rect>,
    /// The user zoomed or panned since the last fit: keep their framing (re-centred) instead of re-fitting.
    pub user_moved: bool,
    /// Size of the rect at the last fit.
    fit_size: Vec2,
}

impl ViewState {
    /// Keep the drawing centred when the view's rect changes. A big change (overview cell to full tab, window
    /// resize) re-fits an untouched view; anything else keeps the zoom and the world point at the centre, so the
    /// side panel growing by a few pixels when a point is selected does not make the drawing jump.
    fn follow_rect(&mut self, rect: Rect, bounds: (DVec2, DVec2)) {
        if !self.initialised || self.needs_fit {
            self.fit(rect, bounds);
        } else if self.rect != Some(rect) {
            let ratio = rect.size() / self.fit_size.max(Vec2::splat(1.0));
            let big = !(0.85..=1.15).contains(&ratio.x) || !(0.85..=1.15).contains(&ratio.y);
            if big && !self.user_moved {
                self.fit(rect, bounds);
            } else if let Some(old) = self.rect {
                self.xf.origin += rect.center() - old.center();
            }
        }
        self.rect = Some(rect);
    }

    fn fit(&mut self, rect: Rect, bounds: (DVec2, DVec2)) {
        self.xf = Transform::fit(rect, bounds.0, bounds.1, 24.0);
        self.initialised = true;
        self.needs_fit = false;
        self.user_moved = false;
        self.fit_size = rect.size();
    }
}

pub struct Canvas {
    pub painter: egui::Painter,
    pub response: egui::Response,
    pub rect: Rect,
    pub xf: Transform,
    /// Spacing (cm) of the grid drawn this frame, for grid snapping; `None` when no grid is shown.
    pub grid_step: std::cell::Cell<Option<f64>>,
}

impl Canvas {
    /// Allocate the remaining space, handle zoom (wheel / pinch) and pan (middle or secondary drag, or space+drag).
    pub fn show(
        ui: &mut egui::Ui,
        state: &mut ViewState,
        bounds: (DVec2, DVec2),
        bg: Color32,
    ) -> Canvas {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, bg);
        state.follow_rect(rect, bounds);
        if response.hovered() {
            let (scroll, zoom, hover) = ui.input(|i| {
                (
                    i.smooth_scroll_delta(),
                    i.zoom_delta(),
                    i.pointer.hover_pos(),
                )
            });
            if let Some(h) = hover {
                let mut factor = zoom;
                if scroll.y != 0.0 {
                    factor *= (scroll.y * 0.0025).exp();
                }
                if factor != 1.0 {
                    state.xf.zoom_about(h, factor);
                    state.user_moved = true;
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (space && response.dragged_by(egui::PointerButton::Primary))
        {
            let d = response.drag_delta();
            if d != Vec2::ZERO {
                state.xf.origin += d;
                state.user_moved = true;
            }
        }
        Canvas {
            painter: painter.with_clip_rect(rect),
            response,
            rect,
            xf: state.xf,
            grid_step: Default::default(),
        }
    }

    pub fn polyline(&self, pts: impl IntoIterator<Item = DVec2>, stroke: Stroke) {
        let pts: Vec<Pos2> = pts.into_iter().map(|p| self.xf.to_screen(p)).collect();
        if pts.len() >= 2 {
            self.painter.add(Shape::line(pts, stroke));
        }
    }

    pub fn dashed(&self, pts: impl IntoIterator<Item = DVec2>, stroke: Stroke) {
        let pts: Vec<Pos2> = pts.into_iter().map(|p| self.xf.to_screen(p)).collect();
        if pts.len() >= 2 {
            self.painter
                .extend(Shape::dashed_line(&pts, stroke, 6.0, 4.0));
        }
    }

    pub fn segment(&self, a: DVec2, b: DVec2, stroke: Stroke) {
        self.painter
            .line_segment([self.xf.to_screen(a), self.xf.to_screen(b)], stroke);
    }

    pub fn text(&self, at: DVec2, anchor: egui::Align2, text: impl ToString, color: Color32) {
        self.painter.text(
            self.xf.to_screen(at),
            anchor,
            text,
            egui::FontId::proportional(12.0),
            color,
        );
    }

    /// Grid every `step` cm with a stronger line every `major` steps.
    pub fn grid(&self, step: f64, major: usize, color: Color32) {
        let min = self.xf.to_world(self.rect.left_bottom());
        let max = self.xf.to_world(self.rect.right_top());
        // Coarsen when lines would be closer than 6 px.
        let mut step = step;
        while self.xf.scale as f64 * step < 6.0 {
            step *= 10.0;
        }
        self.grid_step.set(Some(step));
        let faint = Stroke::new(1.0, color.gamma_multiply(0.45));
        let strong = Stroke::new(1.0, color);
        let i0 = (min.x / step).floor() as i64;
        let i1 = (max.x / step).ceil() as i64;
        for i in i0..=i1 {
            let x = i as f64 * step;
            let s = if i.rem_euclid(major as i64) == 0 {
                strong
            } else {
                faint
            };
            self.segment(DVec2::new(x, min.y), DVec2::new(x, max.y), s);
        }
        let j0 = (min.y / step).floor() as i64;
        let j1 = (max.y / step).ceil() as i64;
        for j in j0..=j1 {
            let y = j as f64 * step;
            let s = if j.rem_euclid(major as i64) == 0 {
                strong
            } else {
                faint
            };
            self.segment(DVec2::new(min.x, y), DVec2::new(max.x, y), s);
        }
    }
}

/// Sample a spline into points (optionally mapped, e.g. mirrored).
pub fn sample(spline: &Spline, per_segment: usize, map: impl Fn(DVec2) -> DVec2) -> Vec<DVec2> {
    let mut pts = Vec::with_capacity(spline.segment_count() * per_segment + 1);
    for (i, c) in spline.segments().enumerate() {
        let start = if i == 0 { 0 } else { 1 };
        for j in start..=per_segment {
            pts.push(map(c.point(j as f64 / per_segment as f64)));
        }
    }
    pts
}

pub fn ident(p: DVec2) -> DVec2 {
    p
}

pub fn mirror_y(p: DVec2) -> DVec2 {
    DVec2::new(p.x, -p.y)
}

pub fn mirror_x(p: DVec2) -> DVec2 {
    DVec2::new(-p.x, p.y)
}
