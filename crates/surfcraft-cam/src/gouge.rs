//! Drop-cutter gouge verification (`cam.md` §8.8): the board as a dense cloud of surface points in the machine
//! frame, bucketed on a plan grid; the lowest tip height at a plan position is the highest of `z - e(d)` over the
//! points under the tool, where `e(d)` is the height of the tool bottom at distance `d` from the axis.

use rayon::prelude::*;
use surfcraft_core::{Board, DVec2, DVec3};

use crate::cutter::Profile;
use crate::eval::{Part3, Surf, section_samples};
use crate::holding::Placement;

/// Surface points of a board (both halves) in the reference frame, mm.
pub(crate) fn cloud(
    board: &Board,
    placement: &Placement,
    part: Part3,
    step: f64,
    spacing: f64,
    parallel: bool,
) -> Vec<DVec3> {
    let surf = Surf::new(board);
    // The y < 0 half of an asymmetric board has its own shape.
    let other = surf.other_half();
    let l = surf.len;
    // Four times denser within 3 cm of the tips, where the surface turns fast.
    let end = 3.0f64.min(l / 4.0);
    let mut xs = Vec::new();
    let mut x = 0.1;
    while x < l - 0.1 {
        xs.push(x);
        let near = x < end || x > l - end;
        x += if near { step / 4.0 } else { step };
    }
    xs.push(l - 0.1);
    let one = |x: f64| -> Vec<DVec3> {
        let near = x < end || x > l - end;
        let sp = if near { spacing / 2.0 } else { spacing };
        let mut v = Vec::new();
        match &other {
            None => {
                for p in section_samples(&surf, x, part, sp) {
                    v.push(placement.point(p));
                    if p.y.abs() > 1e-9 {
                        v.push(placement.point(DVec3::new(p.x, -p.y, p.z)));
                    }
                }
            }
            Some(o) => {
                for p in section_samples(&surf, x, part, sp) {
                    v.push(placement.point(p));
                }
                for p in section_samples(o, x, part, sp) {
                    if p.y.abs() > 1e-9 {
                        v.push(placement.point(DVec3::new(p.x, -p.y, p.z)));
                    }
                }
            }
        }
        v
    };
    let rows: Vec<Vec<DVec3>> = if parallel {
        xs.par_iter().map(|&x| one(x)).collect()
    } else {
        xs.iter().map(|&x| one(x)).collect()
    };
    rows.into_iter().flatten().collect()
}

/// Upper envelope of a point cloud, for tools with a vertical axis.
pub(crate) struct Envelope {
    pts: Vec<DVec3>,
    start: Vec<u32>,
    lo: DVec2,
    cell: f64,
    nx: usize,
    ny: usize,
    pub top: f64,
}

impl Envelope {
    pub fn new(points: Vec<DVec3>, cell: f64) -> Envelope {
        let cell = cell.max(0.5);
        if points.is_empty() {
            return Envelope {
                pts: Vec::new(),
                start: vec![0, 0],
                lo: DVec2::ZERO,
                cell,
                nx: 1,
                ny: 1,
                top: f64::NEG_INFINITY,
            };
        }
        let mut lo = DVec2::splat(f64::MAX);
        let mut hi = DVec2::splat(f64::MIN);
        let mut top = f64::NEG_INFINITY;
        for p in &points {
            lo = lo.min(p.truncate());
            hi = hi.max(p.truncate());
            top = top.max(p.z);
        }
        let nx = ((hi.x - lo.x) / cell).floor() as usize + 1;
        let ny = ((hi.y - lo.y) / cell).floor() as usize + 1;
        let id = |p: &DVec3| {
            let i = (((p.x - lo.x) / cell) as usize).min(nx - 1);
            let j = (((p.y - lo.y) / cell) as usize).min(ny - 1);
            i * ny + j
        };
        let mut start = vec![0u32; nx * ny + 1];
        for p in &points {
            start[id(p) + 1] += 1;
        }
        for i in 0..nx * ny {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut pts = vec![DVec3::ZERO; points.len()];
        for p in &points {
            let c = id(p);
            pts[fill[c] as usize] = *p;
            fill[c] += 1;
        }
        Envelope {
            pts,
            start,
            lo,
            cell,
            nx,
            ny,
            top,
        }
    }

    /// Penetration of the tool at `tip` into the board (the shorter way out of the tool body, down or sideways;
    /// 0 when clear) and the lowest tip height at which the tool is clear (negative infinity away from the board).
    pub fn check(&self, tip: DVec3, tool: &Profile) -> (f64, f64) {
        let at = tip.truncate();
        let r = tool.radius();
        let r2 = r * r;
        let rel = at - self.lo;
        let i0 = ((rel.x - r) / self.cell).floor();
        let i1 = ((rel.x + r) / self.cell).floor();
        let j0 = ((rel.y - r) / self.cell).floor();
        let j1 = ((rel.y + r) / self.cell).floor();
        if i1 < 0.0 || j1 < 0.0 || i0 >= self.nx as f64 || j0 >= self.ny as f64 {
            return (0.0, f64::NEG_INFINITY);
        }
        let (i0, i1) = (i0.max(0.0) as usize, (i1 as usize).min(self.nx - 1));
        let (j0, j1) = (j0.max(0.0) as usize, (j1 as usize).min(self.ny - 1));
        let mut drop = f64::NEG_INFINITY;
        let mut pen = 0.0f64;
        for i in i0..=i1 {
            let base = i * self.ny;
            let (s, e) = (
                self.start[base + j0] as usize,
                self.start[base + j1 + 1] as usize,
            );
            for p in &self.pts[s..e] {
                let dx = p.x - at.x;
                let dy = p.y - at.y;
                let d2 = dx * dx + dy * dy;
                if d2 > r2 || p.z <= tip.z {
                    continue;
                }
                let d = d2.sqrt();
                if let Some(e) = tool.bottom(d) {
                    let z = p.z - e;
                    drop = drop.max(z);
                    if z > tip.z {
                        let h = p.z - tip.z;
                        pen = pen.max((z - tip.z).min(tool.radius_at(h) - d));
                    }
                }
            }
        }
        (pen, drop)
    }

    /// Lowest gouge-free tip height at plan position `at` (negative infinity away from the board).
    #[allow(dead_code)]
    pub fn drop(&self, at: DVec2, tool: &Profile) -> f64 {
        let r = tool.radius();
        let r2 = r * r;
        let rel = at - self.lo;
        let i0 = ((rel.x - r) / self.cell).floor();
        let i1 = ((rel.x + r) / self.cell).floor();
        let j0 = ((rel.y - r) / self.cell).floor();
        let j1 = ((rel.y + r) / self.cell).floor();
        if i1 < 0.0 || j1 < 0.0 || i0 >= self.nx as f64 || j0 >= self.ny as f64 {
            return f64::NEG_INFINITY;
        }
        let (i0, i1) = (i0.max(0.0) as usize, (i1 as usize).min(self.nx - 1));
        let (j0, j1) = (j0.max(0.0) as usize, (j1 as usize).min(self.ny - 1));
        let mut best = f64::NEG_INFINITY;
        for i in i0..=i1 {
            let base = i * self.ny;
            let (s, e) = (
                self.start[base + j0] as usize,
                self.start[base + j1 + 1] as usize,
            );
            for p in &self.pts[s..e] {
                let dx = p.x - at.x;
                let dy = p.y - at.y;
                let d2 = dx * dx + dy * dy;
                if d2 > r2 || p.z <= best {
                    continue;
                }
                if let Some(e) = tool.bottom(d2.sqrt()) {
                    best = best.max(p.z - e);
                }
            }
        }
        best
    }
}
