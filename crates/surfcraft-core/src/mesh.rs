//! Triangle mesh of the whole board, sampled from the surface model.
//!
//! Rings of points are taken at stations along the length (denser near nose and tail). Each ring runs around
//! the full cross-section (both halves); the tail and nose are closed with fans so the mesh is watertight.

use glam::DVec3;
use rayon::prelude::*;

use crate::asym::Rail;
use crate::board::Board;
use crate::surface::WHOLE;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    /// `(x along, y across, z up)` in cm.
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub struct MeshDetail {
    /// Stations along the length.
    pub stations: usize,
    /// Points along one half cross-section, deck centre to bottom centre.
    pub half_section: usize,
}

impl Default for MeshDetail {
    fn default() -> Self {
        Self {
            stations: 80,
            half_section: 24,
        }
    }
}

impl MeshDetail {
    pub const PREVIEW: MeshDetail = MeshDetail {
        stations: 48,
        half_section: 14,
    };
}

impl Mesh {
    pub fn of_board(board: &Board, detail: MeshDetail) -> Mesh {
        let length = board.length();
        if board.sections.len() < 3 || length <= 0.0 {
            return Mesh::default();
        }
        let n = detail.stations.max(4);
        let h = detail.half_section.max(4);
        // Cosine spacing: more stations where curvature is high at the ends.
        let end_gap = 0.1;
        let xs: Vec<f64> = (0..n)
            .map(|i| {
                let u = i as f64 / (n - 1) as f64;
                let c = (1.0 - (std::f64::consts::PI * u).cos()) / 2.0;
                end_gap + c * (length - 2.0 * end_gap)
            })
            .collect();
        // Half ring, deck centre (s=0) to bottom centre (s=1), on the y >= 0 (left) rail.
        let half_ring = |x: f64, rail: Rail| -> Vec<DVec3> {
            (0..=h)
                .map(|j| {
                    let s = j as f64 / h as f64;
                    board.surface_point_on(x, s, WHOLE.0, WHOLE.1, rail)
                })
                .collect()
        };
        let asym = board.is_asymmetric();
        // The right rail's half ring (already at y <= 0) on an asymmetric board.
        let rings: Vec<(Vec<DVec3>, Option<Vec<DVec3>>)> = xs
            .par_iter()
            .map(|&x| {
                let right = asym.then(|| half_ring(x, Rail::Right));
                (half_ring(x, Rail::Left), right)
            })
            .collect();

        // Full ring: the y >= 0 half then the y <= 0 half (mirrored for a symmetric board), without repeating
        // the centre points.
        let ring_len = 2 * h;
        let mut positions = Vec::with_capacity(n * ring_len + 2);
        for (half, right) in &rings {
            positions.extend(half.iter().copied());
            match right {
                Some(r) => positions.extend(r[1..h].iter().rev().copied()),
                None => {
                    positions.extend(half[1..h].iter().rev().map(|p| DVec3::new(p.x, -p.y, p.z)))
                }
            }
        }
        let mut triangles = Vec::with_capacity(2 * n * ring_len);
        for i in 0..n - 1 {
            let a = (i * ring_len) as u32;
            let b = ((i + 1) * ring_len) as u32;
            for j in 0..ring_len as u32 {
                let j1 = (j + 1) % ring_len as u32;
                triangles.push([a + j, b + j, b + j1]);
                triangles.push([a + j, b + j1, a + j1]);
            }
        }
        // End caps: fan around each end ring's centroid.
        // Winding must run opposite to the side triangles sharing each ring edge.
        for (ring, outward_first) in [(0usize, false), (n - 1, true)] {
            let base = (ring * ring_len) as u32;
            let centre = (0..ring_len)
                .map(|j| positions[base as usize + j])
                .sum::<DVec3>()
                / ring_len as f64;
            let c = positions.len() as u32;
            positions.push(centre);
            for j in 0..ring_len as u32 {
                let j1 = (j + 1) % ring_len as u32;
                if outward_first {
                    triangles.push([c, base + j1, base + j]);
                } else {
                    triangles.push([c, base + j, base + j1]);
                }
            }
        }
        let mut mesh = Mesh {
            positions,
            normals: Vec::new(),
            triangles,
        };
        mesh.compute_normals();
        mesh
    }

    /// Add the triangles of `other` (as separate shells) to this mesh.
    pub fn append(&mut self, other: &Mesh) {
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.triangles
            .extend(other.triangles.iter().map(|t| t.map(|i| i + offset)));
    }

    /// Area-weighted vertex normals.
    pub fn compute_normals(&mut self) {
        let mut normals = vec![DVec3::ZERO; self.positions.len()];
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.positions[i as usize]);
            let n = (b - a).cross(c - a);
            for &i in t {
                normals[i as usize] += n;
            }
        }
        for n in &mut normals {
            *n = n.normalize_or_zero();
        }
        self.normals = normals;
    }

    pub fn face_normal(&self, t: [u32; 3]) -> DVec3 {
        let [a, b, c] = t.map(|i| self.positions[i as usize]);
        (b - a).cross(c - a).normalize_or_zero()
    }

    /// Enclosed volume (cm^3) by the divergence theorem; positive when triangles face outward.
    pub fn volume(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| self.positions[i as usize]);
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    /// Every edge shared by exactly two triangles in opposite directions.
    pub fn is_watertight(&self) -> bool {
        use std::collections::HashMap;
        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += if a < b { 1 } else { -1 };
            }
        }
        edges.values().all(|&v| v == 0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::knot::Knot;
    use crate::section::CrossSection;
    use crate::spline::{Spline, polyline};
    use glam::DVec2;

    /// Simple synthetic board: elliptic-ish outline, flat rocker, constant section shape.
    pub(crate) fn toy_board() -> Board {
        let k = 0.5523;
        let section = |pos: f64| {
            CrossSection::new(
                pos,
                Spline::from_knots(vec![
                    Knot::new(
                        DVec2::new(0.0, 0.0),
                        DVec2::new(-k, 0.0),
                        DVec2::new(k * 20.0, 0.0),
                    ),
                    Knot::new(
                        DVec2::new(25.0, 3.0),
                        DVec2::new(25.0, 3.0 - k * 3.0),
                        DVec2::new(25.0, 3.0 + k * 3.0),
                    ),
                    Knot::new(
                        DVec2::new(0.0, 6.0),
                        DVec2::new(k * 20.0, 6.0),
                        DVec2::new(-k, 6.0),
                    ),
                ]),
            )
        };
        let zero = |pos: f64| {
            CrossSection::new(
                pos,
                Spline::from_knots(vec![Knot::default(), Knot::default()]),
            )
        };
        let mut b = Board {
            outline: polyline(&[
                (0.0, 0.0),
                (0.0, 15.0),
                (90.0, 25.0),
                (180.0, 10.0),
                (180.0, 0.0),
            ]),
            bottom: polyline(&[(0.0, 0.0), (90.0, 0.0), (180.0, 0.0)]),
            deck: polyline(&[(0.0, 0.0), (0.0, 6.0), (180.0, 6.0), (180.0, 0.0)]),
            sections: vec![
                zero(0.0),
                section(30.0),
                section(90.0),
                section(150.0),
                zero(180.0),
            ],
            ..Default::default()
        };
        b.fit_sections();
        b
    }

    #[test]
    fn mesh_is_closed_and_matches_volume() {
        let b = toy_board();
        let m = Mesh::of_board(&b, MeshDetail::default());
        assert!(m.is_watertight());
        let mesh_volume = m.volume();
        let board_volume = b.volume();
        assert!(mesh_volume > 0.0, "triangles face inward");
        let rel = (mesh_volume - board_volume).abs() / board_volume;
        assert!(rel < 0.02, "mesh {mesh_volume} vs integral {board_volume}");
    }
}
