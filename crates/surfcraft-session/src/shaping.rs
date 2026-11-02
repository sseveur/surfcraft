//! Section-shaping tools: bottom contours (concaves, vee, channels), applied to every
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

use serde::{Deserialize, Serialize};
use surfcraft_core::math::DEG;
use surfcraft_core::section::APEX_ANGLE;
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

/// One contour zone along the board. Lengths in cm, positions from the tail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContourZone {
    pub kind: ContourKind,
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
}

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
    let shift = f * (r_end - y_r);

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
    if !section_valid(&result, cs) {
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
    for z in zones {
        report.sections_added += ensure_sections(b, z, spacing);
    }
    for i in b.real_sections() {
        let x = b.sections[i].position;
        let mut cs = b.sections[i].clone();
        let mut changed = false;
        let mut skipped = false;
        for z in zones {
            let f = z.fade_at(x);
            if f <= 0.0 {
                continue;
            }
            match contour_section(&mut cs, z, f) {
                Ok(c) => changed |= c,
                Err(_) => skipped = true,
            }
        }
        if skipped {
            report.skipped.push(x);
        }
        if changed {
            b.sections[i] = cs;
            report.sections_changed += 1;
        }
    }
    if report.sections_changed > 0 || report.sections_added > 0 {
        edit::after_edit(b, CurveId::Section(1));
    }
    report.volume_after = b.volume();
    Ok(report)
}

/// Flatten the bottom of every section between `start` and `end` (straight and level from the stringer to the
/// run-out), removing concaves, vee and channels there.
pub fn remove_contours(
    b: &mut Board,
    start: f64,
    end: f64,
    run_out: RunOut,
) -> Result<ShapeReport, String> {
    let zone = ContourZone {
        kind: ContourKind::Flat,
        depth: 0.0,
        start,
        end,
        fade_in: 0.0,
        fade_out: 0.0,
        run_out,
        ..Default::default()
    };
    zone.validate()?;
    // Removing contours never needs new sections: only the existing ones carry contours.
    let mut report = ShapeReport {
        volume_before: b.volume(),
        ..Default::default()
    };
    for i in b.real_sections() {
        let x = b.sections[i].position;
        if zone.fade_at(x) <= 0.0 {
            continue;
        }
        let mut cs = b.sections[i].clone();
        match contour_section(&mut cs, &zone, 1.0) {
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
            for kind in ContourKind::ALL {
                let mut b = b0.clone();
                let z = ContourZone::for_board(kind, &b);
                let r = apply_contours(&mut b, std::slice::from_ref(&z), DEFAULT_SPACING).unwrap();
                assert!(r.sections_changed > 0, "{t} {kind:?}");
                assert!(r.skipped.is_empty(), "{t} {kind:?} skipped {:?}", r.skipped);
                assert!((r.volume_before - v0).abs() < 1e-6);
                let dv = r.volume_after - r.volume_before;
                // Small and of the expected sign.
                assert!(dv.abs() < 0.06 * v0, "{t} {kind:?} {dv}");
                match kind {
                    ContourKind::SingleConcave | ContourKind::Flat => {
                        assert!(dv > 0.0, "{t} {kind:?} {dv}")
                    }
                    ContourKind::Vee | ContourKind::Channels => {
                        assert!(dv < 0.0, "{t} {kind:?} {dv}")
                    }
                    ContourKind::DoubleConcave => {}
                }
                for s in &b.sections[b.real_sections()] {
                    assert!(
                        !self_intersects(&s.spline),
                        "{t} {kind:?} at {}",
                        s.position
                    );
                    assert!(
                        s.spline
                            .knots()
                            .iter()
                            .all(|k| k.points.iter().all(|p| p.is_finite()))
                    );
                }
                // Width, stringer thickness and rocker at stations are unchanged.
                for i in 1..20 {
                    let x = l * i as f64 / 20.0;
                    assert!((b.width_at(x) - b0.width_at(x)).abs() < 1e-9);
                    assert!((b.thickness_at(x) - b0.thickness_at(x)).abs() < 1e-9);
                    let st = b.section_point(x, 1.0);
                    assert!(st.y.abs() < 0.05, "{t} {kind:?} bottom centre at {x}: {st}");
                }
                let mesh = Mesh::of_board(&b, MeshDetail::PREVIEW);
                assert!(!mesh.triangles.is_empty());
                assert!(mesh.positions.iter().all(|p| p.is_finite()));
            }
        }
    }

    #[test]
    fn remove_contours_flattens() {
        let mut b = boards()[0].clone();
        let z = ContourZone::for_board(ContourKind::SingleConcave, &b);
        apply_contours(&mut b, &[z], DEFAULT_SPACING).unwrap();
        let l = b.length();
        let r = remove_contours(&mut b, 0.0, l, RunOut::FromRail(2.54)).unwrap();
        assert!(r.sections_changed > 0);
        for s in &b.sections[b.real_sections()] {
            let u_r = s.spline.max_x() - 2.54;
            for i in 0..=10 {
                let u = u_r * i as f64 / 10.0;
                assert!(
                    s.bottom_at(u).abs() < 0.08,
                    "{} {u} {}",
                    s.position,
                    s.bottom_at(u)
                );
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
}
