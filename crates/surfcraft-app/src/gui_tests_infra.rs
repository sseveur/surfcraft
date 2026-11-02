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
