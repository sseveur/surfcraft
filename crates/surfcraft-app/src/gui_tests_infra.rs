//! GUI tests for logging, bug reports and autosave/recovery. Every test uses its own temporary data folder, never
//! the user's.

use std::path::PathBuf;

use eframe::egui::{self, Modifiers};
use egui_kittest::kittest::Queryable as _;

use crate::gui_tests::*;

fn step(h: &mut H) {
    h.run_steps(3);
}

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
    h.run_steps(2);
}

fn menu(h: &mut H, top: &str, item: &str) {
    click(h, top);
    click(h, item);
    step(h);
}

/// A fresh, empty data folder for one test.
fn data_dir(name: &str) -> PathBuf {
    let d = temp_dir(&format!("infra-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn harness_with_data(name: &str) -> (H, PathBuf) {
    let mut h = harness();
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    let dir = data_dir(name);
    h.state_mut().data_dir = Some(dir.clone());
    step(&mut h);
    (h, dir)
}

#[test]
fn help_menu_detailed_logging_and_log_folder() {
    let (mut h, dir) = harness_with_data("log-menu");
    crate::platform::take_opened();
    click(&mut h, "Help");
    snap(&mut h, "qa-help-menu-logging");
    assert!(!h.state().set.detailed_logging, "off by default");
    click(&mut h, "Detailed logging");
    step(&mut h);
    assert!(h.state().set.detailed_logging);
    assert!(crate::logging::detailed());
    menu(&mut h, "Help", "Open log folder");
    let logs = crate::logging::log_dir(&dir);
    assert!(logs.is_dir(), "the folder is created when missing");
    assert_eq!(
        crate::platform::take_opened(),
        [format!("open:{}", logs.display())]
    );
    // Back off: the setting drives the global level.
    h.state_mut().set.detailed_logging = false;
    step(&mut h);
    assert!(!crate::logging::detailed());
}

// ------------------------------------------------------------------------------------------------ bug report

/// Log lines `age` seconds old, each in the file of its own day.
fn plant_logs(dir: &std::path::Path, lines: &[(i64, &str)]) {
    use crate::logging::{file_name, format_line, log_dir, now};
    let logs = log_dir(dir);
    std::fs::create_dir_all(&logs).unwrap();
    let mut sorted = lines.to_vec();
    sorted.sort_by_key(|(age, _)| -age);
    for (age, msg) in sorted {
        let t = now().0 - age;
        let path = logs.join(file_name(t.div_euclid(86_400)));
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text += &format_line(t, 0, log::Level::Info, "surfcraft", msg);
        std::fs::write(&path, text).unwrap();
    }
}

/// Open the period picker of the bug report (the combo box on the row of its label) and pick `choice`.
fn pick_since(h: &mut H, choice: &str) {
    let label = h
        .query_all_by_value("Include logs from the last:")
        .next()
        .expect("period label")
        .rect();
    let combo = h
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .map(|n| n.rect())
        .find(|r| (r.center().y - label.center().y).abs() < 10.0 && r.min.x > label.min.x)
        .expect("period picker");
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
    h.run_steps(2);
    click(h, choice);
    step(h);
}

fn has_text(h: &H, text: &str) -> bool {
    h.query_all_by(|n| {
        n.label().is_some_and(|l| l.contains(text)) || n.value().is_some_and(|v| v.contains(text))
    })
    .next()
    .is_some()
}

#[test]
fn report_a_bug_window_filters_logs_and_saves_a_zip() {
    use crate::bugreport::Since;
    let (mut h, dir) = harness_with_data("bug-report");
    plant_logs(
        &dir,
        &[
            (2 * 86_400, "two days ago"),
            (3 * 3600, "three hours ago"),
            (1800, "half an hour ago"),
        ],
    );
    crate::platform::take_opened();
    menu(&mut h, "Help", "Report a bug…");
    step(&mut h);
    assert!(h.state().bug_report.as_ref().is_some_and(|b| b.open));
    assert_eq!(h.state().bug_report.as_ref().unwrap().since, Since::Day);
    assert!(has_text(&h, "2 lines,"), "24 hours by default");
    // The preview shows the log text.
    click(&mut h, "Preview");
    step(&mut h);
    assert!(has_text(&h, "three hours ago") && !has_text(&h, "two days ago"));
    snap(&mut h, "qa-bug-report");
    // Shorter and longer periods.
    pick_since(&mut h, "1 hour");
    assert_eq!(h.state().bug_report.as_ref().unwrap().since, Since::Hour);
    assert!(has_text(&h, "1 line,"));
    assert!(has_text(&h, "half an hour ago") && !has_text(&h, "three hours ago"));
    pick_since(&mut h, "All");
    assert!(has_text(&h, "3 lines,") && has_text(&h, "two days ago"));

    // Send as a GitHub issue: the zip goes to the chosen folder, the browser gets the URL.
    let out = dir.join("out");
    {
        let b = h.state_mut().bug_report.as_mut().unwrap();
        b.out_dir = Some(out.clone());
        b.title = "Flip & crash".into();
        b.description = "Board > Flip, then the view went blank.".into();
        b.include_board = true;
    }
    h.state_mut()
        .doc
        .edit(|b| b.info.name = "Secret shape".into());
    click(&mut h, "GitHub issue");
    step(&mut h);
    let zip = h
        .state()
        .bug_report
        .as_ref()
        .unwrap()
        .saved
        .clone()
        .expect("zip saved");
    assert_eq!(zip.parent(), Some(out.as_path()));
    let name = zip.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("SurfCraft-bug-report-") && name.ends_with(".zip"));
    let bytes = std::fs::read(&zip).unwrap();
    let contains = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
    assert!(contains("report.txt") && contains("system-info.txt") && contains("board.brd"));
    assert!(
        contains("Secret shape"),
        "the in-memory board, with unsaved changes"
    );
    assert!(contains("two days ago"), "logs of the chosen period");
    let opened = crate::platform::take_opened();
    assert_eq!(opened[0], format!("reveal:{}", zip.display()));
    assert!(opened[1].starts_with(
        "url:https://github.com/sseveur/surfcraft/issues/new?title=Flip%20%26%20crash&body="
    ));
    assert!(opened[1].contains(&crate::bugreport::encode(&name)));
    assert!(
        !opened[1].contains("two%20days%20ago"),
        "logs never go into the URL"
    );
    assert!(has_text(&h, "Report saved to"));
    snap(&mut h, "qa-bug-report-sent");

    // E-mail.
    click(&mut h, "E-mail");
    step(&mut h);
    let opened = crate::platform::take_opened();
    assert!(opened[1].starts_with("url:mailto:steven.seveur@gmail.com?subject="));

    // Copy report text, then close.
    click(&mut h, "Copy report text");
    step(&mut h);
    assert!(
        h.state()
            .status
            .as_ref()
            .is_some_and(|s| s.0.contains("copied"))
    );
    h.state_mut().bug_report.as_mut().unwrap().open = false;
    step(&mut h);
    assert!(!has_text(&h, "Include logs from the last:"));
}
