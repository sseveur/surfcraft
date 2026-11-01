//! A chain of cubic Bezier segments through a list of knots, with the queries the board needs: values as a
//! function of x, extremes, arc length, normal-angle searches and nearest points.

use std::sync::OnceLock;

use glam::DVec2;

use crate::knot::{Handle, Knot};
use crate::math::{DEG, ONE, ZERO, find_root};
use crate::segment::{ANGLE_TOL, Segment};

/// Cubic Bezier spline. Segment `i` joins knots `i` and `i + 1`.
///
/// Segment lengths are cached; every mutable access to the knots clears the cache.
#[derive(Clone, Debug, Default)]
pub struct Spline {
    knots: Vec<Knot>,
    lengths: OnceLock<Vec<f64>>,
}

impl PartialEq for Spline {
    fn eq(&self, other: &Self) -> bool {
        self.knots == other.knots
    }
}

impl From<Vec<Knot>> for Spline {
    fn from(knots: Vec<Knot>) -> Self {
        Self::from_knots(knots)
    }
}

/// A spline of straight segments through `points` (handles at a third of each segment).
pub fn polyline(points: &[(f64, f64)]) -> Spline {
    let p: Vec<DVec2> = points.iter().map(|&(x, y)| DVec2::new(x, y)).collect();
    let n = p.len();
    let knots = (0..n)
        .map(|i| {
            let prev = if i > 0 {
                p[i] + (p[i - 1] - p[i]) / 3.0
            } else {
                p[i]
            };
            let next = if i + 1 < n {
                p[i] + (p[i + 1] - p[i]) / 3.0
            } else {
                p[i]
            };
            Knot::new(p[i], prev, next)
        })
        .collect();
    Spline::from_knots(knots)
}

impl Spline {
    pub fn from_knots(knots: Vec<Knot>) -> Self {
        Self {
            knots,
            lengths: OnceLock::new(),
        }
    }

    pub fn knots(&self) -> &[Knot] {
        &self.knots
    }

    /// Mutable access to the knots (clears the cached lengths).
    pub fn knots_mut(&mut self) -> &mut Vec<Knot> {
        self.lengths = OnceLock::new();
        &mut self.knots
    }

    /// Number of knots.
    pub fn len(&self) -> usize {
        self.knots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.knots.is_empty()
    }

    pub fn segment_count(&self) -> usize {
        self.knots.len().saturating_sub(1)
    }

    /// Segment `i` (between knots `i` and `i + 1`).
    pub fn segment(&self, i: usize) -> Segment {
        let (a, b) = (&self.knots[i], &self.knots[i + 1]);
        Segment::new(a.point(), a.next(), b.prev(), b.point())
    }

    pub fn segments(&self) -> impl ExactSizeIterator<Item = Segment> + '_ {
        (0..self.segment_count()).map(|i| self.segment(i))
    }

    pub fn first(&self) -> Option<&Knot> {
        self.knots.first()
    }

    pub fn last(&self) -> Option<&Knot> {
        self.knots.last()
    }

    pub fn insert(&mut self, i: usize, k: Knot) {
        self.knots_mut().insert(i, k);
    }

    pub fn remove(&mut self, i: usize) -> Knot {
        self.knots_mut().remove(i)
    }

    pub fn push(&mut self, k: Knot) {
        self.knots_mut().push(k);
    }

    /// Scale every knot about the origin: x by `sx`, y by `sy`.
    pub fn scale(&mut self, sx: f64, sy: f64) {
        for k in self.knots_mut() {
            k.scale(sx, sy);
        }
    }

    /// Translate every knot.
    pub fn translate(&mut self, d: DVec2) {
        for k in self.knots_mut() {
            k.translate(d);
        }
    }

    // ------------------------------------------------------------------------------------ lookup by x

    /// First segment whose end points bracket `x` in increasing order, else the first whose numerical
    /// x-range contains it.
    pub fn segment_for_x(&self, x: f64) -> Option<usize> {
        let n = self.segment_count();
        (0..n)
            .find(|&i| self.knots[i].point().x <= x && x <= self.knots[i + 1].point().x)
            .or_else(|| {
                (0..n).find(|&i| {
                    let s = self.segment(i);
                    s.min_x() <= x && x <= s.max_x()
                })
            })
    }

    /// Like [`Self::segment_for_x`] but scanning from the end, accepting end points in either order.
    pub fn segment_for_x_reverse(&self, x: f64) -> Option<usize> {
        let n = self.segment_count();
        (0..n)
            .rev()
            .find(|&i| {
                let (a, b) = (self.knots[i].point().x, self.knots[i + 1].point().x);
                a.min(b) <= x && x <= a.max(b)
            })
            .or_else(|| {
                (0..n).rev().find(|&i| {
                    let s = self.segment(i);
                    s.min_x() <= x && x <= s.max_x()
                })
            })
    }

    /// y at abscissa `x` (0 outside the curve).
    pub fn value_at(&self, x: f64) -> f64 {
        self.segment_for_x(x)
            .map_or(0.0, |i| self.segment(i).y_at_x(x))
    }

    /// y at abscissa `x`, searching from the end of the curve (the deck side of a cross-section).
    pub fn value_at_reverse(&self, x: f64) -> f64 {
        self.segment_for_x_reverse(x)
            .map_or(0.0, |i| self.segment(i).y_at_x(x))
    }

    /// Tangent angle (radians, from "up" towards +x) at abscissa `x`.
    pub fn tangent_at(&self, x: f64) -> f64 {
        self.segment_for_x(x)
            .map_or(0.0, |i| self.segment(i).tangent_at_x(x))
    }

    /// Unit normal at abscissa `x` (tangent turned by +90°).
    pub fn normal_at(&self, x: f64) -> DVec2 {
        let a = self.tangent_at(x) + 90.0 * DEG;
        DVec2::new(a.sin(), a.cos())
    }

    /// Signed curvature at abscissa `x`.
    pub fn curvature_at(&self, x: f64) -> f64 {
        self.segment_for_x(x)
            .map_or(0.0, |i| self.segment(i).curvature_at_x(x))
    }

    /// Integral of [`Self::value_at`] over `[a, b]` (Simpson, `n` panels).
    pub fn integral(&self, a: f64, b: f64, n: usize) -> f64 {
        crate::math::simpson(|x| self.value_at(x), a, b, n)
    }

    // ------------------------------------------------------------------------------------ extremes

    /// Fold a per-segment value; a spline without segments yields its single knot's coordinate (or 0).
    fn fold_segments(
        &self,
        f: impl Fn(&Segment) -> f64,
        max: bool,
        single: impl Fn(DVec2) -> f64,
    ) -> f64 {
        if self.segment_count() == 0 {
            return self.knots.first().map_or(0.0, |k| single(k.point()));
        }
        let mut best = if max { f64::MIN } else { f64::MAX };
        for s in self.segments() {
            let v = f(&s);
            if (max && v > best) || (!max && v < best) {
                best = v;
            }
        }
        best
    }

    pub fn max_x(&self) -> f64 {
        self.fold_segments(Segment::max_x, true, |p| p.x)
    }

    pub fn min_x(&self) -> f64 {
        self.fold_segments(Segment::min_x, false, |p| p.x)
    }

    pub fn max_y(&self) -> f64 {
        self.fold_segments(Segment::max_y, true, |p| p.y)
    }

    pub fn min_y(&self) -> f64 {
        self.fold_segments(Segment::min_y, false, |p| p.y)
    }

    /// Largest y over every segment but the last (used by cross-section matching, which was tuned with it).
    pub(crate) fn max_y_except_last(&self) -> f64 {
        let n = self.segment_count();
        if n <= 1 {
            return self.max_y();
        }
        (0..n - 1)
            .map(|i| self.segment(i).max_y())
            .fold(f64::MIN, f64::max)
    }

    /// Abscissa of the highest point.
    pub fn x_of_max_y(&self) -> f64 {
        let n = self.segment_count();
        if n == 0 {
            return self.knots.first().map_or(0.0, |k| k.point().x);
        }
        let mut best = (f64::MIN, 0);
        for i in 0..n {
            let v = self.segment(i).max_y();
            if v > best.0 {
                best = (v, i);
            }
        }
        let s = self.segment(best.1);
        s.point(s.t_of_max_y()).x
    }

    /// Highest point with abscissa in `[x0, x1]`, as `(x, y)`. `None` when no segment overlaps the range.
    pub fn max_y_in(&self, x0: f64, x1: f64) -> Option<DVec2> {
        let mut best: Option<DVec2> = None;
        for s in self.segments() {
            if s.max_x() < x0 || s.min_x() > x1 {
                continue;
            }
            let t0 = s.t_for_x(x0.max(s.p[0].x.min(s.p[3].x)), 0.1);
            let t1 = s.t_for_x(x1.min(s.p[0].x.max(s.p[3].x)), 0.9);
            let (lo, hi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
            let (y, t) = s.max_y_between(lo, hi);
            if best.is_none_or(|b| y > b.y) {
                best = Some(DVec2::new(s.point(t).x, y));
            }
        }
        best
    }

    // ------------------------------------------------------------------------------------ arc length

    /// Lengths of the segments (cached).
    pub fn segment_lengths(&self) -> &[f64] {
        self.lengths
            .get_or_init(|| self.segments().map(|s| s.length()).collect())
    }

    /// Total arc length.
    pub fn length(&self) -> f64 {
        self.segment_lengths().iter().sum()
    }

    /// Arc length from the start to the point with abscissa `x`.
    pub fn length_to_x(&self, x: f64) -> f64 {
        let Some(i) = self.segment_for_x(x) else {
            return 0.0;
        };
        let s = self.segment(i);
        let t = s.t_for_x(x, ONE);
        let before: f64 = self.segment_lengths()[..i].iter().sum();
        before + s.length_between(ZERO, t)
    }

    /// Fraction of the total length reached at abscissa `x`.
    pub fn s_at_x(&self, x: f64) -> f64 {
        self.length_to_x(x) / self.length()
    }

    /// Segment and parameter at arc length `l` from the start, walking the segments (`l` within the curve).
    fn locate_length(&self, mut l: f64) -> Option<(usize, f64)> {
        let lengths = self.segment_lengths();
        for (i, &sl) in lengths.iter().enumerate() {
            if l < sl {
                return Some((i, self.segment(i).t_for_length(l)));
            }
            l -= sl;
        }
        None
    }

    /// Point at arc length `l` from the start (clamped to the ends).
    pub fn point_at_length(&self, l: f64) -> DVec2 {
        let n = self.segment_count();
        if n == 0 {
            return self.knots.first().map_or(DVec2::ZERO, Knot::point);
        }
        if l <= 0.0 {
            return self.segment(0).point(ZERO);
        }
        if l >= self.length() {
            return self.segment(n - 1).point(ONE);
        }
        match self.locate_length(l) {
            Some((i, t)) => self.segment(i).point(t),
            None => self.segment(n - 1).point(ONE),
        }
    }

    /// Point at fraction `s` of the total length (0 = first knot, 1 = last knot).
    pub fn point_at_s(&self, s: f64) -> DVec2 {
        self.point_at_length(s * self.length())
    }

    /// Same as [`Self::point_at_s`].
    pub fn point_by_s(&self, s: f64) -> DVec2 {
        self.point_at_s(s)
    }

    /// Tangent angle at arc length `l`.
    pub fn tangent_at_length(&self, l: f64) -> f64 {
        let n = self.segment_count();
        if n == 0 {
            return 0.0;
        }
        if l <= 0.0 {
            return self.segment(0).tangent_angle(ZERO);
        }
        match self.locate_length(l) {
            Some((i, t)) => self.segment(i).tangent_angle(t),
            None => self.segment(n - 1).tangent_angle(ONE),
        }
    }

    /// Curvature at arc length `l` (0 at and beyond the ends).
    pub fn curvature_at_length(&self, l: f64) -> f64 {
        if l <= 0.0 || l >= self.length() {
            return 0.0;
        }
        self.locate_length(l)
            .map_or(0.0, |(i, t)| self.segment(i).curvature(t))
    }

    // ------------------------------------------------------------------------------------ global parameter

    /// Segment and local parameter for the global parameter `g ∈ [0, 1]` (equal share per segment).
    fn split_g(&self, g: f64) -> Option<(usize, f64)> {
        let m = self.segment_count();
        if m == 0 {
            return None;
        }
        if g >= 1.0 || g.is_nan() {
            return Some((m - 1, ONE));
        }
        let gm = g * m as f64;
        let i = (gm.floor().max(0.0) as usize).min(m - 1);
        Some((i, gm - i as f64))
    }

    pub fn point_by_g(&self, g: f64) -> DVec2 {
        match self.split_g(g) {
            Some((i, t)) => self.segment(i).point(t),
            None => self.knots.first().map_or(DVec2::ZERO, Knot::point),
        }
    }

    /// Normal angle (radians) at global parameter `g`.
    pub fn normal_by_g(&self, g: f64) -> f64 {
        self.split_g(g)
            .map_or(0.0, |(i, t)| self.segment(i).tangent_angle(t) + 90.0 * DEG)
    }

    pub fn g_for_x(&self, x: f64) -> f64 {
        find_root(|g| self.point_by_g(g).x, x, 0.0, 1.0)
    }

    pub fn g_for_normal(&self, angle: f64) -> f64 {
        find_root(|g| self.normal_by_g(g), angle, 0.0, 1.0)
    }

    // ------------------------------------------------------------------------------------ normal searches

    /// Fraction of the length (from the first knot) at which the normal angle equals `angle` (radians),
    /// searching from the last segment backwards. This is how rail features (apex, tuck-under) are found on a
    /// cross-section. With `snap_to_corner`, a target that falls into a corner returns the corner knot.
    pub fn s_by_normal_reverse(&self, angle: f64, snap_to_corner: bool) -> f64 {
        let m = self.segment_count();
        let lengths = self.segment_lengths();
        let total: f64 = lengths.iter().sum();
        if m == 0 || total.is_nan() || total <= 0.0 {
            return ZERO;
        }
        let target = angle - 90.0 * DEG;
        let mut length = 0.0;
        let mut best: Option<(f64, f64, usize)> = None; // (error, t, segment)
        let mut found = false;
        // Number of whole segments (from the start) to add in the end.
        let mut whole = 0;
        for i in (0..m).rev() {
            let s = self.segment(i);
            let a0 = s.tangent_angle(ZERO);
            let a1 = s.tangent_angle(ONE);
            if a1 > target {
                if snap_to_corner {
                    whole = i + 1;
                } else {
                    length = s.length_between(ZERO, ONE - 0.05);
                    whole = i;
                }
                found = true;
                break;
            }
            let t0 = ((target - a0) / (a1 - a0)).clamp(0.0, 1.0);
            let mut tl = t0 + 0.1;
            if tl > 1.0 {
                tl -= 0.2;
            }
            let t = s.t_for_tangent(target, t0, tl);
            let err = (s.tangent_angle(t) - target).abs();
            if err < best.map_or(1e4, |b| b.0) {
                best = Some((err, t, i));
            }
            if err <= ANGLE_TOL {
                length = s.length_between(ZERO, t);
                whole = i;
                found = true;
                break;
            }
        }
        if !found {
            match best {
                Some((_, t, i)) => {
                    length = self.segment(i).length_between(ZERO, t);
                    whole = i;
                }
                None => whole = 0,
            }
        }
        length += lengths[..whole].iter().sum::<f64>();
        let s = length / total;
        if s.is_nan() { ZERO } else { s.clamp(ZERO, ONE) }
    }

    /// Forward variant of [`Self::s_by_normal_reverse`] (used for cutter profiles): the first segment that
    /// reaches the target angle.
    pub fn s_by_normal_forward(&self, angle: f64) -> f64 {
        let target = angle - 90.0 * DEG;
        let lengths = self.segment_lengths();
        let total: f64 = lengths.iter().sum();
        let mut length = 0.0;
        for (i, s) in self.segments().enumerate() {
            let a0 = s.tangent_angle(ZERO);
            let a1 = s.tangent_angle(ONE);
            let t0 = ((target - a0) / (a1 - a0)).clamp(0.0, 1.0);
            let mut tl = t0 + 0.1;
            if tl > 1.0 {
                tl -= 0.2;
            }
            let t = s.t_for_tangent(target, t0, tl);
            if (s.tangent_angle(t) - target).abs() < ANGLE_TOL {
                length += s.length_between(ZERO, t);
                break;
            }
            length += lengths[i];
        }
        if total > 0.0 { length / total } else { 0.0 }
    }

    // ------------------------------------------------------------------------------------ nearest point

    /// Nearest segment to `q`: `(segment, parameter, distance)`; the first segment wins ties.
    pub fn nearest(&self, q: DVec2) -> Option<(usize, f64, f64)> {
        let mut best: Option<(usize, f64, f64)> = None;
        for (i, s) in self.segments().enumerate() {
            let t = s.nearest_t(q);
            let d = s.point(t).distance(q);
            if best.is_none_or(|b| d < b.2) {
                best = Some((i, t, d));
            }
        }
        best
    }

    /// Distance from `q` to the curve (infinite for a curve without segments).
    pub fn distance_to(&self, q: DVec2) -> f64 {
        self.nearest(q).map_or(f64::INFINITY, |b| b.2)
    }

    /// Insert a knot at the point of the curve nearest to `q` without changing the shape. Returns the index of
    /// the new knot, or `None` for a curve without segments.
    pub fn split_near(&mut self, q: DVec2) -> Option<usize> {
        let (i, t, _) = self.nearest(q)?;
        Some(self.split_at(i, t))
    }

    /// Like [`Self::split_near`], placing the new knot at the precise nearest point (interactive editing).
    pub fn split_near_precise(&mut self, q: DVec2) -> Option<usize> {
        let mut best: Option<(usize, f64, f64)> = None;
        for (i, s) in self.segments().enumerate() {
            let t = s.nearest_t_precise(q);
            let d = s.point(t).distance(q);
            if best.is_none_or(|b| d < b.2) {
                best = Some((i, t, d));
            }
        }
        let (i, t, _) = best?;
        Some(self.split_at(i, t))
    }

    /// Split segment `i` at parameter `t`; returns the index of the new knot.
    pub fn split_at(&mut self, i: usize, t: f64) -> usize {
        let sp = self.segment(i).split(t);
        let knots = self.knots_mut();
        knots[i].set(Handle::Next, sp.start_next);
        knots[i + 1].set(Handle::Prev, sp.end_prev);
        knots.insert(i + 1, Knot::new(sp.point, sp.prev, sp.next));
        i + 1
    }

    /// Remove interior knot `i`, lengthening the neighbouring handles so that the merged segment stays close
    /// to the old shape (the exact inverse of a split).
    pub fn remove_compensated(&mut self, i: usize) -> bool {
        if i == 0 || i + 1 >= self.knots.len() {
            return false;
        }
        let (a, k, b) = (&self.knots[i - 1], &self.knots[i], &self.knots[i + 1]);
        let tmp = Segment::new(a.point(), a.next(), b.prev(), b.point());
        let t = tmp.nearest_t(k.point());
        let knots = self.knots_mut();
        if t != 0.0 && t != 1.0 {
            let a = &mut knots[i - 1];
            let hn = a.point() + (a.next() - a.point()) / t;
            a.set(Handle::Next, hn);
            let b = &mut knots[i + 1];
            let hp = b.point() + (b.prev() - b.point()) / (1.0 - t);
            b.set(Handle::Prev, hp);
        }
        knots.remove(i);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half of a rounded cross-section-like shape: up from (0,0), round a "rail" at (10, 2), back to (0, 4).
    fn bump() -> Spline {
        Spline::from_knots(vec![
            Knot::new(DVec2::ZERO, DVec2::new(-3.0, 0.0), DVec2::new(6.0, 0.0)),
            Knot::smooth(
                DVec2::new(10.0, 2.0),
                DVec2::new(10.0, 0.8),
                DVec2::new(10.0, 3.2),
            ),
            Knot::new(
                DVec2::new(0.0, 4.0),
                DVec2::new(6.0, 4.0),
                DVec2::new(-3.0, 4.0),
            ),
        ])
    }

    #[test]
    fn values_along_x() {
        let s = polyline(&[(0.0, 0.0), (10.0, 5.0), (20.0, 5.0)]);
        assert!((s.value_at(5.0) - 2.5).abs() < 0.01);
        assert!((s.value_at(15.0) - 5.0).abs() < 1e-9);
        assert_eq!(s.value_at(25.0), 0.0);
        assert!((s.tangent_at(15.0) - 90.0 * DEG).abs() < 1e-9);
        assert!((s.length() - (125f64.sqrt() + 10.0)).abs() < 1e-6);
        assert!((s.length_to_x(15.0) - (125f64.sqrt() + 5.0)).abs() < 0.01);
    }

    #[test]
    fn extremes_include_last_segment() {
        let s = polyline(&[(0.0, 0.0), (10.0, 1.0), (20.0, 7.0)]);
        // The grid refinement stops just short of the segment end.
        assert!((s.max_y() - 7.0).abs() < 0.1);
        assert!((s.x_of_max_y() - 20.0).abs() < 0.3);
        assert!((s.max_y_except_last() - 1.0).abs() < 0.05);
        let one = Spline::from_knots(vec![Knot::default()]);
        assert_eq!(one.max_x(), 0.0);
        assert_eq!(one.length(), 0.0);
    }

    #[test]
    fn arc_length_parametrisation() {
        let s = bump();
        let l = s.length();
        assert!(s.point_at_s(0.0).length() < 1e-6);
        assert!((s.point_at_s(1.0) - DVec2::new(0.0, 4.0)).length() < 1e-6);
        let mid = s.point_at_length(l / 2.0);
        assert!((mid - DVec2::new(10.0, 2.0)).length() < 0.05, "{mid}");
    }

    #[test]
    fn normal_search_finds_apex_and_corners() {
        let s = bump();
        // Apex (normal pointing outward, 90 degrees) is the middle knot: half the length by symmetry.
        let apex = s.s_by_normal_reverse(90.0 * DEG, true);
        assert!((apex - 0.5).abs() < 0.01, "{apex}");
        let p = s.point_at_s(apex);
        assert!((p - DVec2::new(10.0, 2.0)).length() < 0.1);
        // 135 degrees lies on the lower half.
        assert!(s.s_by_normal_reverse(135.0 * DEG, true) < apex);
        // Generic root search on the global parameter gives the same apex.
        let g = s.g_for_normal(90.0 * DEG);
        assert!((s.point_by_g(g) - DVec2::new(10.0, 2.0)).length() < 0.1);
    }

    #[test]
    fn split_and_remove_keep_shape() {
        let mut s = bump();
        let before: Vec<DVec2> = (0..=20).map(|i| s.point_at_s(i as f64 / 20.0)).collect();
        let i = s.split_near(DVec2::new(8.0, 0.5)).unwrap();
        assert_eq!(i, 1);
        assert_eq!(s.len(), 4);
        for (j, p) in before.iter().enumerate() {
            assert!((s.point_at_s(j as f64 / 20.0) - *p).length() < 0.01);
        }
        assert!(s.remove_compensated(i));
        // Removal restores the knot count and approximately the shape.
        assert_eq!(s.len(), 3);
        for (j, p) in before.iter().enumerate() {
            assert!((s.point_at_s(j as f64 / 20.0) - *p).length() < 0.3);
        }
    }

    #[test]
    fn nearest_and_distance() {
        let s = polyline(&[(0.0, 0.0), (10.0, 0.0)]);
        let (i, t, d) = s.nearest(DVec2::new(4.0, 3.0)).unwrap();
        assert_eq!(i, 0);
        assert!((t - 0.4).abs() < 0.01);
        assert!((d - 3.0).abs() < 0.01);
        assert_eq!(Spline::default().distance_to(DVec2::ZERO), f64::INFINITY);
    }
}
