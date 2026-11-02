//! GUI tests of Board → Design by numbers and File → New from measurements, driven through their widgets.
//! Screenshots go to `target/gui-shots/numbers-*.png`.

use eframe::egui::{self, Modifiers, accesskit::Role};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

use crate::gui_tests::*;
use crate::settings::Unit;

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

/// Text fields of a window, top to bottom, left to right.
fn fields(h: &H, title: &str) -> Vec<egui::Rect> {
    let w = h
        .get_all_by_role(Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window {title:?}"));
    let mut r: Vec<egui::Rect> = w
        .get_all_by_role(Role::TextInput)
        .map(|n| n.rect())
        .collect();
    r.sort_by(|a, b| {
        (a.center().y.round(), a.center().x)
            .partial_cmp(&(b.center().y.round(), b.center().x))
            .unwrap()
    });
    r
}

/// The text field on the row of `label` in window `title`.
fn field_on_row(h: &H, title: &str, label: &str) -> egui::Rect {
    let y = h
        .get_all_by_role(Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window {title:?}"))
        .query_all_by_value(label)
        .next()
        .unwrap_or_else(|| panic!("no text {label:?}"))
        .rect()
        .center()
        .y;
    fields(h, title)
        .into_iter()
        .find(|r| (r.center().y - y).abs() < 8.0)
        .unwrap_or_else(|| panic!("no field next to {label:?}"))
}

/// Replace the text of the field at `rect` and press Enter.
fn type_into(h: &mut H, rect: egui::Rect, text: &str) {
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
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.to_owned()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

fn harness_cm() -> H {
    let mut h = harness();
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    h.state_mut().set.unit = Unit::Centimetres;
    h.run_steps(2);
    h
}

#[test]
fn design_by_numbers_edits_a_number_as_one_undo_step() {
    let mut h = harness_cm();
    let before = h.state().doc.board.clone();
    click(&mut h, "Board");
    click(&mut h, "Design by numbers…");
    assert!(h.state().dialogs.numbers.is_some());
    // Metric display also offers the widths 30 cm from each end.
    assert!(h.query_all_by_value("Tail width at 30 cm").next().is_some());
    let f = field_on_row(&h, "Design by numbers", "Tail width at 12\"");
    type_into(&mut h, f, "40cm");
    settle(&mut h);
    snap(&mut h, "numbers-window");
    let b = h.state().doc.board.clone();
    assert!(
        (b.width_at(30.48) - 40.0).abs() < 0.01,
        "{}",
        b.width_at(30.48)
    );
    assert!(
        (b.width_at(b.length() - 30.48) - before.width_at(before.length() - 30.48)).abs() < 1e-3
    );
    // The window shows the new number.
    assert!(
        h.query_all_by_value("40.00 cm").next().is_some(),
        "new value shown"
    );
    // A rocker change is a second step; undo takes back one at a time.
    let f = field_on_row(&h, "Design by numbers", "Nose rocker");
    type_into(&mut h, f, "14");
    assert!((h.state().doc.board.rocker_at(b.length() - 0.005) - 14.0).abs() < 0.01);
    h.state_mut().doc.undo();
    h.run_steps(2);
    assert_eq!(h.state().doc.board, b);
    h.state_mut().doc.undo();
    h.run_steps(2);
    assert_eq!(h.state().doc.board, before);
    // A refused value says why and changes nothing.
    let f = field_on_row(&h, "Design by numbers", "Max width");
    type_into(&mut h, f, "500cm");
    assert_eq!(h.state().doc.board, before);
    assert!(
        h.query_all_by_value("That value is out of range for this board.")
            .next()
            .is_some()
    );
}
