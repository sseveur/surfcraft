//! Design by numbers: the shaper's key dimensions of a board (widths at standard stations, rocker, thickness),
//! each settable to an exact value by a smooth local reshaping of the outline, bottom and deck curves.
//!
//! A change is a displacement field over the curve's x: a smooth bump around the station (falling to zero over
//! the span on either side, never past the board's ends) for widths and thicknesses, a smooth ramp from the flat
//! part of the bottom to the station for rocker. Knot points move with the field (respecting their movement
//! factors) and handles are carried by the field's local linearisation, so smooth knots stay smooth and x stays
//! monotonic. The amplitude is found by a few secant steps until the measured value hits the target within
//! [`TOLERANCE`]. When a curve has no knot near the station, one is inserted there first (without changing the
//! shape), so the change stays local.

use serde_json::{Value, json};
use surfcraft_core::units::FOOT;
use surfcraft_core::{Board, DVec2, Handle, Segment, Spline};

use crate::commands::Context;
use crate::document::Document;
use crate::edit::{self, CurveId};

/// How close (cm) the measured value must come to the target: 0.05 mm.
pub const TOLERANCE: f64 = 0.005;

/// How close (cm) the deck's high point comes to its target position: 0.5 mm. The thickness is nearly flat around
/// its maximum, so its position is only defined that well.
pub const FOIL_TOLERANCE: f64 = 0.05;

/// A dimension shown and edited in the "Design by numbers" window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    MaxWidth,
    /// Wide point from the centre, positive toward the nose.
    WidePoint,
    NoseWidth12,
    TailWidth12,
    NoseWidth30cm,
    TailWidth30cm,
    /// Width across the tail-end corners of a square, squash or swallow tail.
    TailBlock,
    CenterWidth,
    NoseRocker,
    TailRocker,
    NoseRocker12,
    NoseRocker24,
    TailRocker12,
    TailRocker24,
    MaxThickness,
    NoseThickness12,
    TailThickness12,
    /// Position of the maximum thickness from the centre, positive toward the nose.
    Foil,
}

/// Which curves a dimension reshapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Outline,
    Rocker,
    Thickness,
}

impl Dimension {
    pub const ALL: [Dimension; 18] = [
        Dimension::MaxWidth,
        Dimension::WidePoint,
        Dimension::NoseWidth12,
        Dimension::TailWidth12,
        Dimension::NoseWidth30cm,
        Dimension::TailWidth30cm,
        Dimension::TailBlock,
        Dimension::CenterWidth,
        Dimension::NoseRocker,
        Dimension::TailRocker,
        Dimension::NoseRocker12,
        Dimension::NoseRocker24,
        Dimension::TailRocker12,
        Dimension::TailRocker24,
        Dimension::MaxThickness,
        Dimension::NoseThickness12,
        Dimension::TailThickness12,
        Dimension::Foil,
    ];

    /// Name used by the `set_dimension` command.
    pub fn code(self) -> &'static str {
        match self {
            Dimension::MaxWidth => "max_width",
            Dimension::WidePoint => "wide_point",
            Dimension::NoseWidth12 => "nose_width_12in",
            Dimension::TailWidth12 => "tail_width_12in",
            Dimension::NoseWidth30cm => "nose_width_30cm",
            Dimension::TailWidth30cm => "tail_width_30cm",
            Dimension::TailBlock => "tail_block_width",
            Dimension::CenterWidth => "center_width",
            Dimension::NoseRocker => "nose_rocker",
            Dimension::TailRocker => "tail_rocker",
            Dimension::NoseRocker12 => "nose_rocker_12in",
            Dimension::NoseRocker24 => "nose_rocker_24in",
            Dimension::TailRocker12 => "tail_rocker_12in",
            Dimension::TailRocker24 => "tail_rocker_24in",
            Dimension::MaxThickness => "max_thickness",
            Dimension::NoseThickness12 => "nose_thickness_12in",
            Dimension::TailThickness12 => "tail_thickness_12in",
            Dimension::Foil => "foil",
        }
    }

    pub fn from_code(s: &str) -> Option<Dimension> {
        Dimension::ALL.into_iter().find(|d| d.code() == s)
    }

    pub fn group(self) -> Group {
        use Dimension::*;
        match self {
            MaxWidth | WidePoint | NoseWidth12 | TailWidth12 | NoseWidth30cm | TailWidth30cm
            | TailBlock | CenterWidth => Group::Outline,
            NoseRocker | TailRocker | NoseRocker12 | NoseRocker24 | TailRocker12 | TailRocker24 => {
                Group::Rocker
            }
            MaxThickness | NoseThickness12 | TailThickness12 | Foil => Group::Thickness,
        }
    }

    /// Positions rather than sizes: may be negative.
    pub fn is_position(self) -> bool {
        matches!(self, Dimension::WidePoint | Dimension::Foil)
    }

    /// Station (from the tail) at which a station dimension is measured; `None` for the others.
    pub fn station(self, b: &Board) -> Option<f64> {
        let l = b.length();
        use Dimension::*;
        Some(match self {
            NoseWidth12 | NoseRocker12 | NoseThickness12 => l - FOOT,
            TailWidth12 | TailRocker12 | TailThickness12 => FOOT,
            NoseWidth30cm => l - 30.0,
            TailWidth30cm => 30.0,
            NoseRocker24 => l - 2.0 * FOOT,
            TailRocker24 => 2.0 * FOOT,
            CenterWidth => l / 2.0,
            _ => return None,
        })
    }

    /// The current value (cm) on `b`, or `None` when the board does not have it (too short for the station, no
    /// tail block).
    pub fn measure(self, b: &Board) -> Option<f64> {
        let l = b.length();
        if b.outline.segment_count() == 0 || l <= 0.0 {
            return None;
        }
        if let Some(x) = self.station(b) {
            // A station must lie on its own half of the board (the centre is its own).
            let ok = if self == Dimension::CenterWidth {
                true
            } else if self.is_nose_side() {
                x > l / 2.0 && x < l
            } else {
                x > 0.0 && x < l / 2.0
            };
            if !ok {
                return None;
            }
            return Some(match self.group() {
                Group::Outline => b.width_at(x),
                Group::Rocker => b.rocker_at(x),
                Group::Thickness => b.thickness_at(x),
            });
        }
        match self {
            Dimension::MaxWidth => Some(b.max_width()),
            Dimension::WidePoint => Some(b.max_width_pos() - l / 2.0),
            Dimension::TailBlock => {
                tail_corner(&b.outline).map(|i| 2.0 * b.outline.knots()[i].point().y)
            }
            Dimension::NoseRocker => Some(b.rocker_at(l - 0.005)),
            Dimension::TailRocker => Some(b.rocker_at(0.001)),
            Dimension::MaxThickness => Some(max_thickness(b).0),
            Dimension::Foil => Some(max_thickness(b).1 - l / 2.0),
            _ => None,
        }
    }

    fn is_nose_side(self) -> bool {
        use Dimension::*;
        matches!(
            self,
            NoseWidth12
                | NoseWidth30cm
                | NoseRocker
                | NoseRocker12
                | NoseRocker24
                | NoseThickness12
        )
    }
}

/// Settings of a change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Distance (cm) over which a width or thickness change fades out on each side of the station; `None` for a
    /// third of the board.
    pub span: Option<f64>,
    /// Rocker changes leave the flat part of the bottom (within 1 mm of the lowest point) untouched.
    pub keep_flat: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            span: None,
            keep_flat: true,
        }
    }
}

/// What a change did besides reaching the target.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Knots inserted to keep the change local.
    pub inserted_knots: usize,
    /// Curves that gained an inflection (a curvature sign change), by label.
    pub new_inflections: Vec<&'static str>,
}

/// Why a change was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The board has no such dimension (see [`Dimension::measure`]).
    NotAvailable,
    /// The target is not a sensible value (negative width, position off the board…).
    OutOfRange,
    /// The curves cannot be reshaped that far without folding over.
    TooFar,
}

impl Refusal {
    pub fn message(&self) -> &'static str {
        match self {
            Refusal::NotAvailable => "this board does not have that dimension",
            Refusal::OutOfRange => "that value is out of range for this board",
            Refusal::TooFar => {
                "cannot reshape the curves that far smoothly; try a smaller change or a longer span"
            }
        }
    }
}

// --------------------------------------------------------------------------------------------- measuring

/// Index of the corner knot at the tail end of a square/squash/swallow outline: the first interior knot, within
/// 5 cm of the tail, where the curve turns.
pub fn tail_corner(outline: &Spline) -> Option<usize> {
    let k = outline.knots();
    if k.len() < 3 {
        return None;
    }
    let c = &k[1];
    (c.point().x < 5.0 && c.point().y > 0.5 && !c.is_geometrically_continuous()).then_some(1)
}

/// Maximum stringer thickness and its station, refined to well under a millimetre.
pub fn max_thickness(b: &Board) -> (f64, f64) {
    let l = b.length();
    let n = (l.max(1.0) as usize).clamp(10, 2000);
    let (lo, hi) = (0.05, l - 0.05);
    let mut best = (f64::MIN, lo);
    for i in 0..=n {
        let x = lo + (hi - lo) * i as f64 / n as f64;
        let t = b.thickness_at(x);
        if t > best.0 {
            best = (t, x);
        }
    }
    // Golden-section search around the best sample.
    let step = (hi - lo) / n as f64;
    let (mut a, mut c) = ((best.1 - step).max(lo), (best.1 + step).min(hi));
    let g = 0.618_033_988_749_895;
    for _ in 0..40 {
        let x1 = c - g * (c - a);
        let x2 = a + g * (c - a);
        if b.thickness_at(x1) >= b.thickness_at(x2) {
            c = x2;
        } else {
            a = x1;
        }
    }
    let x = (a + c) / 2.0;
    let t = b.thickness_at(x);
    if t >= best.0 { (t, x) } else { best }
}

/// Every dimension the board has, with its value (cm).
pub fn all(b: &Board) -> Vec<(Dimension, f64)> {
    Dimension::ALL
        .iter()
        .filter_map(|&d| d.measure(b).map(|v| (d, v)))
        .collect()
}

// --------------------------------------------------------------------------------------------- fairness

/// Smallest turn (radians, a quarter of a degree) of a stretch of curve bending one way for it to count as a
/// real bend rather than numerical noise on an almost straight part.
const MIN_LOBE_TURN: f64 = 0.25 * std::f64::consts::PI / 180.0;

/// Number of inflections along a curve: changes in the direction it bends between stretches that each turn by
/// at least a quarter of a degree. Corners at knots are not counted (see [`worst_smooth_kink`]).
pub fn inflections(s: &Spline) -> usize {
    inflections_over(s, MIN_LOBE_TURN)
}

/// [`inflections`] counting only stretches that turn by at least `min_turn` radians.
pub fn inflections_over(s: &Spline, min_turn: f64) -> usize {
    count_lobes(s.segments().flat_map(curvature_samples), min_turn)
}

/// Curvature and arc length of 32 pieces of a segment.
fn curvature_samples(seg: Segment) -> Vec<(f64, f64)> {
    const SAMPLES: usize = 32;
    if seg.length() < 1e-6 {
        return Vec::new();
    }
    (0..SAMPLES)
        .map(|i| {
            let t = (i as f64 + 0.5) / SAMPLES as f64;
            (
                seg.curvature(t),
                seg.derivative(t).length() / SAMPLES as f64,
            )
        })
        .filter(|(k, ds)| k.is_finite() && ds.is_finite())
        .collect()
}

/// Sign changes between significant one-way stretches of `(curvature, arc length)` samples.
fn count_lobes(samples: impl IntoIterator<Item = (f64, f64)>, min_turn: f64) -> usize {
    let mut lobes: Vec<f64> = Vec::new();
    for (k, ds) in samples {
        let turn = k * ds;
        match lobes.last_mut() {
            Some(t) if (*t >= 0.0) == (turn >= 0.0) => *t += turn,
            _ => lobes.push(turn),
        }
    }
    let mut count = 0;
    let mut last: Option<bool> = None;
    for t in lobes.into_iter().filter(|t| t.abs() >= min_turn) {
        let sign = t > 0.0;
        if last.is_some_and(|l| l != sign) {
            count += 1;
        }
        last = Some(sign);
    }
    count
}

/// Inflections inside one segment.
fn segment_inflections(seg: &Segment) -> usize {
    count_lobes(curvature_samples(*seg), MIN_LOBE_TURN)
}

/// Remove an S-bend from segment `i` without moving its end points or turning its end tangents: when the two
/// tangent lines meet ahead of both ends, handles reaching past the meeting point are shortened to stop before
/// it, which makes the control polygon (and so the curve) turn one way only. Smooth knots stay smooth since only
/// handle lengths change.
fn fair_segment(s: &mut Spline, i: usize) {
    let seg = s.segment(i);
    let [p0, p1, p2, p3] = seg.p;
    let (d0, d3) = (p1 - p0, p2 - p3);
    let (l0, l3) = (d0.length(), d3.length());
    if l0 < 1e-9 || l3 < 1e-9 {
        return;
    }
    let (u0, u3) = (d0 / l0, d3 / l3);
    // p0 + s u0 = p3 + t u3
    let den = u0.perp_dot(u3);
    if den.abs() < 1e-9 {
        return;
    }
    let w = p3 - p0;
    let s0 = w.perp_dot(u3) / den;
    let t3 = w.perp_dot(u0) / den;
    if s0 <= 0.0 || t3 <= 0.0 {
        return;
    }
    let knots = s.knots_mut();
    if l0 > 0.95 * s0 {
        knots[i].set(Handle::Next, p0 + u0 * 0.95 * s0);
    }
    if l3 > 0.95 * t3 {
        knots[i + 1].set(Handle::Prev, p3 + u3 * 0.95 * t3);
    }
}

/// Largest turn (degrees) at a knot marked smooth: 0 when every smooth knot really is smooth.
pub fn worst_smooth_kink(s: &Spline) -> f64 {
    s.knots()
        .iter()
        .filter(|k| k.continuous)
        .filter_map(|k| {
            let (a, b) = (k.point() - k.prev(), k.next() - k.point());
            (a.length() > 1e-9 && b.length() > 1e-9).then(|| {
                a.normalize()
                    .dot(b.normalize())
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees()
            })
        })
        .fold(0.0, f64::max)
}

// --------------------------------------------------------------------------------------------- fields

/// Weight of a change along the board, 0 to 1, and its derivative.
#[derive(Clone, Debug)]
enum Field {
    /// 1 at `c`, fading to 0 at `c - left` and `c + right` as `(1 - u²)³`.
    Bump { c: f64, left: f64, right: f64 },
    /// A bump times the signed distance from its centre (over the span): raises one side and lowers the other,
    /// which moves a flat maximum at `c` along x without changing its height much.
    Tilt { c: f64, left: f64, right: f64 },
    /// Values sampled at increasing stations, interpolated linearly (0 outside). No derivative.
    Table { xs: Vec<f64>, ws: Vec<f64> },
    /// 0 at `x0` and beyond it, 1 at `x1` and beyond it, a smootherstep in between.
    Ramp { x0: f64, x1: f64 },
    /// The rise of `base` above its tangent line at `x0` (height `r0`, slope `s0`), relative to that rise at the
    /// station (`rs`), on the station's side of `x0`: scales a rocker curve beyond its flat part. Starts with zero
    /// value and slope at `x0`. No derivative.
    Rise {
        base: Spline,
        x0: f64,
        nose: bool,
        r0: f64,
        s0: f64,
        rs: f64,
    },
}

impl Field {
    fn eval(&self, x: f64) -> (f64, f64) {
        match *self {
            Field::Bump { c, left, right } => {
                let span = if x < c { left } else { right };
                if span <= 0.0 {
                    return (if x == c { 1.0 } else { 0.0 }, 0.0);
                }
                let u = (x - c) / span;
                if u.abs() >= 1.0 {
                    return (0.0, 0.0);
                }
                let v = 1.0 - u * u;
                (v * v * v, 3.0 * v * v * (-2.0 * u) / span)
            }
            Field::Tilt { c, left, right } => {
                let span = if x < c { left } else { right };
                let (w, _) = Field::Bump { c, left, right }.eval(x);
                if span <= 0.0 {
                    return (0.0, 0.0);
                }
                ((x - c) / span * w, 0.0)
            }
            Field::Table { ref xs, ref ws } => {
                let i = xs.partition_point(|&v| v <= x);
                if i == 0 || i >= xs.len() {
                    return (0.0, 0.0);
                }
                let f = (x - xs[i - 1]) / (xs[i] - xs[i - 1]);
                (ws[i - 1] + f * (ws[i] - ws[i - 1]), 0.0)
            }
            Field::Ramp { x0, x1 } => {
                let d = x1 - x0;
                if d == 0.0 {
                    return (if (x - x0) * 1.0 >= 0.0 { 1.0 } else { 0.0 }, 0.0);
                }
                let u = (x - x0) / d;
                if u <= 0.0 {
                    (0.0, 0.0)
                } else if u >= 1.0 {
                    (1.0, 0.0)
                } else {
                    let w = u * u * u * (u * (6.0 * u - 15.0) + 10.0);
                    let dw = 30.0 * u * u * (1.0 - u) * (1.0 - u) / d;
                    (w, dw)
                }
            }
            Field::Rise {
                ref base,
                x0,
                nose,
                r0,
                s0,
                rs,
            } => {
                if (nose && x <= x0) || (!nose && x >= x0) {
                    return (0.0, 0.0);
                }
                let l = base.max_x();
                let r = base.value_at(x.clamp(0.001, l - 0.005)) - r0 - s0 * (x - x0);
                ((r / rs).max(0.0), 0.0)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// Displace curve `s` by `a · field` along `axis`. Fails when the curve would fold back in x.
///
/// Vertical changes keep every control point's x and solve, by least squares over the whole curve, for a height
/// change and a slope change per knot (two slopes at a corner) so that the curve's change follows the field.
/// Changing a smooth knot's handles by one slope keeps them in line. Knots outside the field stay exactly where
/// they are, and so do points that may not move vertically (and the end points, with `freeze_ends`).
///
/// Horizontal changes move each knot point by the field and stretch its handles by the field's local slope.
fn deform(
    s: &mut Spline,
    field: &Field,
    a: f64,
    axis: Axis,
    freeze_ends: bool,
) -> Result<(), Refusal> {
    match axis {
        Axis::Y => deform_y(s, field, a, freeze_ends)?,
        Axis::X => {
            let n = s.len();
            for (i, k) in s.knots_mut().iter_mut().enumerate() {
                let p = k.point();
                let (w, dw) = field.eval(p.x);
                let mut d = DVec2::new(a * w, 0.0) * k.axis;
                if freeze_ends && (i == 0 || i + 1 == n) {
                    d = DVec2::ZERO;
                }
                let carry = |h: DVec2| DVec2::new(h.x * (1.0 + a * dw), h.y);
                let (vp, vn) = (k.handle_vector(Handle::Prev), k.handle_vector(Handle::Next));
                let np = p + d;
                k.points = [np, np + carry(vp), np + carry(vn)];
                k.apply_locks();
            }
            for seg in s.segments() {
                let x = seg.p.map(|p| p.x);
                if x.windows(2).any(|w| w[1] < w[0] - 1e-9) {
                    return Err(Refusal::TooFar);
                }
            }
        }
    }
    if s.knots()
        .iter()
        .any(|k| !k.points.iter().all(|p| p.is_finite()))
    {
        return Err(Refusal::TooFar);
    }
    Ok(())
}

/// Unknowns of one knot in [`deform_y`]: height, and slope of the previous and next handle (the same unknown on
/// a smooth knot). `None` = fixed at zero.
#[derive(Clone, Copy, Default)]
struct KnotVars {
    dy: Option<usize>,
    prev: Option<usize>,
    next: Option<usize>,
}

fn deform_y(s: &mut Spline, field: &Field, a: f64, freeze_ends: bool) -> Result<(), Refusal> {
    let n = s.len();
    if n < 2 || a == 0.0 {
        return Ok(());
    }
    let knots = s.knots().to_vec();
    let segs: Vec<Segment> = s.segments().collect();
    let mut vars = vec![KnotVars::default(); n];
    let mut count = 0;
    let mut next_var = || {
        count += 1;
        Some(count - 1)
    };
    for (i, k) in knots.iter().enumerate() {
        // Knots where the field is zero stay exactly as they are.
        if field.eval(k.point().x).0 <= 1e-9 {
            continue;
        }
        let frozen = k.axis.y == 0.0 || (freeze_ends && (i == 0 || i + 1 == n));
        let dy = if frozen { None } else { next_var() };
        let prev = if i > 0 { next_var() } else { None };
        let next = if i + 1 == n {
            None
        } else if k.continuous && prev.is_some() {
            prev
        } else {
            next_var()
        };
        vars[i] = KnotVars { dy, prev, next };
    }
    if count == 0 {
        return Ok(());
    }
    let mut ls = crate::linalg::LeastSquares::new(count);
    const SAMPLES: usize = 32;
    for (i, seg) in segs.iter().enumerate() {
        let extent = seg.max_x() - seg.min_x();
        if extent < 0.5 {
            continue;
        }
        let (k0, k1) = (&knots[i], &knots[i + 1]);
        let (v0, v1) = (vars[i], vars[i + 1]);
        let hx0 = k0.next().x - k0.point().x;
        let hx1 = k1.prev().x - k1.point().x;
        for j in 0..=SAMPLES {
            let t = j as f64 / SAMPLES as f64;
            let u = 1.0 - t;
            let bern = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            let mut terms: Vec<(usize, f64)> = Vec::with_capacity(6);
            if let Some(v) = v0.dy {
                terms.push((v, bern[0] + bern[1]));
            }
            if let Some(v) = v0.next {
                terms.push((v, bern[1] * hx0));
            }
            if let Some(v) = v1.prev {
                terms.push((v, bern[2] * hx1));
            }
            if let Some(v) = v1.dy {
                terms.push((v, bern[2] + bern[3]));
            }
            let x = seg.point(t).x;
            let w = extent / SAMPLES as f64;
            ls.row(&terms, a * field.eval(x).0, w);
        }
    }
    // Each knot point follows the field, also where no segment is sampled (vertical ends of the deck).
    for (k, v) in knots.iter().zip(&vars) {
        if let Some(i) = v.dy {
            ls.row(&[(i, 1.0)], a * field.eval(k.point().x).0, 1.0);
        }
    }
    // A little damping keeps weakly constrained unknowns (a knot at the very edge of the field) near zero.
    for v in 0..count {
        ls.ridge(v, 1e-4);
    }
    let sol = ls.solve().ok_or(Refusal::TooFar)?;
    let get = |v: Option<usize>| v.map_or(0.0, |v| sol[v]);
    for (k, v) in s.knots_mut().iter_mut().zip(&vars) {
        let p = k.point();
        let dy = get(v.dy);
        let (vp, vn) = (k.handle_vector(Handle::Prev), k.handle_vector(Handle::Next));
        let np = p + DVec2::new(0.0, dy);
        k.points = [
            np,
            np + vp + DVec2::new(0.0, get(v.prev) * vp.x),
            np + vn + DVec2::new(0.0, get(v.next) * vn.x),
        ];
        k.apply_locks();
    }
    Ok(())
}

/// Make sure curve `id` has a knot point within `tol` of station `x` (strictly inside the curve), inserting one
/// on the curve there otherwise. Returns whether a knot was inserted.
fn ensure_knot(b: &mut Board, id: CurveId, x: f64, tol: f64) -> bool {
    let Some(s) = id.spline(b) else {
        return false;
    };
    let n = s.len();
    if n < 2 || x <= 0.5 || x >= b.length() - 0.5 {
        return false;
    }
    if s.knots().iter().any(|k| (k.point().x - x).abs() <= tol) {
        return false;
    }
    let y = s.value_at(x);
    let Some(i) = edit::add_knot(b, id, DVec2::new(x, y)) else {
        return false;
    };
    if let Some(k) = id.spline_mut(b).and_then(|s| s.knots_mut().get_mut(i))
        && k.is_geometrically_continuous()
    {
        k.continuous = true;
    }
    true
}

/// Lowest point of the bottom and the extent of its flat part (within 1 mm of the lowest point), as
/// `(flat_start, lowest, flat_end)`.
fn flat_part(b: &Board) -> (f64, f64, f64) {
    let l = b.length();
    let n = 400;
    let xs: Vec<f64> = (0..=n).map(|i| l * i as f64 / n as f64).collect();
    let r: Vec<f64> = xs
        .iter()
        .map(|&x| b.rocker_at(x.clamp(0.001, l - 0.005)))
        .collect();
    let (imin, &rmin) = r
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .unwrap_or((n / 2, &0.0));
    let flat = |i: usize| r[i] <= rmin + 0.1;
    let mut lo = imin;
    while lo > 0 && flat(lo - 1) {
        lo -= 1;
    }
    let mut hi = imin;
    while hi < n && flat(hi + 1) {
        hi += 1;
    }
    (xs[lo], xs[imin], xs[hi])
}

// --------------------------------------------------------------------------------------------- setting

/// A prepared change: the board with any knots inserted, and how to apply an amplitude to it.
struct Plan {
    board: Board,
    curves: Vec<CurveId>,
    field: Field,
    axis: Axis,
    freeze_ends: bool,
    /// Rough change of the measured value per unit of amplitude (the first secant step).
    gain: f64,
    inserted: usize,
}

impl Plan {
    fn apply(&self, a: f64) -> Result<Board, Refusal> {
        let mut b = self.board.clone();
        for &id in &self.curves {
            let s = id.spline_mut(&mut b).ok_or(Refusal::NotAvailable)?;
            let before: Vec<usize> = s.segments().map(|g| segment_inflections(&g)).collect();
            deform(s, &self.field, a, self.axis, self.freeze_ends)?;
            for (i, &n) in before.iter().enumerate() {
                if segment_inflections(&s.segment(i)) > n {
                    fair_segment(s, i);
                }
            }
        }
        edit::after_edit(&mut b, self.curves[0]);
        Ok(b)
    }
}

fn plan(b: &Board, dim: Dimension, opts: Options) -> Result<Plan, Refusal> {
    let l = b.length();
    let span = opts
        .span
        .filter(|s| s.is_finite() && *s > 1.0)
        .unwrap_or(l / 3.0);
    let station_tol = (span / 4.0).max(3.0);
    let mut board = b.clone();
    let mut inserted = 0;
    let bump = |c: f64| Field::Bump {
        c,
        left: span.min(c),
        right: span.min(l - c),
    };
    // Outline dimensions reshape both rails of an asymmetric board alike.
    let outlines: Vec<CurveId> = if b.is_asymmetric() {
        vec![CurveId::Outline, CurveId::OutlineLeft]
    } else {
        vec![CurveId::Outline]
    };
    let mut ensure_outline_knots = |board: &mut Board, c: f64, tol: f64| {
        for &id in &outlines {
            if ensure_knot(board, id, c, tol) {
                inserted += 1;
            }
        }
    };
    let p = match dim.group() {
        Group::Outline => {
            let (field, axis, gain) = match dim {
                Dimension::TailBlock => {
                    let i = tail_corner(&b.outline).ok_or(Refusal::NotAvailable)?;
                    let c = b.outline.knots()[i].point().x;
                    (
                        Field::Ramp {
                            x0: c + span,
                            x1: c,
                        },
                        Axis::Y,
                        2.0,
                    )
                }
                Dimension::WidePoint => {
                    let c = b.max_width_pos();
                    ensure_outline_knots(&mut board, c, 1.0);
                    (bump(c), Axis::X, 1.0)
                }
                Dimension::MaxWidth => {
                    let c = b.max_width_pos();
                    ensure_outline_knots(&mut board, c, station_tol);
                    (bump(c), Axis::Y, 2.0)
                }
                _ => {
                    let c = dim.station(b).ok_or(Refusal::NotAvailable)?;
                    ensure_outline_knots(&mut board, c, station_tol);
                    // Toward a tail that has a block, the tail end widens with the station (a bump dying out
                    // before the corner would dent the outline).
                    let wide = b.max_width_pos();
                    let tail_side =
                        matches!(dim, Dimension::TailWidth12 | Dimension::TailWidth30cm);
                    let field = if tail_side && c < wide && tail_corner(&b.outline).is_some() {
                        Field::Ramp {
                            x0: (c + span).min(wide),
                            x1: c,
                        }
                    } else {
                        bump(c)
                    };
                    (field, Axis::Y, 2.0)
                }
            };
            Plan {
                board,
                curves: outlines.clone(),
                field,
                axis,
                freeze_ends: false,
                gain,
                inserted,
            }
        }
        Group::Rocker => {
            let xs = match dim {
                Dimension::NoseRocker => l,
                Dimension::TailRocker => 0.0,
                _ => dim.station(b).ok_or(Refusal::NotAvailable)?,
            };
            let (start, lowest, end) = flat_part(b);
            let nose = xs > lowest;
            let mut x0 = match (opts.keep_flat, nose) {
                (true, true) => end,
                (true, false) => start,
                (false, _) => lowest,
            };
            // Leave room for a smooth transition.
            let room = 10.0f64.min((xs - lowest).abs() * 0.8);
            if nose {
                x0 = x0.min(xs - room);
            } else {
                x0 = x0.max(xs + room);
            }
            for id in [CurveId::Bottom, CurveId::Deck] {
                if opts.keep_flat && ensure_knot(&mut board, id, x0, 2.0) {
                    inserted += 1;
                }
                // A station between the flat and the end needs a knot of its own to be reachable.
                if (xs - x0).abs() < (l - xs).min(xs) && ensure_knot(&mut board, id, xs, 3.0) {
                    inserted += 1;
                }
            }
            let base = board.bottom.clone();
            let at = |x: f64| board.rocker_at(x.clamp(0.001, l - 0.005));
            let r0 = at(x0);
            let s0 = (at(x0 + 0.05) - at(x0 - 0.05)) / 0.1;
            let rs = at(xs) - r0 - s0 * (xs - x0);
            let field = if rs > 0.05 {
                Field::Rise {
                    base,
                    x0,
                    nose,
                    r0,
                    s0,
                    rs,
                }
            } else {
                Field::Ramp { x0, x1: xs }
            };
            Plan {
                board,
                curves: vec![CurveId::Bottom, CurveId::Deck],
                field,
                axis: Axis::Y,
                freeze_ends: false,
                gain: 1.0,
                inserted,
            }
        }
        Group::Thickness => {
            let (c, axis) = match dim {
                Dimension::MaxThickness => (max_thickness(b).1, Axis::Y),
                Dimension::Foil => (max_thickness(b).1, Axis::Y),
                _ => (dim.station(b).ok_or(Refusal::NotAvailable)?, Axis::Y),
            };
            let tol = station_tol;
            if ensure_knot(&mut board, CurveId::Deck, c, tol) {
                inserted += 1;
            }
            // Toward the nose or tail, the deck rises or falls from the thickest point to the station and stays
            // that much higher to the end (a local bump there would dent the deck).
            let field = match dim {
                Dimension::NoseThickness12 | Dimension::TailThickness12 => {
                    let xm = max_thickness(b).1;
                    let x0 = if c > xm {
                        (c - span).max(xm)
                    } else {
                        (c + span).min(xm)
                    };
                    Field::Ramp { x0, x1: c }
                }
                // The whole thickness profile scales: the foil keeps its shape and its high point.
                Dimension::MaxThickness => {
                    let tm = max_thickness(b).0;
                    let xs: Vec<f64> = (0..=400).map(|i| l * i as f64 / 400.0).collect();
                    let ws = xs
                        .iter()
                        .map(|&x| (b.thickness_at(x.clamp(0.05, l - 0.05)) / tm).max(0.0))
                        .collect();
                    Field::Table { xs, ws }
                }
                Dimension::Foil => Field::Tilt {
                    c,
                    left: span.min(c),
                    right: span.min(l - c),
                },
                _ => bump(c),
            };
            let gain = if dim == Dimension::Foil { 0.0 } else { 1.0 };
            Plan {
                board,
                curves: vec![CurveId::Deck],
                field,
                axis,
                freeze_ends: true,
                gain,
                inserted,
            }
        }
    };
    Ok(p)
}

/// Secant iteration on the amplitude until `dim` measures `target`.
fn solve(p: &Plan, dim: Dimension, target: f64) -> Result<Board, Refusal> {
    let m = |b: &Board| dim.measure(b).ok_or(Refusal::NotAvailable);
    let (mut a0, mut f0) = (0.0, m(&p.board)? - target);
    if f0.abs() <= TOLERANCE {
        return Ok(p.board.clone());
    }
    // Without a gain estimate, probe with a small step.
    let mut a1 = if p.gain == 0.0 { 0.05 } else { -f0 / p.gain };
    let mut last_err = Refusal::TooFar;
    for _ in 0..30 {
        match p.apply(a1) {
            Ok(b) => {
                let f1 = m(&b)? - target;
                if f1.abs() <= TOLERANCE {
                    return Ok(b);
                }
                let slope = (f1 - f0) / (a1 - a0);
                if !slope.is_finite() || slope.abs() < 1e-6 {
                    return Err(Refusal::TooFar);
                }
                let step =
                    (-f1 / slope).clamp(-4.0 * (a1 - a0).abs() - 1.0, 4.0 * (a1 - a0).abs() + 1.0);
                (a0, f0) = (a1, f1);
                a1 += step;
            }
            Err(e) => {
                // Folded over: back off toward the last good amplitude.
                last_err = e;
                a1 = a0 + (a1 - a0) / 2.0;
                if (a1 - a0).abs() < 1e-6 {
                    break;
                }
            }
        }
    }
    Err(last_err)
}

/// Sanity limits on the target of a dimension.
fn valid_target(b: &Board, dim: Dimension, v: f64) -> bool {
    let l = b.length();
    if !v.is_finite() {
        return false;
    }
    match dim.group() {
        _ if dim.is_position() => v.abs() < l / 2.0 - 5.0,
        Group::Outline => v > 0.1 && v < 1.5 * b.max_width(),
        Group::Rocker => v >= 0.0 && v < l / 2.0,
        Group::Thickness => v > 0.2 && v < 60.0,
    }
}

/// Reshape `b` so that `dim` measures `target` (cm), as described in the module documentation. The board is
/// unchanged when the change is refused.
pub fn set_dimension(
    b: &mut Board,
    dim: Dimension,
    target: f64,
    opts: Options,
) -> Result<Outcome, Refusal> {
    dim.measure(b).ok_or(Refusal::NotAvailable)?;
    if !valid_target(b, dim, target) {
        return Err(Refusal::OutOfRange);
    }
    // When the change would bend a curve the other way somewhere (a bump on an almost straight outline), spread
    // it over a longer span until it does not.
    let l = b.length();
    let span = opts
        .span
        .filter(|s| s.is_finite() && *s > 1.0)
        .unwrap_or(l / 3.0);
    // A target out of reach with a short span (lowering a long flat thickness profile) may be reachable with a
    // longer one too.
    let mut last = None;
    let mut refusal = Refusal::TooFar;
    for k in [1.0, 1.5, 2.25, 3.5] {
        let o = Options {
            span: Some(span * k),
            ..opts
        };
        let (nb, inserted) = match reshape(b, dim, target, o) {
            Ok(r) => r,
            Err(e) => {
                refusal = e;
                if last.is_some() || dim.group() == Group::Rocker {
                    break;
                }
                continue;
            }
        };
        let new_inflections = gained_inflections(b, &nb, dim);
        let done = new_inflections.is_empty() || span * k >= l || dim.group() == Group::Rocker;
        last = Some((
            nb,
            Outcome {
                inserted_knots: inserted,
                new_inflections,
            },
        ));
        if done {
            break;
        }
    }
    let (nb, out) = last.ok_or(refusal)?;
    *b = nb;
    Ok(out)
}

/// Curves of `after` with an inflection `before` did not have, not even weakly. A rocker change carries the deck
/// along with the bottom to keep the thickness; where the deck was almost straight that can bend it gently the
/// other way, so there only bends of more than a degree count.
pub fn gained_inflections(before: &Board, after: &Board, dim: Dimension) -> Vec<&'static str> {
    [CurveId::Outline, CurveId::Bottom, CurveId::Deck]
        .into_iter()
        .filter(|&id| {
            let min = if dim.group() == Group::Rocker && id == CurveId::Deck {
                1f64.to_radians()
            } else {
                MIN_LOBE_TURN
            };
            let (s0, s1) = (id.spline(before).unwrap(), id.spline(after).unwrap());
            inflections_over(s1, min) > inflections_over(s0, MIN_LOBE_TURN / 4.0)
        })
        .map(CurveId::label)
        .collect()
}

/// One attempt of [`set_dimension`] with a fixed span: the new board and the number of knots inserted.
fn reshape(
    b: &Board,
    dim: Dimension,
    target: f64,
    opts: Options,
) -> Result<(Board, usize), Refusal> {
    if dim == Dimension::Foil {
        // Move the deck's high point, then give back the maximum thickness it had. The two interact a little
        // (the thickness curve is flat at its maximum): a few rounds settle them.
        let t0 = max_thickness(b).0;
        let mut moved = b.clone();
        let mut inserted = 0;
        for _ in 0..4 {
            let p = plan(&moved, dim, opts)?;
            moved = solve(&p, dim, target)?;
            let p2 = plan(&moved, Dimension::MaxThickness, opts)?;
            moved = solve(&p2, Dimension::MaxThickness, t0)?;
            inserted += p.inserted + p2.inserted;
            if (dim.measure(&moved).unwrap_or(f64::NAN) - target).abs() <= FOIL_TOLERANCE {
                return Ok((moved, inserted));
            }
        }
        Err(Refusal::TooFar)
    } else {
        let p = plan(b, dim, opts)?;
        Ok((solve(&p, dim, target)?, p.inserted))
    }
}

// --------------------------------------------------------------------------------------------- command

/// Every dimension of the board as JSON.
pub fn dimensions_json(b: &Board, ctx: &Context) -> Value {
    let mut m = serde_json::Map::new();
    for (d, v) in all(b) {
        m.insert(
            d.code().into(),
            json!({"cm": (v * 1000.0).round() / 1000.0, "text": ctx.unit.format(v, ctx.fraction)}),
        );
    }
    Value::Object(m)
}

pub fn tool_spec() -> crate::commands::ToolSpec {
    crate::commands::ToolSpec {
        name: "set_dimension",
        description: "Design by numbers: set one of the shaper's key dimensions to an exact value; the outline, \
                      rocker or deck is reshaped smoothly and locally around that station (one undo step). Widths \
                      at 12\" (and 30 cm) from nose and tail, max width, wide point and foil (from the centre, + \
                      toward the nose), centre width, tail block width, nose/tail rocker and rocker 12\"/24\" from \
                      each end (deck follows, thickness kept), max thickness and thickness 12\" from each end \
                      (deck only). Returns every dimension after the change.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "enum": Dimension::ALL.map(Dimension::code)},
                "value": {"type": ["number", "string"], "description": "Target (cm, or a string with units like \"19 1/2\\\"\" or \"47cm\")"},
                "span": {"type": ["number", "string"], "description": "How far (cm) a width or thickness change spreads each side of the station; default a third of the board"},
                "keep_flat": {"type": "boolean", "description": "Rocker changes leave the flat middle of the bottom alone (default true)"}
            },
            "required": ["name", "value"]
        }),
    }
}

/// The `set_dimension` command: `name`, `value` (cm or a string with units), optional `span` and `keep_flat`.
pub fn command(doc: &mut Document, ctx: &Context, a: &Value) -> Result<Value, String> {
    let get = |k: &str| a.get(k).filter(|v| !v.is_null());
    let name = get("name").and_then(Value::as_str).ok_or("missing name")?;
    let dim = Dimension::from_code(name).ok_or_else(|| {
        format!(
            "unknown dimension {name:?}, expected one of {}",
            Dimension::ALL.map(Dimension::code).join(", ")
        )
    })?;
    let len = |k: &str| -> Result<Option<f64>, String> {
        match get(k) {
            None => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(Value::String(s)) => surfcraft_core::LengthUnit::Centimetres
                .parse(s)
                .map(Some)
                .ok_or_else(|| format!("{k}: cannot read length {s:?}")),
            Some(v) => Err(format!("{k}: expected a number or string, got {v}")),
        }
    };
    let value = len("value")?.ok_or("missing value")?;
    let span = len("span")?;
    if span.is_some_and(|s| !(s.is_finite() && (5.0..=500.0).contains(&s))) {
        return Err("span must be between 5 and 500 cm".into());
    }
    let keep_flat = match get("keep_flat") {
        None => true,
        Some(v) => v.as_bool().ok_or("keep_flat: expected true or false")?,
    };
    let opts = Options { span, keep_flat };
    let mut b = doc.board.clone();
    let before = dim.measure(&b);
    let out =
        set_dimension(&mut b, dim, value, opts).map_err(|e| format!("{name}: {}", e.message()))?;
    doc.edit(|d| *d = b);
    Ok(json!({
        "name": name,
        "before_cm": before.map(|v| (v * 1000.0).round() / 1000.0),
        "inserted_knots": out.inserted_knots,
        "new_inflections": out.new_inflections,
        "dimensions": dimensions_json(&doc.board, ctx),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use surfcraft_io::templates::TEMPLATES;

    fn boards() -> Vec<Board> {
        TEMPLATES.iter().map(|t| t.board()).collect()
    }

    /// No new inflections (checked independently of the outcome), no kinks at smooth knots.
    fn check_fair(before: &Board, after: &Board, what: &str, dim: Dimension) {
        assert!(gained_inflections(before, after, dim).is_empty(), "{what}");
        for id in [CurveId::Outline, CurveId::Bottom, CurveId::Deck] {
            let s1 = id.spline(after).unwrap();
            assert!(worst_smooth_kink(s1) < 0.5, "{what}: kink in {id:?}");
        }
    }

    #[test]
    fn every_dimension_reaches_its_target_fairly_and_locally() {
        for b0 in boards() {
            let name = b0.info.name.clone();
            for (dim, v0) in all(&b0) {
                let delta: f64 = match dim.group() {
                    _ if dim.is_position() => 2.0,
                    Group::Rocker => 0.4,
                    Group::Outline => 1.0,
                    Group::Thickness => 0.3,
                };
                for sign in [1.0, -1.0] {
                    let target = if dim.group() == Group::Rocker && sign < 0.0 {
                        v0 - delta.min(0.3 * v0)
                    } else {
                        v0 + sign * delta
                    };
                    let mut b = b0.clone();
                    // Taking rocker out right next to the flat cannot keep the flat: then it bends the whole half.
                    let out = match set_dimension(&mut b, dim, target, Options::default()) {
                        Err(Refusal::TooFar) if dim.group() == Group::Rocker && sign < 0.0 => {
                            assert_eq!(b, b0);
                            let opts = Options {
                                keep_flat: false,
                                ..Default::default()
                            };
                            set_dimension(&mut b, dim, target, opts)
                        }
                        r => r,
                    }
                    .unwrap_or_else(|e| panic!("{name} {dim:?} -> {target}: {e:?}"));
                    let got = dim.measure(&b).unwrap();
                    let tol = if dim == Dimension::Foil { 0.05 } else { 0.01 };
                    assert!(
                        (got - target).abs() <= tol,
                        "{name} {dim:?}: {got} vs {target}"
                    );
                    // At most one knot per curve near the station (and one at the end of the flat for rocker).
                    let most = if dim.group() == Group::Rocker { 4 } else { 2 };
                    assert!(out.inserted_knots <= most, "{name} {dim:?}");
                    assert!(
                        out.new_inflections.is_empty(),
                        "{name} {dim:?} {target}: new inflections in {:?}",
                        out.new_inflections
                    );
                    check_fair(&b0, &b, &format!("{name} {dim:?} {target}"), dim);
                    // Length never changes.
                    assert!((b.length() - b0.length()).abs() < 1e-9);
                }
            }
        }
    }

    #[test]
    fn station_changes_stay_local() {
        let b0 = TEMPLATES[1].board();
        let mut b = b0.clone();
        let w = Dimension::TailWidth12.measure(&b).unwrap();
        set_dimension(&mut b, Dimension::TailWidth12, w + 1.27, Options::default()).unwrap();
        // The nose is untouched, the max width barely moves; rocker and thickness are not touched at all.
        let l = b.length();
        assert!((b.width_at(l - FOOT) - b0.width_at(l - FOOT)).abs() < 1e-6);
        assert!((b.max_width() - b0.max_width()).abs() < 0.2);
        assert_eq!(b.deck, b0.deck);
        assert_eq!(b.bottom, b0.bottom);
        // Thickness edits leave the outline and the bottom alone; rocker edits keep the thickness.
        let mut b = b0.clone();
        set_dimension(
            &mut b,
            Dimension::MaxThickness,
            b0.max_thickness() + 0.635,
            Options::default(),
        )
        .unwrap();
        assert_eq!(b.outline, b0.outline);
        assert_eq!(b.bottom, b0.bottom);
        let mut b = b0.clone();
        let r = Dimension::NoseRocker.measure(&b).unwrap();
        set_dimension(&mut b, Dimension::NoseRocker, r + 1.27, Options::default()).unwrap();
        for x in [30.0, l / 2.0, l - 30.0] {
            assert!(
                (b.thickness_at(x) - b0.thickness_at(x)).abs() < 0.1,
                "thickness at {x}: {} vs {}",
                b.thickness_at(x),
                b0.thickness_at(x)
            );
        }
        assert_eq!(b.outline, b0.outline);
    }

    #[test]
    fn keep_flat_leaves_the_flat_spot_alone() {
        let b0 = TEMPLATES[2].board();
        let (s, _, e) = flat_part(&b0);
        let mut b = b0.clone();
        let r = Dimension::NoseRocker.measure(&b).unwrap();
        set_dimension(&mut b, Dimension::NoseRocker, r + 2.0, Options::default()).unwrap();
        for i in 0..=10 {
            let x = s + (e - s) * i as f64 / 10.0;
            assert!(
                (b.rocker_at(x) - b0.rocker_at(x)).abs() < 1e-3,
                "rocker at {x}"
            );
        }
        // Without it, the whole nose half bends.
        let mut b = b0.clone();
        let opts = Options {
            keep_flat: false,
            ..Default::default()
        };
        set_dimension(&mut b, Dimension::NoseRocker, r + 2.0, opts).unwrap();
        assert!((b.rocker_at(e) - b0.rocker_at(e)).abs() > 1e-3);
    }

    #[test]
    fn wide_point_moves_and_stays_the_widest() {
        let b0 = TEMPLATES[0].board();
        let mut b = b0.clone();
        let wp = Dimension::WidePoint.measure(&b).unwrap();
        set_dimension(&mut b, Dimension::WidePoint, wp + 5.08, Options::default()).unwrap();
        assert!(
            (b.max_width() - b0.max_width()).abs() < 0.01,
            "max width kept"
        );
        assert!((Dimension::WidePoint.measure(&b).unwrap() - wp - 5.08).abs() <= TOLERANCE);
    }

    #[test]
    fn refusals_leave_the_board_alone() {
        let b0 = TEMPLATES[0].board();
        let mut b = b0.clone();
        assert_eq!(
            set_dimension(&mut b, Dimension::MaxWidth, -3.0, Options::default()),
            Err(Refusal::OutOfRange)
        );
        assert_eq!(
            set_dimension(&mut b, Dimension::WidePoint, 500.0, Options::default()),
            Err(Refusal::OutOfRange)
        );
        assert!(set_dimension(&mut b, Dimension::TailWidth12, 150.0, Options::default()).is_err());
        assert_eq!(b, b0);
        // A pin tail has no tail block.
        let mut pin = b0.clone();
        pin.outline.knots_mut().remove(1);
        assert_eq!(Dimension::TailBlock.measure(&pin), None);
        assert_eq!(
            set_dimension(&mut pin, Dimension::TailBlock, 20.0, Options::default()),
            Err(Refusal::NotAvailable)
        );
    }

    #[test]
    fn inflection_counter() {
        let s = surfcraft_core::polyline(&[(0.0, 0.0), (10.0, 0.0)]);
        assert_eq!(inflections(&s), 0);
        let b = TEMPLATES[0].board();
        assert!(inflections(&b.bottom) <= 1);
    }

    #[test]
    fn codes_round_trip() {
        for d in Dimension::ALL {
            assert_eq!(Dimension::from_code(d.code()), Some(d));
        }
    }
}
