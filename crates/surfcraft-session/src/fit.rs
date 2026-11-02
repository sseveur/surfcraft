//! Fitting fair curves through measurements: cubic Hermite splines in x (Bezier segments whose handles sit at a
//! third of the knot spacing, so x runs evenly along each segment) fitted by penalised least squares, and the
//! monotone cubic interpolation used to fill in between measured stations.

use surfcraft_core::{DVec2, Knot};

use crate::linalg::LeastSquares;

/// A point to fit, with its weight.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub x: f64,
    pub y: f64,
    pub w: f64,
}

/// A fitted curve: knot stations, heights and slopes (dy/dx).
#[derive(Clone, Debug)]
pub(crate) struct Hermite {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub ms: Vec<f64>,
}

/// Basis values on a segment of width `h` at local parameter `t`: weights of (y0, m0, y1, m1).
fn basis(t: f64, h: f64) -> [f64; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        2.0 * t3 - 3.0 * t2 + 1.0,
        h * (t3 - 2.0 * t2 + t),
        -2.0 * t3 + 3.0 * t2,
        h * (t3 - t2),
    ]
}

/// Second-derivative weights of (y0, m0, y1, m1).
fn basis2(t: f64, h: f64) -> [f64; 4] {
    let k = 1.0 / (h * h);
    [
        k * (12.0 * t - 6.0),
        k * h * (6.0 * t - 4.0),
        k * (6.0 - 12.0 * t),
        k * h * (6.0 * t - 2.0),
    ]
}

impl Hermite {
    fn locate(&self, x: f64) -> (usize, f64, f64) {
        let n = self.xs.len();
        let i = self.xs.partition_point(|&v| v <= x).clamp(1, n - 1) - 1;
        let h = self.xs[i + 1] - self.xs[i];
        (i, (x - self.xs[i]) / h, h)
    }

    pub fn eval(&self, x: f64) -> f64 {
        let (i, t, h) = self.locate(x);
        let b = basis(t, h);
        b[0] * self.ys[i] + b[1] * self.ms[i] + b[2] * self.ys[i + 1] + b[3] * self.ms[i + 1]
    }

    /// Bezier knots of the curve: smooth interior knots, end knots without an outward handle.
    pub fn knots(&self) -> Vec<Knot> {
        let n = self.xs.len();
        (0..n)
            .map(|i| {
                let p = DVec2::new(self.xs[i], self.ys[i]);
                let handle = |dx: f64| p + DVec2::new(dx, self.ms[i] * dx);
                let prev = if i > 0 {
                    handle(-(self.xs[i] - self.xs[i - 1]) / 3.0)
                } else {
                    p
                };
                let next = if i + 1 < n {
                    handle((self.xs[i + 1] - self.xs[i]) / 3.0)
                } else {
                    p
                };
                if i > 0 && i + 1 < n {
                    Knot::smooth(p, prev, next)
                } else {
                    Knot::new(p, prev, next)
                }
            })
            .collect()
    }
}

/// Least-squares fit through `samples` with knots at `xs` (increasing, at least two), penalising the integral of
/// the squared second derivative by `lambda`. `start` / `end` fix the height at the first / last knot.
pub(crate) fn fit(
    xs: &[f64],
    samples: &[Sample],
    start: Option<f64>,
    end: Option<f64>,
    lambda: f64,
) -> Option<Hermite> {
    let n = xs.len();
    if n < 2 || xs.windows(2).any(|w| w[1] <= w[0]) {
        return None;
    }
    // Unknown index of each knot's height (None when fixed) and slope.
    let mut idx_y = vec![None; n];
    let mut count = 0;
    for (i, slot) in idx_y.iter_mut().enumerate() {
        let fixed = (i == 0 && start.is_some()) || (i + 1 == n && end.is_some());
        if !fixed {
            *slot = Some(count);
            count += 1;
        }
    }
    let idx_m: Vec<usize> = (0..n).map(|i| count + i).collect();
    let total = count + n;
    let fixed_y = |i: usize| {
        if i == 0 {
            start.unwrap_or(0.0)
        } else {
            end.unwrap_or(0.0)
        }
    };
    let mut ls = LeastSquares::new(total);
    let mut add = |i: usize, b: [f64; 4], value: f64, w: f64| {
        let mut terms = Vec::with_capacity(4);
        let mut v = value;
        for (k, coef) in [(i, b[0]), (i + 1, b[2])] {
            match idx_y[k] {
                Some(u) => terms.push((u, coef)),
                None => v -= coef * fixed_y(k),
            }
        }
        terms.push((idx_m[i], b[1]));
        terms.push((idx_m[i + 1], b[3]));
        ls.row(&terms, v, w);
    };
    for s in samples {
        let i = xs.partition_point(|&v| v <= s.x).clamp(1, n - 1) - 1;
        let h = xs[i + 1] - xs[i];
        add(i, basis((s.x - xs[i]) / h, h), s.y, s.w);
    }
    for i in 0..n - 1 {
        let h = xs[i + 1] - xs[i];
        for (t, wt) in [(0.0, 1.0), (0.5, 4.0), (1.0, 1.0)] {
            add(i, basis2(t, h), 0.0, lambda * h / 6.0 * wt);
        }
    }
    for u in 0..total {
        ls.ridge(u, 1e-10);
    }
    let sol = ls.solve()?;
    let ys = (0..n)
        .map(|i| idx_y[i].map_or_else(|| fixed_y(i), |u| sol[u]))
        .collect();
    let ms = idx_m.iter().map(|&u| sol[u]).collect();
    Some(Hermite {
        xs: xs.to_vec(),
        ys,
        ms,
    })
}

/// `n` knot stations on `[a, b]`, closer together toward both ends (where boards curve most).
pub(crate) fn stations(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let u = i as f64 / (n - 1) as f64;
            a + (b - a) * (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
        })
        .collect()
}

/// Monotone piecewise-cubic interpolation (Fritsch-Carlson) through points with increasing x; constant beyond the
/// first and last point. Never overshoots the data, so it adds no wiggles of its own.
pub(crate) struct Pchip {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ds: Vec<f64>,
}

impl Pchip {
    pub fn new(points: &[(f64, f64)]) -> Self {
        let xs: Vec<f64> = points.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
        let n = xs.len();
        let mut ds = vec![0.0; n];
        if n >= 2 {
            let delta: Vec<f64> = (0..n - 1)
                .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]))
                .collect();
            ds[0] = delta[0];
            ds[n - 1] = delta[n - 2];
            for i in 1..n - 1 {
                ds[i] = if delta[i - 1] * delta[i] <= 0.0 {
                    0.0
                } else {
                    let (h0, h1) = (xs[i] - xs[i - 1], xs[i + 1] - xs[i]);
                    let (w1, w2) = (2.0 * h1 + h0, h1 + 2.0 * h0);
                    (w1 + w2) / (w1 / delta[i - 1] + w2 / delta[i])
                };
            }
        }
        Self { xs, ys, ds }
    }

    pub fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        match n {
            0 => return 0.0,
            1 => return self.ys[0],
            _ => {}
        }
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let i = self.xs.partition_point(|&v| v <= x).clamp(1, n - 1) - 1;
        let h = self.xs[i + 1] - self.xs[i];
        let b = basis((x - self.xs[i]) / h, h);
        b[0] * self.ys[i] + b[1] * self.ds[i] + b[2] * self.ys[i + 1] + b[3] * self.ds[i + 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_a_cubic_exactly() {
        let f = |x: f64| 0.001 * x * x * x - 0.05 * x * x + x + 3.0;
        let samples: Vec<Sample> = (0..=100)
            .map(|i| {
                let x = i as f64;
                Sample { x, y: f(x), w: 1.0 }
            })
            .collect();
        let h = fit(&stations(0.0, 100.0, 4), &samples, Some(3.0), None, 0.0).unwrap();
        for x in [0.0, 12.5, 50.0, 99.0] {
            assert!((h.eval(x) - f(x)).abs() < 1e-6, "{x}");
        }
        // As Bezier knots the curve is the same.
        let s = surfcraft_core::Spline::from_knots(h.knots());
        assert!((s.value_at(37.0) - f(37.0)).abs() < 1e-6);
    }

    #[test]
    fn pchip_is_monotone_and_interpolates() {
        let p = Pchip::new(&[(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 5.0)]);
        assert_eq!(p.eval(1.0), 1.0);
        assert!(
            (1.0..=1.0 + 1e-12).contains(&p.eval(1.5)),
            "flat between equal values"
        );
        assert_eq!(p.eval(-1.0), 0.0);
        assert_eq!(p.eval(9.0), 5.0);
        assert_eq!(Pchip::new(&[(1.0, 2.0)]).eval(5.0), 2.0);
    }
}
