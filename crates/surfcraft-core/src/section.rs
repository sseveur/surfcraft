//! Cross-sections: the half-section curve at a station along the board, its rail metrics and the knot-matching
//! blend between two sections.

use glam::DVec2;

use crate::knot::{Handle, Knot};
use crate::math::{DEG, ZERO};
use crate::spline::Spline;

/// Normal angle (degrees) of the rail apex, where the section is widest.
pub const APEX_ANGLE: f64 = 90.0;
/// Normal angle (degrees) of the tuck-under point, just short of the bottom.
pub const TUCK_UNDER_ANGLE: f64 = 175.0;

/// A cross-section at `position` cm from the tail.
///
/// The spline runs from the bottom centre `(0, 0)` out round the rail and back to the deck centre
/// `(0, thickness)`; its first coordinate is the distance from the stringer, the second the height above the
/// bottom on the stringer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CrossSection {
    pub position: f64,
    pub spline: Spline,
    pub guide_points: Vec<DVec2>,
}

impl CrossSection {
    pub fn new(position: f64, spline: Spline) -> Self {
        Self {
            position,
            spline,
            guide_points: Vec::new(),
        }
    }

    /// A zero-size end section (one knot at the origin) at `position`.
    pub fn end(position: f64) -> Self {
        Self::new(position, Spline::from_knots(vec![Knot::default()]))
    }

    /// Whether this is a zero-size placeholder (at most one knot).
    pub fn is_end_section(&self) -> bool {
        self.spline.len() <= 1
    }

    /// A plain rounded section, 2 cm wide and 1 cm thick: the shape used when a board has no usable
    /// cross-section of its own.
    pub fn fallback_shape() -> Spline {
        Spline::from_knots(vec![
            Knot::new(DVec2::ZERO, DVec2::new(-0.5, 0.0), DVec2::new(0.6, 0.0)),
            Knot::smooth(
                DVec2::new(1.0, 0.35),
                DVec2::new(1.0, 0.1),
                DVec2::new(1.0, 0.6),
            ),
            Knot::new(DVec2::new(0.0, 1.0), DVec2::new(0.6, 1.0), DVec2::new(-0.5, 1.0)),
        ])
    }

    /// Full width (twice the largest distance from the stringer).
    pub fn width(&self) -> f64 {
        2.0 * self.spline.max_x()
    }

    /// Thickness on the stringer: deck centre minus bottom centre.
    pub fn center_thickness(&self) -> f64 {
        match (self.spline.first(), self.spline.last()) {
            (Some(a), Some(b)) => b.point().y - a.point().y,
            _ => 0.0,
        }
    }

    /// Height of the bottom at lateral distance `u`.
    pub fn bottom_at(&self, u: f64) -> f64 {
        self.spline.value_at(u.abs())
    }

    /// Height of the deck at lateral distance `u`.
    pub fn deck_at(&self, u: f64) -> f64 {
        self.spline.value_at_reverse(u.abs())
    }

    pub fn thickness_at(&self, u: f64) -> f64 {
        self.deck_at(u) - self.bottom_at(u)
    }

    /// Point at fraction `s` of the curve length (0 = bottom centre, 1 = deck centre).
    pub fn point_at_s(&self, s: f64) -> DVec2 {
        self.spline.point_at_s(s)
    }

    /// Scale about the section origin to a new centre thickness and width. Does nothing when either target is
    /// at or below 0.1 cm.
    pub fn scale_to(&mut self, thickness: f64, width: f64) {
        scale_spline_to(&mut self.spline, thickness, width);
    }

    /// Angle (radians) between the two handles of the knot at the tuck-under point; π for a soft, rounded
    /// release, smaller for a hard edge. 0 when no knot lies near the tuck-under point.
    pub fn release_angle(&self) -> f64 {
        let s = self.spline.s_by_normal_reverse(TUCK_UNDER_ANGLE * DEG, true);
        let q = self.point_at_s(s);
        let Some(i) = nearest_knot(&self.spline, q) else {
            return 0.0;
        };
        let k = &self.spline.knots()[i];
        if k.point().distance(q) > 0.9 {
            return 0.0;
        }
        k.angle_between_handles()
    }

    /// Mean radius of the rail between the apex and the tuck-under point (cm), from the curvature at five
    /// points; 0 when apex and tuck are less than 0.18 cm apart laterally.
    pub fn tuck_radius(&self) -> f64 {
        let sa = self.spline.s_by_normal_reverse(APEX_ANGLE * DEG, true);
        let st = self.spline.s_by_normal_reverse(TUCK_UNDER_ANGLE * DEG, true);
        if self.point_at_s(sa).x - self.point_at_s(st).x < 0.18 {
            return 0.0;
        }
        let total = self.spline.length();
        let step = (st - sa) / 5.0;
        let mean = (0..5)
            .map(|i| {
                self.spline
                    .curvature_at_length((sa + i as f64 * step) * total)
            })
            .sum::<f64>()
            / 5.0;
        1.0 / mean
    }

    /// Blend this section's curve towards `target` by fraction `f` (0 = this, 1 = target scaled to this
    /// section's size), matching knots by similarity when the knot counts differ. `None` when the curves
    /// cannot be matched (a curve without segments).
    pub fn blend(&self, target: &CrossSection, f: f64) -> Option<Spline> {
        let a = self.spline.clone();
        let mut b = target.spline.clone();
        scale_spline_to(&mut b, self.center_thickness(), self.width());
        let (a, b) = equalize_knot_counts(a, b)?;
        let mut q = b.clone();
        for (k, (ka, kb)) in q
            .knots_mut()
            .iter_mut()
            .zip(a.knots().iter().zip(b.knots()))
        {
            for h in Handle::ALL {
                let (pa, pb) = (ka.get(h), kb.get(h));
                k.set(h, pa + f * (pb - pa));
            }
        }
        Some(q)
    }
}

/// See [`CrossSection::scale_to`].
pub(crate) fn scale_spline_to(spline: &mut Spline, thickness: f64, width: f64) {
    let w_old = (2.0 * spline.max_x()).max(0.1);
    let t_old = match (spline.first(), spline.last()) {
        (Some(a), Some(b)) => b.point().y - a.point().y,
        _ => 0.0,
    }
    .max(0.1);
    let kt = (thickness / t_old).abs();
    let kw = (width / w_old).abs();
    if !(t_old * kt > 0.1) || !(w_old * kw > 0.1) || !kt.is_finite() || !kw.is_finite() {
        return;
    }
    spline.scale(kw, kt);
}

/// Index of the knot whose point or handle is nearest to `q`, testing segment by segment: the start knot's
/// next handle, the end knot's previous handle, the start point, the end point (earlier candidates win ties).
fn nearest_knot(spline: &Spline, q: DVec2) -> Option<usize> {
    let k = spline.knots();
    let mut best: Option<(f64, usize)> = None;
    for i in 0..spline.segment_count() {
        for (j, h) in [
            (i, Handle::Next),
            (i + 1, Handle::Prev),
            (i, Handle::Point),
            (i + 1, Handle::Point),
        ] {
            let d = k[j].get(h).distance(q);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, j));
            }
        }
    }
    best.map(|b| b.1)
}

/// Make two section curves have the same number of knots by splitting the one with fewer knots where the other
/// has knots that match none of its own. Returns `(a, b)` in the original order.
fn equalize_knot_counts(a: Spline, b: Spline) -> Option<(Spline, Spline)> {
    if a.len() == b.len() {
        return Some((a, b));
    }
    let a_fewer = a.len() < b.len();
    let (mut few, many) = if a_fewer { (a, b) } else { (b, a) };
    let sx = few.max_x() / many.max_x();
    let sy = few.max_y_except_last() / many.max_y_except_last();
    while many.len() > few.len() {
        if few.segment_count() == 0 {
            return None;
        }
        let m = many.knots();
        let f = few.knots();
        let mut worst: Option<(f64, usize)> = None;
        for (i, mk) in m.iter().enumerate().take(m.len() - 1).skip(1) {
            let mut best = 1e7_f64;
            for fk in f.iter().take(f.len().saturating_sub(1)).skip(1) {
                let mut c = fk.clone();
                c.move_point_to(DVec2::new(fk.point().x * sx, fk.point().y * sy));
                best = best.min(mk.similarity(&c));
            }
            if best > worst.map_or(0.0, |w| w.0) {
                worst = Some((best, i));
            }
        }
        // Every interior knot matches perfectly: split at the first one.
        let i = worst.map_or(1, |w| w.1);
        let target = m.get(i).map_or(DVec2::ZERO, Knot::point);
        few.split_near(target)?;
    }
    Some(if a_fewer { (few, many) } else { (many, few) })
}

/// Scale a curve copy to the size of a reference section (used by the section-blend surface).
pub(crate) fn scaled_copy(spline: &Spline, sx: f64, sy: f64) -> Spline {
    let mut s = spline.clone();
    s.scale(sx, sy);
    s
}

/// Lateral thickness of a section at the stringer, measured just off the centre line.
pub(crate) fn thickness_near_stringer(section: &CrossSection) -> f64 {
    section.thickness_at(ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> CrossSection {
        // Bottom centre -> hard-ish rail at (20, 1.5) -> deck centre at thickness 6.
        CrossSection::new(
            50.0,
            Spline::from_knots(vec![
                Knot::new(DVec2::ZERO, DVec2::new(-7.0, 0.0), DVec2::new(7.0, 0.0)),
                Knot::smooth(
                    DVec2::new(20.0, 0.3),
                    DVec2::new(14.0, 0.0),
                    DVec2::new(21.0, 0.35),
                ),
                Knot::smooth(
                    DVec2::new(22.0, 2.0),
                    DVec2::new(22.0, 1.0),
                    DVec2::new(22.0, 3.0),
                ),
                Knot::new(DVec2::new(0.0, 6.0), DVec2::new(10.0, 6.0), DVec2::new(-10.0, 6.0)),
            ]),
        )
    }

    #[test]
    fn size_and_scaling() {
        let mut s = section();
        assert!((s.width() - 44.0).abs() < 1e-6);
        assert_eq!(s.center_thickness(), 6.0);
        s.scale_to(3.0, 22.0);
        assert!((s.width() - 22.0).abs() < 1e-6);
        assert!((s.center_thickness() - 3.0).abs() < 1e-12);
        let before = s.clone();
        s.scale_to(0.05, 22.0);
        assert_eq!(s, before, "tiny targets are ignored");
    }

    #[test]
    fn deck_and_bottom_heights() {
        let s = section();
        assert!(s.bottom_at(0.0).abs() < 1e-9);
        assert!((s.deck_at(0.0) - 6.0).abs() < 1e-9);
        assert!(s.thickness_at(10.0) > 0.0 && s.thickness_at(10.0) < 6.0);
    }

    #[test]
    fn rail_metrics() {
        let s = section();
        // The tuck-under point lies near the smooth knot at (20, 0.3): a soft release.
        let r = s.release_angle();
        assert!((r - std::f64::consts::PI).abs() < 0.01, "{r}");
        let radius = s.tuck_radius();
        assert!(radius > 0.0 && radius < 10.0, "{radius}");
    }

    #[test]
    fn blend_with_more_knots() {
        let a = section();
        let mut b = section();
        b.spline.split_near(DVec2::new(10.0, 5.9)).unwrap();
        let q = a.blend(&b, 0.5).unwrap();
        assert_eq!(q.len(), b.spline.len());
        // Same shape on both sides: the blend is that shape.
        for i in 0..=10 {
            let s = i as f64 / 10.0;
            assert!((q.point_at_s(s) - a.point_at_s(s)).length() < 0.05);
        }
    }
}
