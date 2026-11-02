//! QA of the Shape menu windows (bottom contours, rails), driven through their widgets with egui_kittest.
//! Screenshots go to `target/gui-shots/shape-*.png`.

use eframe::egui::{self, Modifiers, accesskit::Role};
use egui_kittest::kittest::Queryable as _;
use surfcraft_session::shaping::{ContourKind, Keep};

use crate::app::Tab;
use crate::gui_tests::{H, harness, set_tab, settle, snap};
use crate::settings::Unit;

fn click(h: &mut H, label: &str) {
    h.get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no widget {label:?}"))
        .click_accesskit();
    h.run_steps(3);
}

fn open(h: &mut H, item: &str) {
    h.state_mut().set.unit = Unit::Centimetres;
    h.run_steps(1);
    click(h, "Shape");
    click(h, item);
}

/// Type into the length field on the same row as the first `label`.
fn set_length(h: &mut H, label: &str, text: &str) {
    let y = h
        .get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no label {label:?}"))
        .rect()
        .center()
        .y;
    {
        let node = h
            .get_all_by_role(Role::TextInput)
            .find(|n| (n.rect().center().y - y).abs() < 8.0)
            .unwrap_or_else(|| panic!("no text field next to {label:?}"));
        node.focus();
    }
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    {
        let node = h
            .get_all_by_role(Role::TextInput)
            .find(|n| n.is_focused())
            .expect("a focused text field");
        node.type_text(text);
    }
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

fn undo_depth(h: &mut H) -> usize {
    let mut n = 0;
    while h.state().doc.can_undo() {
        h.state_mut().doc.undo();
        n += 1;
    }
    for _ in 0..n {
        h.state_mut().doc.redo();
    }
    n
}

#[test]
fn shape_menu_lists_both_tools() {
    let mut h = harness();
    click(&mut h, "Shape");
    assert!(h.query_by_label("Bottom contours…").is_some());
    assert!(h.query_by_label("Rails…").is_some());
}

#[test]
fn contours_window_change_type_preview_apply_undo() {
    let mut h = harness();
    set_tab(&mut h, Tab::Section);
    open(&mut h, "Bottom contours…");
    assert!(h.state().dialogs.shaping.contours_open);
    let before = h.state().doc.board.clone();
    // Change the type through the combo box: single concave > vee.
    let y = h
        .get_all_by_label("Type")
        .last()
        .expect("Type label")
        .rect()
        .center()
        .y;
    let combos: Vec<_> = h
        .get_all_by_role(Role::ComboBox)
        .map(|n| n.rect())
        .collect();
    let pos = combos
        .iter()
        .find(|r| (r.center().y - y).abs() < 8.0)
        .unwrap_or_else(|| panic!("no combo box at {y}: {combos:?}"))
        .center();
    h.get_all_by_role(Role::ComboBox)
        .find(|n| n.rect().center() == pos)
        .unwrap()
        .click();
    h.run_steps(3);
    click(&mut h, "Vee");
    assert_eq!(h.state().set.shaping.zones[0].kind, ContourKind::Vee);
    assert_eq!(
        h.state().set.shaping.zones[0].keep,
        Keep::Rails,
        "cutting foam is the default"
    );
    assert!(h.query_all_by_value("Keep").next().is_some());
    set_length(&mut h, "Depth", "0.5");
    assert!((h.state().set.shaping.zones[0].depth - 0.5).abs() < 1e-9);
    set_length(&mut h, "End (from tail)", "60");
    assert!((h.state().set.shaping.zones[0].end - 60.0).abs() < 1e-9);
    click(&mut h, "Preview");
    assert!(
        h.state().ed.shaping_preview.is_some(),
        "{:?}",
        h.state().dialogs.shaping.contours_open
    );
    assert_eq!(
        h.state().doc.board,
        before,
        "preview leaves the board alone"
    );
    snap(&mut h, "shape-contours-preview");
    click(&mut h, "Apply");
    settle(&mut h);
    assert_ne!(h.state().doc.board, before);
    assert!(h.state().ed.shaping_preview.is_none());
    assert_eq!(undo_depth(&mut h), 1, "one undo step");
    snap(&mut h, "shape-contours-applied");
    h.state_mut().doc.undo();
    h.run_steps(2);
    assert_eq!(h.state().doc.board, before);
    // Remove contours flattens the bottom (one more undo step).
    click(&mut h, "Flatten the bottom");
    assert_ne!(h.state().doc.board, before);
    assert_eq!(undo_depth(&mut h), 1);
}

#[test]
fn contours_window_add_zone_channels() {
    let mut h = harness();
    open(&mut h, "Bottom contours…");
    click(&mut h, "Add zone");
    assert_eq!(h.state().set.shaping.zones.len(), 2);
    h.state_mut().set.shaping.zones[1].kind = ContourKind::Channels;
    h.run_steps(3);
    assert!(
        h.query_by_label("Channels per side").is_some()
            || h.query_all_by_value("Channels per side").next().is_some()
    );
    let before = h.state().doc.board.clone();
    click(&mut h, "Apply");
    assert_ne!(h.state().doc.board, before);
    assert_eq!(undo_depth(&mut h), 1);
    click(&mut h, "Remove zone");
    assert_eq!(h.state().set.shaping.zones.len(), 1);
}

#[test]
fn rails_window_preset_preview_apply_undo() {
    let mut h = harness();
    set_tab(&mut h, Tab::Section);
    open(&mut h, "Rails…");
    assert!(h.state().dialogs.shaping.rails_open);
    let before = h.state().doc.board.clone();
    click(&mut h, "70/30");
    assert!((h.state().set.shaping.rails.middle.apex - 0.3).abs() < 1e-9);
    click(&mut h, "Preview");
    assert!(h.state().ed.shaping_preview.is_some());
    snap(&mut h, "shape-rails-preview");
    click(&mut h, "Apply");
    assert_ne!(h.state().doc.board, before);
    assert_eq!(undo_depth(&mut h), 1);
    h.state_mut().doc.undo();
    h.run_steps(2);
    assert_eq!(h.state().doc.board, before);
    // Zoned rails: three columns of presets.
    click(&mut h, "Blend tail, middle and nose");
    assert!(h.state().set.shaping.rails.zoned);
    assert_eq!(h.get_all_by_label("80/20").count(), 3);
    click(&mut h, "Apply");
    assert_ne!(h.state().doc.board, before);
    snap(&mut h, "shape-rails-zoned");
}

#[test]
fn closing_a_window_drops_its_preview() {
    let mut h = harness();
    open(&mut h, "Rails…");
    click(&mut h, "Preview");
    assert!(h.state().ed.shaping_preview.is_some());
    h.state_mut().dialogs.shaping.rails_open = false;
    h.run_steps(2);
    assert!(h.state().ed.shaping_preview.is_none());
}
