//! Snapping in the 2D views: a dragged or placed point jumps to a nearby control point, guide point, curve, grid
//! line or the drag's start axis. Everything here works in display coordinates (the profile's vertical
//! exaggeration applied), so the snap radius is the same number of screen pixels in every direction.

use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};
use surfcraft_core::{Board, DVec2, Spline};

use crate::edit::CurveId;
use crate::i18n::tr;
use crate::view2d::Canvas;

/// How close (screen pixels) a target must be to catch the point.
pub const SNAP_RADIUS_PX: f32 = 8.0;

/// Snapping preferences (View > Snap), persisted with the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapSettings {
    /// Master switch (toolbar "Snap").
    pub enabled: bool,
    pub grid: bool,
    pub knots: bool,
    /// Keep a dragged point on the x or y it started from when it comes close to it.
    pub axis: bool,
    pub guides: bool,
}

impl SnapSettings {
    /// Some snapping is switched on.
    pub fn any(&self) -> bool {
        self.enabled && (self.grid || self.knots || self.axis || self.guides)
    }

    /// Toolbar toggle: switching on with no target chosen picks control points and grid.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if self.enabled && !(self.grid || self.knots || self.axis || self.guides) {
            self.knots = true;
            self.grid = true;
        }
    }
}

/// What a point snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Knot,
    Guide,
    Curve,
    Grid,
    Axis,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Knot => tr("Snap: control point"),
            SnapKind::Guide => tr("Snap: guide point"),
            SnapKind::Curve => tr("Snap: curve"),
            SnapKind::Grid => tr("Snap: grid"),
            SnapKind::Axis => tr("Snap: axis"),
        }
    }
}

/// The 2D view a curve belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Outline,
    Profile,
    Section(usize),
}

impl Pane {
    pub fn of(id: CurveId) -> Self {
        match id {
            CurveId::Outline => Pane::Outline,
            CurveId::Deck | CurveId::Bottom => Pane::Profile,
            CurveId::Section(i) => Pane::Section(i),
        }
    }
}

/// One snapping request, all in display coordinates.
#[derive(Clone, Copy, Debug, Default)]
pub struct Query<'a> {
    pub raw: DVec2,
    pub radius: f64,
    pub knots: &'a [DVec2],
    pub guides: &'a [DVec2],
    /// Nearest point on a curve, when curves are targets.
    pub curve: Option<DVec2>,
    /// Grid spacing, when the grid is a target.
    pub grid: Option<f64>,
    /// Snap onto this point's x or y when close to it (Axis toggle).
    pub axis_from: Option<DVec2>,
    /// Hard lock (Shift): keep this point's y for a mostly horizontal move, its x for a vertical one.
    pub lock_from: Option<DVec2>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapped {
    pub pos: DVec2,
    pub kind: Option<SnapKind>,
    /// What caught the point, for the indicator.
    pub target: Option<DVec2>,
}

/// Resolve a snap: the nearest control point within the radius wins, then guide points, then the curve, then the
/// start axis and the grid (each coordinate on its own).
pub fn resolve(q: &Query) -> Snapped {
    let r = q.radius;
    let mut p = q.raw;
    let (mut free_x, mut free_y) = (true, true);
    if let Some(o) = q.lock_from {
        let d = p - o;
        if d.x.abs() >= d.y.abs() {
            p.y = o.y;
            free_y = false;
        } else {
            p.x = o.x;
            free_x = false;
        }
    }
    let dist = |c: DVec2| match (free_x, free_y) {
        (true, true) => c.distance(p),
        (true, false) => (c.x - p.x).abs(),
        _ => (c.y - p.y).abs(),
    };
    let curve: Vec<DVec2> = q.curve.into_iter().collect();
    for (kind, list) in [
        (SnapKind::Knot, q.knots),
        (SnapKind::Guide, q.guides),
        (SnapKind::Curve, &curve[..]),
    ] {
        let best = list
            .iter()
            .map(|&c| (dist(c), c))
            .filter(|(d, _)| *d <= r)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, c)) = best {
            let pos = DVec2::new(
                if free_x { c.x } else { p.x },
                if free_y { c.y } else { p.y },
            );
            return Snapped {
                pos,
                kind: Some(kind),
                target: Some(c),
            };
        }
    }
    let one = |v: f64, origin: Option<f64>| -> (f64, Option<SnapKind>) {
        if let Some(o) = origin
            && (v - o).abs() <= r
        {
            return (o, Some(SnapKind::Axis));
        }
        if let Some(g) = q.grid.filter(|g| *g > 0.0) {
            let s = (v / g).round() * g;
            if (v - s).abs() <= r {
                return (s, Some(SnapKind::Grid));
            }
        }
        (v, None)
    };
    let mut kinds = Vec::new();
    if free_x {
        let (x, k) = one(p.x, q.axis_from.map(|o| o.x));
        p.x = x;
        kinds.extend(k);
    }
    if free_y {
        let (y, k) = one(p.y, q.axis_from.map(|o| o.y));
        p.y = y;
        kinds.extend(k);
    }
    let kind = if q.lock_from.is_some() || kinds.contains(&SnapKind::Axis) {
        Some(SnapKind::Axis)
    } else {
        kinds.first().copied()
    };
    Snapped {
        pos: p,
        kind,
        target: kind.map(|_| p),
    }
}

/// Snap targets of one view for this frame, owned so the board can be edited while they are in use.
pub struct ViewSnap {
    pub pane: Pane,
    /// Display y per board y (profile exaggeration).
    pub y_scale: f64,
    /// Snapping switched on and Alt (fine drag) not held.
    active: bool,
    settings: SnapSettings,
    /// Control points (display coordinates) with their owner, mirrored copies included.
    knots: Vec<(CurveId, usize, DVec2)>,
    guides: Vec<DVec2>,
    /// The view's curves in display coordinates, mirrored copies included.
    curves: Vec<Spline>,
    grid: Option<f64>,
    radius: f64,
    xf: crate::view2d::Transform,
}

impl ViewSnap {
    pub fn new(
        ui: &egui::Ui,
        canvas: &Canvas,
        board: &Board,
        pane: Pane,
        y_scale: f64,
        settings: SnapSettings,
    ) -> Self {
        let alt = ui.input(|i| i.modifiers.alt);
        let disp = |p: DVec2| DVec2::new(p.x, p.y * y_scale);
        let (ids, mirror): (Vec<CurveId>, Option<DVec2>) = match pane {
            Pane::Outline => (vec![CurveId::Outline], Some(DVec2::new(1.0, -1.0))),
            Pane::Profile => (vec![CurveId::Deck, CurveId::Bottom], None),
            Pane::Section(i) => (vec![CurveId::Section(i)], Some(DVec2::new(-1.0, 1.0))),
        };
        let mut knots = Vec::new();
        let mut curves = Vec::new();
        for id in ids {
            let Some(s) = id.spline(board) else {
                continue;
            };
            for (i, k) in s.knots().iter().enumerate() {
                let p = disp(k.point());
                knots.push((id, i, p));
                if let Some(m) = mirror {
                    knots.push((id, i, p * m));
                }
            }
            let mut d = s.clone();
            d.scale(1.0, y_scale);
            if let Some(m) = mirror {
                let mut c = d.clone();
                c.scale(m.x, m.y);
                curves.push(c);
            }
            curves.push(d);
        }
        let guides: Vec<DVec2> = match pane {
            Pane::Outline => board.outline_guides.clone(),
            Pane::Profile => [&board.deck_guides[..], &board.bottom_guides[..]].concat(),
            Pane::Section(i) => board
                .sections
                .get(i)
                .map(|s| s.guide_points.clone())
                .unwrap_or_default(),
        }
        .into_iter()
        .map(disp)
        .collect();
        Self {
            pane,
            y_scale,
            active: settings.enabled && !alt,
            settings,
            knots,
            guides,
            curves,
            grid: canvas.grid_step.get(),
            radius: canvas.xf.len_to_world(SNAP_RADIUS_PX),
            xf: canvas.xf,
        }
    }

    /// Board coordinates to display coordinates.
    pub fn disp(&self, p: DVec2) -> DVec2 {
        DVec2::new(p.x, p.y * self.y_scale)
    }

    /// Display coordinates to board coordinates.
    pub fn board(&self, p: DVec2) -> DVec2 {
        DVec2::new(p.x, p.y / self.y_scale)
    }

    /// Snapping would change a drag (some target on and Alt not held).
    pub fn drag_snaps(&self) -> bool {
        self.active && self.settings.any()
    }

    /// Nearest point (display coordinates) on the view's curves and its distance in screen pixels.
    pub fn nearest_curve_point(&self, p: DVec2) -> Option<(DVec2, f32)> {
        self.curves
            .iter()
            .filter_map(|s| {
                let (i, t, d) = s.nearest(p)?;
                Some((s.segment(i).point(t), d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(c, d)| (c, d as f32 * self.xf.scale))
    }

    /// Snap `raw` (board coordinates). `exclude`: control points that must not catch it (those being dragged);
    /// `origin`: where a drag started (axis snapping, and the Shift lock when `lock`); `curve`: also snap onto
    /// the curves (Measure tool). The result is in board coordinates.
    pub fn snap(
        &self,
        raw: DVec2,
        exclude: &dyn Fn(CurveId, usize) -> bool,
        origin: Option<DVec2>,
        lock: bool,
        curve: bool,
    ) -> Snapped {
        let s = &self.settings;
        let on = self.active;
        let knots: Vec<DVec2> = if on && s.knots {
            self.knots
                .iter()
                .filter(|(c, i, _)| !exclude(*c, *i))
                .map(|k| k.2)
                .collect()
        } else {
            Vec::new()
        };
        let raw = self.disp(raw);
        let curve = if on && s.knots && curve {
            self.nearest_curve_point(raw).map(|c| c.0)
        } else {
            None
        };
        let origin = origin.map(|o| self.disp(o));
        let r = resolve(&Query {
            raw,
            radius: self.radius,
            knots: &knots,
            guides: if on && s.guides { &self.guides } else { &[] },
            curve,
            grid: self.grid.filter(|_| on && s.grid),
            axis_from: origin.filter(|_| on && s.axis),
            lock_from: origin.filter(|_| lock),
        });
        Snapped {
            pos: self.board(r.pos),
            ..r
        }
    }

    /// Ring and crosshair on the snap target.
    pub fn draw(&self, canvas: &Canvas, s: &Snapped, color: Color32) {
        let (Some(_), Some(t)) = (s.kind, s.target) else {
            return;
        };
        let c = canvas.xf.to_screen(t);
        let st = Stroke::new(1.5, color);
        canvas.painter.circle_stroke(c, SNAP_RADIUS_PX, st);
        let a = SNAP_RADIUS_PX + 4.0;
        canvas
            .painter
            .line_segment([c - egui::vec2(a, 0.0), c + egui::vec2(a, 0.0)], st);
        canvas
            .painter
            .line_segment([c - egui::vec2(0.0, a), c + egui::vec2(0.0, a)], st);
    }
}

/// View > Snap submenu.
pub fn menu(ui: &mut egui::Ui, s: &mut SnapSettings) {
    ui.menu_button(tr("Snap"), |ui| {
        ui.checkbox(&mut s.enabled, tr("Snapping on"))
            .on_hover_text(tr(
                "Hold Alt while dragging for a fine move without snapping.",
            ));
        ui.separator();
        let before = (s.grid, s.knots, s.axis, s.guides);
        ui.checkbox(&mut s.grid, tr("Grid"));
        ui.checkbox(&mut s.knots, tr("Control points"));
        ui.checkbox(&mut s.guides, tr("Guide points"));
        ui.checkbox(&mut s.axis, tr("Axis (start x or y)"))
            .on_hover_text(tr(
                "A dragged point stays on the x or y it started from when it comes close. Holding Shift while dragging always keeps it there.",
            ));
        // Choosing a target switches snapping on.
        if (s.grid, s.knots, s.axis, s.guides) != before {
            s.enabled = true;
        }
    });
}

/// Toolbar toggle for all snapping.
pub fn toolbar(ui: &mut egui::Ui, s: &mut SnapSettings) -> egui::Response {
    let r = ui
        .selectable_label(s.enabled, tr("Snap"))
        .on_hover_text(tr(
            "Snap dragged and placed points to control points, guide points or the grid (choose in View > Snap). Hold Alt while dragging for a fine move without snapping; Shift keeps the start x or y.",
        ));
    if r.clicked() {
        s.toggle();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(raw: DVec2) -> Query<'static> {
        Query {
            raw,
            radius: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn nothing_on_keeps_the_point() {
        let p = DVec2::new(3.3, 4.4);
        let s = resolve(&q(p));
        assert_eq!(s.pos, p);
        assert_eq!(s.kind, None);
    }

    #[test]
    fn nearest_candidate_within_the_radius() {
        let knots = [
            DVec2::new(10.0, 0.0),
            DVec2::new(10.6, 0.0),
            DVec2::new(10.0, 5.0),
        ];
        let s = resolve(&Query {
            knots: &knots,
            ..q(DVec2::new(10.5, 0.2))
        });
        assert_eq!(s.pos, knots[1]);
        assert_eq!(s.kind, Some(SnapKind::Knot));
        // Out of reach: unchanged.
        let s = resolve(&Query {
            knots: &knots,
            ..q(DVec2::new(20.0, 0.0))
        });
        assert_eq!(s.pos, DVec2::new(20.0, 0.0));
        assert_eq!(s.kind, None);
    }

    #[test]
    fn priority_knot_then_guide_then_curve_then_grid() {
        let raw = DVec2::new(0.3, 0.3);
        let knots = [DVec2::new(0.9, 0.9)];
        let guides = [DVec2::new(0.35, 0.3)];
        let base = Query {
            grid: Some(1.0),
            curve: Some(DVec2::new(0.3, 0.31)),
            ..q(raw)
        };
        // The knot is farther than the guide but wins.
        let s = resolve(&Query {
            knots: &knots,
            guides: &guides,
            ..base
        });
        assert_eq!((s.pos, s.kind), (knots[0], Some(SnapKind::Knot)));
        let s = resolve(&Query {
            guides: &guides,
            ..base
        });
        assert_eq!((s.pos, s.kind), (guides[0], Some(SnapKind::Guide)));
        let s = resolve(&base);
        assert_eq!(s.kind, Some(SnapKind::Curve));
        let s = resolve(&Query {
            curve: None,
            ..base
        });
        assert_eq!((s.pos, s.kind), (DVec2::ZERO, Some(SnapKind::Grid)));
    }

    #[test]
    fn grid_snaps_each_coordinate_within_the_radius() {
        let s = resolve(&Query {
            grid: Some(2.54),
            radius: 0.5,
            ..q(DVec2::new(5.2, 3.9))
        });
        assert!((s.pos.x - 5.08).abs() < 1e-9);
        // 3.9 is 1.36 from 2.54 and 1.18 from 5.08: out of reach.
        assert_eq!(s.pos.y, 3.9);
        assert_eq!(s.kind, Some(SnapKind::Grid));
    }

    #[test]
    fn axis_lock_and_axis_snap() {
        let o = DVec2::new(10.0, 5.0);
        // Shift: a mostly horizontal move keeps y, a mostly vertical one keeps x.
        let s = resolve(&Query {
            lock_from: Some(o),
            ..q(DVec2::new(30.0, 8.0))
        });
        assert_eq!(s.pos, DVec2::new(30.0, 5.0));
        assert_eq!(s.kind, Some(SnapKind::Axis));
        let s = resolve(&Query {
            lock_from: Some(o),
            ..q(DVec2::new(12.0, -20.0))
        });
        assert_eq!(s.pos, DVec2::new(10.0, -20.0));
        // Locked: a knot only moves the free coordinate.
        let knots = [DVec2::new(30.5, 40.0)];
        let s = resolve(&Query {
            lock_from: Some(o),
            knots: &knots,
            ..q(DVec2::new(30.0, 8.0))
        });
        assert_eq!(s.pos, DVec2::new(30.5, 5.0));
        // Axis toggle: only when close to the start x or y.
        let s = resolve(&Query {
            axis_from: Some(o),
            ..q(DVec2::new(10.4, 20.0))
        });
        assert_eq!(s.pos, DVec2::new(10.0, 20.0));
        assert_eq!(s.kind, Some(SnapKind::Axis));
        let s = resolve(&Query {
            axis_from: Some(o),
            ..q(DVec2::new(14.0, 20.0))
        });
        assert_eq!(s.kind, None);
    }

    #[test]
    fn toolbar_toggle_picks_targets_when_none() {
        let mut s = SnapSettings::default();
        assert!(!s.any());
        s.toggle();
        assert!(s.enabled && s.knots && s.grid && s.any());
        s.toggle();
        assert!(!s.any());
        s.toggle();
        assert!(s.knots && s.grid, "kept");
    }

    #[test]
    fn settings_load_without_snap_fields() {
        let s: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.snap, SnapSettings::default());
    }
}
