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
    let Some(k) = id
        .spline_mut(b)
        .and_then(|s| s.knots_mut().get_mut(i))
    else {
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
    let Some(current) = id.spline(b).and_then(|s| s.knots().get(i)).map(|k| k.get(which)) else {
        return;
    };
    if !target.is_finite() {
        return;
    }
    drag(b, &Selection::single(id, i, which), (i, which), target - current);
    after_edit(b, id);
}

/// Make knot `i` smooth or a corner. Making it smooth turns the next handle to point opposite the previous one.
pub fn set_continuous(b: &mut Board, id: CurveId, i: usize, on: bool) {
    let Some(k) = id
        .spline_mut(b)
        .and_then(|s| s.knots_mut().get_mut(i))
    else {
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
    let Some(k) = id
        .spline_mut(b)
        .and_then(|s| s.knots_mut().get_mut(i))
    else {
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
    let i = id.spline_mut(b)?.split_near(p)?;
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
        set_handle(&mut b, CurveId::Outline, 0, Handle::Point, DVec2::new(5.0, 5.0));
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
        let (p1, p2) = (point(&b, CurveId::Outline, 1), point(&b, CurveId::Outline, 2));
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
        assert!(!delete_knots(&mut b, CurveId::Outline, &BTreeSet::from([0, n - 1])));
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

    #[test]
    fn labels() {
        assert_eq!(CurveId::Section(3).label(), "Cross-section");
        assert_eq!(CurveId::Bottom.label(), "Bottom");
    }
}
