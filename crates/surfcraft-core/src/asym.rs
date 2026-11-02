//! Asymmetric boards: a separate outline for each rail.
//!
//! Frame (as everywhere in SurfCraft): x from the tail to the nose, z up, y across. **y > 0 is the rider's left**
//! when standing on the deck facing the nose, y < 0 the rider's right; the board frame is right-handed, so a
//! 3-D model or a toolpath written in it has the rails where the rider expects them. In the Outline view (the
//! deck seen from above, nose to the right) the left rail is drawn above the stringer and the right rail below.
//!
//! [`Board::outline`] is the **right** rail's half outline. [`Board::outline_left`] is the left rail's, or `None`
//! for a symmetric board, whose single outline then describes both rails (and is drawn above the stringer,
//! mirrored below, as always). Lengths of both outlines are the same: their end points are fixed.
//!
//! The cross-sections stay symmetric shapes: at a station the section shape is scaled to each rail's own half
//! width (and to the stringer thickness), so the surface of each half is the symmetric surface of a board with
//! that rail's outline mirrored. Metrics add the two halves (width = left half + right half, volume = left half
//! volume + right half volume), so the volume of an asymmetric board is the average of the volumes of the two
//! symmetric boards made by mirroring each rail.

use glam::{DVec2, DVec3};

use crate::board::Board;
use crate::spline::Spline;

/// One rail (half) of the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rail {
    /// Rider's left, y > 0, drawn above the stringer in the Outline view.
    Left,
    /// Rider's right, y < 0, drawn below the stringer in the Outline view.
    Right,
}

impl Rail {
    /// Both rails: the y > 0 half first.
    pub const BOTH: [Rail; 2] = [Rail::Left, Rail::Right];

    /// +1 for the left rail (y > 0), -1 for the right.
    pub fn sign(self) -> f64 {
        match self {
            Rail::Left => 1.0,
            Rail::Right => -1.0,
        }
    }

    /// The rail on the side of lateral offset `y` (0 counts as left).
    pub fn of_y(y: f64) -> Rail {
        if y < 0.0 { Rail::Right } else { Rail::Left }
    }

    pub fn other(self) -> Rail {
        match self {
            Rail::Left => Rail::Right,
            Rail::Right => Rail::Left,
        }
    }

    /// A point computed on the y >= 0 half put on this rail (y mirrored for the right rail).
    pub fn place(self, p: DVec3) -> DVec3 {
        match self {
            Rail::Left => p,
            Rail::Right => DVec3::new(p.x, -p.y, p.z),
        }
    }

    /// A 2-D plan point (x, half width) put on this rail.
    pub fn place2(self, p: DVec2) -> DVec2 {
        match self {
            Rail::Left => p,
            Rail::Right => DVec2::new(p.x, -p.y),
        }
    }

    /// English name ("left" / "right").
    pub fn name(self) -> &'static str {
        match self {
            Rail::Left => "left",
            Rail::Right => "right",
        }
    }
}

/// Samples of the coarse search for the widest point of an asymmetric board.
const WIDTH_SAMPLES: usize = 400;

impl Board {
    /// Whether the rails have their own outlines.
    pub fn is_asymmetric(&self) -> bool {
        self.outline_left.is_some()
    }

    /// Half outline of a rail (the common outline for a symmetric board).
    pub fn outline_of(&self, rail: Rail) -> &Spline {
        match (rail, &self.outline_left) {
            (Rail::Left, Some(s)) => s,
            _ => &self.outline,
        }
    }

    /// Guide points of a rail's outline.
    pub fn outline_guides_of(&self, rail: Rail) -> &Vec<DVec2> {
        match rail {
            Rail::Left if self.outline_left.is_some() => &self.outline_left_guides,
            _ => &self.outline_guides,
        }
    }

    /// Half width of a rail at station `x` (distance from the stringer to that rail's edge).
    pub fn half_width_at(&self, x: f64, rail: Rail) -> f64 {
        self.outline_of(rail).value_at(x)
    }

    /// Widest half width of a rail and its distance from the tail.
    pub fn max_half_width(&self, rail: Rail) -> (f64, f64) {
        let s = self.outline_of(rail);
        (s.max_y(), s.x_of_max_y())
    }

    /// Make the board asymmetric (`on`; the left rail starts as a copy of the current outline) or symmetric
    /// (keeping the outline of `keep` for both rails). Returns whether anything changed.
    pub fn set_asymmetric(&mut self, on: bool, keep: Rail) -> bool {
        match (on, self.outline_left.take()) {
            (true, None) => {
                self.outline_left = Some(self.outline.clone());
                self.outline_left_guides = self.outline_guides.clone();
                self.apply_locks();
                true
            }
            (true, Some(left)) => {
                self.outline_left = Some(left);
                false
            }
            (false, None) => false,
            (false, Some(left)) => {
                let guides = std::mem::take(&mut self.outline_left_guides);
                if keep == Rail::Left {
                    self.outline = left;
                    self.outline_guides = guides;
                }
                self.on_outline_changed();
                self.apply_locks();
                true
            }
        }
    }

    /// Swap the rails: the board mirrored across the stringer (the outlines exchanged, the fins mirrored).
    /// Nothing changes on a symmetric board.
    pub fn mirror_rails(&mut self) {
        let Some(left) = self.outline_left.as_mut() else {
            return;
        };
        std::mem::swap(&mut self.outline, left);
        std::mem::swap(&mut self.outline_guides, &mut self.outline_left_guides);
        if let Some(mut s) = self.fin_setup.take() {
            for f in &mut s.fins {
                *f = f.mirrored();
            }
            self.set_fin_setup(s);
        }
    }

    /// The board with its rails swapped (see [`Self::mirror_rails`]).
    pub fn rails_mirrored(&self) -> Board {
        let mut b = self.clone();
        b.mirror_rails();
        b
    }

    /// The symmetric board made of one rail mirrored to both sides.
    pub fn symmetric_of(&self, rail: Rail) -> Board {
        let mut b = self.clone();
        b.set_asymmetric(false, rail);
        b
    }

    /// Widest full width (left + right half) of an asymmetric board and its distance from the tail.
    pub(crate) fn asym_max_width(&self) -> (f64, f64) {
        let l = self.length();
        if l.is_nan() || l <= 0.0 {
            return (0.0, 0.0);
        }
        let w = |x: f64| self.width_at(x);
        let step = l / WIDTH_SAMPLES as f64;
        let mut best = (f64::NEG_INFINITY, 0.0);
        for i in 0..=WIDTH_SAMPLES {
            let x = i as f64 * step;
            let v = w(x);
            if v > best.0 {
                best = (v, x);
            }
        }
        // Golden-section refinement around the best sample.
        let (mut a, mut b) = ((best.1 - step).max(0.0), (best.1 + step).min(l));
        let g = (5f64.sqrt() - 1.0) / 2.0;
        for _ in 0..60 {
            let (c, d) = (b - g * (b - a), a + g * (b - a));
            if w(c) >= w(d) {
                b = d;
            } else {
                a = c;
            }
        }
        let x = (a + b) / 2.0;
        let v = w(x);
        if v > best.0 { (v, x) } else { best }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mesh::tests::toy_board;

    /// The toy board with its left rail 3 cm wider at the middle and its right rail unchanged.
    pub(crate) fn asym_toy() -> Board {
        let mut b = toy_board();
        b.set_asymmetric(true, Rail::Right);
        let left = b.outline_left.as_mut().unwrap();
        left.knots_mut()[2].translate(DVec2::new(0.0, 3.0));
        b.on_outline_changed();
        b
    }

    #[test]
    fn toggling_copies_and_keeps() {
        let mut b = toy_board();
        b.apply_locks();
        let before = b.clone();
        assert!(!b.is_asymmetric());
        assert!(b.set_asymmetric(true, Rail::Left));
        assert!(b.is_asymmetric());
        assert_eq!(b.outline_of(Rail::Left), &before.outline);
        assert!(!b.set_asymmetric(true, Rail::Left));
        let mut a = asym_toy();
        let left = a.outline_left.clone().unwrap();
        assert!(a.set_asymmetric(false, Rail::Left));
        assert_eq!(a.outline, left);
        assert!(!a.is_asymmetric());
        let mut a = asym_toy();
        a.set_asymmetric(false, Rail::Right);
        assert_eq!(a.outline, before.outline);
    }

    #[test]
    fn widths_add_the_two_rails() {
        let b = asym_toy();
        let (l, r) = (
            b.half_width_at(90.0, Rail::Left),
            b.half_width_at(90.0, Rail::Right),
        );
        assert!((l - r - 3.0).abs() < 0.05, "{l} {r}");
        assert!((b.width_at(90.0) - (l + r)).abs() < 1e-12);
        let (w, x) = b.max_width_and_pos();
        assert!((x - 90.0).abs() < 1.0, "{x}");
        assert!((w - 53.0).abs() < 0.05, "{w}");
        assert_eq!(b.max_width(), w);
        let s = toy_board();
        assert_eq!(s.max_width(), 2.0 * s.outline.max_y());
    }

    #[test]
    fn volume_is_the_average_of_the_mirrored_boards() {
        let b = asym_toy();
        let (vl, vr) = (
            b.symmetric_of(Rail::Left).volume(),
            b.symmetric_of(Rail::Right).volume(),
        );
        let v = b.volume();
        assert!(vl > vr * 1.03, "{vl} {vr}");
        assert!(((vl + vr) / 2.0 - v).abs() / v < 1e-3, "{v} vs {vl} {vr}");
        let a = b.area();
        let (al, ar) = (
            b.symmetric_of(Rail::Left).area(),
            b.symmetric_of(Rail::Right).area(),
        );
        assert!(((al + ar) / 2.0 - a).abs() / a < 1e-9);
    }

    #[test]
    fn mirroring_swaps_the_rails() {
        let b = asym_toy();
        let m = b.rails_mirrored();
        assert_eq!(m.outline_of(Rail::Left), b.outline_of(Rail::Right));
        assert_eq!(m.outline_of(Rail::Right), b.outline_of(Rail::Left));
        let p = b.surface_point_on(90.0, 0.5, -90.0, 360.0, Rail::Left);
        let q = m.surface_point_on(90.0, 0.5, -90.0, 360.0, Rail::Right);
        assert!((p - DVec3::new(q.x, -q.y, q.z)).length() < 1e-9);
    }

    #[test]
    fn control_point_model_volume_and_points() {
        let mut b = asym_toy();
        b.model = crate::SurfaceModel::ControlPoint;
        let v = b.volume();
        let (vl, vr) = (
            b.symmetric_of(Rail::Left).volume(),
            b.symmetric_of(Rail::Right).volume(),
        );
        assert!(((vl + vr) / 2.0 - v).abs() / v < 2e-3, "{v} vs {vl} {vr}");
        for rail in Rail::BOTH {
            let apex = b.surface_point_on(90.0, 1.0, -90.0, 90.0, rail);
            let half = b.half_width_at(90.0, rail);
            assert!((apex.y - rail.sign() * half).abs() < 0.1, "{rail:?} {apex}");
            let n = b.surface_normal_on(90.0, 1.0, -90.0, 90.0, rail);
            assert!(n.y * rail.sign() > 0.5, "{rail:?} {n}");
        }
    }

    #[test]
    fn mesh_is_watertight_and_reaches_each_rail() {
        let b = asym_toy();
        let m = crate::Mesh::of_board(&b, crate::MeshDetail::default());
        assert!(m.is_watertight());
        let v = b.volume();
        assert!((m.volume() - v).abs() / v < 0.02, "{} vs {v}", m.volume());
        let ymax = m.positions.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        let ymin = m.positions.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        assert!(
            (ymax - b.max_half_width(Rail::Left).0).abs() < 0.3,
            "{ymax}"
        );
        assert!(
            (-ymin - b.max_half_width(Rail::Right).0).abs() < 0.3,
            "{ymin}"
        );
        // A symmetric board's mesh is unchanged by the asymmetric code path.
        let s = toy_board();
        let mut a = s.clone();
        a.set_asymmetric(true, Rail::Left);
        let (ms, ma) = (
            crate::Mesh::of_board(&s, crate::MeshDetail::PREVIEW),
            crate::Mesh::of_board(&a, crate::MeshDetail::PREVIEW),
        );
        assert_eq!(ms.triangles, ma.triangles);
        for (p, q) in ms.positions.iter().zip(&ma.positions) {
            assert!((*p - *q).length() < 1e-9);
        }
    }

    #[test]
    fn height_at_xy_uses_the_rail_on_that_side() {
        let b = asym_toy();
        let (l, r) = (
            b.half_width_at(90.0, Rail::Left),
            b.half_width_at(90.0, Rail::Right),
        );
        // Between the two rails' edges: inside on the left, outside on the right.
        let y = (l + r) / 2.0;
        assert!(b.deck_at_xy(90.0, y) > 0.0);
        assert_eq!(b.deck_at_xy(90.0, -y), 0.0);
        assert!(b.thickness_at_xy(90.0, -r / 2.0) > 0.0);
    }

    #[test]
    fn fin_presets_follow_each_rail() {
        let b = asym_toy();
        let setup = crate::FinSetup::preset(crate::Preset::Thruster, crate::BoxSystem::FcsII, &b);
        let sides: Vec<_> = setup.fins.iter().filter(|f| f.y != 0.0).collect();
        assert_eq!(sides.len(), 2);
        let (fl, fr) = (sides[0].from_rail(&b), sides[1].from_rail(&b));
        assert!((fl - fr).abs() < 1e-9, "{fl} {fr}");
        assert!(sides[0].y > 0.0 && sides[1].y < 0.0);
        assert!(sides[0].y > -sides[1].y);
    }
}
