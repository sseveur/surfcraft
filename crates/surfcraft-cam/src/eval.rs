//! Smooth evaluation of the board surface for machining.
//!
//! The geometry core evaluates surface points through numerical searches (arc length by bisection, curve values by
//! Newton iteration with a 0.03 mm tolerance), which is accurate enough for display but too noisy for finite
//! differences and too slow for hundreds of thousands of contact points. This module evaluates the same surface
//! definition (`geometry.md` §8: the section curves of a station, parametrised by their arc-length fraction, blended
//! between neighbouring sections and lifted by the rocker) with exact curve values and a tabulated arc length, so
//! that points are smooth in both parameters. The result agrees with the core to a few micrometres.

use surfcraft_core::math::{DEG, ONE, ZERO};
use surfcraft_core::{Board, CrossSection, DVec2, DVec3, Rail, Segment, Spline, SurfaceModel};

/// Table entries per Bezier segment for the arc length.
const TABLE: usize = 32;
/// Half step along the board for the lengthwise derivative (cm).
const DX: f64 = 0.02;
/// Samples of the girth table of a station.
const GIRTH_SAMPLES: usize = 384;

/// The whole half section: deck centre (`s = 0`) round the rail to the bottom centre (`s = 1`).
pub(crate) const WHOLE: (f64, f64) = (-90.0, 360.0);

// ------------------------------------------------------------------------------------------------ profile curves

/// Parameter of a segment where its abscissa equals `x` (the segment brackets `x`).
fn t_for_x(seg: &Segment, x: f64) -> f64 {
    let (x0, x3) = (seg.p[0].x, seg.p[3].x);
    let bracketed = (x0 <= x && x <= x3) || (x3 <= x && x <= x0);
    if bracketed && x0 != x3 {
        let increasing = x3 > x0;
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        // Newton from the linear guess, kept inside the bracket; bisection when it misbehaves.
        let mut t = ((x - x0) / (x3 - x0)).clamp(0.0, 1.0);
        for _ in 0..60 {
            let e = seg.point(t).x - x;
            if e == 0.0 {
                return t;
            }
            if (e < 0.0) == increasing {
                lo = t;
            } else {
                hi = t;
            }
            if hi - lo < 1e-14 {
                break;
            }
            let d = seg.derivative(t).x;
            let mut next = if d != 0.0 { t - e / d } else { f64::NAN };
            if !(next > lo && next < hi) {
                next = (lo + hi) / 2.0;
            }
            if (next - t).abs() < 1e-15 {
                return next;
            }
            t = next;
        }
        return t;
    }
    // Non-monotone segment: dense search, then refinement.
    let n = 64;
    let mut best = (f64::MAX, 0.0);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let e = (seg.point(t).x - x).abs();
        if e < best.0 {
            best = (e, t);
        }
    }
    let mut t = best.1;
    for _ in 0..20 {
        let d = seg.derivative(t).x;
        if d.abs() < 1e-12 {
            break;
        }
        let next = (t - (seg.point(t).x - x) / d).clamp(0.0, 1.0);
        if (next - t).abs() < 1e-15 {
            break;
        }
        t = next;
    }
    t
}

/// Value and slope of a profile curve (outline, rocker, deck) at abscissa `x`, solved exactly.
pub(crate) fn profile(spline: &Spline, x: f64) -> (f64, f64) {
    let Some(i) = spline.segment_for_x(x) else {
        return (0.0, 0.0);
    };
    let seg = spline.segment(i);
    let t = t_for_x(&seg, x);
    let d = seg.derivative(t);
    let slope = if d.x.abs() > 1e-12 { d.y / d.x } else { 0.0 };
    (seg.point(t).y, slope)
}

// ------------------------------------------------------------------------------------------------ section curves

/// A section curve scaled for one station, with an arc-length table.
#[derive(Clone, Debug)]
struct Curve {
    segs: Vec<Segment>,
    cum: Vec<f64>,
    total: f64,
    single: DVec2,
}

impl Curve {
    fn new(spline: &Spline, sx: f64, sy: f64) -> Self {
        let k = DVec2::new(sx, sy);
        let segs: Vec<Segment> = spline
            .segments()
            .map(|s| Segment::new(s.p[0] * k, s.p[1] * k, s.p[2] * k, s.p[3] * k))
            .collect();
        let mut cum = Vec::with_capacity(segs.len() * TABLE + 1);
        cum.push(0.0);
        let mut acc = 0.0;
        for s in &segs {
            let mut prev = s.point(0.0);
            for j in 1..=TABLE {
                let p = s.point(j as f64 / TABLE as f64);
                acc += prev.distance(p);
                cum.push(acc);
                prev = p;
            }
        }
        let single = spline.first().map_or(DVec2::ZERO, |kn| kn.point() * k);
        Curve {
            segs,
            cum,
            total: acc,
            single,
        }
    }

    /// Point and derivative with respect to the length fraction at fraction `f` of the length.
    fn at(&self, f: f64) -> (DVec2, DVec2) {
        if self.segs.is_empty() || self.total.is_nan() || self.total <= 0.0 {
            return (self.single, DVec2::ZERO);
        }
        let l = (f * self.total).clamp(0.0, self.total);
        let n = self.cum.len() - 1;
        let idx = self
            .cum
            .partition_point(|&c| c <= l)
            .saturating_sub(1)
            .min(n - 1);
        let (c0, c1) = (self.cum[idx], self.cum[idx + 1]);
        let span = c1 - c0;
        let frac = if span > 0.0 {
            ((l - c0) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (i, j) = (idx / TABLE, idx % TABLE);
        let t = (j as f64 + frac) / TABLE as f64;
        let s = &self.segs[i];
        let d = if span > 0.0 {
            s.derivative(t) * (self.total / (TABLE as f64 * span))
        } else {
            DVec2::ZERO
        };
        (s.point(t), d)
    }
}

/// One curve of a station with its parameter range and blend weight.
#[derive(Clone, Debug)]
struct Part {
    curve: Curve,
    s_min: f64,
    s_max: f64,
    weight: f64,
}

/// A prepared station: the section curve at one `x` for one parameter range.
#[derive(Clone, Debug)]
pub(crate) struct Station {
    pub x: f64,
    rocker: f64,
    parts: Vec<Part>,
}

impl Station {
    /// Section point (lateral, height above the bottom centre) and its derivative with respect to `s`.
    pub fn section(&self, s: f64) -> (DVec2, DVec2) {
        let mut p = DVec2::ZERO;
        let mut d = DVec2::ZERO;
        for part in &self.parts {
            let k = part.s_max - part.s_min;
            let (q, dq) = part.curve.at(k * s + part.s_min);
            p += part.weight * q;
            d += part.weight * k * dq;
        }
        (p, d)
    }

    /// Surface point in the board frame.
    pub fn point(&self, s: f64) -> DVec3 {
        let (p, _) = self.section(s);
        DVec3::new(self.x, p.x, p.y + self.rocker)
    }

    /// Normal angle (degrees, from up towards +y) of the section at `s`, in `[-90, 270)`.
    pub fn angle(&self, s: f64) -> f64 {
        let (_, d) = self.section(s);
        angle_of(d)
    }
}

/// Outward normal angle of a section tangent pointing from the deck centre towards the bottom centre.
fn angle_of(d: DVec2) -> f64 {
    let a = (-d.y).atan2(d.x) / DEG;
    if a < -90.0 { a + 360.0 } else { a }
}

// ------------------------------------------------------------------------------------------------ surface

/// Surface evaluator for one half of a board: the y >= 0 half of the evaluator's frame, shaped by one of the
/// board's rails (the board's left rail, its y >= 0 side, unless made with [`Surf::on`]).
pub(crate) struct Surf<'a> {
    pub board: &'a Board,
    pub len: f64,
    /// The board rail whose outline shapes this half.
    rail: Rail,
    /// Width and thickness at the stringer of every section.
    sections: Vec<(f64, f64)>,
}

impl<'a> Surf<'a> {
    pub fn new(board: &'a Board) -> Self {
        Surf::on(board, Rail::Left)
    }

    /// The half of an asymmetric board shaped by `rail`, still evaluated on the y >= 0 side (mirror the results
    /// for the right rail).
    pub fn on(board: &'a Board, rail: Rail) -> Self {
        let sections = board
            .sections
            .iter()
            .map(|c| (c.width(), c.thickness_at(ZERO)))
            .collect();
        Surf {
            board,
            len: board.length(),
            rail,
            sections,
        }
    }

    /// The evaluator of the other half of an asymmetric board (the board's right rail), or `None` for a
    /// symmetric board whose other half is the mirror image of this one.
    pub fn other_half(&self) -> Option<Surf<'a>> {
        self.board
            .is_asymmetric()
            .then(|| Surf::on(self.board, self.rail.other()))
    }

    /// Full width at `x` of the symmetric board made of this half.
    pub fn width(&self, x: f64) -> f64 {
        2.0 * profile(self.board.outline_of(self.rail), x).0
    }

    /// Half width and its slope.
    pub fn outline(&self, x: f64) -> (f64, f64) {
        profile(self.board.outline_of(self.rail), x)
    }

    pub fn rocker(&self, x: f64) -> (f64, f64) {
        profile(&self.board.bottom, x)
    }

    pub fn deck(&self, x: f64) -> (f64, f64) {
        profile(&self.board.deck, x)
    }

    /// Station `x` (kept 0.1 cm inside the board, as the core does) over a range of normal angles.
    pub fn station(&self, x: f64, range: (f64, f64)) -> Station {
        let b = self.board;
        let l = self.len;
        let x = if 0.1 > l - 0.1 {
            l / 2.0
        } else if x.is_nan() {
            0.1
        } else {
            x.clamp(0.1, l - 0.1)
        };
        let rocker = self.rocker(x).0;
        let w = self.width(x);
        let t = self.deck(x).0 - rocker;
        let whole = range.0 <= 0.0 && range.1 >= 270.0;
        let part = |spline: &Spline, sx: f64, sy: f64, weight: f64, clamp: bool| {
            let (mut s_min, mut s_max) = (ONE, ZERO);
            if !whole {
                let mut scaled = spline.clone();
                scaled.scale(sx, sy);
                if range.0 > 0.0 {
                    s_min = scaled.s_by_normal_reverse(range.0 * DEG, true);
                }
                if range.1 < 270.0 {
                    s_max = scaled.s_by_normal_reverse(range.1 * DEG, true);
                }
                if clamp {
                    s_min = s_min.min(ONE);
                    s_max = s_max.max(ZERO);
                }
            }
            Part {
                curve: Curve::new(spline, sx, sy),
                s_min,
                s_max,
                weight,
            }
        };
        let fallback = || {
            let shape = CrossSection::fallback_shape();
            part(&shape, w.max(0.0) / 2.0, t.max(0.0), 1.0, false)
        };
        let parts = match b.model {
            SurfaceModel::ControlPoint => match b.interpolated_section_on(x, self.rail) {
                Some(s) => vec![part(&s.spline, 1.0, 1.0, 1.0, true)],
                None => {
                    let shape = CrossSection::fallback_shape();
                    let cw = 2.0 * b.half_width_at(x, self.rail);
                    let (cw, ct) = (cw.max(0.0), b.thickness_at(x).max(0.0));
                    vec![part(&shape, cw / 2.0, ct, 1.0, true)]
                }
            },
            SurfaceModel::SBlend => {
                let found = (
                    b.previous_section_index(x),
                    b.next_section_index(x),
                    b.previous_section_position(x),
                    b.next_section_position(x),
                );
                match found {
                    (Some(i1), Some(i2), Some(p1), Some(p2)) => {
                        let scale = |i: usize| {
                            let (wk, tk) = self.sections[i];
                            let (sx, sy) = (w / wk, t / tk);
                            (sx.is_finite()
                                && sy.is_finite()
                                && b.sections[i].spline.segment_count() > 0)
                                .then_some((sx, sy))
                        };
                        match (scale(i1), scale(i2)) {
                            (Some(a), Some(c)) => {
                                let mut d = (x - p1) / (p2 - p1);
                                if !d.is_finite() {
                                    d = 0.0;
                                }
                                vec![
                                    part(&b.sections[i1].spline, a.0, a.1, 1.0 - d, false),
                                    part(&b.sections[i2].spline, c.0, c.1, d, false),
                                ]
                            }
                            _ => vec![fallback()],
                        }
                    }
                    _ => vec![fallback()],
                }
            }
        };
        Station { x, rocker, parts }
    }
}

// ------------------------------------------------------------------------------------------------ contacts

/// A station prepared for contact points: the whole half section at `x` and its neighbours for the lengthwise
/// derivative, with a girth table.
pub(crate) struct Slice {
    pub c: Station,
    m: Station,
    p: Station,
    /// Second neighbour for a one-sided derivative at the ends of the board.
    q: Option<Station>,
    /// (s, girth from the deck centre, normal angle) samples.
    table: Vec<(f64, f64, f64)>,
}

impl Slice {
    pub fn new(surf: &Surf, x: f64) -> Self {
        let c = surf.station(x, WHOLE);
        let m = surf.station(c.x - DX, WHOLE);
        let p = surf.station(c.x + DX, WHOLE);
        let q = if c.x - m.x < DX / 2.0 {
            Some(surf.station(c.x + 2.0 * DX, WHOLE))
        } else if p.x - c.x < DX / 2.0 {
            Some(surf.station(c.x - 2.0 * DX, WHOLE))
        } else {
            None
        };
        let mut table = Vec::with_capacity(GIRTH_SAMPLES + 1);
        let mut g = 0.0;
        let mut prev: Option<DVec2> = None;
        for j in 0..=GIRTH_SAMPLES {
            let s = j as f64 / GIRTH_SAMPLES as f64;
            let (q, d) = c.section(s);
            if let Some(pq) = prev {
                g += pq.distance(q);
            }
            prev = Some(q);
            table.push((s, g, angle_of(d)));
        }
        Slice { c, m, p, q, table }
    }

    pub fn x(&self) -> f64 {
        self.c.x
    }

    /// Surface point and outward unit normal at `s` (right half).
    pub fn contact(&self, s: f64) -> (DVec3, DVec3) {
        let s = s.clamp(0.0, 1.0);
        let pt = self.c.point(s);
        let (_, ds) = self.c.section(s);
        let ts = DVec3::new(0.0, ds.x, ds.y);
        let dx = self.p.x - self.m.x;
        let tx = match &self.q {
            // Second-order one-sided difference at the ends.
            Some(q) => {
                let h = (q.x - self.c.x) / 2.0;
                let near = if h > 0.0 { &self.p } else { &self.m };
                (-3.0 * pt + 4.0 * near.point(s) - q.point(s)) / (2.0 * h)
            }
            None if dx > 1e-9 => (self.p.point(s) - self.m.point(s)) / dx,
            None => DVec3::ZERO,
        };
        let n = if tx.is_finite() && tx != DVec3::ZERO {
            tx.cross(ts).normalize_or_zero()
        } else {
            DVec3::ZERO
        };
        let n = if n == DVec3::ZERO {
            DVec3::new(0.0, -ts.z, ts.y).normalize_or(DVec3::Z)
        } else {
            n
        };
        (pt, n)
    }

    /// Girth from the deck centre at `s`.
    pub fn girth(&self, s: f64) -> f64 {
        let f = s.clamp(0.0, 1.0) * GIRTH_SAMPLES as f64;
        let i = (f.floor() as usize).min(GIRTH_SAMPLES - 1);
        let t = f - i as f64;
        self.table[i].1 + (self.table[i + 1].1 - self.table[i].1) * t
    }

    /// Parameter at girth `g` from the deck centre.
    pub fn s_at_girth(&self, g: f64) -> f64 {
        let last = self.table[GIRTH_SAMPLES].1;
        if last.is_nan() || last <= 0.0 {
            return 0.0;
        }
        let g = g.clamp(0.0, last);
        let i = self
            .table
            .partition_point(|e| e.1 <= g)
            .saturating_sub(1)
            .min(GIRTH_SAMPLES - 1);
        let (a, b) = (self.table[i], self.table[i + 1]);
        let t = if b.1 > a.1 {
            (g - a.1) / (b.1 - a.1)
        } else {
            0.0
        };
        a.0 + (b.0 - a.0) * t
    }

    /// Parameter where the outward normal angle first reaches `angle` (degrees) going from the deck centre;
    /// inside a corner this is the corner. Angles at or below 0 give the deck centre, at or above 180 the bottom
    /// centre.
    pub fn s_at_angle(&self, angle: f64) -> f64 {
        if angle <= 0.0 {
            return 0.0;
        }
        if angle >= 180.0 {
            return 1.0;
        }
        let Some(j) = self.table.iter().position(|e| e.2 >= angle) else {
            return 1.0;
        };
        if j == 0 {
            return 0.0;
        }
        let (mut lo, mut hi) = (self.table[j - 1].0, self.table[j].0);
        for _ in 0..40 {
            let m = (lo + hi) / 2.0;
            if self.c.angle(m) >= angle {
                hi = m;
            } else {
                lo = m;
            }
        }
        hi
    }

    /// Parameter of the rail apex (normal angle 90 degrees).
    pub fn apex(&self) -> f64 {
        self.s_at_angle(90.0)
    }

    /// Parameter at band fraction `f` between the parameters `a` and `b`, spaced by girth.
    pub fn band(&self, a: f64, b: f64, f: f64) -> f64 {
        if f <= 0.0 {
            return a;
        }
        if f >= 1.0 {
            return b;
        }
        let (ga, gb) = (self.girth(a), self.girth(b));
        self.s_at_girth(ga + (gb - ga) * f)
    }

    /// Parameter of the point at lateral distance `y` on the deck (`deck`) or the bottom side of the apex.
    pub fn s_at_lateral(&self, y: f64, deck: bool) -> f64 {
        let apex = self.apex();
        let (mut lo, mut hi) = if deck { (0.0, apex) } else { (apex, 1.0) };
        for _ in 0..50 {
            let m = (lo + hi) / 2.0;
            let q = self.c.section(m).0.x;
            // y grows from the deck centre to the apex and shrinks again towards the bottom centre.
            if (q < y) == deck {
                lo = m;
            } else {
                hi = m;
            }
        }
        (lo + hi) / 2.0
    }
}

/// Which part of the half section to sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part3 {
    /// Deck centre to a little past the rail apex.
    Deck,
    /// A little before the rail apex to the bottom centre.
    Bottom,
    Whole,
}

/// Surface points of the right half section at `x`, about `spacing` cm apart along the girth, with the corners
/// of the section included exactly.
pub(crate) fn section_samples(surf: &Surf, x: f64, part: Part3, spacing: f64) -> Vec<DVec3> {
    let st = surf.station(x, WHOLE);
    const N: usize = 256;
    let mut table: Vec<(f64, f64, f64)> = Vec::with_capacity(N + 1);
    let mut g = 0.0;
    let mut prev: Option<DVec2> = None;
    for j in 0..=N {
        let s = j as f64 / N as f64;
        let (q, d) = st.section(s);
        if let Some(pq) = prev {
            g += pq.distance(q);
        }
        prev = Some(q);
        table.push((s, g, angle_of(d)));
    }
    let apex = match table.iter().position(|e| e.2 >= 90.0) {
        Some(0) => 0.0,
        Some(j) => {
            let (mut lo, mut hi) = (table[j - 1].0, table[j].0);
            for _ in 0..30 {
                let m = (lo + hi) / 2.0;
                if st.angle(m) >= 90.0 { hi = m } else { lo = m }
            }
            hi
        }
        None => 1.0,
    };
    let (s0, s1) = match part {
        Part3::Deck => (0.0, (apex + 0.04).min(1.0)),
        Part3::Bottom => ((apex - 0.04).max(0.0), 1.0),
        Part3::Whole => (0.0, 1.0),
    };
    let girth = |s: f64| {
        let f = s * N as f64;
        let i = (f.floor() as usize).min(N - 1);
        table[i].1 + (table[i + 1].1 - table[i].1) * (f - i as f64)
    };
    let span = (girth(s1) - girth(s0)).max(0.0);
    let n = ((span / spacing).ceil() as usize).clamp(2, 4000);
    let mut ss: Vec<f64> = (0..=n)
        .map(|i| s0 + (s1 - s0) * i as f64 / n as f64)
        .collect();
    // Corners: where the normal turns by more than a few degrees between samples, add the turning point.
    let mut extra = Vec::new();
    for w in ss.windows(2) {
        let (a0, a1) = (st.angle(w[0]), st.angle(w[1]));
        if (a1 - a0).abs() > 4.0 {
            let mid = (a0 + a1) / 2.0;
            let up = a1 > a0;
            let (mut lo, mut hi) = (w[0], w[1]);
            for _ in 0..30 {
                let m = (lo + hi) / 2.0;
                if (st.angle(m) >= mid) == up {
                    hi = m
                } else {
                    lo = m
                }
            }
            extra.push(lo);
            extra.push(hi);
        }
    }
    ss.extend(extra);
    ss.into_iter().map(|s| st.point(s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> Board {
        let bytes = std::fs::read(format!(
            "{}/../../tests/fixtures/{name}.brd",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        surfcraft_io::brd::read(&bytes).unwrap()
    }

    #[test]
    fn agrees_with_the_core_surface() {
        for name in ["shortboard", "funboard", "longboard"] {
            let board = load(name);
            let surf = Surf::new(&board);
            let l = board.length();
            let mut worst: f64 = 0.0;
            for i in 0..=40 {
                let x = 0.1 + (l - 0.2) * i as f64 / 40.0;
                let st = surf.station(x, WHOLE);
                for j in 0..=20 {
                    let s = j as f64 / 20.0;
                    let a = st.point(s);
                    let b = board.surface_point(x, s, WHOLE.0, WHOLE.1);
                    worst = worst.max(a.distance(b));
                }
            }
            assert!(worst < 0.01, "{name}: {worst} cm");
        }
    }

    #[test]
    fn normals_are_outward_and_perpendicular() {
        let board = load("shortboard");
        let surf = Surf::new(&board);
        for x in [5.0, 60.0, 120.0, 175.0] {
            let sl = Slice::new(&surf, x);
            for j in 1..20 {
                let s = j as f64 / 20.0;
                let (_, n) = sl.contact(s);
                assert!((n.length() - 1.0).abs() < 1e-9);
                let (p, _) = sl.contact(s - 1e-7);
                let (q, _) = sl.contact(s + 1e-7);
                assert!(
                    n.dot((q - p).normalize()).abs() < 1e-3,
                    "{x} {s} {n} {} {}",
                    (q - p).normalize(),
                    sl.c.section(s).1
                );
                let angle = sl.c.angle(s);
                let n2 = n.y.atan2(n.z) / DEG;
                assert!((angle - (if n2 < -90.0 { n2 + 360.0 } else { n2 })).abs() < 5.0);
            }
            let apex = sl.apex();
            assert!((sl.contact(apex).0.y - surf.width(x) / 2.0).abs() < 0.05);
        }
    }
}
