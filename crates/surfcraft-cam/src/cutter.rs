//! Cutters and tool compensation (`cam.md` §8).
//!
//! All positions are in millimetres in a frame whose +Z is the tool axis. The controlled point ("tip") is the
//! centre of the lowest end of the tool.

use surfcraft_core::DVec3;

use crate::Error;
use crate::config::{BullnoseCutter, Cutter};

// ------------------------------------------------------------------------------------------------ STL cutter

/// Directions per cube-face edge of the contact search buckets.
const GRID: usize = 12;

/// Cutter shape read from an ASCII STL (mm, tip at the origin, axis +Z).
#[derive(Clone, Debug, PartialEq)]
pub struct StlCutter {
    /// Distinct vertices in file order.
    vertices: Vec<DVec3>,
    /// Facets as vertex triples (for the tool's lower envelope).
    facets: Vec<[DVec3; 3]>,
    /// Candidate vertices (indices into `vertices`, ascending) for every direction bucket.
    buckets: Vec<Vec<u32>>,
}

/// Bucket of a unit direction: cube face and cell.
fn bucket_of(n: DVec3) -> usize {
    let a = n.abs();
    let (face, u, v, m) = if a.x >= a.y && a.x >= a.z {
        (if n.x >= 0.0 { 0 } else { 1 }, n.y, n.z, a.x)
    } else if a.y >= a.z {
        (if n.y >= 0.0 { 2 } else { 3 }, n.x, n.z, a.y)
    } else {
        (if n.z >= 0.0 { 4 } else { 5 }, n.x, n.y, a.z)
    };
    let cell = |c: f64| ((((c / m) + 1.0) / 2.0 * GRID as f64) as usize).min(GRID - 1);
    (face * GRID + cell(u)) * GRID + cell(v)
}

/// Unit direction of a cube-face point.
fn face_dir(face: usize, u: f64, v: f64) -> DVec3 {
    let s = if face % 2 == 0 { 1.0 } else { -1.0 };
    match face / 2 {
        0 => DVec3::new(s, u, v),
        1 => DVec3::new(u, s, v),
        _ => DVec3::new(u, v, s),
    }
    .normalize()
}

impl StlCutter {
    /// Read an ASCII STL: `facet normal` / `vertex` keywords in any case and position, all vertices used, the
    /// coordinates multiplied by `scale`.
    pub fn from_ascii(text: &str, scale: DVec3) -> Result<StlCutter, Error> {
        if !scale.is_finite() || scale.cmple(DVec3::ZERO).any() {
            return Err(Error::InvalidStl(format!("invalid scale {scale}")));
        }
        let mut raw: Vec<DVec3> = Vec::new();
        for (ln, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.trim_start().starts_with("endsolid") {
                break;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("vertex")) else {
                continue;
            };
            let nums: Vec<f64> = words[i + 1..]
                .iter()
                .take(3)
                .map(|w| w.parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| Error::InvalidStl(format!("line {}: bad vertex", ln + 1)))?;
            if nums.len() < 3 || nums.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidStl(format!("line {}: bad vertex", ln + 1)));
            }
            raw.push(DVec3::new(nums[0], nums[1], nums[2]) * scale);
        }
        if raw.len() < 3 {
            return Err(Error::InvalidStl("no facets".into()));
        }
        let facets: Vec<[DVec3; 3]> = raw.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let mut vertices: Vec<DVec3> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for v in &raw {
            let key = (v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
            if seen.insert(key) {
                vertices.push(*v);
            }
        }
        let mut c = StlCutter {
            vertices,
            facets,
            buckets: Vec::new(),
        };
        c.build_buckets();
        let (r, lo, hi) = c.bounds();
        if !(r > 0.0) || !(hi > lo) {
            return Err(Error::InvalidStl("the cutter has no width or height".into()));
        }
        Ok(c)
    }

    /// For every direction bucket, the vertices that can minimise `n · v` for some `n` in the bucket.
    fn build_buckets(&mut self) {
        let lens: Vec<f64> = self.vertices.iter().map(|v| v.length()).collect();
        let mut buckets = Vec::with_capacity(6 * GRID * GRID);
        for face in 0..6 {
            for i in 0..GRID {
                for j in 0..GRID {
                    let at = |k: f64| -1.0 + 2.0 * k / GRID as f64;
                    let (u0, u1, v0, v1) = (at(i as f64), at(i as f64 + 1.0), at(j as f64), at(j as f64 + 1.0));
                    let c = face_dir(face, (u0 + u1) / 2.0, (v0 + v1) / 2.0);
                    let eps = [(u0, v0), (u0, v1), (u1, v0), (u1, v1)]
                        .iter()
                        .map(|&(u, v)| face_dir(face, u, v).distance(c))
                        .fold(0.0, f64::max)
                        * 1.001
                        + 1e-9;
                    let bound = self
                        .vertices
                        .iter()
                        .zip(&lens)
                        .map(|(v, l)| c.dot(*v) + eps * l)
                        .fold(f64::MAX, f64::min);
                    let list: Vec<u32> = self
                        .vertices
                        .iter()
                        .zip(&lens)
                        .enumerate()
                        .filter(|(_, (v, l))| c.dot(**v) - eps * **l <= bound + 1e-9)
                        .map(|(k, _)| k as u32)
                        .collect();
                    buckets.push(list);
                }
            }
        }
        self.buckets = buckets;
    }

    /// (radius, lowest z, highest z) in mm.
    pub fn bounds(&self) -> (f64, f64, f64) {
        let r = self
            .vertices
            .iter()
            .map(|v| v.x.hypot(v.y))
            .fold(0.0, f64::max);
        let lo = self.vertices.iter().map(|v| v.z).fold(f64::MAX, f64::min);
        let hi = self.vertices.iter().map(|v| v.z).fold(f64::MIN, f64::max);
        (r, lo, hi)
    }

    fn better(n: DVec3, v: DVec3, best: Option<(f64, f64, DVec3)>) -> Option<(f64, f64, DVec3)> {
        let d = n.dot(v);
        let l = v.length();
        match best {
            Some((bd, bl, _)) if !(d < bd || (d == bd && l < bl)) => best,
            _ => Some((d, l, v)),
        }
    }

    /// The cutter vertex touching a surface with normal `n`: the vertex extreme in the direction `-n`
    /// (smallest `n · v`, ties to the vertex nearest the tip).
    pub fn contact_vertex(&self, n: DVec3) -> DVec3 {
        let Some(list) = self.buckets.get(bucket_of(n)) else {
            return self.contact_vertex_brute_force(n);
        };
        let mut best = None;
        for &k in list {
            best = Self::better(n, self.vertices[k as usize], best);
        }
        best.map_or(DVec3::ZERO, |b| b.2)
    }

    /// Reference search over every vertex.
    pub fn contact_vertex_brute_force(&self, n: DVec3) -> DVec3 {
        let mut best = None;
        for v in &self.vertices {
            best = Self::better(n, *v, best);
        }
        best.map_or(DVec3::ZERO, |b| b.2)
    }

    /// Tool tip for a contact point and surface normal.
    pub fn offset(&self, contact: DVec3, normal: DVec3) -> DVec3 {
        if !normal.is_finite() || normal == DVec3::ZERO {
            return contact;
        }
        contact - self.contact_vertex(normal.normalize())
    }

    /// Lower envelope of the tool: lowest height above the tip at each distance from the axis, from the facets,
    /// made non-decreasing outward (convex tool).
    fn envelope(&self, bins: usize) -> Vec<f64> {
        let (radius, lo, _) = self.bounds();
        let mut low = vec![f64::MAX; bins + 1];
        let n = 16;
        for t in &self.facets {
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
                    let p = t[0] + (t[1] - t[0]) * u + (t[2] - t[0]) * v;
                    let d = p.x.hypot(p.y);
                    let b = ((d / radius * bins as f64).floor() as usize).min(bins);
                    low[b] = low[b].min(p.z);
                }
            }
        }
        let mut last = low.iter().cloned().find(|z| *z < f64::MAX).unwrap_or(lo);
        let mut running = f64::MIN;
        low.iter()
            .map(|z| {
                let z = if *z < f64::MAX { *z } else { last };
                last = z;
                running = running.max(z);
                running
            })
            .collect()
    }
}

// ------------------------------------------------------------------------------------------------ profile

/// Lower envelope of a rotationally symmetric tool for the gouge checks: height of the tool bottom above the tip
/// at distance `d` from the axis.
#[derive(Clone, Debug)]
pub(crate) enum Profile {
    /// Flat radius and corner radius.
    Torus { flat: f64, corner: f64 },
    /// Table over `[0, radius]`.
    Table { radius: f64, z: Vec<f64> },
}

impl Profile {
    pub fn radius(&self) -> f64 {
        match self {
            Profile::Torus { flat, corner } => flat + corner,
            Profile::Table { radius, .. } => *radius,
        }
    }

    /// Radius of the tool body at height `h` above the tip.
    pub fn radius_at(&self, h: f64) -> f64 {
        match self {
            Profile::Torus { flat, corner } => {
                if h >= *corner {
                    flat + corner
                } else if h <= 0.0 {
                    *flat
                } else {
                    let e = corner - h;
                    flat + (corner * corner - e * e).max(0.0).sqrt()
                }
            }
            Profile::Table { radius, z } => {
                let bins = z.len() - 1;
                let i = z.partition_point(|&v| v <= h);
                if i > bins {
                    *radius
                } else {
                    radius * i as f64 / bins as f64
                }
            }
        }
    }

    /// Height of the tool bottom at distance `d` (`None` outside the tool).
    #[inline]
    pub fn bottom(&self, d: f64) -> Option<f64> {
        match self {
            Profile::Torus { flat, corner } => {
                if d <= *flat {
                    Some(0.0)
                } else if d <= flat + corner {
                    let e = d - flat;
                    Some(corner - (corner * corner - e * e).max(0.0).sqrt())
                } else {
                    None
                }
            }
            Profile::Table { radius, z } => {
                if d > *radius {
                    return None;
                }
                let bins = z.len() - 1;
                // The bin's own value: the envelope is non-decreasing, so this is the lowest nearby value.
                let b = ((d / radius * bins as f64).floor() as usize).min(bins);
                Some(z[b])
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ compensation

/// Tool tip position (and A angle on the rotating board) for a contact point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    pub pos: DVec3,
    pub a: Option<f64>,
}

/// Rotation about X by `angle` radians (right-hand rule).
pub(crate) fn rot_x(angle: f64, p: DVec3) -> DVec3 {
    let (s, c) = angle.sin_cos();
    DVec3::new(p.x, c * p.y - s * p.z, s * p.y + c * p.z)
}

/// `a` (degrees) moved by whole turns to the value nearest `near`.
pub(crate) fn unwrap_near(a: f64, near: f64) -> f64 {
    a + ((near - a) / 360.0).round() * 360.0
}

fn bullnose_offset(b: &BullnoseCutter, p: DVec3, n: DVec3) -> DVec3 {
    let big = b.diameter * 5.0;
    let r = (b.corner_radius * 10.0).min(big);
    let flat = big - r;
    let rho = n.x.hypot(n.y);
    if rho < 1e-9 {
        return p;
    }
    let h = DVec3::new(n.x / rho, n.y / rho, 0.0);
    if r <= 0.0 {
        return p + big * h;
    }
    let phi = rho.atan2(n.z);
    p + (flat + r * phi.sin()) * h - r * (1.0 - phi.cos()) * DVec3::Z
}

impl Cutter {
    /// The bullnose cutter, also inside a rotating-board cutter.
    pub fn bullnose(&self) -> Option<&BullnoseCutter> {
        match self {
            Cutter::Bullnose(b) => Some(b),
            Cutter::RotatingBoard(inner) => inner.bullnose(),
            _ => None,
        }
    }

    /// The cutter that touches the board (the inner cutter of a rotating board).
    pub fn mounted(&self) -> &Cutter {
        match self {
            Cutter::RotatingBoard(inner) => inner.mounted(),
            c => c,
        }
    }

    /// Radius of the tool body (mm): the cylinder used for collisions.
    pub fn radius(&self) -> f64 {
        match self {
            Cutter::Bullnose(b) => b.diameter.max(b.shank_diameter) * 5.0,
            Cutter::Flat(f) => f.radius * 10.0,
            Cutter::Stl(s) => s.bounds().0,
            Cutter::RotatingBoard(inner) => inner.radius(),
        }
    }

    /// Radius of the cutting end (mm).
    pub fn cutting_radius(&self) -> f64 {
        match self {
            Cutter::Bullnose(b) => b.diameter * 5.0,
            Cutter::Flat(f) => f.radius * 10.0,
            Cutter::Stl(s) => s.bounds().0,
            Cutter::RotatingBoard(inner) => inner.cutting_radius(),
        }
    }

    /// Corner radius (mm; 0 for flat and STL cutters).
    pub fn corner_radius(&self) -> f64 {
        match self.mounted() {
            Cutter::Bullnose(b) => b.corner_radius * 10.0,
            _ => 0.0,
        }
    }

    pub(crate) fn profile(&self) -> Profile {
        match self.mounted() {
            Cutter::Bullnose(b) => {
                let big = b.diameter * 5.0;
                let r = (b.corner_radius * 10.0).min(big);
                Profile::Torus {
                    flat: big - r,
                    corner: r,
                }
            }
            Cutter::Flat(f) => Profile::Torus {
                flat: f.radius * 10.0,
                corner: 0.0,
            },
            Cutter::Stl(s) => Profile::Table {
                radius: s.bounds().0,
                z: s.envelope(400),
            },
            Cutter::RotatingBoard(_) => unreachable!("mounted() unwraps rotating boards"),
        }
    }

    /// Compensation with the tool axis along +Z: the tip that puts the tool tangent to a surface through
    /// `contact` with outward unit `normal`.
    pub fn offset(&self, contact: DVec3, normal: DVec3) -> DVec3 {
        match self {
            Cutter::Bullnose(b) => bullnose_offset(b, contact, normal),
            Cutter::Flat(f) => {
                let rho = normal.x.hypot(normal.y);
                if rho < 1e-9 {
                    contact
                } else {
                    contact
                        + f.radius * 10.0 * DVec3::new(normal.x / rho, normal.y / rho, 0.0)
                }
            }
            Cutter::Stl(s) => s.offset(contact, normal),
            Cutter::RotatingBoard(inner) => inner.offset(contact, normal),
        }
    }

    /// Keep the tool edge `width` mm away from the stringer plane (`cam.md` §8.4): `side` +1 right, -1 left, 0 by
    /// the sign of the tip (0 goes left).
    pub(crate) fn stay_away(&self, mut tip: DVec3, width: f64, side: f64) -> DVec3 {
        let big = self.cutting_radius();
        if tip.y.abs() - big < width {
            let sign = if side > 0.0 {
                1.0
            } else if side < 0.0 || tip.y <= 0.0 {
                -1.0
            } else {
                1.0
            };
            tip.y = sign * (big + width);
        }
        tip
    }

    /// Tool tip for `contact` with surface `normal` (reference frame, mm), keeping `stringer_width` (cm) clear of the
    /// centreline when given; `side` is +1 right, -1 left; `a` is the current A angle (degrees).
    ///
    /// On the rotating board the contact and normal are first turned about X so that the normal points up; the
    /// returned position is in that turned frame and `a` holds the turn in degrees, unwrapped next to the given
    /// angle.
    pub fn tip(
        &self,
        contact: DVec3,
        normal: DVec3,
        stringer_width: Option<f64>,
        side: f64,
        a: Option<f64>,
    ) -> Tip {
        match self {
            Cutter::RotatingBoard(inner) => {
                let alpha = normal.y.atan2(normal.z);
                let p = rot_x(alpha, contact);
                let n = rot_x(alpha, normal);
                let mut pos = inner.offset(p, n);
                let deg = alpha.to_degrees();
                if let Some(w) = stringer_width
                    && deg.abs() <= 45.0
                {
                    pos = inner.stay_away(pos, w * 10.0, side);
                }
                let a = match a {
                    Some(prev) => unwrap_near(deg, prev),
                    None => deg,
                };
                Tip { pos, a: Some(a) }
            }
            c => {
                let mut pos = c.offset(contact, normal);
                if let Some(w) = stringer_width {
                    pos = c.stay_away(pos, w * 10.0, side);
                }
                Tip { pos, a: None }
            }
        }
    }
}
