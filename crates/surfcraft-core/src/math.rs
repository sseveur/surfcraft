//! Small numerical helpers shared by the geometry: near-endpoint parameters, a bracketed root finder and the
//! integration rules used by the board metrics.

use glam::DVec2;
use rayon::prelude::*;

/// Curve parameter just after the start of a segment. Used instead of 0 so that the tangent direction is
/// defined even when the first handle has zero length.
pub const ZERO: f64 = 1e-10;

/// Curve parameter just before the end of a segment (see [`ZERO`]).
pub const ONE: f64 = 1.0 - 1e-10;

/// Degrees to radians.
pub const DEG: f64 = std::f64::consts::PI / 180.0;

/// Unsigned angle (radians, in `[0, π]`) between two vectors, or 0 when it is undefined (a zero vector or
/// rounding just outside the domain of `acos`).
pub fn angle_between(a: DVec2, b: DVec2) -> f64 {
    let v = (a.dot(b) / (a.length() * b.length())).acos();
    if v.is_nan() { 0.0 } else { v }
}

/// Find `x` in `[lo, hi]` with `f(x) = target` (value tolerance 0.001).
///
/// A secant iteration is tried first, starting from the linear-interpolation guess; if it does not reach the
/// tolerance a bisection over the whole bracket takes over. Both phases are limited to 50 iterations, so the
/// result is always returned, accurate or not. The starting guess is the interpolation *fraction*, which equals
/// the position only for the unit interval used by every caller.
pub fn find_root(f: impl Fn(f64) -> f64, target: f64, lo: f64, hi: f64) -> f64 {
    const TOL: f64 = 0.001;
    let fa = f(lo);
    let fb = f(hi);
    let mut x = ((target - fa) / (fb - fa)).clamp(lo, hi);
    let mut x_last = x + (hi - lo) / 10.0;
    if x_last > hi {
        x_last = x - (hi - lo) / 10.0;
    }
    let mut v = f(x);
    let mut v_last = f(x_last);
    let mut e = target - v;
    let mut n = 0;
    while e.abs() > TOL && n < 50 && v != v_last {
        n += 1;
        let d = ((x - x_last) / (v - v_last)) * e;
        x_last = x;
        x = (x + d).clamp(lo, hi);
        v_last = v;
        v = f(x);
        e = target - v;
    }
    // A NaN step leaves x as NaN (clamp keeps NaN); the bisection below then takes over.
    let reached = (f(x) - target).abs() <= TOL;
    if reached {
        return x;
    }
    let (mut a, mut b) = (lo, hi);
    let mut e_lo = f(a) - target;
    let mut cur = 1e8_f64;
    let mut x = 0.0;
    let mut n = 0;
    while cur.abs() > TOL && n < 50 && (b - a) > 1e-4 {
        n += 1;
        x = (a + b) / 2.0;
        cur = f(x) - target;
        if cur * e_lo < 0.0 {
            b = x;
        } else {
            a = x;
            e_lo = cur;
        }
    }
    x
}

/// The abscissas Simpson's rule visits over `[a, b]` with `n` panels: for each panel its start, middle and end
/// (the end of one panel is the start of the next). Positions are advanced by repeated addition.
fn simpson_nodes(a: f64, b: f64, n: usize) -> (f64, Vec<[f64; 2]>) {
    let h = (b - a) / n as f64;
    let mut p = a;
    let mut panels = Vec::with_capacity(n);
    for _ in 0..n {
        let end = p + h;
        panels.push([(p + end) / 2.0, end]);
        p = end;
    }
    (h, panels)
}

fn nan_to_zero(v: f64) -> f64 {
    if v.is_nan() { 0.0 } else { v }
}

/// Simpson's rule with `n` panels. NaN function values count as 0.
pub fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let (_, panels) = simpson_nodes(a, b, n);
    let mut sum = 0.0;
    let mut p = a;
    let mut f0 = nan_to_zero(f(a));
    for [mid, end] in panels {
        let f1 = nan_to_zero(f(mid));
        let f2 = nan_to_zero(f(end));
        sum += ((end - p) / 6.0) * (f0 + 4.0 * f1 + f2);
        f0 = f2;
        p = end;
    }
    sum
}

/// Same result as [`simpson`], with the function evaluated on all nodes in parallel. Use it for expensive
/// integrands such as cross-section areas.
pub fn simpson_par(f: impl Fn(f64) -> f64 + Sync, a: f64, b: f64, n: usize) -> f64 {
    let (_, panels) = simpson_nodes(a, b, n);
    let mut xs = Vec::with_capacity(2 * n + 1);
    xs.push(a);
    for [mid, end] in &panels {
        xs.push(*mid);
        xs.push(*end);
    }
    let ys: Vec<f64> = xs.par_iter().map(|&x| nan_to_zero(f(x))).collect();
    let mut sum = 0.0;
    let mut p = a;
    for (i, [_, end]) in panels.iter().enumerate() {
        let (f0, f1, f2) = (ys[2 * i], ys[2 * i + 1], ys[2 * i + 2]);
        sum += ((end - p) / 6.0) * (f0 + 4.0 * f1 + f2);
        p = *end;
    }
    sum
}

/// Area between a parametric curve and the axis `y = 0` by the trapezoid rule with `n` steps over `[a, b]`;
/// the x extent of each step is counted positively.
pub fn trapezoid(curve: impl Fn(f64) -> DVec2, a: f64, b: f64, n: usize) -> f64 {
    let step = (b - a) / n as f64;
    let mut s = a;
    let mut prev = curve(s);
    let mut area = 0.0;
    for _ in 0..n {
        s += step;
        let cur = curve(s);
        area += ((prev.y + cur.y) / 2.0) * (cur.x - prev.x).abs();
        prev = cur;
    }
    area
}

/// Length of the polyline through `curve(a + i (b - a) / n)`, `i = 0..=n`.
pub fn polyline_length(curve: impl Fn(f64) -> DVec2, a: f64, b: f64, n: usize) -> f64 {
    let mut prev = curve(a);
    let mut sum = 0.0;
    for i in 1..=n {
        let cur = curve(a + i as f64 * (b - a) / n as f64);
        sum += (cur - prev).length();
        prev = cur;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simpson_is_exact_for_cubics() {
        let f = |x: f64| x * x * x - 2.0 * x + 1.0;
        let exact = |x: f64| x.powi(4) / 4.0 - x * x + x;
        let got = simpson(f, -1.0, 3.0, 4);
        assert!((got - (exact(3.0) - exact(-1.0))).abs() < 1e-12);
        assert!((simpson_par(f, -1.0, 3.0, 4) - got).abs() < 1e-15);
    }

    #[test]
    fn simpson_ignores_nan() {
        let got = simpson(|x| if x == 0.5 { f64::NAN } else { 1.0 }, 0.0, 1.0, 1);
        assert!((got - 2.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn root_finder_secant_and_bisection() {
        let r = find_root(|x| x * x, 0.25, 0.0, 1.0);
        assert!((r * r - 0.25).abs() <= 0.001);
        // A step function defeats the secant phase; bisection still brackets it.
        let r = find_root(|x| if x < 0.3 { 0.0 } else { 1.0 }, 0.5, 0.0, 1.0);
        assert!((r - 0.3).abs() < 1e-3);
    }

    #[test]
    fn trapezoid_of_a_triangle() {
        let a = trapezoid(|s| DVec2::new(s, 1.0 - s), 0.0, 1.0, 10);
        assert!((a - 0.5).abs() < 1e-12);
        let l = polyline_length(|s| DVec2::new(3.0 * s, 4.0 * s), 0.0, 1.0, 10);
        assert!((l - 5.0).abs() < 1e-12);
    }

    #[test]
    fn angles() {
        assert!((angle_between(DVec2::X, DVec2::Y) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        assert_eq!(angle_between(DVec2::ZERO, DVec2::Y), 0.0);
    }
}
