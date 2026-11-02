//! GUI tests of Fit in blank: open the window from the Board menu, pick a library blank, read the result, auto fit,
//! undo, a user blank through the editor, and the overlays in the outline and rocker views. Screenshots go to
//! `target/gui-shots/blank-*.png`.

use eframe::egui::{self, Modifiers};
use egui_kittest::kittest::Queryable as _;

use crate::app::Tab;
use crate::gui_tests::*;

/// Click the widget with this label (the last exact match, else the first label starting with it).
fn click(h: &mut H, label: &str) {
    let exact: Vec<egui::Rect> = h.query_all_by_label(label).map(|n| n.rect()).collect();
    let rect = exact.last().copied().unwrap_or_else(|| {
        h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(label)))
            .next()
            .unwrap_or_else(|| panic!("no widget labelled {label:?}"))
            .rect()
    });
    let pos = rect.center();
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

fn has_text(h: &H, text: &str) -> bool {
    h.query_all_by(|n| {
        n.label().is_some_and(|l| l.contains(text)) || n.value().is_some_and(|v| v.contains(text))
    })
    .next()
    .is_some()
}

fn open_window(h: &mut H) {
    click(h, "Board");
    click(h, "Fit in blank…");
    assert!(h.state().dialogs.blank.open, "window open");
}

/// Open the blank picker (the combo box on the "Blank" row) and pick `name`.
fn pick(h: &mut H, name: &str) {
    let combo = h
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .map(|n| n.rect())
        .next_back()
        .expect("blank picker");
    let pos = combo.center();
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
    click(h, name);
}

#[test]
fn fit_in_blank_window_picks_a_blank_and_reports() {
    let mut h = harness();
    open_window(&mut h);
    assert!(has_text(&h, "Choose a blank to check"));
    assert!(!h.state().doc.can_undo());
    // The 6' shortboard template in a 6'5" R blank: too big centred, fits after Auto fit.
    pick(&mut h, "6'5\" shortboard (US Blanks 6'5\" R)");
    let s = h.state().doc.board.blank.clone().expect("blank chosen");
    assert_eq!(s.blank.name, "6'5\" shortboard (US Blanks 6'5\" R)");
    assert!(h.state().doc.can_undo(), "choosing is an undoable edit");
    assert!(
        has_text(&h, "Too big"),
        "centred, the template does not fit"
    );
    assert!(has_text(&h, "sticks out by"));
    snap(&mut h, "blank-too-big");
    click(&mut h, "Auto fit");
    let p = h.state().doc.board.blank.as_ref().unwrap().placement;
    assert!(p.tail_offset.is_some() && p.tilt != 0.0, "{p:?}");
    assert!(has_text(&h, "Fits"), "fits after auto fit");
    assert!(!has_text(&h, "Too big"));
    // The overlays draw in both views.
    for tab in [Tab::Outline, Tab::Profile, Tab::Overview] {
        set_tab(&mut h, tab);
        h.run_steps(2);
    }
    snap(&mut h, "blank-fits-overview");
    // Undo the auto fit, then the choice.
    h.state_mut().doc.undo();
    h.run_steps(3);
    assert!(has_text(&h, "Too big"));
    h.state_mut().doc.undo();
    h.run_steps(3);
    assert!(h.state().doc.board.blank.is_none());
}

/// The fields of the blank editor are wide enough to read the values (regression: the window opened at egui's
/// default width and squeezed every field to a few characters).
#[test]
fn blank_editor_fields_are_readable() {
    use egui_kittest::kittest::NodeT as _;
    let mut h = harness();
    open_window(&mut h);
    click(&mut h, "New blank…");
    h.run_steps(3);
    let w = h
        .get_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Blank dimensions"))
        .expect("editor window")
        .rect();
    let fields: Vec<egui::Rect> = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .filter(|r| w.contains(r.center()))
        .collect();
    assert_eq!(fields.len(), 13, "name, 6 main values, 6 stations");
    for r in &fields {
        assert!(r.width() >= 110.0, "field {r:?} squeezed in {w:?}");
        assert!(r.max.x <= w.max.x, "field {r:?} cut off by {w:?}");
    }
}

#[test]
fn new_blank_is_saved_and_used() {
    let mut h = harness();
    h.state_mut().set.blanks.user.clear();
    open_window(&mut h);
    click(&mut h, "New blank…");
    assert!(has_text(&h, "Blank dimensions"));
    click(&mut h, "Save and use");
    let s = h.state().doc.board.blank.clone().expect("new blank used");
    // Starts from the board's size plus a little foam; the result is shown.
    assert!(s.blank.length > h.state().doc.board.length());
    assert!(has_text(&h, "Fits") || has_text(&h, "Too big"));
    assert_eq!(h.state().set.blanks.user_blanks().len(), 1);
    assert!(has_text(&h, "Blank for Shortboard"));
    snap(&mut h, "blank-user");
    // Delete it from the user blanks; the board keeps it.
    click(&mut h, "Delete");
    assert!(h.state().set.blanks.user.is_empty());
    assert!(h.state().doc.board.blank.is_some());
    click(&mut h, "Remove from board");
    assert!(h.state().doc.board.blank.is_none());
}
