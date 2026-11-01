//! A single cubic Bezier segment and the numerical searches done on it.
//!
//! The searches reproduce the sampling grids, tolerances and termination rules of the reference program
//! (see `docs/spec/geometry.md` §3), because the reference values of the test suite depend on them.

use glam::DVec2;

use crate::math::{DEG, ONE, ZERO};

/// Tolerance on x when solving for the parameter at a given x (cm).
const X_TOL: f64 = 0.003;
/// Tolerance of the adaptive arc length (cm) and of its smallest parameter interval.
const LENGTH_TOL: f64 = 0.001;
/// Tolerance of the tangent-angle search (radians).
pub(crate) const ANGLE_TOL: f64 = 0.05 * DEG;

/// Cubic Bezier segment with its power-basis coefficients.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    /// Control points: start point, start handle, end handle, end point.
    pub p: [DVec2; 4],
    a: DVec2,
    b: DVec2,
    c: DVec2,
}

/// Result of splitting a segment at a parameter.
#[derive(Clone, Copy, Debug)]
pub struct Split {
    /// New start handle of the first half (replaces the start knot's next handle).
    pub start_next: DVec2,
    /// The new knot: point, previous handle, next handle.
    pub point: DVec2,
    pub prev: DVec2,
    pub next: DVec2,
    /// New end handle of the second half (replaces the end knot's previous handle).
    pub end_prev: DVec2,
}

impl Segment {
    pub fn new(p0: DVec2, p1: DVec2, p2: DVec2, p3: DVec2) -> Self {
        Self {
            p: [p0, p1, p2, p3],
            a: p3 - 3.0 * p2 + 3.0 * p1 - p0,
            b: 3.0 * (p2 - 2.0 * p1 + p0),
            c: 3.0 * (p1 - p0),
        }
    }

    /// Point at parameter `t` (any real `t`; outside `[0, 1]` the polynomial is extrapolated).
    #[inline]
    pub fn point(&self, t: f64) -> DVec2 {
        ((self.a * t + self.b) * t + self.c) * t + self.p[0]
    }

    #[inline]
    fn x(&self, t: f64) -> f64 {
        ((self.a.x * t + self.b.x) * t + self.c.x) * t + self.p[0].x
    }

    #[inline]
    fn y(&self, t: f64) -> f64 {
        ((self.a.y * t + self.b.y) * t + self.c.y) * t + self.p[0].y
    }

    /// First derivative with respect to `t`.
    #[inline]
    pub fn derivative(&self, t: f64) -> DVec2 {
        (3.0 * self.a * t + 2.0 * self.b) * t + self.c
    }

    /// Second derivative with respect to `t`.
    #[inline]
    pub fn second_derivative(&self, t: f64) -> DVec2 {
        6.0 * self.a * t + 2.0 * self.b
    }

    /// Direction of travel as an angle measured from "up" (+y) towards +x, in `(-π, π]`.
    #[inline]
    pub fn tangent_angle(&self, t: f64) -> f64 {
        let d = self.derivative(t);
        d.x.atan2(d.y)
    }

    /// Signed curvature (positive when turning counter-clockwise).
    pub fn curvature(&self, t: f64) -> f64 {
        let d = self.derivative(t);
        let dd = self.second_derivative(t);
        (d.x * dd.y - d.y * dd.x) / (d.x * d.x + d.y * d.y).powf(1.5)
    }

    /// Arc length between two parameters by adaptive subdivision.
    pub fn length_between(&self, t0: f64, t1: f64) -> f64 {
        self.length_rec(t0, self.point(t0), t1, self.point(t1))
    }

    fn length_rec(&self, t0: f64, p0: DVec2, t1: f64, p1: DVec2) -> f64 {
        let tm = t0 + (t1 - t0) / 2.0;
        let pm = self.point(tm);
        let poly = p0.distance(pm) + pm.distance(p1);
        let chord = p0.distance(p1);
        if (poly - chord) > LENGTH_TOL && (t1 - t0) > LENGTH_TOL {
            self.length_rec(t0, p0, tm, pm) + self.length_rec(tm, pm, t1, p1)
        } else {
            poly
        }
    }

    /// Length of the whole segment.
    pub fn length(&self) -> f64 {
        self.length_between(ZERO, ONE)
    }

    /// Parameter at which the arc length from the start reaches `target`.
    pub fn t_for_length(&self, target: f64) -> f64 {
        let (mut t0, mut t1, mut target) = (ZERO, ONE, target);
        loop {
            let ts = t0 + (t1 - t0) / 2.0;
            if (t0 - t1).abs() < 1e-5 {
                return t0;
            }
            let sl = self.length_between(t0, ts);
            if (sl - target).abs() > LENGTH_TOL {
                if sl > target {
                    t1 = ts;
                } else {
                    t0 = ts;
                    target -= sl;
                }
            } else {
                return ts;
            }
        }
    }

    /// Grid search for the extreme of a coordinate, refined around the best sample.
    /// Returns `(value, parameter)`.
    fn refine_extreme(&self, f: impl Fn(f64) -> f64, max: bool) -> (f64, f64) {
        self.refine_extreme_in(f, max, 0.0, 1.0, 96)
    }

    fn refine_extreme_in(
        &self,
        f: impl Fn(f64) -> f64,
        max: bool,
        mut t0: f64,
        mut t1: f64,
        mut n: usize,
    ) -> (f64, f64) {
        loop {
            let seg = (t1 - t0) / n as f64;
            let mut best = if max { -1e7 } else { 1e7 };
            let mut best_t = 0.0;
            for i in 0..n {
                let t = seg * i as f64 + t0;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let v = f(t);
                if (max && v >= best) || (!max && v <= best) {
                    best = v;
                    best_t = t;
                }
            }
            // The termination test compares the best parameter with the half-width of the interval (not its
            // midpoint), as the reference program does.
            if (best_t - (t1 - t0) / 2.0) < 0.0001 || n <= 2 {
                return (best, best_t);
            }
            t0 = best_t - seg;
            t1 = best_t + seg;
            n /= 2;
        }
    }

    pub fn max_x(&self) -> f64 {
        self.refine_extreme(|t| self.x(t), true).0
    }

    pub fn min_x(&self) -> f64 {
        self.refine_extreme(|t| self.x(t), false).0
    }

    pub fn max_y(&self) -> f64 {
        self.refine_extreme(|t| self.y(t), true).0
    }

    pub fn min_y(&self) -> f64 {
        self.refine_extreme(|t| self.y(t), false).0
    }

    /// Parameter of the largest y.
    pub fn t_of_max_y(&self) -> f64 {
        self.refine_extreme(|t| self.y(t), true).1
    }

    /// Largest y between two parameters (96-sample refinement of that interval).
    pub fn max_y_between(&self, t0: f64, t1: f64) -> (f64, f64) {
        self.refine_extreme_in(|t| self.y(t), true, t0, t1, 96)
    }

    /// Fallback search for the parameter at a given x: grid sampling refined around the closest sample.
    fn t_for_x_by_subdivision(&self, x: f64) -> f64 {
        let (mut t0, mut t1, mut n) = (0.0, 1.0, 96usize);
        loop {
            let seg = (t1 - t0) / n as f64;
            let mut best_err = 1e9;
            let mut best_t = 0.0;
            for i in 1..n {
                let t = seg * i as f64 + t0;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let e = (x - self.x(t)).abs();
                if e < best_err {
                    best_err = e;
                    best_t = t;
                }
            }
            if best_err < X_TOL || (best_t - (t1 - t0) / 2.0).abs() < 0.0001 || n <= 2 {
                return best_t;
            }
            t0 = best_t - seg;
            t1 = best_t + seg;
            n /= 2;
        }
    }

    /// Parameter where the curve has abscissa `x`: Newton iteration from `t_start`, falling back to a grid
    /// search when it leaves the segment or does not converge.
    pub fn t_for_x(&self, x: f64, t_start: f64) -> f64 {
        let mut tn = t_start;
        let mut e = x - self.x(tn);
        let mut n = 0;
        while e.abs() > X_TOL && {
            n += 1;
            n <= 30
        } {
            tn += e / self.derivative(tn).x;
            e = x - self.x(tn);
        }
        if !(0.0..=1.0).contains(&tn) || n >= 30 || e.abs() > X_TOL {
            tn = self.t_for_x_by_subdivision(x);
        }
        tn
    }

    /// Starting guess for Newton: the linear interpolation of `x` between the end points.
    pub fn x_guess(&self, x: f64) -> f64 {
        (x - self.p[0].x) / (self.p[3].x - self.p[0].x)
    }

    /// Starting guess measured from the end point (used by the tangent and curvature queries).
    pub fn x_guess_mirrored(&self, x: f64) -> f64 {
        (x - self.p[3].x) / (self.p[0].x - self.p[3].x)
    }

    /// y at abscissa `x`.
    pub fn y_at_x(&self, x: f64) -> f64 {
        self.y(self.t_for_x(x, self.x_guess(x)))
    }

    /// Tangent angle at abscissa `x`.
    pub fn tangent_at_x(&self, x: f64) -> f64 {
        self.tangent_angle(self.t_for_x(x, self.x_guess_mirrored(x)))
    }

    /// Curvature at abscissa `x`.
    pub fn curvature_at_x(&self, x: f64) -> f64 {
        self.curvature(self.t_for_x(x, self.x_guess_mirrored(x)))
    }

    /// Parameter at which the tangent angle equals `target` (radians), starting from two parameter guesses.
    ///
    /// A secant-like iteration (in the reference program's form, which multiplies the error by the slope) is
    /// tried first; if it fails or leaves the segment, a bisection that assumes a decreasing tangent angle
    /// (true for cross-sections, which turn clockwise from the bottom to the deck) finishes the job.
    pub fn t_for_tangent(&self, target: f64, mut t_cur: f64, mut t_last: f64) -> f64 {
        let mut a_cur = self.tangent_angle(t_cur);
        let mut a_last = self.tangent_angle(t_last);
        let mut e = target - a_cur;
        let mut n = 0;
        while e.abs() > ANGLE_TOL
            && {
                n += 1;
                n <= 50
            }
            && t_cur > ZERO
            && t_cur < ONE
        {
            let slope = (a_cur - a_last) / (t_cur - t_last);
            t_last = t_cur;
            t_cur += e * slope;
            a_last = a_cur;
            a_cur = self.tangent_angle(t_cur);
            e = target - a_cur;
        }
        let good = (self.tangent_angle(t_cur) - target).abs() <= ANGLE_TOL;
        if !good || !(t_cur >= ZERO) || !(t_cur <= ONE) {
            let (mut lo, mut hi) = (0.0, 1.0);
            if e.is_nan() {
                e = target - self.tangent_angle(0.5);
            }
            let mut n = 0;
            while e.abs() > ANGLE_TOL
                && {
                    n += 1;
                    n <= 50
                }
                && (hi - lo) > 1e-5
            {
                t_cur = lo + (hi - lo) / 2.0;
                e = target - self.tangent_angle(t_cur);
                if e < 0.0 {
                    lo = t_cur;
                } else {
                    hi = t_cur;
                }
            }
        }
        t_cur
    }

    /// Parameter of the point nearest to `q` (32-sample grid, refined).
    pub fn nearest_t(&self, q: DVec2) -> f64 {
        let (mut t0, mut t1, mut n) = (0.0, 1.0, 32usize);
        loop {
            let seg = (t1 - t0) / n as f64;
            let mut best = 1e9;
            let mut best_t = 0.0;
            for i in 0..n {
                let t = seg * i as f64 + t0;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let d = self.point(t).distance(q);
                if d <= best {
                    best = d;
                    best_t = t;
                }
            }
            if (best_t - (t1 - t0) / 2.0) < 0.001 || n <= 2 {
                return best_t;
            }
            t0 = best_t - seg;
            t1 = best_t + seg;
            n /= 2;
        }
    }

    /// Parameter where the distance from `q` equals `d` (machining helper; 96-sample grid, refined).
    pub fn t_at_distance(&self, q: DVec2, d: f64) -> f64 {
        let (mut t0, mut t1, mut n) = (0.0, 1.0, 96usize);
        loop {
            let seg = (t1 - t0) / n as f64;
            let mut best = 1e9;
            let mut best_t = 0.0;
            for i in 0..n {
                let t = seg * i as f64 + t0;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let err = (self.point(t).distance(q) - d).abs();
                if err < best {
                    best = err;
                    best_t = t;
                }
            }
            if (best_t - (t1 - t0) / 2.0) < 0.0001 || n <= 2 {
                return best_t;
            }
            t0 = best_t - seg;
            t1 = best_t + seg;
            n /= 2;
        }
    }

    /// Split at `t` (de Casteljau): the new knot and the shortened neighbouring handles.
    pub fn split(&self, t: f64) -> Split {
        let [p0, p1, p2, p3] = self.p;
        let q1 = p0 + t * (p1 - p0);
        let q2 = p1 + t * (p2 - p1);
        let q3 = p2 + t * (p3 - p2);
        let r2 = q1 + t * (q2 - q1);
        let r3 = q2 + t * (q3 - q2);
        let r1 = r2 + t * (r3 - r2);
        Split {
            start_next: q1,
            point: r1,
            prev: r2,
            next: r3,
            end_prev: p3 + (1.0 - t) * (p2 - p3),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarter_circle() -> Segment {
        // Standard approximation of a unit quarter circle from (1, 0) to (0, 1).
        let k = 0.552_284_749_830_793_4;
        Segment::new(
            DVec2::new(1.0, 0.0),
            DVec2::new(1.0, k),
            DVec2::new(k, 1.0),
            DVec2::new(0.0, 1.0),
        )
    }

    fn line() -> Segment {
        Segment::new(
            DVec2::ZERO,
            DVec2::new(1.0, 1.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(3.0, 3.0),
        )
    }

    #[test]
    fn evaluation_matches_bernstein_form() {
        let s = quarter_circle();
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let u = 1.0 - t;
            let b = s.p[0] * u * u * u
                + s.p[1] * 3.0 * u * u * t
                + s.p[2] * 3.0 * u * t * t
                + s.p[3] * t * t * t;
            assert!((s.point(t) - b).length() < 1e-14);
        }
        assert_eq!(s.point(0.0), s.p[0]);
        assert!((s.point(1.0) - s.p[3]).length() < 1e-15);
    }

    #[test]
    fn length_of_line_and_arc() {
        assert!((line().length() - 18f64.sqrt()).abs() < 1e-6);
        let arc = quarter_circle().length();
        assert!((arc - std::f64::consts::FRAC_PI_2).abs() < 1e-3, "{arc}");
    }

    #[test]
    fn t_for_length_inverts_length() {
        let s = quarter_circle();
        let l = s.length();
        for f in [0.1, 0.5, 0.9] {
            let t = s.t_for_length(f * l);
            assert!((s.length_between(ZERO, t) - f * l).abs() < 0.002);
        }
    }

    #[test]
    fn curvature_of_circle_is_one() {
        let s = quarter_circle();
        for t in [0.2, 0.5, 0.8] {
            assert!((s.curvature(t) - 1.0).abs() < 0.01);
        }
        assert_eq!(line().curvature(0.3), 0.0);
    }

    #[test]
    fn tangent_angle_convention() {
        // Travelling +x is +90 degrees; travelling +y is 0.
        let s = Segment::new(
            DVec2::ZERO,
            DVec2::new(1.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(3.0, 0.0),
        );
        assert!((s.tangent_angle(0.5) - 90.0 * DEG).abs() < 1e-12);
        let q = quarter_circle();
        // Starts travelling up, ends travelling in -x.
        assert!(q.tangent_angle(ZERO).abs() < 1e-6);
        assert!((q.tangent_angle(ONE) + 90.0 * DEG).abs() < 1e-6);
    }

    #[test]
    fn t_for_x_and_y_at_x() {
        let s = quarter_circle();
        for x in [0.1, 0.5, 0.9] {
            let y = s.y_at_x(x);
            assert!((y - (1.0 - x * x).sqrt()).abs() < 0.01, "x {x}: {y}");
        }
    }

    #[test]
    fn t_for_tangent_finds_angle() {
        let q = quarter_circle();
        // Tangent angle runs from 0 down to -90 degrees.
        let target = -45.0 * DEG;
        let t = q.t_for_tangent(target, 0.3, 0.4);
        assert!((q.tangent_angle(t) - target).abs() <= ANGLE_TOL);
        assert!((t - 0.5).abs() < 0.01);
    }

    #[test]
    fn extremes() {
        let q = quarter_circle();
        assert!((q.max_x() - 1.0).abs() < 1e-9);
        assert!((q.max_y() - 1.0).abs() < 1e-3);
        assert!(q.min_y().abs() < 1e-9);
    }

    #[test]
    fn split_keeps_shape() {
        let s = quarter_circle();
        let sp = s.split(0.3);
        let a = Segment::new(s.p[0], sp.start_next, sp.prev, sp.point);
        let b = Segment::new(sp.point, sp.next, sp.end_prev, s.p[3]);
        assert!((a.point(0.5) - s.point(0.15)).length() < 1e-12);
        assert!((b.point(0.5) - s.point(0.65)).length() < 1e-12);
        // Beyond the middle the grid search is refined; before it the 1/32 grid sample is returned.
        let near = s.nearest_t(s.point(0.72) + DVec2::new(0.001, 0.0));
        assert!((near - 0.72).abs() < 0.01);
        let near = s.nearest_t(s.point(0.42));
        assert!((near - 0.42).abs() <= 1.0 / 32.0);
    }
}
