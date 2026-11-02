//! The two 3-D surface models: points, normals and section areas at a station.
//!
//! A surface point is addressed by a station `x`, a parameter `s ∈ [0, 1]` and a range of the half
//! cross-section given by two normal angles in degrees (see [`DECK`], [`BOTTOM`], [`WHOLE`]). The returned point
//! is `(x, distance from stringer, height)` in the board frame.
//!
//! On an asymmetric board each rail has its own surface: the functions taking a [`Rail`] scale the sections to
//! that rail's half width and return the point on that rail (y < 0 for the right rail). The functions without a
//! rail evaluate the y >= 0 (left) half, which is the whole story for a symmetric board.

use glam::{DVec2, DVec3};

use crate::asym::Rail;
use crate::board::{Board, SurfaceModel, clamp_soft};
use crate::math::{DEG, ONE, ZERO, find_root, polyline_length, trapezoid};
use crate::section::{CrossSection, scaled_copy, thickness_near_stringer};
use crate::spline::Spline;

/// Deck centre (`s = 0`) to rail apex (`s = 1`).
pub const DECK: (f64, f64) = (-90.0, 90.0);
/// Rail apex (`s = 0`) to bottom centre (`s = 1`).
pub const BOTTOM: (f64, f64) = (90.0, 360.0);
/// Deck centre (`s = 0`) round the rail to the bottom centre (`s = 1`).
pub const WHOLE: (f64, f64) = (-90.0, 360.0);

/// Arc-length fractions bounding the requested part of a section curve.
fn s_range(spline: &Spline, start: f64, end: f64, clamp: bool) -> (f64, f64) {
    let mut s_min = if start > 0.0 {
        spline.s_by_normal_reverse(start * DEG, true)
    } else {
        ONE
    };
    let mut s_max = if end < 270.0 {
        spline.s_by_normal_reverse(end * DEG, true)
    } else {
        ZERO
    };
    if clamp {
        s_min = s_min.min(ONE);
        s_max = s_max.max(ZERO);
    }
    (s_min, s_max)
}

/// One section curve prepared for evaluation over a parameter range.
#[derive(Clone, Debug)]
struct Ranged {
    spline: Spline,
    s_min: f64,
    s_max: f64,
}

impl Ranged {
    fn new(spline: Spline, start: f64, end: f64, clamp: bool) -> Self {
        let (s_min, s_max) = s_range(&spline, start, end, clamp);
        Self {
            spline,
            s_min,
            s_max,
        }
    }

    fn s_cur(&self, s: f64) -> f64 {
        (self.s_max - self.s_min) * s + self.s_min
    }

    fn at(&self, s_cur: f64) -> DVec2 {
        self.spline.point_at_s(s_cur)
    }
}

/// Everything needed to evaluate the surface at one station for one parameter range, so that many `s` values
/// can be evaluated without recomputing the section shapes.
#[derive(Clone, Debug)]
pub struct Station {
    x: f64,
    rocker: f64,
    kind: StationKind,
}

#[derive(Clone, Debug)]
enum StationKind {
    ControlPoint(Ranged),
    /// The two neighbouring sections scaled to the station, blended with weight `d` towards the second.
    SBlend([Ranged; 2], f64),
}

impl Station {
    /// Prepare station `x` (clamped to 0.1 cm inside the board) for the range `(start, end)` in degrees, on
    /// the y >= 0 half (the left rail).
    pub fn new(board: &Board, x: f64, start: f64, end: f64) -> Station {
        Station::new_on(board, x, start, end, Rail::Left)
    }

    /// [`Self::new`] for one rail: the section is scaled to that rail's half width. The points are still
    /// returned on the y >= 0 side; [`Rail::place`] puts them on the rail.
    pub fn new_on(board: &Board, x: f64, start: f64, end: f64, rail: Rail) -> Station {
        let l = board.length();
        let x = clamp_soft(x, 0.1, l - 0.1);
        let rocker = board.rocker_at(x);
        let kind = match board.model {
            SurfaceModel::ControlPoint => match board.interpolated_section_on(x, rail) {
                Some(s) => StationKind::ControlPoint(Ranged::new(s.spline, start, end, true)),
                None => StationKind::ControlPoint(Ranged::new(
                    fallback_at(board, x, rail),
                    start,
                    end,
                    true,
                )),
            },
            SurfaceModel::SBlend => sblend_kind(board, x, start, end, rail),
        };
        Station { x, rocker, kind }
    }

    /// The (clamped) station.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Point in the section plane (lateral, height above the bottom on the stringer) at parameter `s`.
    pub fn section_point(&self, s: f64) -> DVec2 {
        match &self.kind {
            StationKind::ControlPoint(r) => r.at(r.s_cur(s)),
            StationKind::SBlend([a, b], d) => {
                let (p, q) = (a.at(a.s_cur(s)), b.at(b.s_cur(s)));
                (1.0 - d) * p + d * q
            }
        }
    }

    /// Surface point at parameter `s` in the board frame.
    pub fn point(&self, s: f64) -> DVec3 {
        let p = self.section_point(s);
        DVec3::new(self.x, p.x, p.y + self.rocker)
    }

    /// Section point next to `s` (0.01 further along each curve, or 0.01 back near the end of the range) and
    /// whether the backward step was taken.
    fn offset_point(&self, s: f64) -> (DVec2, bool) {
        match &self.kind {
            StationKind::ControlPoint(r) => {
                let sc = r.s_cur(s);
                let (so, flip) = if sc + 0.01 > 1.0 {
                    (sc - 0.01, true)
                } else {
                    (sc + 0.01, false)
                };
                (r.at(so), flip)
            }
            StationKind::SBlend([a, b], d) => {
                let (sa, sb) = (a.s_cur(s), b.s_cur(s));
                let flip = sa + 0.01 > 1.0 || sb + 0.01 > 1.0;
                let o = if flip { -0.01 } else { 0.01 };
                ((1.0 - d) * a.at(sa + o) + d * b.at(sb + o), flip)
            }
        }
    }
}

/// The fallback shape scaled to a rail's local width and thickness.
fn fallback_at(board: &Board, x: f64, rail: Rail) -> Spline {
    let mut s = CrossSection::fallback_shape();
    let w = 2.0 * board.half_width_at(x, rail);
    let (w, t) = (w.max(0.0), board.thickness_at(x).max(0.0));
    s.scale(w / 2.0, t);
    s
}

fn sblend_kind(board: &Board, x: f64, start: f64, end: f64, rail: Rail) -> StationKind {
    let w = 2.0 * board.half_width_at(x, rail);
    let t = board.thickness_at(x);
    let fallback = || {
        let r = Ranged::new(fallback_at(board, x, rail), start, end, false);
        StationKind::SBlend([r.clone(), r], 0.0)
    };
    let (Some(i1), Some(i2), Some(p1), Some(p2)) = (
        board.previous_section_index(x),
        board.next_section_index(x),
        board.previous_section_position(x),
        board.next_section_position(x),
    ) else {
        return fallback();
    };
    let scaled = |i: usize| -> Option<Spline> {
        let c = &board.sections[i];
        let (wk, tk) = (c.width(), thickness_near_stringer(c));
        let (sx, sy) = (w / wk, t / tk);
        (sx.is_finite() && sy.is_finite() && c.spline.segment_count() > 0)
            .then(|| scaled_copy(&c.spline, sx, sy))
    };
    let (Some(a), Some(b)) = (scaled(i1), scaled(i2)) else {
        return fallback();
    };
    let mut d = (x - p1) / (p2 - p1);
    if !d.is_finite() {
        d = 0.0;
    }
    StationKind::SBlend(
        [
            Ranged::new(a, start, end, false),
            Ranged::new(b, start, end, false),
        ],
        d,
    )
}

impl Board {
    /// Surface point at station `x` and parameter `s` over the range `(start, end)` of normal angles in
    /// degrees, using the board's surface model.
    pub fn surface_point(&self, x: f64, s: f64, start: f64, end: f64) -> DVec3 {
        if self.outline.is_empty() {
            return DVec3::ZERO;
        }
        Station::new(self, x, start, end).point(s)
    }

    /// [`Self::surface_point`] on one rail (y < 0 on the right rail).
    pub fn surface_point_on(&self, x: f64, s: f64, start: f64, end: f64, rail: Rail) -> DVec3 {
        if self.outline.is_empty() {
            return DVec3::ZERO;
        }
        rail.place(Station::new_on(self, x, start, end, rail).point(s))
    }

    /// Point of the half cross-section at `(x, s)` over the whole section, without the rocker.
    pub fn section_point(&self, x: f64, s: f64) -> DVec2 {
        if self.outline.is_empty() {
            return DVec2::ZERO;
        }
        Station::new(self, x, WHOLE.0, WHOLE.1).section_point(s)
    }

    /// Prepared station for evaluating many surface points at one `x`.
    pub fn station(&self, x: f64, start: f64, end: f64) -> Station {
        Station::new(self, x, start, end)
    }

    /// Prepared station of one rail (points on the y >= 0 side, see [`Station::new_on`]).
    pub fn station_on(&self, x: f64, start: f64, end: f64, rail: Rail) -> Station {
        Station::new_on(self, x, start, end, rail)
    }

    /// Outward unit normal of the surface at `(x, s)` (up on the deck, down on the bottom, outward at the rail).
    pub fn surface_normal(&self, x: f64, s: f64, start: f64, end: f64) -> DVec3 {
        self.surface_normal_y(x, s, start, end, Rail::Left)
    }

    /// [`Self::surface_normal`] on one rail (pointing towards -y at the right rail).
    pub fn surface_normal_on(&self, x: f64, s: f64, start: f64, end: f64, rail: Rail) -> DVec3 {
        rail.place(self.surface_normal_y(x, s, start, end, rail))
    }

    /// Normal of a rail's surface computed on the y >= 0 side.
    fn surface_normal_y(&self, x: f64, s: f64, start: f64, end: f64, rail: Rail) -> DVec3 {
        if self.outline.is_empty() {
            return DVec3::Z;
        }
        let l = self.length();
        let n = match self.model {
            SurfaceModel::ControlPoint => {
                let x = clamp_soft(clamp_soft(x, 0.1, l - 0.1), 1.0, l - 1.0);
                let xo = x + 0.1;
                let st = Station::new_on(self, x, start, end, rail);
                let StationKind::ControlPoint(r) = &st.kind else {
                    return DVec3::Z;
                };
                let sc = r.s_cur(s);
                let s2 = self
                    .interpolated_section_on(xo, rail)
                    .map_or_else(|| fallback_at(self, xo, rail), |c| c.spline);
                let (p, (ps, flip)) = (r.at(sc), st.offset_point(s));
                let px = s2.point_at_s(sc);
                let (rk, ro) = (self.rocker_at(x), self.rocker_at(xo));
                let vc = DVec3::new(0.0, p.x - ps.x, p.y - ps.y).normalize_or_zero();
                let vl = DVec3::new(0.1, px.x - p.x, px.y - p.y + ro - rk).normalize_or_zero();
                let n = vl.cross(vc).normalize_or_zero();
                if flip { -n } else { n }
            }
            SurfaceModel::SBlend => {
                let st = Station::new_on(self, x, start, end, rail);
                let p = st.point(s);
                let (ps, flip) = st.offset_point(s);
                let ps = DVec3::new(st.x, ps.x, ps.y + st.rocker);
                let pxo = Station::new_on(self, st.x + 0.1, start, end, rail).point(s);
                let vc = DVec3::new(0.0, p.y - ps.y, p.z - ps.z);
                let vl = DVec3::new(0.1, pxo.y - p.y, pxo.z - p.z);
                let n = vl.cross(vc).normalize_or_zero();
                if flip { -n } else { n }
            }
        };
        if n == DVec3::ZERO { DVec3::Z } else { n }
    }

    /// Area of the cross-section at station `x` (cm²). `splits` is the number of integration steps of the
    /// control-point model (the section-blend model always uses 10).
    pub fn section_area_at(&self, x: f64, splits: usize) -> f64 {
        if self.sections.len() < 3 || self.outline.is_empty() {
            return 0.0;
        }
        let area = if self.is_asymmetric() {
            let half = |rail| self.half_section_area(x, splits, rail).unwrap_or(0.0);
            half(Rail::Left) + half(Rail::Right)
        } else {
            match self.half_section_area(x, splits, Rail::Left) {
                Some(a) => 2.0 * a,
                None => return 0.0,
            }
        };
        if area.is_finite() { area.max(0.0) } else { 0.0 }
    }

    /// Area of the half of the section at `x` on `rail`; `None` where the control-point model has no section.
    fn half_section_area(&self, x: f64, splits: usize, rail: Rail) -> Option<f64> {
        Some(match self.model {
            SurfaceModel::ControlPoint => {
                let s = self.interpolated_section_on(x, rail)?;
                let sp = &s.spline;
                let gr = sp.g_for_normal(90.0 * DEG);
                let n = splits.max(1);
                let deck = trapezoid(|g| sp.point_by_g(g), gr, 1.0, n);
                let bottom = trapezoid(|g| sp.point_by_g(g), 0.0, gr, n);
                deck - bottom
            }
            SurfaceModel::SBlend => {
                let deck = Station::new_on(self, x, DECK.0, DECK.1, rail);
                let bottom = Station::new_on(self, x, BOTTOM.0, BOTTOM.1, rail);
                let uz = |st: &Station, s: f64| {
                    let p = st.point(s);
                    DVec2::new(p.y, p.z)
                };
                let a = trapezoid(|s| uz(&deck, s), 0.0, 1.0, 10);
                let b = trapezoid(|s| uz(&bottom, s), 0.0, 1.0, 10);
                a - b
            }
        })
    }

    /// Girth of the deck (`deck = true`) or of the bottom of the half section at `x`: the length of the curve
    /// from the stringer to the rail apex, measured as a 10-step polyline.
    pub fn half_girth(&self, x: f64, deck: bool) -> f64 {
        self.half_girth_on(x, deck, Rail::Left)
    }

    /// [`Self::half_girth`] of the half on one rail.
    pub fn half_girth_on(&self, x: f64, deck: bool, rail: Rail) -> f64 {
        let (a, b) = if deck { DECK } else { BOTTOM };
        let st = Station::new_on(self, x, a, b, rail);
        polyline_length(
            |s| {
                let p = st.point(s);
                DVec2::new(p.y, p.z)
            },
            0.0,
            1.0,
            10,
        )
    }

    /// Surface point on the deck (`deck = true`) or the bottom at lateral distance `y` from the stringer.
    fn point_at_lateral(&self, x: f64, y: f64, deck: bool, rail: Rail) -> DVec3 {
        let (a, b) = if deck { DECK } else { (90.0, 270.0) };
        let st = Station::new_on(self, x, a, b, rail);
        let s = find_root(|s| st.point(s).y, y, 0.0, 1.0);
        st.point(s)
    }

    /// Rail and lateral distance of a point `(x, y)`: on an asymmetric board the sign of `y` picks the rail
    /// (y < 0 is the right rail); a symmetric board uses `y` as given.
    fn lateral(&self, y: f64) -> (Rail, f64) {
        if self.is_asymmetric() {
            (Rail::of_y(y), y.abs())
        } else {
            (Rail::Left, y)
        }
    }

    /// Height of the deck at `(x, y)`; 0 outside the outline.
    pub fn deck_at_xy(&self, x: f64, y: f64) -> f64 {
        let (rail, y) = self.lateral(y);
        if self.half_width_at(x, rail) < y {
            return 0.0;
        }
        if y == 0.0 {
            return self.deck_at(x);
        }
        self.point_at_lateral(x, y, true, rail).z
    }

    /// Height of the bottom at `(x, y)`; 0 outside the outline.
    pub fn bottom_at_xy(&self, x: f64, y: f64) -> f64 {
        let (rail, y) = self.lateral(y);
        if self.half_width_at(x, rail) < y {
            return 0.0;
        }
        if y == 0.0 {
            return self.rocker_at(x);
        }
        self.point_at_lateral(x, y, false, rail).z
    }

    /// Thickness at `(x, y)`.
    pub fn thickness_at_xy(&self, x: f64, y: f64) -> f64 {
        self.deck_at_xy(x, y) - self.bottom_at_xy(x, y)
    }

    fn height_gradient(&self, x: f64, y: f64, f: impl Fn(f64, f64) -> f64) -> DVec2 {
        const H: f64 = 0.01;
        let v = f(x, y);
        let gx = if x - H >= 0.0 {
            (v - f(x - H, y)) / H
        } else {
            (f(x + H, y) - v) / H
        };
        let gy = if y - H >= 0.0 {
            (v - f(x, y - H)) / H
        } else {
            (f(x, y + H) - v) / H
        };
        DVec2::new(gx, gy)
    }

    /// Outward (upward) unit normal of the deck at `(x, y)`.
    pub fn deck_normal_at(&self, x: f64, y: f64) -> DVec3 {
        let g = self.height_gradient(x, y, |x, y| self.deck_at_xy(x, y));
        DVec3::new(-g.x, -g.y, 1.0).normalize_or(DVec3::Z)
    }

    /// Outward (downward) unit normal of the bottom at `(x, y)`.
    pub fn bottom_normal_at(&self, x: f64, y: f64) -> DVec3 {
        let g = self.height_gradient(x, y, |x, y| self.bottom_at_xy(x, y));
        DVec3::new(g.x, g.y, -1.0).normalize_or(DVec3::NEG_Z)
    }

    /// Polyline of a rail feature line: surface points at the end of the range (-45°, `angle`), from the tail
    /// to the nose with `steps` points between consecutive sections.
    pub fn feature_line(&self, angle: f64, steps: usize) -> Vec<DVec3> {
        self.feature_line_on(angle, steps, Rail::Left)
    }

    /// [`Self::feature_line`] along one rail (y < 0 on the right rail).
    pub fn feature_line_on(&self, angle: f64, steps: usize, rail: Rail) -> Vec<DVec3> {
        let mut xs = vec![0.01];
        let mut prev = 0.0;
        for s in self.sections.iter().skip(1) {
            let next = s.position;
            for i in 1..=steps.max(1) {
                xs.push(prev + (next - prev) * i as f64 / steps.max(1) as f64);
            }
            prev = next;
        }
        use rayon::prelude::*;
        xs.par_iter()
            .map(|&x| self.surface_point_on(x, 1.0, -45.0, angle, rail))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::mesh::tests::toy_board;
    use crate::{Board, SurfaceModel};

    use super::*;

    fn both(b: &Board) -> [Board; 2] {
        let mut cp = b.clone();
        cp.model = SurfaceModel::ControlPoint;
        let mut sb = b.clone();
        sb.model = SurfaceModel::SBlend;
        [cp, sb]
    }

    #[test]
    fn ranges_meet_at_the_apex() {
        for b in both(&toy_board()) {
            let deck_end = b.surface_point(90.0, 1.0, DECK.0, DECK.1);
            let bottom_start = b.surface_point(90.0, 0.0, BOTTOM.0, BOTTOM.1);
            assert!((deck_end - bottom_start).length() < 1e-6, "{:?}", b.model);
            // The apex is the widest point: half the outline width.
            assert!((deck_end.y - b.width_at(90.0) / 2.0).abs() < 0.05);
            let deck_centre = b.surface_point(90.0, 0.0, DECK.0, DECK.1);
            // Within the arc-length tolerance of the stringer.
            assert!(deck_centre.y.abs() < 2e-3);
            assert!((deck_centre.z - b.deck_at(90.0)).abs() < 1e-3);
        }
    }

    #[test]
    fn normals_point_outward() {
        for b in both(&toy_board()) {
            let up = b.surface_normal(90.0, 0.3, DECK.0, DECK.1);
            assert!(up.z > 0.5, "{:?} {up}", b.model);
            let down = b.surface_normal(90.0, 0.7, BOTTOM.0, BOTTOM.1);
            assert!(down.z < -0.5, "{:?} {down}", b.model);
            let out = b.surface_normal(90.0, 1.0, DECK.0, DECK.1);
            assert!(out.y > 0.5, "{:?} {out}", b.model);
        }
    }

    #[test]
    fn section_area_and_xy_heights() {
        for b in both(&toy_board()) {
            let a = b.section_area_at(90.0, 10);
            // A 50 x 6 section with rounded rails: a little less than the bounding rectangle.
            assert!(a > 200.0 && a < 300.0, "{:?} {a}", b.model);
            // The deck crowns from 6 cm on the stringer towards the rail.
            let d = b.deck_at_xy(90.0, 3.0);
            assert!(d > 5.8 && d <= 6.01, "{d}");
            assert!(b.deck_at_xy(90.0, 20.0) < d);
            let n = b.deck_normal_at(90.0, 10.0);
            assert!(n.z > 0.99);
            let n = b.bottom_normal_at(90.0, 10.0);
            assert!(n.z < -0.99);
            assert_eq!(b.deck_at_xy(90.0, 40.0), 0.0);
        }
    }

    #[test]
    fn degenerate_boards_do_not_panic() {
        let mut b = toy_board();
        b.sections.truncate(2);
        for b in both(&b) {
            let _ = b.surface_point(50.0, 0.5, WHOLE.0, WHOLE.1);
            let _ = b.surface_normal(50.0, 0.5, DECK.0, DECK.1);
            assert_eq!(b.volume(), 0.0);
        }
        let empty = Board::default();
        assert_eq!(empty.surface_point(1.0, 0.5, 0.0, 0.0), DVec3::ZERO);
        assert_eq!(empty.volume(), 0.0);
    }
}
