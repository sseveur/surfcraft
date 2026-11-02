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

/// Some label or text on screen matches.
fn shows(h: &H, f: impl Fn(&str) -> bool) -> bool {
    h.query_all_by(|n| n.label().is_some_and(|l| f(&l)) || n.value().is_some_and(|v| f(&v)))
        .next()
        .is_some()
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

// ------------------------------------------------------------------------------------------------ measure

#[test]
fn measure_between_two_knots_with_knot_snapping() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    h.key_press(egui::Key::M);
    h.run_steps(2);
    assert_eq!(h.state().ed.tool, Tool::Measure, "M picks the Measure tool");
    h.state_mut().set.snap = SnapSettings {
        enabled: true,
        knots: true,
        ..Default::default()
    };
    h.run_steps(2);
    let (a, b) = (outline_knot(&h, 1), outline_knot(&h, 2));
    let from = outline_screen(&h, 1, Handle::Point) + egui::vec2(3.0, 2.0);
    let to = outline_screen(&h, 2, Handle::Point) + egui::vec2(-2.0, 4.0);
    drag(&mut h, from, to, Modifiers::NONE);
    h.run_steps(2);
    let m = h.state().ed.measure.last.expect("measured");
    assert_eq!(m.pane, crate::snap::Pane::Outline);
    assert!(
        (m.a - a).length() < 1e-9 && (m.b - b).length() < 1e-9,
        "{m:?}"
    );
    let set = &h.state().set;
    let label = format!("Distance {}", set.fmt(a.distance(b)));
    assert!(m.label(set).starts_with(&label));
    assert!(!h.state().doc.can_undo(), "not an edit");
    // Shown in the status bar.
    h.hover_at(egui::pos2(700.0, 880.0));
    h.run_steps(2);
    assert!(
        shows(&h, |t| t.starts_with(&label)),
        "status bar shows {label}"
    );
    snap(&mut h, "measure-outline");
    // Pin keeps it, Esc clears the latest only, Clear measurements clears all.
    click(&mut h, "Pin");
    assert_eq!(h.state().ed.measure.pinned.len(), 1);
    drag(&mut h, from, to + egui::vec2(0.0, 60.0), Modifiers::NONE);
    assert!(h.state().ed.measure.last.is_some());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().ed.measure.last.is_none(), "Esc clears");
    assert_eq!(h.state().ed.tool, Tool::Select);
    assert_eq!(h.state().ed.measure.pinned.len(), 1, "pinned stays");
    click(&mut h, "Edit");
    click(&mut h, "Clear measurements");
    assert!(h.state().ed.measure.pinned.is_empty());
}

#[test]
fn measure_click_click_with_shift_in_the_overview_profile() {
    let mut h = harness();
    h.state_mut().tab = Tab::Overview;
    h.state_mut().ed.tool = Tool::Measure;
    h.run_steps(3);
    let xf = h.state().ed.profile_view.xf;
    let a = xf.to_screen(DVec2::new(40.0, 3.0));
    let b = a + egui::vec2(150.0, -12.0);
    click_pos(&mut h, a);
    assert!(h.state().ed.measure.last.is_none(), "first click starts");
    h.hover_at(b);
    h.run_steps(2);
    h.event_modifiers(
        egui::Event::PointerButton {
            pos: b,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    h.event_modifiers(
        egui::Event::PointerButton {
            pos: b,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    h.run_steps(3);
    let m = h.state().ed.measure.last.expect("measured");
    assert_eq!(m.pane, crate::snap::Pane::Profile);
    assert!((m.b.y - m.a.y).abs() < 1e-9, "Shift: horizontal {m:?}");
    assert_eq!(m.angle(), 0.0);
    let expect = 150.0 / xf.scale as f64;
    assert!((m.distance() - expect).abs() < 0.05, "{m:?} {expect}");
    snap(&mut h, "measure-overview-profile");
    // Another tool drops it.
    h.state_mut().ed.tool = Tool::Select;
    h.run_steps(3);
    assert!(h.state().ed.measure.last.is_none());
}

#[test]
fn measure_hover_reads_the_width() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    h.state_mut().set.unit = crate::settings::Unit::Centimetres;
    h.state_mut().ed.tool = Tool::Measure;
    h.run_steps(2);
    let x = 60.0;
    let w = h.state().doc.board.width_at(x);
    let at = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(x, -w / 2.0));
    h.hover_at(at);
    h.run_steps(2);
    h.hover_at(at);
    h.run_steps(2);
    let set = &h.state().set;
    let needle = format!("width {}", set.fmt(w));
    assert!(
        shows(&h, |t| t.contains(&needle)),
        "read-out shows {needle}"
    );
    snap(&mut h, "measure-hover-outline");
    // Cross-section: height and thickness at a distance from the stringer.
    set_tab(&mut h, Tab::Section);
    let i = h.state().ed.current_section;
    let deck = h.state().doc.board.sections[i].deck_at(10.0);
    let at = h
        .state()
        .ed
        .section_view
        .xf
        .to_screen(DVec2::new(-10.0, deck));
    h.hover_at(at);
    h.run_steps(2);
    h.hover_at(at);
    h.run_steps(2);
    let needle = format!(
        "{} from stringer: deck {}",
        h.state().set.fmt(10.0),
        h.state().set.fmt(deck)
    );
    assert!(shows(&h, |t| t.starts_with(&needle)), "{needle}");
    snap(&mut h, "measure-hover-section");
}
