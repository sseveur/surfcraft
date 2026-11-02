//! Knot and cross-section editing shared by the GUI and the command interface.
//!
//! Point moves go through the knots' movement factors (frozen end points) and move the tied end points of the
//! deck and the bottom together; handle moves go through the handle locks, and on a smooth knot the opposite
//! handle is turned to stay in line, keeping its own length. After an edit, [`after_edit`] restores the board's
//! invariants (sections fitted to the outline and rocker, lowest rocker point at zero).

use std::collections::BTreeSet;

use surfcraft_core::{Board, DVec2, Handle, Spline};

/// One of the board's editable curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveId {
    Outline,
    Deck,
    Bottom,
    /// Index into `Board::sections`.
    Section(usize),
}

impl CurveId {
    /// Translation key naming the curve.
    pub fn label(self) -> &'static str {
        match self {
            CurveId::Outline => "Outline",
            CurveId::Deck => "Deck",
            CurveId::Bottom => "Bottom",
            CurveId::Section(_) => "Cross-section",
        }
    }

    pub fn spline(self, b: &Board) -> Option<&Spline> {
        match self {
            CurveId::Outline => Some(&b.outline),
            CurveId::Deck => Some(&b.deck),
            CurveId::Bottom => Some(&b.bottom),
            CurveId::Section(i) => b.sections.get(i).map(|s| &s.spline),
        }
    }

    pub fn spline_mut(self, b: &mut Board) -> Option<&mut Spline> {
        match self {
            CurveId::Outline => Some(&mut b.outline),
            CurveId::Deck => Some(&mut b.deck),
            CurveId::Bottom => Some(&mut b.bottom),
            CurveId::Section(i) => b.sections.get_mut(i).map(|s| &mut s.spline),
        }
    }

    pub fn guides(self, b: &Board) -> Option<&Vec<DVec2>> {
        match self {
            CurveId::Outline => Some(&b.outline_guides),
            CurveId::Deck => Some(&b.deck_guides),
            CurveId::Bottom => Some(&b.bottom_guides),
            CurveId::Section(i) => b.sections.get(i).map(|s| &s.guide_points),
        }
    }

    pub fn guides_mut(self, b: &mut Board) -> Option<&mut Vec<DVec2>> {
        match self {
            CurveId::Outline => Some(&mut b.outline_guides),
            CurveId::Deck => Some(&mut b.deck_guides),
            CurveId::Bottom => Some(&mut b.bottom_guides),
            CurveId::Section(i) => b.sections.get_mut(i).map(|s| &mut s.guide_points),
        }
    }

    /// Fewest knots the curve may be reduced to by deleting.
    fn min_knots(self) -> usize {
        match self {
            CurveId::Section(_) => 3,
            _ => 2,
        }
    }
}

/// Selected knots of one curve and the element (point or handle) being worked on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub curve: CurveId,
    pub knots: BTreeSet<usize>,
    pub handle: Handle,
}

impl Selection {
    pub fn single(curve: CurveId, knot: usize, handle: Handle) -> Self {
        Self {
            curve,
            knots: BTreeSet::from([knot]),
            handle,
        }
    }

    /// The knot shown in the inspector: the first selected one.
    pub fn primary(&self) -> Option<usize> {
        self.knots.first().copied()
    }
}

/// Move knot `i`'s point towards `target` (movement factors apply). The tied end point of the deck or bottom
/// follows, keeping its handles in place relative to it.
fn move_point(b: &mut Board, id: CurveId, i: usize, target: DVec2) {
    let Some(s) = id.spline_mut(b) else {
        return;
    };
    let Some(k) = s.knots_mut().get_mut(i) else {
        return;
    };
    let d = k.move_point_to(target);
    let p = k.point();
    let deck = match id {
        CurveId::Deck => true,
        CurveId::Bottom => false,
        _ => return,
    };
    let Some(j) = b.partner(deck, i) else {
        return;
    };
    let other = if deck { &mut b.bottom } else { &mut b.deck };
    let k = &mut other.knots_mut()[j];
    k.points[0] = p;
    k.points[1] += d;
    k.points[2] += d;
}

/// Move the grabbed handle of knot `i` by `delta` (locks apply); on a smooth knot the other handle is turned to
/// stay opposite.
fn move_handle(b: &mut Board, id: CurveId, i: usize, which: Handle, delta: DVec2) {
    let Some(k) = id.spline_mut(b).and_then(|s| s.knots_mut().get_mut(i)) else {
        return;
    };
    let target = k.get(which) + delta;
    k.set_handle(which, target);
    if k.continuous {
        k.align_opposite(which);
    }
}

/// Drag by `delta`: with several knots selected, or the point grabbed, every selected knot moves; otherwise
/// the grabbed handle of the grabbed knot moves. Call [`after_edit`] when the drag ends.
pub fn drag(b: &mut Board, sel: &Selection, grabbed: (usize, Handle), delta: DVec2) {
    if !delta.is_finite() {
        return;
    }
    let (gi, gh) = grabbed;
    if sel.knots.len() > 1 || gh == Handle::Point {
        let mut knots = sel.knots.clone();
        if knots.is_empty() {
            knots.insert(gi);
        }
        for i in knots {
            let Some(p) = sel
                .curve
                .spline(b)
                .and_then(|s| s.knots().get(i))
                .map(|k| k.point())
            else {
                continue;
            };
            move_point(b, sel.curve, i, p + delta);
        }
    } else {
        move_handle(b, sel.curve, gi, gh, delta);
    }
}

/// Numeric entry: put the point or a handle of knot `i` at `target`, with the same rules as a drag, then
/// restore the board's invariants.
pub fn set_handle(b: &mut Board, id: CurveId, i: usize, which: Handle, target: DVec2) {
    let Some(current) = id
        .spline(b)
        .and_then(|s| s.knots().get(i))
        .map(|k| k.get(which))
    else {
        return;
    };
    if !target.is_finite() {
        return;
    }
    drag(
        b,
        &Selection::single(id, i, which),
        (i, which),
        target - current,
    );
    after_edit(b, id);
}

/// Make knot `i` smooth or a corner. Making it smooth turns the next handle to point opposite the previous one.
pub fn set_continuous(b: &mut Board, id: CurveId, i: usize, on: bool) {
    let Some(k) = id.spline_mut(b).and_then(|s| s.knots_mut().get_mut(i)) else {
        return;
    };
    k.continuous = on;
    if on {
        let prev = k.prev();
        k.set_handle(Handle::Prev, prev);
        k.align_opposite(Handle::Prev);
    }
    after_edit(b, id);
}

/// Make the handles of knot `i` horizontal (or vertical with `vertical`), keeping their lengths and sides.
/// The grabbed element decides which handle leads, as when dragging.
pub fn make_level(b: &mut Board, id: CurveId, i: usize, grabbed: Handle, vertical: bool) {
    let Some(k) = id.spline_mut(b).and_then(|s| s.knots_mut().get_mut(i)) else {
        return;
    };
    let p = k.point();
    let (lp, ln) = (k.handle_length(Handle::Prev), k.handle_length(Handle::Next));
    let comp = |v: DVec2| if vertical { v.y } else { v.x };
    let sp = if comp(k.prev() - p) > 0.0 { 1.0 } else { -1.0 };
    let sn = if comp(k.next() - p) >= 0.0 { 1.0 } else { -1.0 };
    let at = |len: f64| {
        if vertical {
            p + DVec2::new(0.0, len)
        } else {
            p + DVec2::new(len, 0.0)
        }
    };
    let prev_leads = grabbed != Handle::Next;
    if prev_leads {
        k.set_handle(Handle::Prev, at(lp * sp));
        if k.continuous {
            k.set_handle(Handle::Next, at(-ln * sp));
        }
    }
    if !prev_leads || (vertical && grabbed == Handle::Point) {
        k.set_handle(Handle::Next, at(ln * sn));
        if k.continuous {
            k.set_handle(Handle::Prev, at(-lp * sn));
        }
    }
    after_edit(b, id);
}

/// Insert a knot on the curve at the point nearest to `p`, without changing its shape. Returns its index.
pub fn add_knot(b: &mut Board, id: CurveId, p: DVec2) -> Option<usize> {
    if !p.is_finite() {
        return None;
    }
    let i = id.spline_mut(b)?.split_near_precise(p)?;
    after_edit(b, id);
    Some(i)
}

/// Delete the given knots of a curve (never its end knots, never below the curve's minimum knot count). The
/// neighbouring handles are lengthened so that the curve keeps close to its shape. Returns whether anything
/// was deleted.
pub fn delete_knots(b: &mut Board, id: CurveId, knots: &BTreeSet<usize>) -> bool {
    let min = id.min_knots();
    let Some(s) = id.spline_mut(b) else {
        return false;
    };
    let mut deleted = false;
    for &i in knots.iter().rev() {
        if s.len() <= min {
            break;
        }
        if i > 0 && i + 1 < s.len() && s.remove_compensated(i) {
            deleted = true;
        }
    }
    if deleted {
        after_edit(b, id);
    }
    deleted
}

/// Whether section `i` may be moved to `x`: inside the board and not on top of another section.
pub fn section_position_free(b: &Board, i: usize, x: f64) -> bool {
    b.section_position_free(Some(i), x)
}

/// Add a cross-section at `x`, interpolated from its neighbours. Returns its index, or `None` when `x` is
/// outside the board or too close to another section.
pub fn add_section(b: &mut Board, x: f64) -> Option<usize> {
    let i = b.add_section(x)?;
    b.apply_locks();
    Some(i)
}

/// Move section `i` to `x` and fit it there; returns its new index (unchanged when the move is refused).
pub fn move_section(b: &mut Board, i: usize, x: f64) -> usize {
    b.move_section(i, x).unwrap_or(i)
}

/// Remove section `i`; the last real section cannot be removed.
pub fn remove_section(b: &mut Board, i: usize) -> bool {
    b.remove_section(i)
}

/// Restore the board's invariants after an edit of `id`: sections fitted to the outline and rocker, lowest
/// rocker point at zero after a profile edit, editing constraints on every knot (also new ones).
pub fn after_edit(b: &mut Board, id: CurveId) {
    match id {
        CurveId::Outline => b.on_outline_changed(),
        CurveId::Deck | CurveId::Bottom => b.on_rocker_changed(),
        CurveId::Section(_) => b.fit_sections(),
    }
    b.apply_locks();
}

/// Corners turning by more than this are treated as deliberate (a squash-tail corner, the tail block of the deck)
/// when smoothing a whole curve.
pub const SMOOTH_CORNER_LIMIT_DEG: f64 = 30.0;

/// Smooth knots of a curve so it has no kinks there. Each knot becomes continuous, with both handles turned to one
/// tangent direction (the average of the directions the curve comes in and goes out) and their lengths kept, so the
/// shape changes as little as possible apart from losing the corner. A zero-length handle gets a third of the chord
/// to the neighbouring point.
///
/// `knots`: the knots to smooth; `None` smooths every knot whose corner turns by less than
/// [`SMOOTH_CORNER_LIMIT_DEG`] (small kinks, not deliberate corners). The first and last knots are never changed:
/// they sit on the centre line or at the ends of the board. Returns how many knots changed.
pub fn smooth_knots(b: &mut Board, id: CurveId, knots: Option<&BTreeSet<usize>>) -> usize {
    let Some(s) = id.spline_mut(b) else {
        return 0;
    };
    let points: Vec<DVec2> = s.knots().iter().map(|k| k.point()).collect();
    let n = points.len();
    let mut changed = 0;
    for i in 1..n.saturating_sub(1) {
        if knots.is_some_and(|set| !set.contains(&i)) {
            continue;
        }
        let k = &mut s.knots_mut()[i];
        let p = k.point();
        let (vin, vout) = (p - k.prev(), k.next() - p);
        let unit = |v: DVec2, chord: DVec2| {
            if v.length() > 1e-9 {
                v.normalize()
            } else {
                chord.normalize_or_zero()
            }
        };
        let din = unit(vin, p - points[i - 1]);
        let dout = unit(vout, points[i + 1] - p);
        if knots.is_none() {
            let turn = din.dot(dout).clamp(-1.0, 1.0).acos().to_degrees();
            if k.continuous || turn >= SMOOTH_CORNER_LIMIT_DEG {
                continue;
            }
        }
        let mut dir = (din + dout).normalize_or_zero();
        if dir == DVec2::ZERO {
            // The curve doubles back on itself here: follow the chord between the neighbours instead.
            dir = (points[i + 1] - points[i - 1]).normalize_or_zero();
        }
        if dir == DVec2::ZERO {
            continue;
        }
        let len = |v: DVec2, chord: DVec2| {
            if v.length() > 1e-9 {
                v.length()
            } else {
                chord.length() / 3.0
            }
        };
        let lp = len(vin, p - points[i - 1]);
        let ln = len(vout, points[i + 1] - p);
        let before = k.clone();
        k.continuous = true;
        k.set_handle(Handle::Prev, p - dir * lp);
        k.set_handle(Handle::Next, p + dir * ln);
        if !k.is_identical(&before) {
            changed += 1;
        }
    }
    if changed > 0 {
        after_edit(b, id);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Board {
        surfcraft_io::templates::TEMPLATES[0].board()
    }

    fn point(b: &Board, id: CurveId, i: usize) -> DVec2 {
        id.spline(b).unwrap().knots()[i].point()
    }

    #[test]
    fn end_points_are_constrained() {
        let mut b = board();
        set_handle(
            &mut b,
            CurveId::Outline,
            0,
            Handle::Point,
            DVec2::new(5.0, 5.0),
        );
        assert_eq!(point(&b, CurveId::Outline, 0), DVec2::ZERO);
        // Deck ends move vertically only, and take the bottom's end with them.
        let n = b.deck.len();
        let before = point(&b, CurveId::Deck, n - 1);
        let sel = Selection::single(CurveId::Deck, n - 1, Handle::Point);
        drag(&mut b, &sel, (n - 1, Handle::Point), DVec2::new(3.0, 1.0));
        let after = point(&b, CurveId::Deck, n - 1);
        assert_eq!(after, before + DVec2::new(0.0, 1.0));
        assert_eq!(point(&b, CurveId::Bottom, b.bottom.len() - 1), after);
    }

    #[test]
    fn smooth_knot_handles_stay_opposite() {
        let mut b = board();
        let i = (1..b.outline.len() - 1)
            .find(|&i| b.outline.knots()[i].continuous)
            .unwrap();
        let k0 = b.outline.knots()[i].clone();
        let sel = Selection::single(CurveId::Outline, i, Handle::Next);
        drag(&mut b, &sel, (i, Handle::Next), DVec2::new(-3.0, 4.0));
        let k = &b.outline.knots()[i];
        let (a, c) = (k.next() - k.point(), k.prev() - k.point());
        assert!((a.normalize() + c.normalize()).length() < 1e-9);
        assert!((c.length() - (k0.prev() - k0.point()).length()).abs() < 1e-9);
    }

    #[test]
    fn handle_locks_apply() {
        let mut b = board();
        // The outline's next handles may not point towards the tail.
        let k = b.outline.knots()[1].clone();
        set_handle(
            &mut b,
            CurveId::Outline,
            1,
            Handle::Next,
            k.point() - DVec2::new(10.0, 0.0),
        );
        assert!(b.outline.knots()[1].next().x >= b.outline.knots()[1].point().x);
    }

    #[test]
    fn multi_select_moves_every_knot() {
        let mut b = board();
        let sel = Selection {
            curve: CurveId::Outline,
            knots: BTreeSet::from([1, 2]),
            handle: Handle::Prev,
        };
        let (p1, p2) = (
            point(&b, CurveId::Outline, 1),
            point(&b, CurveId::Outline, 2),
        );
        drag(&mut b, &sel, (1, Handle::Prev), DVec2::new(0.5, 0.25));
        assert_eq!(point(&b, CurveId::Outline, 1), p1 + DVec2::new(0.5, 0.25));
        assert_eq!(point(&b, CurveId::Outline, 2), p2 + DVec2::new(0.5, 0.25));
    }

    #[test]
    fn add_and_delete_knot_keep_shape() {
        let mut b = board();
        let w = b.width_at(60.0);
        let n = b.outline.len();
        let at = DVec2::new(140.0, b.width_at(140.0) / 2.0);
        let i = add_knot(&mut b, CurveId::Outline, at).unwrap();
        assert_eq!(b.outline.len(), n + 1);
        assert!((point(&b, CurveId::Outline, i).x - 140.0).abs() < 1.0);
        assert!((b.width_at(60.0) - w).abs() < 1e-6);
        // The new knot gets the curve's handle locks.
        assert_eq!(b.outline.knots()[i].locks, b.outline.knots()[1].locks);
        assert!(delete_knots(&mut b, CurveId::Outline, &BTreeSet::from([i])));
        assert_eq!(b.outline.len(), n);
        assert!((b.width_at(160.0) - board().width_at(160.0)).abs() < 0.2);
        // End knots are never deleted.
        assert!(!delete_knots(
            &mut b,
            CurveId::Outline,
            &BTreeSet::from([0, n - 1])
        ));
    }

    #[test]
    fn continuity_toggle_realigns() {
        let mut b = board();
        let i = 1;
        set_continuous(&mut b, CurveId::Deck, i, false);
        assert!(!b.deck.knots()[i].continuous);
        set_continuous(&mut b, CurveId::Deck, i, true);
        let k = &b.deck.knots()[i];
        assert!(k.continuous);
        let (a, c) = (k.next() - k.point(), k.prev() - k.point());
        if a.length() > 0.0 && c.length() > 0.0 {
            assert!((a.normalize() + c.normalize()).length() < 1e-9);
        }
    }

    #[test]
    fn sections_add_move_remove() {
        let mut b = board();
        let r = b.real_sections();
        let p = b.sections[r.start].position;
        assert!(add_section(&mut b, p + 0.05).is_none());
        let i = add_section(&mut b, p + 10.0).unwrap();
        assert_eq!(b.sections[i].position, p + 10.0);
        assert!((b.sections[i].width() - b.width_at(p + 10.0)).abs() < 0.05);
        assert!((b.sections[i].center_thickness() - b.thickness_at(p + 10.0)).abs() < 1e-6);
        // Moving onto another section is refused.
        let other = b.sections[i + 1].position;
        assert!(!section_position_free(&b, i, other));
        assert_eq!(move_section(&mut b, i, other), i);
        let x = b.length() - 20.0;
        let j = move_section(&mut b, i, x);
        assert_eq!(b.sections[j].position, b.length() - 20.0);
        assert!((b.sections[j].width() - b.width_at(b.length() - 20.0)).abs() < 0.05);
        while b.real_sections().len() > 1 {
            let first = b.real_sections().start;
            assert!(remove_section(&mut b, first));
        }
        let first = b.real_sections().start;
        assert!(!remove_section(&mut b, first));
        assert!(b.volume() > 0.0);
    }

    #[test]
    fn profile_edit_keeps_rocker_at_zero() {
        let mut b = board();
        let i = b.bottom.len() / 2;
        let target = point(&b, CurveId::Bottom, i) - DVec2::new(0.0, 1.0);
        set_handle(&mut b, CurveId::Bottom, i, Handle::Point, target);
        assert!(b.bottom.min_y().abs() < 1e-9);
    }

    fn turn_deg(b: &Board, id: CurveId, i: usize) -> f64 {
        let k = &id.spline(b).unwrap().knots()[i];
        let (din, dout) = (
            (k.point() - k.prev()).normalize(),
            (k.next() - k.point()).normalize(),
        );
        din.dot(dout).clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// Board with a small kink at the wide point of the outline.
    fn kinked() -> (Board, usize) {
        let mut b = board();
        let wide = (1..b.outline.len() - 1)
            .max_by(|&i, &j| {
                let y = |k: usize| point(&b, CurveId::Outline, k).y;
                y(i).total_cmp(&y(j))
            })
            .unwrap();
        let k = &mut b.outline.knots_mut()[wide];
        k.continuous = false;
        k.rotate_handle(Handle::Next, (-8.0f64).to_radians());
        assert!(turn_deg(&b, CurveId::Outline, wide) > 7.0);
        (b, wide)
    }

    #[test]
    fn smoothing_removes_small_kinks_and_keeps_handle_lengths() {
        let (mut b, wide) = kinked();
        let k = b.outline.knots()[wide].clone();
        let (lp, ln) = (k.handle_length(Handle::Prev), k.handle_length(Handle::Next));
        let ends = (
            b.outline.knots()[0].clone(),
            b.outline.knots().last().unwrap().clone(),
        );
        assert!(smooth_knots(&mut b, CurveId::Outline, None) >= 1);
        let k = &b.outline.knots()[wide];
        assert!(k.continuous);
        assert!(
            turn_deg(&b, CurveId::Outline, wide) < 1e-6,
            "handles in line"
        );
        assert!((k.handle_length(Handle::Prev) - lp).abs() < 1e-9);
        assert!((k.handle_length(Handle::Next) - ln).abs() < 1e-9);
        assert_eq!(&b.outline.knots()[0], &ends.0, "end points untouched");
        assert_eq!(b.outline.knots().last().unwrap(), &ends.1);
        // Smoothing again changes nothing.
        assert_eq!(smooth_knots(&mut b, CurveId::Outline, None), 0);
    }

    #[test]
    fn whole_curve_smoothing_keeps_deliberate_corners_selected_smoothing_does_not() {
        let b0 = board();
        // The squash-tail corner of the built-in shortboard turns sharply.
        let corner = (1..b0.outline.len() - 1)
            .find(|&i| !b0.outline.knots()[i].continuous)
            .expect("tail corner");
        assert!(turn_deg(&b0, CurveId::Outline, corner) > SMOOTH_CORNER_LIMIT_DEG);
        let mut b = b0.clone();
        smooth_knots(&mut b, CurveId::Outline, None);
        assert_eq!(b.outline.knots()[corner], b0.outline.knots()[corner]);
        let mut b = b0.clone();
        let sel = BTreeSet::from([corner]);
        assert_eq!(smooth_knots(&mut b, CurveId::Outline, Some(&sel)), 1);
        assert!(b.outline.knots()[corner].continuous);
        assert!(turn_deg(&b, CurveId::Outline, corner) < 1.0);
        // Selecting an end point does nothing.
        let ends = BTreeSet::from([0, b0.outline.len() - 1]);
        let mut b = b0.clone();
        assert_eq!(smooth_knots(&mut b, CurveId::Outline, Some(&ends)), 0);
        assert_eq!(b, b0);
    }

    #[test]
    fn smoothing_every_curve_of_every_template_is_safe() {
        for t in surfcraft_io::templates::TEMPLATES {
            let mut b = t.board();
            let volume = b.volume();
            // Outline and rocker kinks are small: the board barely changes.
            for id in [CurveId::Outline, CurveId::Deck, CurveId::Bottom] {
                smooth_knots(&mut b, id, None);
            }
            assert!((b.volume() - volume).abs() / volume < 0.01, "{}", t.name);
            // Cross-sections: rounding a hard rail edge adds volume, but everything stays valid.
            for i in 0..b.sections.len() {
                smooth_knots(&mut b, CurveId::Section(i), None);
            }
            assert!(
                b.volume().is_finite() && b.volume() > volume * 0.9,
                "{}",
                t.name
            );
        }
    }

    #[test]
    fn labels() {
        assert_eq!(CurveId::Section(3).label(), "Cross-section");
        assert_eq!(CurveId::Bottom.label(), "Bottom");
    }
}
