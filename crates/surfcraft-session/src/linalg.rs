//! Small dense least-squares problems (a few dozen unknowns) for curve fitting.

/// Normal equations `AᵀWA x = AᵀWb`, accumulated row by row.
pub(crate) struct LeastSquares {
    n: usize,
    m: Vec<f64>,
    r: Vec<f64>,
}

impl LeastSquares {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n,
            m: vec![0.0; n * n],
            r: vec![0.0; n],
        }
    }

    /// Add the observation `Σ coef·x[idx] ≈ value` with weight `w`.
    pub(crate) fn row(&mut self, terms: &[(usize, f64)], value: f64, w: f64) {
        for &(i, a) in terms {
            self.r[i] += w * a * value;
            for &(j, b) in terms {
                self.m[i * self.n + j] += w * a * b;
            }
        }
    }

    /// Add `w·x[i]²` to the objective (ridge term).
    pub(crate) fn ridge(&mut self, i: usize, w: f64) {
        self.m[i * self.n + i] += w;
    }

    /// Solve; `None` when the system is singular or the result not finite.
    pub(crate) fn solve(self) -> Option<Vec<f64>> {
        solve(self.m, self.r, self.n)
    }
}

/// Gaussian elimination with partial pivoting on the row-major `n × n` matrix `a`.
pub(crate) fn solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i * n + c].abs().total_cmp(&a[j * n + c].abs()))?;
        if a[p * n + c].abs() < 1e-14 {
            return None;
        }
        if p != c {
            for k in 0..n {
                a.swap(p * n + k, c * n + k);
            }
            b.swap(p, c);
        }
        for i in c + 1..n {
            let f = a[i * n + c] / a[c * n + c];
            if f != 0.0 {
                for k in c..n {
                    a[i * n + k] -= f * a[c * n + k];
                }
                b[i] -= f * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for c in (0..n).rev() {
        let s: f64 = (c + 1..n).map(|k| a[c * n + k] * x[k]).sum();
        x[c] = (b[c] - s) / a[c * n + c];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_a_line() {
        let mut ls = LeastSquares::new(2);
        for i in 0..10 {
            let x = i as f64;
            ls.row(&[(0, 1.0), (1, x)], 2.0 + 3.0 * x, 1.0);
        }
        let v = ls.solve().unwrap();
        assert!((v[0] - 2.0).abs() < 1e-9 && (v[1] - 3.0).abs() < 1e-9);
        assert!(solve(vec![0.0; 4], vec![1.0, 1.0], 2).is_none());
    }
}
