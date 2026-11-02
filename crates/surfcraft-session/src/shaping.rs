//! Section-shaping tools: bottom contours (concaves, vee, channels) and rail shapes, applied to every
//! cross-section in a range of the board at once.
//!
//! A cross-section is a half section in its own frame: it starts at the bottom centre (on the stringer, the height
//! of the rocker there), runs out round the rail and ends at the deck centre; x is the distance from the stringer.
//! The surface model scales each section to the outline width and the stringer thickness at its station, so a
//! tool may change the shape freely as long as it keeps the two centre knots and the widest point.
//!
//! **Bottom contours.** The bottom between the stringer and the *run-out* (where the contour ends, given as a
//! fraction of the local half-width or as a distance in from the rail) is replaced by the contour shape; the
//! stringer point (the rocker) never moves. Depth is measured the way a shaper measures it, with a straight edge
//! laid across the bottom between the two run-out points:
//!
//! - single concave: the stringer is `depth` above the straight edge (the run-out points end `depth` below the
//!   stringer), with a smooth dish in between;
//! - double concave inside a single: the single concave plus a second dish of `inner_depth` either side of the
//!   stringer, measured from the single concave's curve, leaving a low rounded ridge on the stringer;
//! - vee: the stringer is `depth` below the straight edge (the run-out points end `depth` above it), with nearly
//!   flat panels from the stringer that ease into the run-out;
//! - channels: grooves of `depth` and `channel_width` cut into the existing bottom, the outermost one ending at
//!   the run-out, separated by flats as wide as a channel;
//! - flat: a straight, level bottom from the stringer to the run-out (used to remove contours).
//!
//! Because the rocker stays on the stringer, a concave lowers the bottom towards the rails (adding a little
//! volume) and a vee raises it (removing a little). The rail outboard of the run-out keeps its shape: its lower
//! part, up to the rail apex, is stretched or compressed vertically to meet the new run-out height; the apex and
//! the deck do not move. Contours fade in and out along the board over the given lengths.
//!
//! **Rails.** The rail between the bottom near the edge and the deck at `band` in from the edge is rebuilt from
//! four knots: the bottom, the edge (tuck), the apex at the widest point, and the deck. The rail height is the
//! distance from the bottom line (extended to the edge) up to the deck at the band; the apex sits at `apex` of that
//! height (0.5 for a 50/50 rail, 0.4 for 60/40, 0.3 for 70/30). `tuck` is how far the edge sits inboard of the
//! apex; `softness` goes from a hard edge (the rail leaves the bottom at a steep angle, 0) to a soft, rounded rail
//! (the rail rises tangentially from the bottom, 1). Width, stringer thickness and the bottom contour inboard of
//! the rail are kept.

use serde::{Deserialize, Serialize};
use surfcraft_core::math::DEG;
use surfcraft_core::section::{APEX_ANGLE, TUCK_UNDER_ANGLE};
use surfcraft_core::{Board, CrossSection, DVec2, Knot, Segment, Spline};

use crate::edit::{self, CurveId};

/// Largest length accepted for any shaping parameter (cm).
const MAX_LEN: f64 = 1000.0;

// ============================================================================================ bottom contours

/// Kind of bottom contour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContourKind {
    Flat,
    #[default]
    SingleConcave,
    DoubleConcave,
    Vee,
    Channels,
}

impl ContourKind {
    pub const ALL: [ContourKind; 5] = [
        ContourKind::Flat,
        ContourKind::SingleConcave,
        ContourKind::DoubleConcave,
        ContourKind::Vee,
        ContourKind::Channels,
    ];

    /// Name used by the JSON commands.
    pub fn slug(self) -> &'static str {
        match self {
            ContourKind::Flat => "flat",
            ContourKind::SingleConcave => "single_concave",
            ContourKind::DoubleConcave => "double_concave",
            ContourKind::Vee => "vee",
            ContourKind::Channels => "channels",
        }
    }

    pub fn from_slug(s: &str) -> Option<ContourKind> {
        Self::ALL.into_iter().find(|k| k.slug() == s)
    }
}

/// Where a contour ends across the board.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RunOut {
    /// Fraction of the local half-width, from the stringer.
    Fraction(f64),
    /// Distance in from the rail edge (the widest point of the section), cm.
    FromRail(f64),
}

impl RunOut {
    /// Run-out distance from the stringer for a section of half-width `hw`, before clamping.
    pub fn at(self, hw: f64) -> f64 {
        match self {
            RunOut::Fraction(f) => f * hw,
            RunOut::FromRail(d) => hw - d,
        }
    }
}

/// What a contour keeps where it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Keep {
    /// The rail edge stays and foam is cut away, as when shaping a blank: a concave raises the stringer (the
    /// stringer rocker deepens and the thickness on the stringer drops by the depth); a vee and channels cut
    /// towards the rails. The volume always goes down.
    #[default]
    Rails,
    /// The stringer rocker stays: a concave lowers the bottom towards the rails, a vee raises it.
    Stringer,
}

impl Keep {
    pub fn slug(self) -> &'static str {
        match self {
            Keep::Rails => "rails",
            Keep::Stringer => "stringer",
        }
    }

    pub fn from_slug(s: &str) -> Option<Keep> {
        [Keep::Rails, Keep::Stringer]
            .into_iter()
            .find(|k| k.slug() == s)
    }
}

/// One contour zone along the board. Lengths in cm, positions from the tail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContourZone {
    pub kind: ContourKind,
    /// Whether the rail edge (cut foam) or the stringer rocker stays.
    pub keep: Keep,
    /// Concave or vee depth, or channel depth.
    pub depth: f64,
    /// Depth of the two inner concaves of a double concave.
    pub inner_depth: f64,
    pub start: f64,
    pub end: f64,
    /// Length over which the contour grows from nothing at `start` to full depth.
    pub fade_in: f64,
    /// Length over which the contour dies away before `end`.
    pub fade_out: f64,
    pub run_out: RunOut,
    /// Channels on each side of the stringer.
    pub channel_count: u32,
    pub channel_width: f64,
}

impl Default for ContourZone {
    fn default() -> Self {
        Self {
            kind: ContourKind::SingleConcave,
            depth: 0.3175,
            inner_depth: 0.159,
            start: 0.0,
            end: 120.0,
            fade_in: 0.0,
            fade_out: 45.0,
            run_out: RunOut::FromRail(2.54),
            channel_count: 2,
            channel_width: 2.54,
            keep: Keep::Rails,
        }
    }
}

/// Smooth 0 → 1 ramp (`3t² − 2t³`, clamped).
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn check_len(name: &str, v: f64, min: f64) -> Result<(), String> {
    if v.is_finite() && v >= min && v <= MAX_LEN {
        Ok(())
    } else {
        Err(format!(
            "{name}: must be a length between {min} and {MAX_LEN} cm"
        ))
    }
}

impl ContourZone {
    /// A zone of `kind` with depths and extent typical for `board`: from the tail to about two thirds of the
    /// length, fading out over the last quarter of the zone.
    pub fn for_board(kind: ContourKind, board: &Board) -> Self {
        let l = board.length().max(1.0);
        let (depth, start, end, fade_in, fade_out) = match kind {
            ContourKind::Flat => (0.0, 0.0, l, 0.0, 0.0),
            ContourKind::SingleConcave => (0.3175, 0.0, 0.65 * l, 0.0, 0.25 * l),
            ContourKind::DoubleConcave => (0.3175, 0.0, 0.4 * l, 0.0, 0.15 * l),
            ContourKind::Vee => (0.635, 0.0, 0.25 * l, 0.0, 0.15 * l),
            ContourKind::Channels => (0.476, 0.0, 0.3 * l, 0.0, 0.15 * l),
        };
        Self {
            kind,
            depth,
            start,
            end,
            fade_in,
            fade_out,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        check_len("depth", self.depth, 0.0)?;
        check_len("inner_depth", self.inner_depth, 0.0)?;
        check_len("start", self.start, 0.0)?;
        check_len("end", self.end, 0.0)?;
        check_len("fade_in", self.fade_in, 0.0)?;
        check_len("fade_out", self.fade_out, 0.0)?;
        if self.end <= self.start {
            return Err("end must be further from the tail than start".into());
        }
        if self.depth > 3.0 || self.inner_depth > 3.0 {
            return Err("depth: at most 3 cm".into());
        }
        match self.run_out {
            RunOut::Fraction(f) if !(f.is_finite() && (0.1..=1.0).contains(&f)) => {
                return Err("run_out_fraction: must be between 0.1 and 1".into());
            }
            RunOut::FromRail(d) if !(d.is_finite() && (0.0..=50.0).contains(&d)) => {
                return Err("run_out_from_rail: must be between 0 and 50 cm".into());
            }
            _ => {}
        }
        if self.kind == ContourKind::Channels {
            if !(1..=6).contains(&self.channel_count) {
                return Err("channel_count: 1 to 6 channels per side".into());
            }
            check_len("channel_width", self.channel_width, 0.3)?;
            if self.channel_width > 15.0 {
                return Err("channel_width: at most 15 cm".into());
            }
        }
        Ok(())
    }

    /// Contour strength at station `x`: 0 outside the zone, rising over `fade_in` and falling over `fade_out`.
    pub fn fade_at(&self, x: f64) -> f64 {
        if !(self.start..=self.end).contains(&x) {
            return 0.0;
        }
        let ramp = |d: f64, len: f64| if len > 1e-9 { smoothstep(d / len) } else { 1.0 };
        ramp(x - self.start, self.fade_in).min(ramp(self.end - x, self.fade_out))
    }

    /// The zone as applied on a board of length `l`. Cutting foam changes the stringer rocker, which must stay a
    /// fair curve: a zone that starts or ends inside the board then fades over at least [`MIN_RAMP`].
    fn effective(&self, l: f64) -> ContourZone {
        let mut z = self.clone();
        if z.keep == Keep::Rails {
            if z.start > 0.0 {
                z.fade_in = z.fade_in.max(MIN_RAMP);
            }
            if z.end < l {
                z.fade_out = z.fade_out.max(MIN_RAMP);
            }
        }
        z
    }
}

/// Shortest fade of a foam-cutting zone that starts or ends inside the board (cm).
pub const MIN_RAMP: f64 = 10.0;

/// What a shaping tool did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeReport {
    /// Board volume before and after (cm³).
    pub volume_before: f64,
    pub volume_after: f64,
    /// Sections whose shape changed.
    pub sections_changed: usize,
    /// Interpolated sections inserted so the contour has enough sections to follow.
    pub sections_added: usize,
    /// Positions (cm from tail) of sections in range that were left unchanged because the tool could not shape
    /// them (too narrow, or the result would not be a valid section).
    pub skipped: Vec<f64>,
}

/// Default largest gap between sections inside a contour zone (cm).
pub const DEFAULT_SPACING: f64 = 15.0;

/// Precise lookup of the bottom of a section by lateral distance: the curve from the bottom centre up to (and
/// including) segment `last`.
struct Bottom {
    segs: Vec<Segment>,
}

impl Bottom {
    fn new(s: &Spline, last: usize) -> Self {
        Self {
            segs: (0..=last).map(|i| s.segment(i)).collect(),
        }
    }

    /// Height and slope dy/dx at lateral distance `u`.
    fn eval(&self, u: f64) -> (f64, f64) {
        let i = self
            .segs
            .iter()
            .position(|s| s.p[0].x <= u && u <= s.p[3].x)
            .unwrap_or(if u <= 0.0 { 0 } else { self.segs.len() - 1 });
        let s = &self.segs[i];
        let t = solve_x(s, u);
        let d = s.derivative(t);
        let slope = if d.x.abs() > 1e-12 { d.y / d.x } else { 0.0 };
        (s.point(t).y, slope)
    }
}

/// Parameter of segment `s` where its x equals `x` (bisection, x assumed monotone over the segment).
fn solve_x(s: &Segment, x: f64) -> f64 {
    let (x0, x1) = (s.p[0].x, s.p[3].x);
    if (x1 - x0).abs() < 1e-15 {
        return 0.0;
    }
    let rising = x1 > x0;
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if (s.point(mid).x < x) == rising {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Knots `(u, y, slope in, slope out)` of a cubic Hermite fit of `f` over the sorted breakpoints, refined until
/// the fit is within `tol` everywhere. Slopes are one-sided, so kinks at breakpoints are kept.
fn fit(f: &dyn Fn(f64) -> f64, breaks: &[f64], tol: f64) -> Vec<(f64, f64, f64, f64)> {
    let d_right = |a: f64, b: f64| {
        let h = (b - a) * 1e-4;
        (-3.0 * f(a) + 4.0 * f(a + h) - f(a + 2.0 * h)) / (2.0 * h)
    };
    let d_left = |a: f64, b: f64| {
        let h = (b - a) * 1e-4;
        (3.0 * f(b) - 4.0 * f(b - h) + f(b - 2.0 * h)) / (2.0 * h)
    };
    // Intervals as (a, b, depth); refined depth first so the output is in order.
    let mut stack: Vec<(f64, f64, u32)> =
        breaks.windows(2).rev().map(|w| (w[0], w[1], 0)).collect();
    let mut pieces: Vec<(f64, f64, f64, f64)> = Vec::new(); // (a, b, slope at a, slope at b)
    while let Some((a, b, depth)) = stack.pop() {
        let (sa, sb) = (d_right(a, b), d_left(a, b));
        let (ya, yb) = (f(a), f(b));
        let h = b - a;
        let hermite = |t: f64| {
            let (t2, t3) = (t * t, t * t * t);
            (2.0 * t3 - 3.0 * t2 + 1.0) * ya
                + (t3 - 2.0 * t2 + t) * h * sa
                + (-2.0 * t3 + 3.0 * t2) * yb
                + (t3 - t2) * h * sb
        };
        let err = [0.2, 0.4, 0.5, 0.6, 0.8]
            .iter()
            .map(|&t| (hermite(t) - f(a + t * h)).abs())
            .fold(0.0, f64::max);
        if err > tol && depth < 8 {
            let m = 0.5 * (a + b);
            stack.push((m, b, depth + 1));
            stack.push((a, m, depth + 1));
        } else {
            pieces.push((a, b, sa, sb));
        }
    }
    let mut out = Vec::with_capacity(pieces.len() + 1);
    for (i, &(a, _, sa, _)) in pieces.iter().enumerate() {
        let s_in = if i == 0 { sa } else { pieces[i - 1].3 };
        out.push((a, f(a), s_in, sa));
    }
    if let Some(&(_, b, _, sb)) = pieces.last() {
        out.push((b, f(b), sb, sb));
    }
    out
}

/// Bezier knots through the Hermite fit: handles at a third of the neighbouring intervals along the slopes.
fn knots_from_fit(fit: &[(f64, f64, f64, f64)]) -> Vec<Knot> {
    let n = fit.len();
    (0..n)
        .map(|j| {
            let (u, y, s_in, s_out) = fit[j];
            let p = DVec2::new(u, y);
            let h_in = if j > 0 { (u - fit[j - 1].0) / 3.0 } else { 0.0 };
            let h_out = if j + 1 < n {
                (fit[j + 1].0 - u) / 3.0
            } else {
                0.0
            };
            let mut k = Knot::new(
                p,
                p - h_in * DVec2::new(1.0, s_in),
                p + h_out * DVec2::new(1.0, s_out),
            );
            k.continuous = (s_in - s_out).abs() < 1e-6;
            k
        })
        .collect()
}

/// Index of the knot at parameter `t` of segment `i`, splitting the segment unless `t` is at one of its ends.
fn split_or_existing(s: &mut Spline, i: usize, t: f64) -> usize {
    if t < 1e-6 {
        i
    } else if t > 1.0 - 1e-6 {
        i + 1
    } else {
        s.split_at(i, t)
    }
}

/// Segment of the bottom (searching from the bottom centre) containing lateral distance `u`, and the parameter.
fn bottom_segment(s: &Spline, u: f64) -> Option<(usize, f64)> {
    let i = (0..s.segment_count()).find(|&i| {
        let (a, b) = (s.knots()[i].point().x, s.knots()[i + 1].point().x);
        a <= u && u <= b
    })?;
    Some((i, solve_x(&s.segment(i), u)))
}

/// Segment of the deck (searching from the deck centre) containing lateral distance `u`, and the parameter.
fn deck_segment(s: &Spline, u: f64) -> Option<(usize, f64)> {
    let i = (0..s.segment_count()).rev().find(|&i| {
        let (a, b) = (s.knots()[i].point().x, s.knots()[i + 1].point().x);
        b <= u && u <= a
    })?;
    Some((i, solve_x(&s.segment(i), u)))
}

/// The rail apex: the point where the section's normal is horizontal (as drawn by the Section view).
fn apex_of(cs: &CrossSection) -> DVec2 {
    cs.point_at_s(cs.spline.s_by_normal_reverse(APEX_ANGLE * DEG, true))
}

/// Single-concave dish: 0 at the stringer, 1 at the run-out, level at both ends.
fn dish(t: f64) -> f64 {
    (1.0 - (std::f64::consts::PI * t.clamp(0.0, 1.0)).cos()) / 2.0
}

/// Vee panel: straight from the stringer, easing level into the run-out.
fn vee(t: f64) -> f64 {
    const K: f64 = 4.0;
    let t = t.clamp(0.0, 1.0);
    (K * t - t.powf(K)) / (K - 1.0)
}

/// Fraction of the run-out width taken by the inner concaves of a double concave.
const DOUBLE_WIDTH: f64 = 0.8;

/// Inner dish of a double concave: 0 at the stringer and beyond [`DOUBLE_WIDTH`], 1 in its middle.
fn inner_dish(t: f64) -> f64 {
    if t <= 0.0 || t >= DOUBLE_WIDTH {
        0.0
    } else {
        (std::f64::consts::PI * t / DOUBLE_WIDTH).sin().powi(2)
    }
}

/// Fraction of a channel's half-width taken by each wall.
const CHANNEL_WALL: f64 = 0.4;

/// Channel profile across one channel: `z` from -1 (one edge) to 1 (the other), flat-bottomed with rounded walls.
fn channel(z: f64) -> f64 {
    let a = z.abs();
    if a >= 1.0 {
        0.0
    } else {
        smoothstep((1.0 - a) / CHANNEL_WALL)
    }
}

/// Centres of the channels of `zone` for a run-out at `u_r`, outermost first; channels that would reach the
/// stringer are dropped.
fn channel_centres(zone: &ContourZone, u_r: f64) -> (Vec<f64>, f64) {
    let w = zone.channel_width.clamp(0.3, u_r / 2.0);
    let centres = (0..zone.channel_count)
        .map(|i| u_r - w / 2.0 - i as f64 * 2.0 * w)
        .take_while(|c| c - w / 2.0 > 0.08 * u_r)
        .collect();
    (centres, w)
}

/// Reshape the bottom of one section by `zone` at strength `fade` (0..1). Returns `Ok(false)` when there is
/// nothing to do, an error naming why the section cannot be shaped.
pub fn contour_section(
    cs: &mut CrossSection,
    zone: &ContourZone,
    fade: f64,
) -> Result<bool, &'static str> {
    contour_section_lifted(cs, zone, fade, 0.0)
}

/// Height of the bottom at the run-out of `zone` above the bottom centre, for this section.
pub fn run_out_height(cs: &CrossSection, zone: &ContourZone) -> Option<f64> {
    let sp = &cs.spline;
    if sp.len() < 3 {
        return None;
    }
    let o = sp.knots()[0].point();
    let hw = sp.max_x();
    if hw - o.x < 3.0 {
        return None;
    }
    let u_r = zone
        .run_out
        .at(hw)
        .clamp(o.x + 0.2 * (hw - o.x), o.x + 0.97 * (hw - o.x));
    let (seg, t) = bottom_segment(sp, u_r)?;
    Some(sp.segment(seg).point(t).y - o.y)
}

/// [`contour_section`] for a section whose stringer is raised by `lift` (foam cut on the stringer): the bottom
/// gets the same shape relative to the stringer, while the rail and the deck stay where they were in the board,
/// which in the section's own frame (bottom centre at the stringer) moves them down by `lift`.
fn contour_section_lifted(
    cs: &mut CrossSection,
    zone: &ContourZone,
    fade: f64,
    lift: f64,
) -> Result<bool, &'static str> {
    let f = fade.clamp(0.0, 1.0);
    let sp = cs.spline.clone();
    if sp.len() < 3 || f <= 0.0 {
        return Ok(false);
    }
    let o = sp.knots()[0].point();
    let hw = sp.max_x();
    if hw - o.x < 3.0 {
        return Err("section too narrow");
    }
    let thick = cs.center_thickness();
    if thick < 0.5 {
        return Err("section too thin");
    }
    let apex = apex_of(cs);
    let u_r = zone
        .run_out
        .at(hw)
        .clamp(o.x + 0.2 * (hw - o.x), o.x + 0.97 * (hw - o.x));
    let (seg, t) = bottom_segment(&sp, u_r).ok_or("no bottom at the run-out")?;
    let p_r = sp.segment(seg).point(t);
    if p_r.y >= apex.y - 0.05 {
        return Err("the run-out lies on the rail, not the bottom");
    }
    let bottom = Bottom::new(&sp, seg);
    let base = |u: f64| bottom.eval(u).0 - o.y;
    let (y_r, m0) = (p_r.y - o.y, bottom.eval(u_r).1);
    let span = u_r - o.x;
    // Keep depths sensible for the section's size; a vee may not lift the run-out above most of the lower rail.
    let room = apex.y - p_r.y;
    let mut depth = zone.depth.min(0.3 * thick).min(0.15 * span);
    let inner = zone.inner_depth.min(0.3 * thick).min(0.15 * span);
    if zone.kind == ContourKind::Vee {
        depth = depth.min(y_r + 0.6 * room / f).max(0.0);
    }
    let (centres, cw) = channel_centres(zone, u_r);
    if zone.kind == ContourKind::Channels && centres.is_empty() {
        return Err("no room for channels");
    }
    let grooves = |u: f64| -> f64 {
        centres
            .iter()
            .map(|&c| channel((u - c) / (cw / 2.0)))
            .sum::<f64>()
            * depth
    };
    // Target shape relative to the stringer height, and its height at the run-out.
    let kind = zone.kind;
    let shape = |u: f64| -> f64 {
        let t = (u - o.x) / span;
        match kind {
            ContourKind::Flat => 0.0,
            ContourKind::SingleConcave => -depth * dish(t),
            ContourKind::DoubleConcave => -depth * dish(t) + inner * inner_dish(t),
            ContourKind::Vee => depth * vee(t),
            ContourKind::Channels => base(u) + grooves(u),
        }
    };
    let r_end = shape(u_r);
    // Bend the target so it leaves the run-out along the original bottom (every shape above arrives level,
    // channels along the original bottom already): the new bottom then meets the rail without a kink.
    let bend = if kind == ContourKind::Channels {
        0.0
    } else {
        m0 * span
    };
    let target = |u: f64| -> f64 {
        let t = (u - o.x) / span;
        shape(u) + bend * t * t * (t - 1.0)
    };
    let new_bottom = |u: f64| o.y + (1.0 - f) * base(u) + f * target(u);
    // How far the run-out moves in the board: relative to the stringer, plus the stringer's own lift.
    let shift = f * (r_end - y_r) + lift;
    if lift > 0.0 && lift > 0.5 * thick {
        return Err("section too thin to cut");
    }

    // Breakpoints: the ends, the original knots inside (kinks of the old bottom) and the contour's own features.
    let mut breaks = vec![o.x, u_r];
    breaks.extend(
        sp.knots()[1..=seg]
            .iter()
            .map(|k| k.point().x)
            .filter(|&x| x > o.x + 1e-6 && x < u_r - 1e-6),
    );
    match kind {
        ContourKind::DoubleConcave => breaks.push(o.x + DOUBLE_WIDTH * span),
        ContourKind::Channels => {
            for &c in &centres {
                for k in [-1.0, -(1.0 - CHANNEL_WALL), 1.0 - CHANNEL_WALL, 1.0] {
                    breaks.push(c + k * cw / 2.0);
                }
            }
        }
        _ => {}
    }
    // At least six intervals across, so gentle shapes still get enough knots to be edited later.
    for i in 1..6 {
        breaks.push(o.x + span * i as f64 / 6.0);
    }
    breaks.retain(|&x| x >= o.x && x <= u_r);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    if let Some(l) = breaks.last_mut() {
        *l = u_r;
    }
    if breaks[0] != o.x {
        breaks.insert(0, o.x);
    }
    let fitted = fit(&new_bottom, &breaks, 0.002);
    let mut new_knots = knots_from_fit(&fitted);
    // The stringer knot stays exactly where it was, its handle on the other side mirrored.
    {
        let k0 = &mut new_knots[0];
        k0.set(surfcraft_core::Handle::Point, o);
        let nx = k0.next() - o;
        k0.set(surfcraft_core::Handle::Prev, o + DVec2::new(-nx.x, nx.y));
        k0.continuous = nx.y.abs() < 1e-9;
    }

    // Splice: replace knots 0..=j (the bottom up to the run-out) by the new knots.
    let mut s = sp.clone();
    let j = split_or_existing(&mut s, seg, t);
    let old_next = s.knots()[j].next();
    let last = new_knots.len() - 1;
    new_knots[last].set(surfcraft_core::Handle::Next, old_next);
    let tail: Vec<Knot> = s.knots()[j + 1..].to_vec();
    let mut knots = new_knots;
    let jn = knots.len() - 1;
    knots.extend(tail);
    // Stretch the lower rail between the old run-out height and the apex to meet the new run-out height.
    let (y_lo, y_top) = (p_r.y, apex.y);
    let k = (y_top - y_lo - shift) / (y_top - y_lo);
    let map = |y: f64| -> f64 {
        if y >= y_top {
            y
        } else if y <= y_lo {
            y + shift
        } else {
            y_top - (y_top - y) * k
        }
    };
    {
        let kn = &mut knots[jn];
        let mut nx = kn.next();
        nx.y = map(nx.y);
        kn.set(surfcraft_core::Handle::Next, nx);
    }
    for kn in knots.iter_mut().skip(jn + 1) {
        let above = kn.point().y >= y_top;
        for p in &mut kn.points {
            p.y = map(p.y);
        }
        if above {
            break;
        }
    }
    // Back into the section's frame, whose origin (the stringer) moved up by `lift`.
    if lift != 0.0 {
        let down = DVec2::new(0.0, -lift);
        {
            let kn = &mut knots[jn];
            let nx = kn.next() + down;
            kn.set(surfcraft_core::Handle::Next, nx);
        }
        for kn in knots.iter_mut().skip(jn + 1) {
            kn.translate(down);
        }
    }
    for kn in &mut knots[jn..] {
        if kn.continuous && !kn.is_geometrically_continuous() {
            kn.continuous = false;
        }
    }
    if knots[jn].is_geometrically_continuous() {
        knots[jn].continuous = true;
    }
    let mut result = cs.clone();
    result.spline = Spline::from_knots(knots);
    let mut expected = cs.clone();
    if lift != 0.0
        && let Some(k) = expected.spline.knots_mut().last_mut()
    {
        k.translate(DVec2::new(0.0, -lift));
    }
    if !section_valid(&result, &expected) {
        return Err("the contour would make an invalid section");
    }
    let changed = result.spline != cs.spline;
    *cs = result;
    Ok(changed)
}

/// Whether `new` is a usable reshape of `old`: finite, same stringer points and width, no self-intersection.
pub fn section_valid(new: &CrossSection, old: &CrossSection) -> bool {
    let s = &new.spline;
    if s.knots()
        .iter()
        .any(|k| k.points.iter().any(|p| !p.is_finite()))
    {
        return false;
    }
    let ends = |c: &CrossSection| {
        (
            c.spline.first().map(Knot::point),
            c.spline.last().map(Knot::point),
        )
    };
    if ends(new) != ends(old) || (new.width() - old.width()).abs() > 1e-6 {
        return false;
    }
    !self_intersects(s)
}

/// Whether the curve crosses itself (sampled as a polyline, 16 points per segment).
pub fn self_intersects(s: &Spline) -> bool {
    let mut pts = Vec::new();
    for seg in s.segments() {
        for i in 0..16 {
            pts.push(seg.point(i as f64 / 16.0));
        }
    }
    if let Some(k) = s.last() {
        pts.push(k.point());
    }
    let cross = |a: DVec2, b: DVec2, c: DVec2, d: DVec2| {
        let r = b - a;
        let q = d - c;
        let den = r.perp_dot(q);
        if den.abs() < 1e-14 {
            return false;
        }
        let t = (c - a).perp_dot(q) / den;
        let u = (c - a).perp_dot(r) / den;
        t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9
    };
    let n = pts.len();
    for i in 0..n.saturating_sub(1) {
        for j in i + 2..n - 1 {
            if cross(pts[i], pts[i + 1], pts[j], pts[j + 1]) {
                return true;
            }
        }
    }
    false
}

/// Insert interpolated sections so that `zone` has sections at its ends, where its fades end, and at most
/// `spacing` apart in between. Returns how many were added.
fn ensure_sections(b: &mut Board, zone: &ContourZone, spacing: f64) -> usize {
    let l = b.length();
    let (lo, hi) = (2.0, l - 2.0);
    let (a, e) = (zone.start.max(lo), zone.end.min(hi));
    if a >= e {
        return 0;
    }
    let mut key = vec![a, e, zone.start + zone.fade_in, zone.end - zone.fade_out];
    key.retain(|&x| x >= a && x <= e);
    let n = ((e - a) / spacing).ceil().max(1.0) as usize;
    let regular: Vec<f64> = (1..n).map(|i| a + (e - a) * i as f64 / n as f64).collect();
    let mut added = 0;
    for (xs, near) in [(key, 1.0), (regular, spacing / 2.5)] {
        for x in xs {
            if b.width_at(x) < 6.0
                || b.real_sections()
                    .any(|i| (b.sections[i].position - x).abs() < near)
            {
                continue;
            }
            if b.add_section(x).is_some() {
                added += 1;
            }
        }
    }
    added
}

/// How far a foam-cutting zone raises the stringer along the board: enough to keep the run-out (and so the rail)
/// where it was, faded like the contour and tapered to nothing where the board gets thin (tail and nose tips).
struct Lift {
    zone: ContourZone,
    /// Run-out height above the stringer at the sections, by position.
    samples: Vec<(f64, f64)>,
    /// Run-out height relative to the stringer that the contour asks for.
    target: f64,
    rocker: Bottom,
    deck: Bottom,
}

impl Lift {
    fn new(b: &Board, z: &ContourZone) -> Option<Lift> {
        let target = match z.kind {
            ContourKind::Flat => 0.0,
            ContourKind::SingleConcave | ContourKind::DoubleConcave => -z.depth,
            ContourKind::Vee => z.depth,
            // Channels are cut into the bottom without moving the stringer.
            ContourKind::Channels => return None,
        };
        let samples: Vec<(f64, f64)> = b
            .real_sections()
            .filter_map(|i| {
                let s = &b.sections[i];
                run_out_height(s, z).map(|y| (s.position, y))
            })
            .collect();
        if samples.is_empty() || b.bottom.segment_count() == 0 || b.deck.segment_count() == 0 {
            return None;
        }
        Some(Lift {
            zone: z.clone(),
            samples,
            target,
            rocker: Bottom::new(&b.bottom, b.bottom.segment_count() - 1),
            deck: Bottom::new(&b.deck, b.deck.segment_count() - 1),
        })
    }

    /// Run-out height above the stringer at `x`, linear between sections (as the surface blends them).
    fn run_out(&self, x: f64) -> f64 {
        let s = &self.samples;
        if x <= s[0].0 {
            return s[0].1;
        }
        for w in s.windows(2) {
            if x <= w[1].0 {
                let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
                return w[0].1 + t * (w[1].1 - w[0].1);
            }
        }
        s[s.len() - 1].1
    }

    fn at(&self, x: f64) -> f64 {
        let f = self.zone.fade_at(x);
        if f <= 0.0 {
            return 0.0;
        }
        let lift = (f * (self.run_out(x) - self.target)).max(0.0);
        let thickness = (self.deck.eval(x).0 - self.rocker.eval(x).0).max(0.0);
        lift * smoothstep(thickness / (4.0 * (self.zone.depth + 0.3)))
    }
}

/// Raise the stringer rocker by `lift` inside its zone, refitting the bottom curve there (a fair curve: the lift
/// fades in and out smoothly). `extra` are stations the new knots should include (the sections).
fn lift_rocker(b: &mut Board, lift: &Lift, extra: &[f64]) {
    let l = b.length();
    let (a, e) = (lift.zone.start.clamp(0.0, l), lift.zone.end.clamp(0.0, l));
    if e - a < 0.5 {
        return;
    }
    let old = b.bottom.clone();
    let rocker = |x: f64| lift.rocker.eval(x).0 + lift.at(x);
    let z = &lift.zone;
    let mut breaks = vec![a, e, z.start + z.fade_in, z.end - z.fade_out];
    breaks.extend(old.knots().iter().map(|k| k.point().x));
    breaks.extend_from_slice(extra);
    for i in 1..16 {
        breaks.push(a + (e - a) * i as f64 / 16.0);
    }
    breaks.retain(|&x| x >= a && x <= e);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|p, q| (*p - *q).abs() < 0.2);
    if let Some(x) = breaks.last_mut() {
        *x = e;
    }
    breaks[0] = a;
    if breaks.len() < 2 {
        return;
    }
    let fitted = fit(&rocker, &breaks, 0.001);
    let mut new_knots = knots_from_fit(&fitted);
    // Split the old curve at both ends of the zone (the end first, so the start's index stays valid).
    let mut s = old.clone();
    let at = |s: &Spline, x: f64| bottom_segment(s, x).unwrap_or((0, 0.0));
    let (ie, te) = at(&s, e);
    let je = split_or_existing(&mut s, ie, te);
    let (ia, ta) = at(&s, a);
    let ja = split_or_existing(&mut s, ia, ta);
    let je = (ja + 1..s.len())
        .find(|&i| (s.knots()[i].point().x - e).abs() < 1e-6)
        .unwrap_or(je);
    if je <= ja {
        return;
    }
    let first = s.knots()[ja].clone();
    let last = s.knots()[je].clone();
    let n = new_knots.len();
    new_knots[0].set(surfcraft_core::Handle::Prev, first.prev());
    new_knots[0].set(surfcraft_core::Handle::Point, first.point());
    new_knots[n - 1].set(surfcraft_core::Handle::Next, last.next());
    new_knots[n - 1].set(surfcraft_core::Handle::Point, last.point());
    for k in [0, n - 1] {
        new_knots[k].continuous = new_knots[k].is_geometrically_continuous();
    }
    let mut knots = s.knots()[..ja].to_vec();
    knots.extend(new_knots);
    knots.extend_from_slice(&s.knots()[je + 1..]);
    b.bottom = Spline::from_knots(knots);
}

/// Apply one zone: add sections (when `spacing` is given), raise the stringer rocker when cutting foam, reshape
/// every section in the zone. Returns whether the rocker changed.
fn apply_zone(
    b: &mut Board,
    zone: &ContourZone,
    spacing: Option<f64>,
    report: &mut ShapeReport,
) -> bool {
    let z = zone.effective(b.length());
    if let Some(sp) = spacing {
        report.sections_added += ensure_sections(b, &z, sp);
    }
    let lift = (z.keep == Keep::Rails).then(|| Lift::new(b, &z)).flatten();
    let positions: Vec<f64> = b.real_sections().map(|i| b.sections[i].position).collect();
    let mut lifted = false;
    if let Some(l) = &lift
        && positions.iter().any(|&x| l.at(x) > 1e-9)
    {
        lift_rocker(b, l, &positions);
        lifted = true;
    }
    for i in b.real_sections() {
        let x = b.sections[i].position;
        let f = z.fade_at(x);
        if f <= 0.0 {
            continue;
        }
        let u = lift.as_ref().map_or(0.0, |l| l.at(x));
        let mut cs = b.sections[i].clone();
        match contour_section_lifted(&mut cs, &z, f, u) {
            Ok(true) => {
                b.sections[i] = cs;
                report.sections_changed += 1;
            }
            Ok(false) => {}
            Err(_) => report.skipped.push(x),
        }
    }
    lifted
}

fn finish(b: &mut Board, lifted: bool, report: &mut ShapeReport) {
    if lifted {
        // The rocker moved: lowest point back at zero, sections fitted to the new thickness.
        edit::after_edit(b, CurveId::Bottom);
    } else if report.sections_changed > 0 || report.sections_added > 0 {
        edit::after_edit(b, CurveId::Section(1));
    }
    report.skipped.sort_by(f64::total_cmp);
    report.skipped.dedup();
    report.volume_after = b.volume();
}

/// Apply contour zones (in order) to the board, inserting sections where needed. One call = one edit.
pub fn apply_contours(
    b: &mut Board,
    zones: &[ContourZone],
    spacing: f64,
) -> Result<ShapeReport, String> {
    if zones.is_empty() {
        return Err("no contour zone".into());
    }
    for z in zones {
        z.validate()?;
    }
    if !(spacing.is_finite() && (2.0..=100.0).contains(&spacing)) {
        return Err("section spacing: between 2 and 100 cm".into());
    }
    if b.real_sections().is_empty() || b.length() <= 0.0 {
        return Err("the board has no cross-sections".into());
    }
    let mut report = ShapeReport {
        volume_before: b.volume(),
        ..Default::default()
    };
    let mut lifted = false;
    for z in zones {
        lifted |= apply_zone(b, z, Some(spacing), &mut report);
    }
    finish(b, lifted, &mut report);
    Ok(report)
}

/// Flatten the bottom of every section between `start` and `end` (straight and level from the stringer to the
/// run-out), removing concaves, vee and channels there. With [`Keep::Rails`] foam is cut: whichever of the
/// stringer and the run-out is higher sets the level.
pub fn remove_contours(
    b: &mut Board,
    start: f64,
    end: f64,
    run_out: RunOut,
    keep: Keep,
) -> Result<ShapeReport, String> {
    let zone = ContourZone {
        kind: ContourKind::Flat,
        keep,
        depth: 0.0,
        start,
        end,
        fade_in: 0.0,
        fade_out: 0.0,
        run_out,
        ..Default::default()
    };
    zone.validate()?;
    if b.real_sections().is_empty() || b.length() <= 0.0 {
        return Err("the board has no cross-sections".into());
    }
    // Removing contours never needs new sections: only the existing ones carry contours.
    let mut report = ShapeReport {
        volume_before: b.volume(),
        ..Default::default()
    };
    let lifted = apply_zone(b, &zone, None, &mut report);
    finish(b, lifted, &mut report);
    Ok(report)
}

// ============================================================================================ rails

/// Rail shape parameters. Lengths in cm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RailSpec {
    /// Apex height as a fraction of the rail height, from the bottom (0.5 = 50/50, 0.4 = 60/40, 0.3 = 70/30).
    pub apex: f64,
    /// 0 = hard edge, 1 = soft, fully rounded rail.
    pub softness: f64,
    /// How far the edge sits inboard of the apex.
    pub tuck: f64,
    /// Distance in from the edge where the rail meets the deck.
    pub band: f64,
}

impl Default for RailSpec {
    fn default() -> Self {
        RAIL_PRESETS[1].1
    }
}

/// Named rail presets: `(name, spec)`.
pub const RAIL_PRESETS: [(&str, RailSpec); 4] = [
    (
        "50/50",
        RailSpec {
            apex: 0.5,
            softness: 0.9,
            tuck: 0.3,
            band: 7.0,
        },
    ),
    (
        "60/40",
        RailSpec {
            apex: 0.4,
            softness: 0.55,
            tuck: 0.6,
            band: 6.5,
        },
    ),
    (
        "70/30",
        RailSpec {
            apex: 0.3,
            softness: 0.3,
            tuck: 0.8,
            band: 6.0,
        },
    ),
    (
        "80/20",
        RailSpec {
            apex: 0.2,
            softness: 0.1,
            tuck: 1.0,
            band: 6.0,
        },
    ),
];

impl RailSpec {
    pub fn preset(name: &str) -> Option<RailSpec> {
        RAIL_PRESETS.iter().find(|p| p.0 == name).map(|p| p.1)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.apex.is_finite() && (0.1..=0.9).contains(&self.apex)) {
            return Err("apex: fraction of the rail height between 0.1 and 0.9".into());
        }
        if !(self.softness.is_finite() && (0.0..=1.0).contains(&self.softness)) {
            return Err("softness: between 0 (hard) and 1 (soft)".into());
        }
        if !(self.tuck.is_finite() && (0.0..=5.0).contains(&self.tuck)) {
            return Err("tuck: between 0 and 5 cm".into());
        }
        if !(self.band.is_finite() && (1.0..=30.0).contains(&self.band)) {
            return Err("band: between 1 and 30 cm".into());
        }
        Ok(())
    }

    fn lerp(&self, o: &RailSpec, t: f64) -> RailSpec {
        let m = |a: f64, b: f64| a + (b - a) * t;
        RailSpec {
            apex: m(self.apex, o.apex),
            softness: m(self.softness, o.softness),
            tuck: m(self.tuck, o.tuck),
            band: m(self.band, o.band),
        }
    }
}

/// Rails along the board: one spec everywhere, or tail / middle / nose specs that blend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RailParams {
    pub middle: RailSpec,
    pub tail: RailSpec,
    pub nose: RailSpec,
    /// Use the tail and nose specs (otherwise `middle` everywhere).
    pub zoned: bool,
    /// Only shape sections between these stations (cm from tail); `None` = the whole board.
    pub range: Option<(f64, f64)>,
}

impl Default for RailParams {
    fn default() -> Self {
        Self {
            middle: RAIL_PRESETS[1].1,
            tail: RAIL_PRESETS[2].1,
            nose: RAIL_PRESETS[0].1,
            zoned: false,
            range: None,
        }
    }
}

impl RailParams {
    /// A single spec along the whole board.
    pub fn uniform(spec: RailSpec) -> Self {
        Self {
            middle: spec,
            tail: spec,
            nose: spec,
            zoned: false,
            range: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.middle.validate()?;
        if self.zoned {
            self.tail.validate()?;
            self.nose.validate()?;
        }
        if let Some((a, e)) = self.range
            && !(a.is_finite() && e.is_finite() && a >= 0.0 && e > a && e <= MAX_LEN)
        {
            return Err("range: start and end from the tail, end beyond start".into());
        }
        Ok(())
    }

    /// The spec at station `x` of a board of length `l`: the tail spec up to 20 % of the length, the middle one
    /// at half, the nose one from 80 %, blended linearly in between.
    pub fn spec_at(&self, x: f64, l: f64) -> RailSpec {
        if !self.zoned || l <= 0.0 {
            return self.middle;
        }
        let u = x / l;
        if u <= 0.2 {
            self.tail
        } else if u < 0.5 {
            self.tail.lerp(&self.middle, (u - 0.2) / 0.3)
        } else if u < 0.8 {
            self.middle.lerp(&self.nose, (u - 0.5) / 0.3)
        } else {
            self.nose
        }
    }
}

/// Handle length factor of a cubic quarter ellipse.
const KAPPA: f64 = 0.5523;

/// Rebuild the rail of one section to `spec`. Returns `Ok(false)` when nothing changed.
pub fn rail_section(cs: &mut CrossSection, spec: &RailSpec) -> Result<bool, &'static str> {
    let sp = cs.spline.clone();
    if sp.len() < 3 {
        return Ok(false);
    }
    let o = sp.knots()[0].point();
    let hw = sp.max_x();
    if hw - o.x < 3.0 {
        return Err("section too narrow");
    }
    let apex0 = apex_of(cs);
    let band = spec.band.clamp(0.5, 0.45 * (hw - o.x));
    let u_d = hw - band;
    let (id, td) = deck_segment(&sp, u_d).ok_or("no deck at the rail band")?;
    let dseg = sp.segment(id);
    let d_pt = dseg.point(td);
    if d_pt.y <= apex0.y {
        return Err("the rail band reaches below the apex");
    }
    // Outward direction along the deck at D (the curve runs inward there).
    let d_out = (-dseg.derivative(td)).normalize_or_zero();
    if d_out.x <= 0.0 {
        return Err("deck turns back at the rail band");
    }
    // Bottom side: start of the rail, moved inward while the bottom is already turning up into the old rail.
    let mut u_b = (hw - (spec.tuck + 0.5 + spec.softness * 0.5 * band))
        .clamp(o.x + 0.3 * (hw - o.x), hw - 0.6);
    let (ib, tb, b_pt, m_b) = loop {
        let (ib, tb) = bottom_segment(&sp, u_b).ok_or("no bottom at the rail")?;
        let bseg = sp.segment(ib);
        let d = bseg.derivative(tb);
        let m = if d.x > 1e-12 {
            d.y / d.x
        } else {
            f64::INFINITY
        };
        if m.abs() <= 0.3 || u_b <= o.x + 0.3 * (hw - o.x) {
            break (ib, tb, bseg.point(tb), m);
        }
        u_b -= 0.25;
    };
    if !m_b.is_finite() || m_b.abs() > 1.0 {
        return Err("the bottom is too steep near the rail");
    }
    if ib > id || (ib == id && tb >= td) || b_pt.y >= apex0.y {
        return Err("bottom and deck limits overlap");
    }
    let bottom_line = |x: f64| b_pt.y + m_b * (x - u_b);
    let y_edge = bottom_line(hw);
    let h = d_pt.y - y_edge;
    if h < 0.3 {
        return Err("rail too thin");
    }
    let y_a = y_edge + spec.apex * h;
    let h_lo = y_a - y_edge;
    let x_c = (hw - spec.tuck - spec.softness * (0.8 * h_lo).min(hw - spec.tuck - u_b - 0.3))
        .clamp(u_b + 0.3, hw - 0.02);
    let c_pt = DVec2::new(x_c, bottom_line(x_c));
    let bdir = DVec2::new(1.0, m_b).normalize();
    let theta = m_b.atan() + (1.0 - spec.softness) * 70.0 * DEG;
    let l_c = KAPPA * ((1.0 - spec.softness) * 0.7 * h_lo + spec.softness * (hw - x_c));
    let mut c_next = c_pt + l_c * DVec2::new(theta.cos(), theta.sin());
    c_next.x = c_next.x.min(hw);
    let mut knot_c = Knot::new(c_pt, c_pt - (x_c - u_b) / 3.0 * bdir, c_next);
    knot_c.continuous = knot_c.is_geometrically_continuous();
    let a_pt = DVec2::new(hw, y_a);
    let lo_len = KAPPA * h_lo * (0.6 + 0.4 * spec.softness);
    let up_len = KAPPA * (d_pt.y - y_a);
    let knot_a = Knot::smooth(
        a_pt,
        a_pt - DVec2::new(0.0, lo_len),
        a_pt + DVec2::new(0.0, up_len),
    );
    // Deck handle towards the apex; never dipping below a fifth of the upper rail.
    let mut d_len = KAPPA * (hw - u_d) / d_out.x.max(0.3);
    let floor = y_a + 0.2 * (d_pt.y - y_a);
    if d_out.y < 0.0 && d_pt.y + d_len * d_out.y < floor {
        d_len = (d_pt.y - floor) / -d_out.y;
    }

    let mut s = sp.clone();
    // Split at the deck point first (later on the curve), then at the bottom point; the second split may shift
    // the deck knot by one, so find it again by position.
    let jd = split_or_existing(&mut s, id, td);
    let jb = split_or_existing(&mut s, ib, tb);
    let jd = (jb + 1..s.len())
        .find(|&i| s.knots()[i].point().distance(d_pt) < 1e-6)
        .unwrap_or(jd);
    if jd <= jb {
        return Err("bottom and deck limits overlap");
    }
    let mut knots = s.knots().to_vec();
    {
        let kb = &mut knots[jb];
        let len = (x_c - u_b) / 3.0;
        kb.set(surfcraft_core::Handle::Next, b_pt + len * bdir);
        kb.continuous = kb.is_geometrically_continuous();
    }
    {
        let kd = &mut knots[jd];
        kd.set(surfcraft_core::Handle::Prev, d_pt + d_len * d_out);
        kd.continuous = kd.is_geometrically_continuous();
    }
    knots.splice(jb + 1..jd, [knot_c, knot_a]);
    let mut result = cs.clone();
    result.spline = Spline::from_knots(knots);
    if !section_valid(&result, cs) {
        return Err("the rail would make an invalid section");
    }
    let changed = result.spline != cs.spline;
    *cs = result;
    Ok(changed)
}

/// Rebuild the rails of every section in range. One call = one edit.
pub fn apply_rails(b: &mut Board, p: &RailParams) -> Result<ShapeReport, String> {
    p.validate()?;
    if b.real_sections().is_empty() || b.length() <= 0.0 {
        return Err("the board has no cross-sections".into());
    }
    let l = b.length();
    let mut report = ShapeReport {
        volume_before: b.volume(),
        ..Default::default()
    };
    for i in b.real_sections() {
        let x = b.sections[i].position;
        if let Some((a, e)) = p.range
            && !(a..=e).contains(&x)
        {
            continue;
        }
        let mut cs = b.sections[i].clone();
        match rail_section(&mut cs, &p.spec_at(x, l)) {
            Ok(true) => {
                b.sections[i] = cs;
                report.sections_changed += 1;
            }
            Ok(false) => {}
            Err(_) => report.skipped.push(x),
        }
    }
    if report.sections_changed > 0 {
        edit::after_edit(b, CurveId::Section(1));
    }
    report.volume_after = b.volume();
    Ok(report)
}

/// Measured rail of a section, with the points the Section view marks (rail apex and tuck-under point): the
/// apex height above the tuck-under point as a fraction of the rail height (tuck-under point up to the deck at
/// `band` in from the edge), and the horizontal distance from the apex in to the tuck-under point.
pub fn measure_rail(cs: &CrossSection, band: f64) -> Option<(f64, f64)> {
    let sp = &cs.spline;
    let o = sp.first()?.point();
    let hw = sp.max_x();
    if hw - o.x < 3.0 {
        return None;
    }
    let apex = apex_of(cs);
    let tuck = cs.point_at_s(sp.s_by_normal_reverse(TUCK_UNDER_ANGLE * DEG, true));
    let u_d = hw - band.clamp(0.5, 0.45 * (hw - o.x));
    let (id, td) = deck_segment(sp, u_d)?;
    let top = sp.segment(id).point(td).y;
    let h = top - tuck.y;
    (h > 0.0).then(|| ((apex.y - tuck.y) / h, apex.x - tuck.x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use surfcraft_core::{Mesh, MeshDetail};
    use surfcraft_io::templates::TEMPLATES;

    fn boards() -> Vec<Board> {
        TEMPLATES.iter().map(|t| t.board()).collect()
    }

    fn mid_section(b: &Board) -> CrossSection {
        let i = b.nearest_section_index(b.length() / 2.0).unwrap();
        b.sections[i].clone()
    }

    #[test]
    fn fade_ramps() {
        let z = ContourZone {
            start: 10.0,
            end: 100.0,
            fade_in: 10.0,
            fade_out: 30.0,
            ..Default::default()
        };
        assert_eq!(z.fade_at(5.0), 0.0);
        assert_eq!(z.fade_at(10.0), 0.0);
        assert!((z.fade_at(15.0) - 0.5).abs() < 1e-12);
        assert_eq!(z.fade_at(50.0), 1.0);
        assert!((z.fade_at(85.0) - 0.5).abs() < 1e-12);
        assert_eq!(z.fade_at(101.0), 0.0);
    }

    #[test]
    fn profiles() {
        assert_eq!(dish(0.0), 0.0);
        assert!((dish(1.0) - 1.0).abs() < 1e-12);
        assert!((vee(1.0) - 1.0).abs() < 1e-12);
        assert_eq!(inner_dish(0.0), 0.0);
        assert!((inner_dish(DOUBLE_WIDTH / 2.0) - 1.0).abs() < 1e-12);
        assert_eq!(channel(0.0), 1.0);
        assert_eq!(channel(1.0), 0.0);
    }

    /// Single concave on a mid section: stringer kept, run-out `depth` below the stringer, dish in between,
    /// width, thickness and deck unchanged.
    #[test]
    fn single_concave_depth_is_measured_from_the_stringer() {
        let b = &boards()[0];
        let cs0 = mid_section(b);
        let hw = cs0.spline.max_x();
        let z = ContourZone {
            depth: 0.6,
            run_out: RunOut::FromRail(2.54),
            ..Default::default()
        };
        let mut cs = cs0.clone();
        assert_eq!(contour_section(&mut cs, &z, 1.0), Ok(true));
        let u_r = hw - 2.54;
        assert!(cs.bottom_at(0.0).abs() < 1e-9);
        assert!(
            (cs.bottom_at(u_r) - -0.6).abs() < 0.01,
            "{}",
            cs.bottom_at(u_r)
        );
        let mid = cs.bottom_at(u_r / 2.0);
        assert!(mid < -0.2 && mid > -0.4, "{mid}");
        assert!((cs.width() - cs0.width()).abs() < 1e-6);
        assert_eq!(cs.center_thickness(), cs0.center_thickness());
        for u in [0.0, 5.0, 10.0, hw - 8.0] {
            assert!((cs.deck_at(u) - cs0.deck_at(u)).abs() < 1e-6, "deck at {u}");
        }
        // The apex does not move.
        assert!((apex_of(&cs) - apex_of(&cs0)).length() < 0.02);
        // Applying again gives the same shape (depth is absolute, not added).
        let mut again = cs.clone();
        contour_section(&mut again, &z, 1.0).unwrap();
        for i in 0..=20 {
            let u = u_r * i as f64 / 20.0;
            assert!((again.bottom_at(u) - cs.bottom_at(u)).abs() < 0.005);
        }
    }

    #[test]
    fn vee_double_and_channels_shapes() {
        let b = &boards()[1];
        let cs0 = mid_section(b);
        let hw = cs0.spline.max_x();
        let u_r = hw - 2.54;
        let mut v = cs0.clone();
        let z = ContourZone {
            kind: ContourKind::Vee,
            depth: 0.6,
            ..Default::default()
        };
        contour_section(&mut v, &z, 1.0).unwrap();
        assert!((v.bottom_at(u_r) - 0.6).abs() < 0.01);
        assert!(v.bottom_at(u_r / 2.0) > 0.25);

        let mut d = cs0.clone();
        let z = ContourZone {
            kind: ContourKind::DoubleConcave,
            depth: 0.4,
            inner_depth: 0.3,
            ..Default::default()
        };
        contour_section(&mut d, &z, 1.0).unwrap();
        // Low ridge on the stringer: the bottom rises either side of it.
        let ridge = d.bottom_at(0.0);
        let inside = d.bottom_at(0.4 * u_r);
        assert!(inside > ridge + 0.05, "{inside} vs {ridge}");
        assert!((d.bottom_at(u_r) - -0.4).abs() < 0.01);

        let mut c = cs0.clone();
        let z = ContourZone {
            kind: ContourKind::Channels,
            depth: 0.5,
            channel_count: 2,
            channel_width: 2.5,
            ..Default::default()
        };
        contour_section(&mut c, &z, 1.0).unwrap();
        let centre = u_r - 1.25;
        let lift = c.bottom_at(centre) - cs0.bottom_at(centre);
        assert!((lift - 0.5).abs() < 0.02, "{lift}");
        let flat = u_r - 2.5 - 1.25;
        assert!((c.bottom_at(flat) - cs0.bottom_at(flat)).abs() < 0.01);
        assert!(
            (c.bottom_at(u_r) - cs0.bottom_at(u_r)).abs() < 1e-4,
            "channels keep the run-out"
        );
    }

    #[test]
    fn half_fade_is_halfway() {
        let b = &boards()[0];
        let cs0 = mid_section(b);
        let u_r = cs0.spline.max_x() - 2.54;
        let z = ContourZone {
            depth: 0.6,
            ..Default::default()
        };
        let mut cs = cs0.clone();
        contour_section(&mut cs, &z, 0.5).unwrap();
        let expect = 0.5 * cs0.bottom_at(u_r) + 0.5 * -0.6;
        assert!((cs.bottom_at(u_r) - expect).abs() < 0.01);
    }

    #[test]
    fn every_contour_on_every_template() {
        for (t, b0) in boards().into_iter().enumerate() {
            let l = b0.length();
            let v0 = b0.volume();
            for keep in [Keep::Rails, Keep::Stringer] {
                for kind in ContourKind::ALL {
                    let mut b = b0.clone();
                    let z = ContourZone {
                        keep,
                        ..ContourZone::for_board(kind, &b)
                    };
                    let r =
                        apply_contours(&mut b, std::slice::from_ref(&z), DEFAULT_SPACING).unwrap();
                    let what = format!("{t} {keep:?} {kind:?}");
                    assert!(r.sections_changed > 0, "{what}");
                    assert!(r.skipped.is_empty(), "{what} skipped {:?}", r.skipped);
                    assert!((r.volume_before - v0).abs() < 1e-6);
                    let dv = r.volume_after - r.volume_before;
                    // Small and of the expected sign.
                    assert!(dv.abs() < 0.06 * v0, "{what} {dv}");
                    match (keep, kind) {
                        // Cutting foam always removes volume.
                        (Keep::Rails, _) => assert!(dv < 0.0, "{what} {dv}"),
                        (_, ContourKind::SingleConcave | ContourKind::Flat) => {
                            assert!(dv > 0.0, "{what} {dv}")
                        }
                        (_, ContourKind::Vee | ContourKind::Channels) => {
                            assert!(dv < 0.0, "{what} {dv}")
                        }
                        (_, ContourKind::DoubleConcave) => {}
                    }
                    for s in &b.sections[b.real_sections()] {
                        assert!(!self_intersects(&s.spline), "{what} at {}", s.position);
                        assert!(
                            s.spline
                                .knots()
                                .iter()
                                .all(|k| k.points.iter().all(|p| p.is_finite()))
                        );
                    }
                    // The deck keeps its shape (the board may move down as a whole when the lowest rocker point
                    // is raised).
                    let dz = b.deck_at(l / 2.0) - b0.deck_at(l / 2.0);
                    for i in 1..20 {
                        let x = l * i as f64 / 20.0;
                        assert!((b.width_at(x) - b0.width_at(x)).abs() < 1e-9);
                        assert!(
                            (b.deck_at(x) - b0.deck_at(x) - dz).abs() < 1e-6,
                            "{what} deck at {x}"
                        );
                        let st = b.section_point(x, 1.0);
                        assert!(st.y.abs() < 0.05, "{what} bottom centre at {x}: {st}");
                        let dt = b.thickness_at(x) - b0.thickness_at(x);
                        match keep {
                            Keep::Stringer => assert!(dt.abs() < 1e-9, "{what}"),
                            // Foam is only ever removed on the stringer.
                            Keep::Rails => assert!(dt < 1e-3, "{what} thicker at {x}: {dt}"),
                        }
                    }
                    if keep == Keep::Rails
                        && matches!(
                            kind,
                            ContourKind::SingleConcave | ContourKind::DoubleConcave
                        )
                    {
                        // Inside the zone at full depth: thinner on the stringer by about the depth (plus the
                        // old rise of the bottom to the run-out), the rail edge where it was below the deck.
                        let x = b0
                            .real_sections()
                            .map(|i| b0.sections[i].position)
                            .find(|&x| z.fade_at(x) == 1.0)
                            .expect("a section at full depth");
                        let dt = b0.thickness_at(x) - b.thickness_at(x);
                        assert!(dt > 0.9 * z.depth && dt < z.depth + 0.3, "{what} {dt}");
                        let edge = |b: &Board| {
                            let s = &b.sections[b.nearest_section_index(x).unwrap()];
                            assert!((s.position - x).abs() < 1e-9);
                            let u_r = s.spline.max_x() - 2.54;
                            s.center_thickness() - s.bottom_at(u_r)
                        };
                        assert!(
                            (edge(&b) - edge(&b0)).abs() < 0.02,
                            "{what} rail edge moved"
                        );
                    }
                    let mesh = Mesh::of_board(&b, MeshDetail::PREVIEW);
                    assert!(!mesh.triangles.is_empty());
                    assert!(mesh.positions.iter().all(|p| p.is_finite()));
                }
            }
        }
    }

    #[test]
    fn foam_cut_rocker_is_fair() {
        let b0 = boards()[0].clone();
        let mut b = b0.clone();
        let z = ContourZone {
            start: 20.0,
            end: 120.0,
            fade_in: 0.0,
            fade_out: 30.0,
            depth: 0.4,
            ..Default::default()
        };
        apply_contours(&mut b, std::slice::from_ref(&z), DEFAULT_SPACING).unwrap();
        // The rocker rises smoothly: no step where the zone starts (a short ramp is added), no kinks.
        let n = 400;
        let l = b.length();
        let lift = |x: f64| b0.thickness_at(x) - b.thickness_at(x);
        let mut prev_slope: Option<f64> = None;
        for i in 1..n {
            let (x0, x1) = (l * (i - 1) as f64 / n as f64, l * i as f64 / n as f64);
            let slope = (lift(x1) - lift(x0)) / (x1 - x0);
            assert!(slope.abs() < 0.2, "steep at {x1}: {slope}");
            if let Some(p) = prev_slope {
                assert!((slope - p).abs() < 0.05, "kink at {x1}: {p} > {slope}");
            }
            prev_slope = Some(slope);
        }
        assert!(lift(80.0) > 0.35 && lift(80.0) < 0.6, "{}", lift(80.0));
        assert!(
            lift(10.0).abs() < 1e-4 && lift(150.0).abs() < 1e-4,
            "{} {}",
            lift(10.0),
            lift(150.0)
        );
    }

    #[test]
    fn remove_contours_flattens() {
        for keep in [Keep::Rails, Keep::Stringer] {
            let mut b = boards()[0].clone();
            let z = ContourZone {
                keep,
                ..ContourZone::for_board(ContourKind::SingleConcave, &b)
            };
            apply_contours(&mut b, &[z], DEFAULT_SPACING).unwrap();
            let l = b.length();
            let v = b.volume();
            let r = remove_contours(&mut b, 0.0, l, RunOut::FromRail(2.54), keep).unwrap();
            assert!(r.sections_changed > 0);
            if keep == Keep::Rails {
                assert!(r.volume_after < v, "cutting removes foam");
            }
            for s in &b.sections[b.real_sections()] {
                let u_r = s.spline.max_x() - 2.54;
                for i in 0..=10 {
                    let u = u_r * i as f64 / 10.0;
                    assert!(
                        s.bottom_at(u).abs() < 0.08,
                        "{keep:?} {} {u} {}",
                        s.position,
                        s.bottom_at(u)
                    );
                }
            }
        }
    }

    #[test]
    fn bad_zones_are_rejected() {
        let mut b = boards()[0].clone();
        let before = b.clone();
        for z in [
            ContourZone {
                end: -1.0,
                ..Default::default()
            },
            ContourZone {
                depth: f64::NAN,
                ..Default::default()
            },
            ContourZone {
                run_out: RunOut::Fraction(2.0),
                ..Default::default()
            },
            ContourZone {
                kind: ContourKind::Channels,
                channel_count: 0,
                ..Default::default()
            },
        ] {
            assert!(apply_contours(&mut b, &[z], 15.0).is_err());
        }
        assert!(apply_contours(&mut b, &[], 15.0).is_err());
        assert_eq!(b, before);
    }

    #[test]
    fn rail_presets_on_every_template() {
        for (t, b0) in boards().into_iter().enumerate() {
            let l = b0.length();
            let v0 = b0.volume();
            for (name, spec) in RAIL_PRESETS {
                let mut b = b0.clone();
                let r = apply_rails(&mut b, &RailParams::uniform(spec)).unwrap();
                assert!(r.sections_changed > 0, "{t} {name}");
                assert!(r.skipped.is_empty(), "{t} {name} skipped {:?}", r.skipped);
                let dv = r.volume_after - v0;
                assert!(dv.abs() < 0.06 * v0, "{t} {name} {dv}");
                for i in 1..20 {
                    let x = l * i as f64 / 20.0;
                    assert!((b.width_at(x) - b0.width_at(x)).abs() < 1e-9);
                    assert!((b.thickness_at(x) - b0.thickness_at(x)).abs() < 1e-9);
                }
                for (s, s0) in b.sections[b.real_sections()]
                    .iter()
                    .zip(&b0.sections[b0.real_sections()])
                {
                    assert!(!self_intersects(&s.spline));
                    assert!((s.width() - b0.width_at(s.position).max(0.5)).abs() < 1e-6);
                    assert!((s.center_thickness() - b0.thickness_at(s.position)).abs() < 1e-6);
                    // The bottom inboard of the rail is untouched.
                    let u = 0.3 * s.spline.max_x();
                    assert!((s.bottom_at(u) - s0.bottom_at(u)).abs() < 1e-3);
                    let (apex, _) = measure_rail(s, spec.band).unwrap();
                    assert!(
                        (apex - spec.apex).abs() < 0.08,
                        "{t} {name} at {}: apex {apex}",
                        s.position
                    );
                }
                let mesh = Mesh::of_board(&b, MeshDetail::PREVIEW);
                assert!(mesh.positions.iter().all(|p| p.is_finite()));
            }
        }
    }

    #[test]
    fn harder_rails_have_sharper_release() {
        let cs0 = mid_section(&boards()[0]);
        let mut soft = cs0.clone();
        rail_section(&mut soft, &RailSpec::preset("50/50").unwrap()).unwrap();
        let mut hard = cs0.clone();
        rail_section(&mut hard, &RailSpec::preset("80/20").unwrap()).unwrap();
        assert!(
            hard.release_angle() < soft.release_angle() - 0.3,
            "{} vs {}",
            hard.release_angle(),
            soft.release_angle()
        );
        assert!(apex_of(&hard).y < apex_of(&soft).y);
    }

    #[test]
    fn zoned_rails_blend() {
        let p = RailParams {
            zoned: true,
            ..Default::default()
        };
        let l = 200.0;
        assert_eq!(p.spec_at(10.0, l), p.tail);
        assert_eq!(p.spec_at(100.0, l), p.middle);
        assert_eq!(p.spec_at(190.0, l), p.nose);
        let s = p.spec_at(70.0, l);
        assert!(s.apex > p.tail.apex && s.apex < p.middle.apex);
        let mut b = boards()[2].clone();
        let r = apply_rails(&mut b, &p).unwrap();
        assert!(r.sections_changed > 0 && r.skipped.is_empty(), "{r:?}");
    }

    #[test]
    fn contours_then_rails_keep_the_contour() {
        let mut b = boards()[0].clone();
        let z = ContourZone::for_board(ContourKind::SingleConcave, &b);
        apply_contours(&mut b, &[z], DEFAULT_SPACING).unwrap();
        let before = b.clone();
        apply_rails(&mut b, &RailParams::uniform(RAIL_PRESETS[2].1)).unwrap();
        for (s, s0) in b.sections.iter().zip(&before.sections).skip(1) {
            if s.is_end_section() {
                continue;
            }
            let u = 0.5 * s.spline.max_x();
            assert!((s.bottom_at(u) - s0.bottom_at(u)).abs() < 1e-4);
        }
    }
}
