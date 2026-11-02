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

#[test]
fn board_from_measurements_fills_a_table_and_creates_a_board() {
    let mut h = harness_cm();
    click(&mut h, "File");
    click(&mut h, "New from measurements…");
    assert!(h.state().dialogs.measure.is_some());
    let title = "Board from measurements";
    // Create with an empty table explains what is missing.
    click(&mut h, "Create");
    assert!(
        h.query_all_by_value("Enter the board length.")
            .next()
            .is_some()
    );
    let f = fields(&h, title);
    type_into(&mut h, f[0], "190");
    // Rows (4 fields each): tail 0, tail 12", tail 24", centre, nose 24", nose 12", nose 0.
    let cell = |h: &H, row: usize, col: usize| fields(h, title)[1 + 4 * row + col];
    for (row, col, text) in [
        (0, 2, "6"),
        (1, 1, "37"),
        (1, 3, "5.2"),
        (3, 1, "52"),
        (3, 2, "0"),
        (3, 3, "6.6"),
        (5, 1, "38"),
        (5, 3, "5.4"),
        (6, 2, "13"),
    ] {
        let r = cell(&h, row, col);
        type_into(&mut h, r, text);
    }
    snap(&mut h, "numbers-measurements");
    click(&mut h, "Create");
    settle(&mut h);
    let app = h.state();
    assert!(app.dialogs.measure.is_none(), "window closes");
    let b = &app.doc.board;
    assert!((b.length() - 190.0).abs() < 1e-6);
    assert!(
        (b.width_at(95.0) - 52.0).abs() < 0.1,
        "{}",
        b.width_at(95.0)
    );
    assert!((b.width_at(30.48) - 37.0).abs() < 0.1);
    assert!((b.rocker_at(190.0 - 0.005) - 13.0).abs() < 0.1);
    assert!((b.thickness_at(95.0) - 6.6).abs() < 0.1);
    assert!(app.doc.path.is_none() && !app.doc.is_dirty());
}

#[test]
fn measurements_csv_save_and_import() {
    let mut h = harness_cm();
    click(&mut h, "File");
    click(&mut h, "New from measurements…");
    let title = "Board from measurements";
    let f = fields(&h, title);
    type_into(&mut h, f[0], "6'");
    let dir = temp_dir("numbers-csv");
    let p = dir.join("table.csv");
    let _ = std::fs::remove_file(&p);
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(p.clone());
    click(&mut h, "Save CSV…");
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(
        text.contains("length,6'") && text.contains("tail 12"),
        "{text}"
    );
    std::fs::write(
        &p,
        "length,7'6\"\ntail,swallow\nposition,width\ncentre,21\"\n",
    )
    .unwrap();
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(p.clone());
    click(&mut h, "Import CSV…");
    let st = h.state().dialogs.measure.as_ref().unwrap();
    assert_eq!(st.table.length, "7'6\"");
    assert_eq!(st.table.rows.len(), 1);
    assert_eq!(st.table.rows[0].width, "21\"");
}

#[test]
fn measurements_window_applies_bottom_and_rail_choices() {
    use surfcraft_session::measurements;
    use surfcraft_session::shaping::ContourKind;
    let mut h = harness_cm();
    click(&mut h, "File");
    click(&mut h, "New from measurements…");
    let table = measurements::sample(
        &surfcraft_io::templates::TEMPLATES[0].board(),
        30.48,
        surfcraft_core::LengthUnit::Centimetres,
        16,
    );
    h.state_mut().dialogs.measure.as_mut().unwrap().table = table.clone();
    h.run_steps(2);
    // Bottom combo (the upper "As the template"), then the rail combo.
    let combos = |h: &H| -> Vec<egui::Rect> {
        let mut r: Vec<egui::Rect> = h
            .query_all_by(|n| {
                n.label().as_deref() == Some("As the template")
                    || n.value().as_deref() == Some("As the template")
            })
            .map(|n| n.rect())
            .collect();
        r.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        r
    };
    let bottom = combos(&h)[0];
    click_rect(&mut h, bottom);
    click(&mut h, "Single concave");
    let depth = field_on_row(&h, "Board from measurements", "Depth");
    type_into(&mut h, depth, "0.5cm");
    let rail = combos(&h)[0];
    click_rect(&mut h, rail);
    click(&mut h, "60/40");
    snap(&mut h, "numbers-measurements-shaping");
    let t = h.state().dialogs.measure.as_ref().unwrap().table.clone();
    assert_eq!(t.bottom, Some(ContourKind::SingleConcave));
    assert_eq!(t.bottom_depth, "0.5cm");
    assert_eq!(t.rail, Some(1));
    click(&mut h, "Create");
    settle(&mut h);
    let expected = measurements::build(
        &measurements::parse(&t, surfcraft_core::LengthUnit::Centimetres).unwrap(),
    )
    .unwrap()
    .0;
    let b = &h.state().doc.board;
    assert_eq!(b.sections, expected.sections);
    let flat = measurements::build(
        &measurements::parse(&table, surfcraft_core::LengthUnit::Centimetres).unwrap(),
    )
    .unwrap()
    .0;
    assert!(b.volume() < flat.volume());
}

fn click_rect(h: &mut H, rect: egui::Rect) {
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
