//! Measure tool: dimension lines between two points of a 2D view (distance, dx, dy, angle) and a read-out of the
//! board at the pointer (width, rocker, thickness) the way shapers read a board. View annotations only: nothing is
//! saved in the board and nothing goes on the undo stack.

use std::f64::consts::FRAC_PI_4;

use eframe::egui::{self, Align2, Color32, Pos2, Stroke};
use surfcraft_core::{Board, DVec2};

use crate::i18n::{tr, trf};
use crate::settings::Settings;
use crate::snap::{Pane, SnapKind, ViewSnap};
use crate::view2d::Canvas;
use crate::views::Palette;

/// Measurements kept on screen with Pin.
pub const MAX_PINNED: usize = 5;
/// The pointer must be this close (screen pixels) to a curve for the read-out.
const READOUT_PX: f32 = 30.0;

/// A dimension between two points, in board coordinates (cm, no profile exaggeration).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub pane: Pane,
    pub a: DVec2,
    pub b: DVec2,
}

impl Measurement {
    pub fn distance(&self) -> f64 {
        self.a.distance(self.b)
    }

    pub fn angle(&self) -> f64 {
        angle_from_horizontal(self.b - self.a)
    }

    pub fn label(&self, set: &Settings) -> String {
        let d = self.b - self.a;
        trf(
            "Distance {0}  dx {1}  dy {2}  angle {3}°",
            &[
                &set.fmt(self.distance()),
                &set.fmt(d.x.abs()),
                &set.fmt(d.y.abs()),
                &format!("{:.1}", self.angle()),
            ],
        )
    }
}

/// Angle of `d` from the horizontal in degrees, 0 to 90 whatever the direction.
pub fn angle_from_horizontal(d: DVec2) -> f64 {
    if d.length() < 1e-12 {
        0.0
    } else {
        d.y.abs().atan2(d.x.abs()).to_degrees()
    }
}

/// `b` moved onto the nearest horizontal, vertical or 45° line through `a` (Shift).
pub fn constrain_45(a: DVec2, b: DVec2) -> DVec2 {
    let d = b - a;
    if d.length() < 1e-12 {
        return b;
    }
    let q = (d.y.atan2(d.x) / FRAC_PI_4).round() * FRAC_PI_4;
    let dir = DVec2::new(q.cos(), q.sin());
    a + dir * d.dot(dir)
}

/// What a shaper reads off the board at the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Readout {
    /// Station `x` from the tail: distance from the nose and full width there.
    Outline { x: f64, from_nose: f64, width: f64 },
    /// Station `x`: bottom rocker, deck height and thickness on the stringer.
    Profile {
        x: f64,
        rocker: f64,
        deck: f64,
        thickness: f64,
    },
    /// `u` from the stringer on the current section (`side` -1 or 1): deck and bottom heights and thickness.
    Section {
        u: f64,
        side: f64,
        deck: f64,
        bottom: f64,
        thickness: f64,
    },
}

/// The read-out at `p` (board coordinates) in `pane`.
pub fn readout(board: &Board, pane: Pane, p: DVec2) -> Option<Readout> {
    let length = board.length();
    match pane {
        Pane::Outline => {
            let x = p.x.clamp(0.0, length);
            Some(Readout::Outline {
                x,
                from_nose: length - x,
                width: board.width_at(x),
            })
        }
        Pane::Profile => {
            let x = p.x.clamp(0.0, length);
            Some(Readout::Profile {
                x,
                rocker: board.rocker_at(x),
                deck: board.deck_at(x),
                thickness: board.thickness_at(x),
            })
        }
        Pane::Section(i) => {
            let cs = board.sections.get(i)?;
            let u = p.x.abs().min(cs.width() / 2.0);
            let (deck, bottom) = (cs.deck_at(u), cs.bottom_at(u));
            Some(Readout::Section {
                u,
                side: if p.x < 0.0 { -1.0 } else { 1.0 },
                deck,
                bottom,
                thickness: deck - bottom,
            })
        }
    }
}

impl Readout {
    pub fn text(&self, set: &Settings) -> String {
        match *self {
            Readout::Outline {
                x,
                from_nose,
                width,
            } => trf(
                "{0} from tail, {1} from nose: width {2}",
                &[&set.fmt(x), &set.fmt(from_nose), &set.fmt(width)],
            ),
            Readout::Profile {
                x,
                rocker,
                deck,
                thickness,
            } => trf(
                "{0} from tail: rocker {1}, deck {2}, thickness {3}",
                &[
                    &set.fmt(x),
                    &set.fmt(rocker),
                    &set.fmt(deck),
                    &set.fmt(thickness),
                ],
            ),
            Readout::Section {
                u,
                deck,
                bottom,
                thickness,
                ..
            } => trf(
                "{0} from stringer: deck {1}, bottom {2}, thickness {3}",
                &[
                    &set.fmt(u),
                    &set.fmt(deck),
                    &set.fmt(bottom),
                    &set.fmt(thickness),
                ],
            ),
        }
    }

    /// The line measured, in board coordinates.
    fn line(&self) -> (DVec2, DVec2) {
        match *self {
            Readout::Outline { x, width, .. } => {
                (DVec2::new(x, -width / 2.0), DVec2::new(x, width / 2.0))
            }
            Readout::Profile {
                x, rocker, deck, ..
            } => (DVec2::new(x, rocker), DVec2::new(x, deck)),
            Readout::Section {
                u,
                side,
                deck,
                bottom,
                ..
            } => (DVec2::new(u * side, bottom), DVec2::new(u * side, deck)),
        }
    }
}

/// Measure tool state, kept in the editor.
#[derive(Default)]
pub struct MeasureState {
    /// The latest measurement; cleared by Esc or another tool.
    pub last: Option<Measurement>,
    /// Kept with Pin until Clear measurements.
    pub pinned: Vec<Measurement>,
    /// First point of a measurement in progress (drag, or click then click).
    pending: Option<(Pane, DVec2)>,
    /// End point under the pointer while measuring.
    end: Option<DVec2>,
    dragging: bool,
    /// Status-bar text from the views this frame (measurement in progress or read-out).
    pub status: Option<String>,
}

impl MeasureState {
    /// Drop the latest measurement and any in progress (Esc).
    pub fn cancel(&mut self) {
        self.last = None;
        self.pending = None;
        self.end = None;
        self.dragging = false;
    }

    /// Drop every measurement, pinned ones included.
    pub fn clear(&mut self) {
        self.cancel();
        self.pinned.clear();
    }

    /// Keep the latest measurement on screen; the oldest pinned one goes beyond [`MAX_PINNED`].
    pub fn pin(&mut self) {
        if let Some(m) = self.last.take() {
            self.pinned.push(m);
            if self.pinned.len() > MAX_PINNED {
                self.pinned.remove(0);
            }
        }
    }

    fn finish(&mut self, m: Measurement, scale: f32) {
        self.pending = None;
        self.end = None;
        self.dragging = false;
        // A click on the start point again cancels.
        if m.distance() * scale as f64 > 2.0 {
            self.last = Some(m);
        }
    }
}

/// Draw pinned measurements of this view and, with the Measure tool (`active`), handle measuring and the
/// read-out. Called from the view's interaction after the curves are drawn.
#[allow(clippy::too_many_arguments)]
pub fn view(
    ui: &egui::Ui,
    canvas: &Canvas,
    board: &Board,
    ms: &mut MeasureState,
    hint: &mut Option<SnapKind>,
    active: bool,
    snap: &ViewSnap,
    set: &Settings,
) {
    let pal = Palette::new(ui.visuals().dark_mode);
    let pane = snap.pane;
    for m in ms.pinned.iter().filter(|m| m.pane == pane) {
        dimension(canvas, snap, m, &m.label(set), pal.curve_alt, pal.bg);
    }
    if !active {
        ms.cancel();
        return;
    }
    if let Some(m) = ms.last.filter(|m| m.pane == pane) {
        dimension(canvas, snap, &m, &m.label(set), pal.accent, pal.bg);
    }
    let resp = &canvas.response;
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let (shift, space) = ui.input(|i| (i.modifiers.shift, i.key_down(egui::Key::Space)));
    let mut at = |p: Pos2| {
        let s = snap.snap(
            snap.board(canvas.xf.to_world(p)),
            &|_, _| false,
            None,
            false,
            true,
        );
        snap.draw(canvas, &s, pal.selected);
        *hint = s.kind.or(*hint);
        s.pos
    };
    if resp.drag_started_by(egui::PointerButton::Primary)
        && !space
        && let Some(p) = resp.interact_pointer_pos()
    {
        ms.pending = Some((pane, at(p - resp.drag_delta())));
        ms.dragging = true;
    }
    let pending = ms.pending.filter(|(p, _)| *p == pane).map(|(_, a)| a);
    if let Some(a) = pending {
        if let Some(p) = resp.interact_pointer_pos().or(resp.hover_pos()) {
            let b = at(p);
            ms.end = Some(if shift { constrain_45(a, b) } else { b });
        }
        let m = Measurement {
            pane,
            a,
            b: ms.end.unwrap_or(a),
        };
        canvas
            .painter
            .circle_filled(canvas.xf.to_screen(snap.disp(a)), 3.0, pal.accent);
        dimension(canvas, snap, &m, &m.label(set), pal.accent, pal.bg);
        ms.status = Some(m.label(set));
        let done = if ms.dragging {
            resp.drag_stopped()
        } else {
            resp.clicked_by(egui::PointerButton::Primary)
        };
        if done {
            ms.finish(m, canvas.xf.scale);
        }
    } else if resp.clicked_by(egui::PointerButton::Primary)
        && !space
        && let Some(p) = resp.interact_pointer_pos()
    {
        ms.pending = Some((pane, at(p)));
        ms.end = None;
    } else if let Some(p) = resp.hover_pos() {
        at(p);
        let w = snap.board(canvas.xf.to_world(p));
        let near = snap
            .nearest_curve_point(snap.disp(w))
            .is_some_and(|(_, d)| d < READOUT_PX);
        if near && let Some(r) = readout(board, pane, w) {
            let (a, b) = r.line();
            let st = Stroke::new(1.0, pal.curve_alt);
            let (sa, sb) = (
                canvas.xf.to_screen(snap.disp(a)),
                canvas.xf.to_screen(snap.disp(b)),
            );
            canvas.painter.line_segment([sa, sb], st);
            for e in [sa, sb] {
                canvas
                    .painter
                    .line_segment([e - egui::vec2(5.0, 0.0), e + egui::vec2(5.0, 0.0)], st);
            }
            let text = r.text(set);
            label(
                canvas,
                p + egui::vec2(14.0, -14.0),
                Align2::LEFT_BOTTOM,
                &text,
                pal.curve_alt,
                pal.bg,
            );
            ms.status = Some(text);
        }
    }
}

/// A dimension line with end ticks and its label beside the middle.
fn dimension(
    canvas: &Canvas,
    snap: &ViewSnap,
    m: &Measurement,
    text: &str,
    color: Color32,
    bg: Color32,
) {
    let a = canvas.xf.to_screen(snap.disp(m.a));
    let b = canvas.xf.to_screen(snap.disp(m.b));
    let st = Stroke::new(1.5, color);
    canvas.painter.line_segment([a, b], st);
    let dir = (b - a).normalized();
    let dir = if dir.is_finite() && dir != egui::Vec2::ZERO {
        dir
    } else {
        egui::Vec2::X
    };
    // Normal pointing up the screen, for the ticks and the label.
    let mut n = dir.rot90();
    if n.y > 0.0 {
        n = -n;
    }
    for e in [a, b] {
        canvas.painter.line_segment([e - n * 6.0, e + n * 6.0], st);
    }
    let mid = a + (b - a) / 2.0 + n * 12.0;
    let anchor = if n.x.abs() > 0.7 {
        if n.x > 0.0 {
            Align2::LEFT_CENTER
        } else {
            Align2::RIGHT_CENTER
        }
    } else {
        Align2::CENTER_BOTTOM
    };
    label(canvas, mid, anchor, text, color, bg);
}

fn label(canvas: &Canvas, at: Pos2, anchor: Align2, text: &str, color: Color32, bg: Color32) {
    let galley =
        canvas
            .painter
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
    let mut rect = anchor.anchor_size(at, galley.size());
    // Keep the label inside the view.
    let inner = canvas.rect.shrink(4.0);
    rect = rect.translate(egui::vec2(
        (inner.right() - rect.right()).min(0.0) + (inner.left() - rect.left()).max(0.0),
        (inner.bottom() - rect.bottom()).min(0.0) + (inner.top() - rect.top()).max(0.0),
    ));
    canvas
        .painter
        .rect_filled(rect.expand(3.0), 3.0, bg.gamma_multiply(0.85));
    canvas.painter.galley(rect.min, galley, color);
}

/// Status bar: the measurement in progress, the read-out or the latest measurement, with Pin and Clear.
pub fn status_bar(ui: &mut egui::Ui, ms: &mut MeasureState, set: &Settings) {
    if let Some(s) = ms.status.take() {
        ui.label(s);
    } else if let Some(m) = &ms.last {
        ui.label(m.label(set));
    }
    if ms.last.is_some()
        && ui
            .small_button(tr("Pin"))
            .on_hover_text(tr("Keep this measurement on screen (up to 5)"))
            .clicked()
    {
        ms.pin();
    }
    if !ms.pinned.is_empty() && ui.small_button(tr("Clear measurements")).clicked() {
        ms.clear();
    }
}

/// Edit menu entries.
pub fn edit_menu(ui: &mut egui::Ui, ms: &mut MeasureState) {
    if ui
        .add_enabled(ms.last.is_some(), egui::Button::new(tr("Pin measurement")))
        .on_hover_text(tr("Keep this measurement on screen (up to 5)"))
        .clicked()
    {
        ms.pin();
    }
    if ui
        .add_enabled(
            ms.last.is_some() || !ms.pinned.is_empty(),
            egui::Button::new(tr("Clear measurements")),
        )
        .clicked()
    {
        ms.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Unit;

    fn cm() -> Settings {
        Settings {
            unit: Unit::Centimetres,
            ..Default::default()
        }
    }

    #[test]
    fn distance_angle_and_label() {
        let m = Measurement {
            pane: Pane::Outline,
            a: DVec2::new(10.0, 2.0),
            b: DVec2::new(13.0, -2.0),
        };
        assert!((m.distance() - 5.0).abs() < 1e-12);
        assert!((m.angle() - 53.130_102_354).abs() < 1e-6);
        assert_eq!(
            m.label(&cm()),
            format!(
                "Distance {}  dx {}  dy {}  angle 53.1°",
                cm().fmt(5.0),
                cm().fmt(3.0),
                cm().fmt(4.0)
            )
        );
        // Imperial: 30.48 cm is one foot.
        let f = Measurement {
            pane: Pane::Profile,
            a: DVec2::ZERO,
            b: DVec2::new(-30.48, 0.0),
        };
        let set = Settings::default();
        assert!(
            f.label(&set)
                .starts_with(&format!("Distance {}", set.fmt(30.48)))
        );
        assert_eq!(f.angle(), 0.0);
        assert_eq!(angle_from_horizontal(DVec2::new(0.0, -3.0)), 90.0);
        assert_eq!(angle_from_horizontal(DVec2::ZERO), 0.0);
    }

    #[test]
    fn shift_constrains_to_45_degree_steps() {
        let a = DVec2::new(1.0, 1.0);
        let h = constrain_45(a, DVec2::new(11.0, 2.0));
        assert!((h - DVec2::new(11.0, 1.0)).length() < 1e-9, "{h:?}");
        let v = constrain_45(a, DVec2::new(0.5, -9.0));
        assert!((v - DVec2::new(1.0, -9.0)).length() < 1e-9, "{v:?}");
        let d = constrain_45(a, DVec2::new(6.0, 5.0));
        assert!(
            (d.x - a.x - (d.y - a.y)).abs() < 1e-9,
            "on the diagonal: {d:?}"
        );
        assert!((angle_from_horizontal(d - a) - 45.0).abs() < 1e-9);
        assert_eq!(constrain_45(a, a), a);
    }

    #[test]
    fn pin_keeps_the_last_five() {
        let mut ms = MeasureState::default();
        for i in 0..7 {
            ms.last = Some(Measurement {
                pane: Pane::Outline,
                a: DVec2::ZERO,
                b: DVec2::new(i as f64 + 1.0, 0.0),
            });
            ms.pin();
            assert!(ms.last.is_none());
        }
        assert_eq!(ms.pinned.len(), MAX_PINNED);
        assert_eq!(ms.pinned[0].b.x, 3.0);
        ms.last = ms.pinned.first().copied();
        ms.cancel();
        assert!(ms.last.is_none() && ms.pinned.len() == MAX_PINNED);
        ms.clear();
        assert!(ms.pinned.is_empty());
        // A zero-length measurement is dropped.
        ms.finish(
            Measurement {
                pane: Pane::Outline,
                a: DVec2::ZERO,
                b: DVec2::new(0.001, 0.0),
            },
            10.0,
        );
        assert!(ms.last.is_none());
    }

    #[test]
    fn readouts_match_the_board() {
        let b = surfcraft_io::templates::TEMPLATES[0].board();
        let l = b.length();
        let Some(Readout::Outline {
            x,
            from_nose,
            width,
        }) = readout(&b, Pane::Outline, DVec2::new(60.0, -3.0))
        else {
            panic!()
        };
        assert_eq!(x, 60.0);
        assert!((from_nose - (l - 60.0)).abs() < 1e-9);
        assert!((width - b.width_at(60.0)).abs() < 1e-9 && width > 20.0);
        // Clamped to the board.
        assert!(matches!(
            readout(&b, Pane::Outline, DVec2::new(-5.0, 0.0)),
            Some(Readout::Outline { x: 0.0, .. })
        ));
        let Some(Readout::Profile {
            rocker,
            deck,
            thickness,
            ..
        }) = readout(&b, Pane::Profile, DVec2::new(l / 2.0, 3.0))
        else {
            panic!()
        };
        assert!((rocker - b.rocker_at(l / 2.0)).abs() < 1e-9);
        assert!((thickness - (deck - rocker)).abs() < 1e-9 && thickness > 3.0);
        let i = b.nearest_section_index(l / 2.0).unwrap();
        let Some(Readout::Section {
            u, side, thickness, ..
        }) = readout(&b, Pane::Section(i), DVec2::new(-4.0, 1.0))
        else {
            panic!()
        };
        assert_eq!((u, side), (4.0, -1.0));
        assert!((thickness - b.sections[i].thickness_at(4.0)).abs() < 1e-9);
        assert!(readout(&b, Pane::Section(999), DVec2::ZERO).is_none());
        let t = readout(&b, Pane::Outline, DVec2::new(60.0, 0.0))
            .unwrap()
            .text(&cm());
        assert!(
            t.contains(&cm().fmt(60.0)) && t.contains(&cm().fmt(b.width_at(60.0))),
            "{t}"
        );
    }
}
