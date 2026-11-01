//! Control points ("knots") of the Bezier curves: a point on the curve, two handle points and the editing
//! constraints attached to them.

use glam::DVec2;

use crate::math::angle_between;

/// One of the three points of a knot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Handle {
    /// The point the curve passes through.
    Point,
    /// The handle controlling the segment that arrives at the point.
    Prev,
    /// The handle controlling the segment that leaves the point.
    Next,
}

impl Handle {
    pub const ALL: [Handle; 3] = [Handle::Point, Handle::Prev, Handle::Next];

    /// Index into [`Knot::points`].
    pub const fn index(self) -> usize {
        match self {
            Handle::Point => 0,
            Handle::Prev => 1,
            Handle::Next => 2,
        }
    }

    /// The other handle (`Prev` ↔ `Next`); the point maps to itself.
    pub const fn opposite(self) -> Handle {
        match self {
            Handle::Point => Handle::Point,
            Handle::Prev => Handle::Next,
            Handle::Next => Handle::Prev,
        }
    }
}

/// Constraints applied to a handle whenever it is set, each one clamping a coordinate relative to the knot point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Locks(u8);

impl Locks {
    pub const NONE: Locks = Locks(0);
    /// The handle may not lie left of the point (`x ≥ point.x`).
    pub const X_NOT_LESS: Locks = Locks(1);
    /// The handle may not lie right of the point (`x ≤ point.x`).
    pub const X_NOT_MORE: Locks = Locks(2);
    /// The handle may not lie below the point (`y ≥ point.y`).
    pub const Y_NOT_LESS: Locks = Locks(4);
    /// The handle may not lie above the point (`y ≤ point.y`).
    pub const Y_NOT_MORE: Locks = Locks(8);

    pub const fn contains(self, other: Locks) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Locks) -> Locks {
        Locks(self.0 | other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Clamp handle `h` of a knot at `p`.
    pub fn apply(self, p: DVec2, mut h: DVec2) -> DVec2 {
        if self.contains(Locks::X_NOT_LESS) && h.x < p.x {
            h.x = p.x;
        }
        if self.contains(Locks::X_NOT_MORE) && h.x > p.x {
            h.x = p.x;
        }
        if self.contains(Locks::Y_NOT_LESS) && h.y < p.y {
            h.y = p.y;
        }
        if self.contains(Locks::Y_NOT_MORE) && h.y > p.y {
            h.y = p.y;
        }
        h
    }
}

impl std::ops::BitOr for Locks {
    type Output = Locks;
    fn bitor(self, rhs: Locks) -> Locks {
        self.union(rhs)
    }
}

/// A control point of a cubic Bezier spline.
///
/// Handles are stored as absolute positions. `continuous` asks interactive editing to keep the two handles
/// opposite each other; evaluation never enforces it. `aux` is a flag carried by `.brd` files without a known
/// meaning; it is only preserved.
#[derive(Clone, Debug, PartialEq)]
pub struct Knot {
    /// Point, previous handle and next handle, indexed by [`Handle::index`].
    pub points: [DVec2; 3],
    pub continuous: bool,
    pub aux: bool,
    /// Movement factors per axis for point moves: 1 = free, 0 = frozen.
    pub axis: DVec2,
    /// Constraints of the previous and the next handle.
    pub locks: [Locks; 2],
}

impl Default for Knot {
    fn default() -> Self {
        Self {
            points: [DVec2::ZERO; 3],
            continuous: false,
            aux: false,
            axis: DVec2::ONE,
            locks: [Locks::NONE; 2],
        }
    }
}

impl Knot {
    pub fn new(point: DVec2, prev: DVec2, next: DVec2) -> Self {
        Self {
            points: [point, prev, next],
            ..Default::default()
        }
    }

    /// A smooth knot (`continuous` set).
    pub fn smooth(point: DVec2, prev: DVec2, next: DVec2) -> Self {
        Self {
            continuous: true,
            ..Self::new(point, prev, next)
        }
    }

    pub fn point(&self) -> DVec2 {
        self.points[0]
    }

    pub fn prev(&self) -> DVec2 {
        self.points[1]
    }

    pub fn next(&self) -> DVec2 {
        self.points[2]
    }

    pub fn get(&self, h: Handle) -> DVec2 {
        self.points[h.index()]
    }

    /// Assign one of the three points without any constraint.
    pub fn set(&mut self, h: Handle, p: DVec2) {
        self.points[h.index()] = p;
    }

    /// Vector from the point to a handle.
    pub fn handle_vector(&self, h: Handle) -> DVec2 {
        self.get(h) - self.point()
    }

    pub fn handle_length(&self, h: Handle) -> f64 {
        self.handle_vector(h).length()
    }

    /// Unsigned angle (radians) between the "up" axis and a handle vector; 0 for a zero-length handle.
    pub fn handle_angle(&self, h: Handle) -> f64 {
        angle_between(DVec2::Y, self.handle_vector(h))
    }

    /// Unsigned angle (radians) between the two handle vectors; π for a smooth knot.
    pub fn angle_between_handles(&self) -> f64 {
        angle_between(
            self.handle_vector(Handle::Prev),
            self.handle_vector(Handle::Next),
        )
    }

    /// Locks of a handle (`Point` has none).
    pub fn locks_of(&self, h: Handle) -> Locks {
        match h {
            Handle::Point => Locks::NONE,
            Handle::Prev => self.locks[0],
            Handle::Next => self.locks[1],
        }
    }

    /// Translate the point and both handles.
    pub fn translate(&mut self, d: DVec2) {
        for p in &mut self.points {
            *p += d;
        }
    }

    /// Move the point towards `target`, scaled per axis by the movement factors, carrying the handles along.
    /// Returns the displacement actually applied.
    pub fn move_point_to(&mut self, target: DVec2) -> DVec2 {
        let d = (target - self.point()) * self.axis;
        self.translate(d);
        d
    }

    /// Set a handle and apply its locks. Setting [`Handle::Point`] moves the knot (see [`Self::move_point_to`]).
    pub fn set_handle(&mut self, h: Handle, target: DVec2) {
        match h {
            Handle::Point => {
                self.move_point_to(target);
            }
            _ => {
                let p = self.point();
                self.points[h.index()] = self.locks_of(h).apply(p, target);
            }
        }
    }

    /// Re-apply the locks of both handles.
    pub fn apply_locks(&mut self) {
        let p = self.point();
        self.points[1] = self.locks[0].apply(p, self.points[1]);
        self.points[2] = self.locks[1].apply(p, self.points[2]);
    }

    /// Scale all three points about the origin of the frame.
    pub fn scale(&mut self, sx: f64, sy: f64) {
        let k = DVec2::new(sx, sy);
        for p in &mut self.points {
            *p *= k;
        }
    }

    /// Scale a handle's vector by `k` about the point (locks apply).
    pub fn scale_handle(&mut self, h: Handle, k: f64) {
        let target = self.point() + self.handle_vector(h) * k;
        self.set_handle(h, target);
    }

    /// Rotate a handle counter-clockwise about the point by `angle` radians (locks apply).
    pub fn rotate_handle(&mut self, h: Handle, angle: f64) {
        let v = DVec2::from_angle(angle).rotate(self.handle_vector(h));
        self.set_handle(h, self.point() + v);
    }

    /// Mirror the handle opposite to `moved` so that it points away from `moved` while keeping its own length.
    /// Does nothing when either handle has zero length.
    pub fn align_opposite(&mut self, moved: Handle) {
        if moved == Handle::Point {
            return;
        }
        let other = moved.opposite();
        let dir = self.handle_vector(moved);
        let len = self.handle_length(other);
        if dir.length() == 0.0 || len == 0.0 || !dir.is_finite() {
            return;
        }
        let target = self.point() - dir.normalize() * len;
        self.set_handle(other, target);
    }

    /// Bit-identical coordinates and flags.
    pub fn is_identical(&self, other: &Knot) -> bool {
        self.points
            .iter()
            .zip(&other.points)
            .all(|(a, b)| a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits())
            && self.continuous == other.continuous
            && self.aux == other.aux
    }

    /// Whether the handles are opposite each other within 0.02 rad, judged as files have always been judged:
    /// by the angles of the handles to the vertical. Mirror images about the vertical (a "V") would also pass
    /// that test, so when both handles have a length they must in addition really point in opposite directions.
    pub fn is_geometrically_continuous(&self) -> bool {
        let a = self.handle_angle(Handle::Prev);
        let b = self.handle_angle(Handle::Next);
        let by_angles = ((std::f64::consts::PI - a) - b).abs() < 0.02;
        let p = self.handle_vector(Handle::Prev);
        let n = self.handle_vector(Handle::Next);
        let (lp, ln) = (p.length(), n.length());
        if lp > 0.0 && ln > 0.0 {
            let sine = p.perp_dot(n).abs() / (lp * ln);
            by_angles && sine < 0.02 && p.dot(n) < 0.0
        } else {
            by_angles
        }
    }

    /// Similarity score used to match the knots of two cross-sections; 0 for identical knots, growing with the
    /// differences in continuity, position, handle direction and handle length. Truncated to a whole number.
    pub fn similarity(&self, other: &Knot) -> f64 {
        if self.is_identical(other) {
            return 0.0;
        }
        let mut score = 0.0;
        if self.continuous != other.continuous {
            score += 1.0;
        }
        let dpos = (self.point() - other.point()).length() / 0.5;
        score += 4.0 * dpos * dpos;
        for h in [Handle::Prev, Handle::Next] {
            let delta = angle_between(self.handle_vector(h), other.handle_vector(h)).to_degrees();
            score += 15.0 * (delta / 5.0).powi(2);
        }
        for h in [Handle::Prev, Handle::Next] {
            let dl = (self.handle_length(h) - other.handle_length(h)).abs();
            score += 0.3 * dl * dl;
        }
        if score.is_nan() { 0.0 } else { score.trunc() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_clamp_handles() {
        let mut k = Knot::new(DVec2::new(1.0, 1.0), DVec2::ZERO, DVec2::new(2.0, 2.0));
        k.locks = [Locks::X_NOT_MORE, Locks::X_NOT_LESS | Locks::Y_NOT_MORE];
        k.set_handle(Handle::Prev, DVec2::new(3.0, 0.0));
        assert_eq!(k.prev(), DVec2::new(1.0, 0.0));
        k.set_handle(Handle::Next, DVec2::new(0.0, 5.0));
        assert_eq!(k.next(), DVec2::new(1.0, 1.0));
    }

    #[test]
    fn axis_factors_freeze_moves() {
        let mut k = Knot::new(DVec2::ZERO, DVec2::new(-1.0, 0.0), DVec2::new(1.0, 0.0));
        k.axis = DVec2::new(0.0, 1.0);
        let d = k.move_point_to(DVec2::new(5.0, 2.0));
        assert_eq!(d, DVec2::new(0.0, 2.0));
        assert_eq!(k.next(), DVec2::new(1.0, 2.0));
    }

    #[test]
    fn continuity_test_rejects_mirrored_v() {
        let smooth = Knot::new(DVec2::ZERO, DVec2::new(-1.0, 0.2), DVec2::new(2.0, -0.4));
        assert!(smooth.is_geometrically_continuous());
        let v = Knot::new(DVec2::ZERO, DVec2::new(-1.0, 1.0), DVec2::new(-1.0, -1.0));
        assert!(!v.is_geometrically_continuous());
        // Zero-length handles keep the plain angle rule: (0,0)/(0,-1) has angles 0 and π.
        let end = Knot::new(DVec2::ZERO, DVec2::ZERO, DVec2::new(0.0, -1.0));
        assert!(end.is_geometrically_continuous());
    }

    #[test]
    fn align_keeps_other_length() {
        let mut k = Knot::new(DVec2::ZERO, DVec2::new(-2.0, 0.0), DVec2::new(0.0, 1.0));
        k.align_opposite(Handle::Next);
        assert!((k.prev() - DVec2::new(0.0, -2.0)).length() < 1e-12);
    }

    #[test]
    fn similarity_of_identical_and_shifted() {
        let a = Knot::smooth(DVec2::ZERO, DVec2::new(-1.0, 0.0), DVec2::new(1.0, 0.0));
        assert_eq!(a.similarity(&a.clone()), 0.0);
        let mut b = a.clone();
        b.translate(DVec2::new(0.5, 0.0));
        // Only the position differs: 4 * (0.5 / 0.5)^2.
        assert_eq!(a.similarity(&b), 4.0);
    }
}
