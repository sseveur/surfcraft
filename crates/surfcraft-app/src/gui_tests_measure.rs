//! GUI tests for snapping and the Measure tool.

use eframe::egui::{self, Modifiers};
use egui_kittest::kittest::Queryable as _;
use surfcraft_core::{DVec2, Handle};

use crate::app::Tab;
use crate::gui_tests::*;
use crate::snap::SnapSettings;
use crate::views::Tool;

/// Click the last widget with this label.
fn click(h: &mut H, label: &str) {
    let pos = h
        .query_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("no {label:?}"))
        .rect()
        .center();
    click_pos(h, pos);
}

fn click_pos(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(3);
}

fn outline_knot(h: &H, i: usize) -> DVec2 {
    h.state().doc.board.outline.knots()[i].point()
}

/// Grid spacing the outline view shows (imperial default: 1" coarsened by tens until 6 px apart).
fn outline_grid(h: &H) -> f64 {
    let scale = h.state().ed.outline_view.xf.scale as f64;
    let mut step = if h.state().set.length_unit().is_imperial() {
        2.54
    } else {
        1.0
    };
    while scale * step < 6.0 {
        step *= 10.0;
    }
    step
}

fn on_grid(v: f64, step: f64) -> bool {
    ((v / step).round() * step - v).abs() < 1e-6
}

#[test]
fn dragged_knot_snaps_to_the_grid() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    h.state_mut().set.snap = SnapSettings {
        enabled: true,
        grid: true,
        ..Default::default()
    };
    h.run_steps(2);
    let step = outline_grid(&h);
    let before = outline_knot(&h, 2);
    assert!(!on_grid(before.x, step) || !on_grid(before.y, step));
    let from = outline_screen(&h, 2, Handle::Point);
    drag(
        &mut h,
        from,
        from + egui::vec2(13.0, -17.0),
        Modifiers::NONE,
    );
    let after = outline_knot(&h, 2);
    assert!(
        on_grid(after.x, step) && on_grid(after.y, step),
        "{after:?} on {step}"
    );
    assert!((after - before).length() > 0.5, "moved");
    assert!(h.state().doc.can_undo(), "one undo step");
    // Alt: a fine move without snapping.
    h.state_mut().doc.undo();
    h.run_steps(2);
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -20.0), Modifiers::ALT);
    let fine = outline_knot(&h, 2).y - before.y;
    let expect = 2.0 / h.state().ed.outline_view.xf.scale as f64;
    assert!(
        (fine - expect).abs() < expect * 0.3,
        "fine {fine} expected {expect}"
    );
}

#[test]
fn dragged_knot_snaps_to_a_guide_point_and_shift_keeps_the_axis() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let k = outline_knot(&h, 2);
    let guide = k + DVec2::new(4.0, 1.0);
    h.state_mut().doc.board.outline_guides.push(guide);
    h.state_mut().set.snap = SnapSettings {
        enabled: true,
        guides: true,
        ..Default::default()
    };
    h.run_steps(2);
    let from = outline_screen(&h, 2, Handle::Point);
    let xf = h.state().ed.outline_view.xf;
    // Release 4 px off the guide: caught.
    let to = xf.to_screen(guide) + egui::vec2(3.0, -2.5);
    drag(&mut h, from, to, Modifiers::NONE);
    assert!((outline_knot(&h, 2) - guide).length() < 1e-9);
    h.state_mut().doc.undo();
    // Snapping off: Shift still keeps a mostly horizontal drag on the start y.
    h.state_mut().set.snap.enabled = false;
    h.run_steps(2);
    let from = outline_screen(&h, 2, Handle::Point);
    drag(
        &mut h,
        from,
        from + egui::vec2(40.0, -9.0),
        Modifiers::SHIFT,
    );
    let after = outline_knot(&h, 2);
    assert!((after.y - k.y).abs() < 1e-9, "y kept: {after:?} {k:?}");
    assert!(after.x > k.x + 1.0, "x moved");
}

#[test]
fn add_guide_snaps_and_the_status_bar_says_so() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    h.state_mut().set.snap = SnapSettings {
        enabled: true,
        knots: true,
        ..Default::default()
    };
    h.state_mut().ed.tool = Tool::AddGuide;
    h.run_steps(2);
    let k = outline_knot(&h, 3);
    let at = outline_screen(&h, 3, Handle::Point) + egui::vec2(4.0, 3.0);
    h.hover_at(at);
    h.run_steps(3);
    h.get_by_label("Snap: control point");
    snap(&mut h, "snap-hover-knot");
    click_pos(&mut h, at);
    let g = h.state().doc.board.outline_guides.clone();
    assert_eq!(g.len(), 1);
    assert!((g[0] - k).length() < 1e-9, "{g:?} {k:?}");
}

#[test]
fn snap_menu_and_toolbar_toggle() {
    let mut h = harness();
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    click(&mut h, "Snap");
    let s = h.state().set.snap;
    assert!(s.enabled && s.knots && s.grid, "{s:?}");
    click(&mut h, "Snap");
    assert!(!h.state().set.snap.enabled);
    click(&mut h, "View");
    // The submenu entry, below the toolbar's "Snap".
    let item = h
        .query_all_by_label("Snap")
        .map(|n| n.rect())
        .max_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    click_pos(&mut h, item.center());
    snap(&mut h, "snap-menu");
    for l in ["Snapping on", "Guide points", "Axis (start x or y)"] {
        assert!(h.query_all_by_label(l).next().is_some(), "{l}");
    }
}
