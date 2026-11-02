//! QA of asymmetric boards in the GUI: Board > Asymmetric outline, editing each rail in the Outline view, keeping a
//! rail when going back to symmetric, undo. Screenshots go to `target/gui-shots/asym-*.png`.

use eframe::egui::{self, Modifiers, Pos2};
use egui_kittest::kittest::Queryable as _;
use surfcraft_core::{DVec2, Handle, Rail};

use crate::app::Tab;
use crate::edit::CurveId;
use crate::gui_tests::{H, drag, harness, set_tab, settle, snap};

fn click(h: &mut H, label: &str) {
    h.get_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("no widget {label:?}"))
        .click_accesskit();
    h.run_steps(3);
}

fn toggle_menu(h: &mut H) {
    click(h, "Board");
    click(h, "Asymmetric outline");
    settle(h);
}

/// Screen position of a knot point in the Outline view: the left rail above the stringer, the right rail of an
/// asymmetric board below it.
fn knot_screen(h: &H, curve: CurveId, i: usize) -> Pos2 {
    let app = h.state();
    let b = &app.doc.board;
    let p = curve.spline(b).unwrap().knots()[i].get(Handle::Point);
    let sign = curve.rail(b).map_or(1.0, Rail::sign);
    app.ed
        .outline_view
        .xf
        .to_screen(DVec2::new(p.x, p.y * sign))
}

fn point(h: &H, curve: CurveId, i: usize) -> DVec2 {
    curve.spline(&h.state().doc.board).unwrap().knots()[i].point()
}

#[test]
fn asymmetric_outline_toggle_edit_each_rail_keep_and_undo() {
    let mut h = harness();
    h.state_mut().set.snap.enabled = false;
    set_tab(&mut h, Tab::Outline);
    settle(&mut h);
    let symmetric = h.state().doc.board.clone();
    assert!(!symmetric.is_asymmetric());

    // On: the left rail starts as a copy, one undo step.
    toggle_menu(&mut h);
    assert!(h.state().doc.board.is_asymmetric());
    assert_eq!(
        h.state().doc.board.outline_left.as_ref(),
        Some(&symmetric.outline)
    );
    h.state_mut().doc.undo();
    assert_eq!(h.state().doc.board, symmetric);
    h.state_mut().doc.redo();
    settle(&mut h);

    // Drag a left-rail knot outward (up on screen): only the left rail changes.
    let i = symmetric.outline.len() / 2;
    let before = h.state().doc.board.clone();
    let from = knot_screen(&h, CurveId::OutlineLeft, i);
    drag(&mut h, from, from + egui::vec2(0.0, -25.0), Modifiers::NONE);
    settle(&mut h);
    let moved = point(&h, CurveId::OutlineLeft, i);
    let was = before.outline_left.as_ref().unwrap().knots()[i].point();
    assert!(moved.y > was.y + 0.5, "left knot {was} -> {moved}");
    assert_eq!(
        h.state().doc.board.outline,
        before.outline,
        "right rail unchanged"
    );
    assert_eq!(
        h.state().ed.selection.as_ref().map(|s| s.curve),
        Some(CurveId::OutlineLeft)
    );
    let b = &h.state().doc.board;
    assert!(b.max_half_width(Rail::Left).0 > b.max_half_width(Rail::Right).0 + 0.5);
    snap(&mut h, "asym-left-dragged");

    // One undo step brings the left rail back.
    h.state_mut().doc.undo();
    assert_eq!(h.state().doc.board, before);
    h.state_mut().doc.redo();
    settle(&mut h);

    // Drag a right-rail knot outward (down on screen, below the stringer): only the right rail changes.
    let mid = h.state().doc.board.clone();
    let from = knot_screen(&h, CurveId::Outline, i);
    drag(&mut h, from, from + egui::vec2(0.0, 25.0), Modifiers::NONE);
    settle(&mut h);
    let moved = point(&h, CurveId::Outline, i);
    let was = mid.outline.knots()[i].point();
    assert!(moved.y > was.y + 0.5, "right knot {was} -> {moved}");
    assert_eq!(h.state().doc.board.outline_left, mid.outline_left);
    snap(&mut h, "asym-both-dragged");

    // Off: the window asks which rail to keep.
    let left = h.state().doc.board.outline_left.clone().unwrap();
    toggle_menu(&mut h);
    assert!(h.state().dialogs.asym_keep);
    assert!(
        h.state().doc.board.is_asymmetric(),
        "nothing changes before the choice"
    );
    click(&mut h, "Keep the left rail (upper half)");
    settle(&mut h);
    assert!(!h.state().doc.board.is_asymmetric());
    assert_eq!(h.state().doc.board.outline, left);
    assert!(!h.state().dialogs.asym_keep);
    h.state_mut().doc.undo();
    assert!(h.state().doc.board.is_asymmetric());
}
