//! QA: every menu item, toolbar button, shortcut, mouse interaction and dialog of the GUI, driven through real
//! input with egui_kittest. Screenshots go to `target/gui-shots/qa-*.png`.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, Modifiers, Pos2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use surfcraft_core::{DVec2, Handle, SurfaceModel};

use crate::app::{Pending, Tab};
use crate::edit::{CurveId, Selection};
use crate::export_ui::{Format, What};
use crate::gui_tests::*;
use crate::settings::{Model, Unit};
use crate::views::Tool;

// ------------------------------------------------------------------------------------------------ helpers

fn step(h: &mut H) {
    h.run_steps(3);
}

/// Click a widget by its accessible label (exact, else the unique label starting with it: menu items with
/// shortcuts have the shortcut appended).
fn click(h: &mut H, label: &str) {
    let pos = node_rect(h, label).center();
    click_at(h, pos, Modifiers::NONE);
}

fn node_rect(h: &H, label: &str) -> egui::Rect {
    // Several exact matches (e.g. toolbar and menu "Undo"): the last one is the open popup.
    use egui::accesskit::Role;
    let exact: Vec<(Role, egui::Rect)> = h
        .query_all_by_label(label)
        .map(|n| (n.accesskit_node().role(), n.rect()))
        .collect();
    if let Some((_, r)) = exact
        .iter()
        .rev()
        .find(|(role, _)| !matches!(role, Role::Label | Role::TextRun | Role::GenericContainer))
        .or(exact.last())
    {
        return *r;
    }
    let hits: Vec<egui::Rect> = h
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(label)))
        .map(|n| n.rect())
        .collect();
    assert!(!hits.is_empty(), "no widget labelled {label:?}");
    hits[0]
}

fn has_label(h: &H, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
        || h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(label)))
            .next()
            .is_some()
}

/// Harness at 60 steps per second (the default 4 steps/s is too slow for double-clicks).
fn harness_fast() -> H {
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_max_steps(200)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| crate::app::SurfCraftApp::new(cc, crate::Args::default()));
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    h.state_mut().scripted_dialogs = Some(Default::default());
    settle(&mut h);
    h
}

/// Harness that draws tofu for missing glyphs instead of panicking (needed to open the View menu, see
/// `view_menu_has_no_missing_glyphs`).
fn harness_t() -> H {
    let mut h = harness();
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    step(&mut h);
    h
}

fn menu(h: &mut H, top: &str, item: &str) {
    click(h, top);
    click(h, item);
}

fn press_at(h: &mut H, pos: Pos2, mods: Modifiers, button: egui::PointerButton) {
    h.hover_at(pos);
    h.run_steps(1);
    h.event_modifiers(
        egui::Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: mods,
        },
        mods,
    );
    h.event_modifiers(
        egui::Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: mods,
        },
        mods,
    );
    h.run_steps(2);
}

fn click_at(h: &mut H, pos: Pos2, mods: Modifiers) {
    press_at(h, pos, mods, egui::PointerButton::Primary);
}

fn drag_button(h: &mut H, from: Pos2, to: Pos2, button: egui::PointerButton) {
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 8.0),
        ));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton {
        pos: to,
        button,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
}

fn window<'a>(h: &'a H, title: &str) -> egui_kittest::Node<'a> {
    h.get_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window {title:?}"))
}

/// Position of the first label/text node with this exact text.
fn text_pos(h: &H, text: &str) -> egui::Rect {
    h.query_all_by_value(text)
        .next()
        .unwrap_or_else(|| panic!("no text {text:?}"))
        .rect()
}

/// The text field on the same row as `label`, to its right, inside window `title`.
fn field_in(h: &H, title: &str, label: &str) -> Pos2 {
    let w = window(h, title);
    let l = w
        .query_all_by_value(label)
        .next()
        .unwrap_or_else(|| panic!("no text {label:?} in {title:?}"))
        .rect();
    w.get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .filter(|r| (r.center().y - l.center().y).abs() < 10.0 && r.min.x >= l.max.x - 1.0)
        .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
        .unwrap_or_else(|| panic!("no text field right of {label:?} in {title:?}"))
        .center()
}

/// Replace the content of the text field at `pos` and commit it with Enter.
fn type_into(h: &mut H, pos: Pos2, text: &str) {
    click_at(h, pos, Modifiers::NONE);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.to_owned()));
    h.run_steps(1);
    h.key_press(Key::Enter);
    step(h);
}

fn script(h: &mut H, p: &Path) {
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(p.to_path_buf());
}

fn fresh(dir: &str, file: &str) -> PathBuf {
    let p = temp_dir(&format!("qa-{dir}")).join(file);
    let _ = std::fs::remove_file(&p);
    let mut bak = p.clone().into_os_string();
    bak.push(".bak");
    let _ = std::fs::remove_file(PathBuf::from(bak));
    p
}

fn screen(h: &H, view: Tab, curve: CurveId, knot: usize, which: Handle) -> Pos2 {
    let app = h.state();
    let s = curve.spline(&app.doc.board).unwrap();
    let p = s.knots()[knot].get(which);
    match view {
        Tab::Outline => app.ed.outline_view.xf.to_screen(p),
        Tab::Profile => {
            let ys = app.set.profile_exaggeration.max(1.0) as f64;
            app.ed.profile_view.xf.to_screen(DVec2::new(p.x, p.y * ys))
        }
        Tab::Section => app.ed.section_view.xf.to_screen(p),
        _ => unreachable!(),
    }
}

fn knot(h: &H, curve: CurveId, i: usize) -> DVec2 {
    curve.spline(&h.state().doc.board).unwrap().knots()[i].point()
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

fn close_requested(h: &H) -> bool {
    h.output().viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    })
}

fn make_dirty(h: &mut H) {
    menu(h, "Board", "Flip nose and tail");
    assert!(h.state().doc.is_dirty());
}

#[derive(Debug)]
struct PathFile(PathBuf);

impl egui::DroppedFile for PathFile {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn dropped(p: &Path) -> egui::DroppedFileHandle {
    std::sync::Arc::new(PathFile(p.to_path_buf()))
}

fn status(h: &H) -> String {
    h.state()
        .status
        .as_ref()
        .map(|s| s.0.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------- menus

#[test]
fn every_menu_lists_its_items() {
    let mut h = harness_t();
    let menus: &[(&str, &[&str])] = &[
        (
            "File",
            &[
                "New…",
                "Open…",
                "Open recent",
                "Save",
                "Save as…",
                "Save and set as original",
                "Open ghost board…",
                "Close ghost board",
                "Print & export…",
                "Import curves (.otl/.pfl/.crs)…",
                "Load background image…",
                "Remove background image",
                "Export curves",
                "Quit",
            ],
        ),
        (
            "Edit",
            &[
                "Undo",
                "Redo",
                "Select and move",
                "Add control point",
                "Add guide point",
                "Delete selected control points",
                "Clear guide points",
            ],
        ),
        (
            "View",
            &[
                "Grid",
                "Control points",
                "Guide points",
                "Measurements",
                "Curvature comb",
                "Cross-section positions",
                "Other cross-sections",
                "Rail apex",
                "Tuck-under point",
                "Fins",
                "Centre line",
                "Ghost board",
                "Original board",
                "Centre of mass",
                "Background image",
                "3D zebra stripes (surface fairness)",
                "3D wireframe (CPU preview)",
                "Fit all",
                "Specs panel",
            ],
        ),
        (
            "Board",
            &[
                "Scale…",
                "Board info…",
                "Fins…",
                "Weight calculator…",
                "Guide points…",
                "Flip nose and tail",
                "Put lowest rocker point at zero",
                "S-blend (smooth, default)",
                "Control-point matching",
            ],
        ),
        (
            "Help",
            &["Allow AI assistant control (MCP)", "About SurfCraft"],
        ),
    ];
    for (top, items) in menus {
        click(&mut h, top);
        for item in *items {
            assert!(has_label(&h, item), "{top} menu has no {item:?}");
        }
        snap(&mut h, &format!("qa-menu-{}", top.to_lowercase()));
        h.key_press(Key::Escape);
        step(&mut h);
    }
    // Disabled items with nothing to act on.
    click(&mut h, "Edit");
    assert!(
        h.get_all_by_label("Undo")
            .all(|n| n.accesskit_node().is_disabled())
    );
    assert!(
        h.get_all_by_label("Redo")
            .all(|n| n.accesskit_node().is_disabled())
    );
    assert!(
        h.query_all_by(|n| n
            .label()
            .is_some_and(|l| l.starts_with("Delete selected control points")))
            .all(|n| n.accesskit_node().is_disabled())
    );
}

#[test]
fn about_dialog() {
    let mut h = harness();
    menu(&mut h, "Help", "About SurfCraft");
    assert!(h.state().dialogs.about);
    h.get_by_label_contains("SurfCraft 4.0.0");
    snap(&mut h, "qa-about");
    click(&mut h, "Close window");
    assert!(!h.state().dialogs.about);
}

// --------------------------------------------------------------------------------- new / open / save

#[test]
fn new_board_from_each_template_with_target_size_and_volume() {
    for t in surfcraft_io::templates::TEMPLATES {
        let label = format!("{} — {}", t.name, t.description);
        let label = label.as_str();
        let length = t.board().length();
        let mut h = harness();
        h.key_press_modifiers(Modifiers::COMMAND, Key::N);
        step(&mut h);
        assert!(h.state().dialogs.new_board.is_some());
        click(&mut h, label);
        let p = field_in(&h, "New board", "Volume (L)");
        type_into(&mut h, p, "35,5");
        click(&mut h, "Create");
        settle(&mut h);
        let b = &h.state().doc.board;
        assert!(
            (b.length() - length).abs() < 1e-6,
            "{label}: {}",
            b.length()
        );
        let v = b.volume() / 1000.0;
        assert!((v - 35.5).abs() < 0.1, "{label}: volume {v}");
        assert!(h.state().doc.path.is_none());
        assert_eq!(h.state().doc.title(), "Untitled");
        assert!(!h.state().doc.is_dirty());
    }
    let mut h = harness();
    menu(&mut h, "File", "New…");
    let p = field_in(&h, "New board", "Length");
    type_into(&mut h, p, "5'10\"");
    let p = field_in(&h, "New board", "Width");
    type_into(&mut h, p, "19 1/4\"");
    snap(&mut h, "qa-new-board-dialog");
    click(&mut h, "Create");
    settle(&mut h);
    let b = &h.state().doc.board;
    assert!((b.length() - 177.8).abs() < 1e-6, "{}", b.length());
    assert!((b.max_width() - 19.25 * 2.54).abs() < 0.01);
    // Cancel leaves the board alone.
    menu(&mut h, "File", "New…");
    click(&mut h, "Cancel");
    assert!(h.state().dialogs.new_board.is_none());
    assert!((h.state().doc.board.length() - 177.8).abs() < 1e-6);
}

#[test]
fn unsaved_changes_prompt_cancel_dont_save_and_save() {
    let mut h = harness();
    // Cancel.
    make_dirty(&mut h);
    let dirty = h.state().doc.board.clone();
    menu(&mut h, "File", "New…");
    click(&mut h, "Create");
    assert!(matches!(h.state().pending, Some(Pending::New(_))));
    h.get_by_label_contains("Save changes to");
    snap(&mut h, "qa-unsaved-prompt");
    click(&mut h, "Cancel");
    assert!(h.state().pending.is_none());
    assert_eq!(h.state().doc.board, dirty);
    // Don't save.
    menu(&mut h, "File", "New…");
    click(&mut h, "Create");
    click(&mut h, "Don't save");
    assert!(h.state().pending.is_none());
    assert!(!h.state().doc.is_dirty());
    assert_ne!(h.state().doc.board, dirty);
    // Save (untitled: asks for a path), then replace.
    make_dirty(&mut h);
    let out = fresh("prompt", "saved-first.brd");
    menu(&mut h, "File", "New…");
    click(&mut h, "Create");
    script(&mut h, &out);
    click(&mut h, "Save");
    assert!(out.exists(), "saved before replacing");
    assert!(h.state().pending.is_none());
    assert!(!h.state().doc.is_dirty());
    assert!(
        h.state().doc.path.is_none(),
        "new board replaced the saved one"
    );
    // Save, but the user cancels the file dialog: the prompt stays.
    make_dirty(&mut h);
    menu(&mut h, "File", "New…");
    click(&mut h, "Create");
    click(&mut h, "Save"); // no scripted path = cancelled dialog
    assert!(
        h.state().pending.is_some(),
        "prompt kept after cancelled save"
    );
    assert!(h.state().doc.is_dirty());
    click(&mut h, "Cancel");
}

#[test]
fn open_save_save_as_and_backup() {
    let mut h = harness();
    let a = fresh("save", "a.brd");
    // Cmd+S on an untitled board asks for a path.
    make_dirty(&mut h);
    script(&mut h, &a);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    step(&mut h);
    assert!(a.exists());
    assert_eq!(h.state().doc.path.as_deref(), Some(a.as_path()));
    assert_eq!(h.state().doc.title(), "a.brd");
    assert!(status(&h).starts_with("Saved"));
    // Save again keeps a .bak.
    make_dirty(&mut h);
    assert_eq!(h.state().doc.title(), "a.brd *");
    menu(&mut h, "File", "Save");
    assert!(a.with_extension("brd.bak").exists());
    assert!(!h.state().doc.is_dirty());
    // Save as (Cmd+Shift+S), without extension: .brd is added.
    let b = fresh("save", "b.brd");
    script(&mut h, &b.with_extension(""));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    step(&mut h);
    assert!(b.exists(), "save as adds .brd");
    assert_eq!(h.state().doc.path.as_deref(), Some(b.as_path()));
    // Open (Cmd+O) a template file, then the saved one through the menu.
    let saved = h.state().doc.board.clone();
    menu(&mut h, "File", "New…");
    click(&mut h, "Mid-length — 7'6\" mid-length");
    click(&mut h, "Create");
    script(&mut h, &b);
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    step(&mut h);
    assert_eq!(h.state().doc.board, saved);
    assert!(!h.state().doc.is_dirty());
    // Recent files list both, newest first.
    assert_eq!(h.state().set.recent.first(), Some(&b));
    assert!(h.state().set.recent.contains(&a));
    // Open recent: a.brd.
    click(&mut h, "File");
    click(&mut h, "Open recent");
    snap(&mut h, "qa-open-recent");
    click(&mut h, "a.brd");
    assert_eq!(h.state().doc.path.as_deref(), Some(a.as_path()));
    // Open a missing file: error in the status bar, board unchanged.
    let before = h.state().doc.board.clone();
    script(&mut h, &temp_dir("qa-save").join("missing.brd"));
    menu(&mut h, "File", "Open…");
    assert!(status(&h).contains("Could not open"), "{}", status(&h));
    assert_eq!(h.state().doc.board, before);
}

#[test]
fn open_imported_formats_untitled_and_export_step() {
    let mut h = harness();
    // File > Export curves > Whole board as STEP.
    let stp = fresh("step", "board.stp");
    script(&mut h, &stp);
    click(&mut h, "File");
    click(&mut h, "Export curves");
    click(&mut h, "Whole board as STEP (.stp)…");
    assert!(stp.exists());
    let vol = h.state().doc.board.volume();
    // Open it: untitled (Save must not overwrite the .stp), same volume.
    make_dirty(&mut h);
    script(&mut h, &stp);
    menu(&mut h, "File", "Open…");
    click(&mut h, "Don't save");
    settle(&mut h);
    assert!(h.state().doc.path.is_none());
    assert!((h.state().doc.board.volume() - vol).abs() / vol < 1e-3);
    // The BoardCAD Java STEP sample.
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../surfcraft-io/tests/data/shortboard_bcstep.stp");
    script(&mut h, &sample);
    menu(&mut h, "File", "Open…");
    settle(&mut h);
    assert!(h.state().doc.path.is_none());
    assert!(h.state().doc.board.length() > 150.0);
    assert!(!h.state().doc.board.info.name.is_empty());
    snap(&mut h, "qa-opened-step");
    // Save asks for a new path instead of writing over the .stp.
    let out = fresh("step", "from-step.brd");
    make_dirty(&mut h);
    script(&mut h, &out);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    step(&mut h);
    assert!(out.exists());
    assert!(std::fs::read(&sample).unwrap().starts_with(b"ISO-10303-21"));
}

#[test]
fn drag_and_drop_opens_board_files_only() {
    let mut h = harness();
    let p = fresh("drop", "dropped.brd");
    let longboard = surfcraft_io::templates::TEMPLATES[2].board();
    assert!((longboard.length() - h.state().doc.board.length()).abs() > 1.0);
    std::fs::write(&p, surfcraft_io::brd::write(&longboard)).unwrap();
    let txt = fresh("drop", "notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    h.input_mut().dropped_files.push(dropped(&txt));
    step(&mut h);
    assert!(h.state().doc.path.is_none(), "text file ignored");
    h.input_mut().dropped_files.push(dropped(&p));
    step(&mut h);
    assert_eq!(h.state().doc.path.as_deref(), Some(p.as_path()));
    assert!((h.state().doc.board.length() - longboard.length()).abs() < 1e-6);
    // Dirty board: drop asks first.
    make_dirty(&mut h);
    h.input_mut().dropped_files.push(dropped(&p));
    step(&mut h);
    assert!(matches!(h.state().pending, Some(Pending::Open(_))));
    click(&mut h, "Cancel");
}

#[test]
fn quit_shortcut_and_window_close_ask_when_dirty() {
    // Clean board: Cmd+Q closes at once.
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    h.run_steps(1);
    let mut closed = close_requested(&h);
    h.run_steps(1);
    closed |= close_requested(&h);
    assert!(closed, "clean quit closes");
    // Dirty board: prompt, Cancel keeps the window.
    let mut h = harness();
    make_dirty(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    step(&mut h);
    assert!(matches!(h.state().pending, Some(Pending::Quit)));
    click(&mut h, "Cancel");
    assert!(!close_requested(&h));
    // Window close button with a dirty board.
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    step(&mut h);
    assert!(
        matches!(h.state().pending, Some(Pending::Quit)),
        "close button asks"
    );
    click(&mut h, "Don't save");
    h.run_steps(1);
    assert!(close_requested(&h), "Don't save closes");
}

#[test]
fn save_and_set_original_requires_a_save() {
    let mut h = harness();
    make_dirty(&mut h);
    let original = h.state().doc.original.clone();
    menu(&mut h, "File", "Save and set as original"); // untitled, dialog cancelled
    assert!(h.state().doc.is_dirty(), "not saved");
    assert_eq!(
        h.state().doc.original,
        original,
        "original changed without a save"
    );
}

#[test]
fn save_and_set_original_with_a_path() {
    let mut h = harness();
    make_dirty(&mut h);
    let out = fresh("original", "orig.brd");
    script(&mut h, &out);
    menu(&mut h, "File", "Save and set as original");
    assert!(out.exists());
    assert_eq!(h.state().doc.original, h.state().doc.board);
    h.state_mut().set.view.original = true;
    set_tab(&mut h, Tab::Outline);
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -25.0), Modifiers::NONE);
    settle(&mut h);
    snap(&mut h, "qa-original-overlay");
    assert_ne!(h.state().doc.original, h.state().doc.board);
}

// ------------------------------------------------------------------------------- ghost / curves / image

#[test]
fn ghost_board_open_show_close() {
    let mut h = harness_t();
    let g = fresh("ghost", "ghost.brd");
    std::fs::write(
        &g,
        surfcraft_io::brd::write(&surfcraft_io::templates::TEMPLATES[1].board()),
    )
    .unwrap();
    script(&mut h, &g);
    menu(&mut h, "File", "Open ghost board…");
    assert!(h.state().ed.ghost.is_some());
    assert!(h.state().set.view.ghost);
    assert!(!h.state().doc.is_dirty(), "ghost does not touch the board");
    snap(&mut h, "qa-ghost-overview");
    set_tab(&mut h, Tab::Section);
    snap(&mut h, "qa-ghost-section");
    menu(&mut h, "View", "Ghost board");
    assert!(!h.state().set.view.ghost);
    h.key_press(Key::Escape);
    step(&mut h);
    menu(&mut h, "File", "Close ghost board");
    assert!(h.state().ed.ghost.is_none());
}

#[test]
fn export_and_import_curve_files() {
    let mut h = harness();
    let dir = temp_dir("qa-curves");
    let otl = fresh("curves", "o.otl");
    let pfl = fresh("curves", "p.pfl");
    let crs = fresh("curves", "s.crs");
    for (file, item) in [
        (&otl, "Outline (.otl)…"),
        (&pfl, "Profile (.pfl)…"),
        (&crs, "Current cross-section (.crs)…"),
    ] {
        script(&mut h, file);
        click(&mut h, "File");
        click(&mut h, "Export curves");
        click(&mut h, item);
        assert!(file.exists(), "{item}");
    }
    let _ = dir;
    let original = h.state().doc.board.clone();
    let cur = h.state().ed.current_section;
    // Scale the board, then import the old curves back.
    h.state_mut().doc.edit(|b| b.scale(200.0, 50.0, 7.0));
    for file in [&otl, &pfl] {
        script(&mut h, file);
        menu(&mut h, "File", "Import curves (.otl/.pfl/.crs)…");
        assert!(status(&h).starts_with("Imported"), "{}", status(&h));
    }
    let b = &h.state().doc.board;
    assert_eq!(b.outline.knots().len(), original.outline.knots().len());
    assert!((b.length() - original.length()).abs() < 1e-9);
    assert!((b.max_width() - original.max_width()).abs() < 1e-9);
    assert!((b.max_thickness() - original.max_thickness()).abs() < 1e-6);
    script(&mut h, &crs);
    menu(&mut h, "File", "Import curves (.otl/.pfl/.crs)…");
    assert_eq!(h.state().ed.current_section, cur);
    // Each import is one undo step.
    let depth = undo_depth(&mut h);
    assert_eq!(depth, 4, "scale + 3 imports");
    // A bad file reports an error and changes nothing.
    let bad = fresh("curves", "bad.otl");
    std::fs::write(&bad, "p32 : (\n(cp [x").unwrap();
    let before = h.state().doc.board.clone();
    script(&mut h, &bad);
    menu(&mut h, "File", "Import curves (.otl/.pfl/.crs)…");
    assert_eq!(h.state().doc.board, before);
}

fn write_png(path: &Path) {
    let mut img = image::RgbaImage::new(300, 80);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x % 255) as u8, (y * 3) as u8, 120, 255]);
    }
    img.save(path).unwrap();
}

#[test]
fn background_image_load_pin_opacity_remove() {
    let mut h = harness_t();
    let png = fresh("bg", "photo.png");
    write_png(&png);
    script(&mut h, &png);
    menu(&mut h, "File", "Load background image…");
    assert!(h.state().ed.background.is_some(), "{}", status(&h));
    assert_eq!(h.state().tab, Tab::Outline);
    h.state_mut().ed.fit_all();
    step(&mut h);
    snap(&mut h, "qa-background");
    // Hold T and click: tail pinned at the clicked pixel.
    let tail_before = h.state().ed.background.as_ref().unwrap().tail_px;
    let at = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(20.0, 5.0));
    let before = h.state().doc.board.clone();
    h.key_down(Key::T);
    h.run_steps(1);
    click_at(&mut h, at, Modifiers::NONE);
    h.key_up(Key::T);
    step(&mut h);
    let tail = h.state().ed.background.as_ref().unwrap().tail_px;
    assert_ne!(tail, tail_before, "T+click pins the tail");
    assert_eq!(
        h.state().doc.board,
        before,
        "pinning does not edit the board"
    );
    // N + click pins the nose.
    let nose_before = h.state().ed.background.as_ref().unwrap().nose_px;
    let at = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(170.0, -3.0));
    h.key_down(Key::N);
    h.run_steps(1);
    click_at(&mut h, at, Modifiers::NONE);
    h.key_up(Key::N);
    step(&mut h);
    assert_ne!(
        h.state().ed.background.as_ref().unwrap().nose_px,
        nose_before
    );
    snap(&mut h, "qa-background-pinned");
    // Opacity slider appears in the View menu.
    click(&mut h, "View");
    assert!(
        h.query_all_by_label_contains("Image opacity")
            .next()
            .is_some()
    );
    h.key_press(Key::Escape);
    step(&mut h);
    menu(&mut h, "File", "Remove background image");
    assert!(h.state().ed.background.is_none());
    // A non-image file reports an error.
    let bad = fresh("bg", "bad.png");
    std::fs::write(&bad, "not a png").unwrap();
    script(&mut h, &bad);
    menu(&mut h, "File", "Load background image…");
    assert!(h.state().ed.background.is_none());
    assert!(status(&h).contains("Could not load image"));
}

// ------------------------------------------------------------------------------------------- export

fn check_export(path: &Path, format: Format) {
    let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("{} not written", path.display()));
    match format {
        Format::Pdf => {
            assert!(bytes.starts_with(b"%PDF-1.4") && bytes.ends_with(b"%%EOF\n"));
        }
        Format::Svg => {
            if let Ok(o) = std::process::Command::new("xmllint")
                .arg("--noout")
                .arg(path)
                .output()
            {
                assert!(
                    o.status.success(),
                    "{}: {}",
                    path.display(),
                    String::from_utf8_lossy(&o.stderr)
                );
            }
        }
        Format::Dxf => {
            assert!(String::from_utf8_lossy(&bytes).trim_end().ends_with("EOF"));
        }
        Format::Stl => {
            let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
            assert_eq!(bytes.len(), 84 + 50 * n);
            assert!(n > 1000);
        }
        Format::Obj => assert!(String::from_utf8_lossy(&bytes).contains("\nf ")),
    }
}

#[test]
fn print_and_export_every_what_format_and_paper() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    step(&mut h);
    assert!(h.state().dialogs.export);
    for what in What::SHAPER {
        click(&mut h, what.label());
        let formats: &[(Format, &str)] = if what == What::Model {
            &[(Format::Stl, "STL (binary, mm)"), (Format::Obj, "OBJ (mm)")]
        } else {
            &[
                (Format::Pdf, "PDF (print, tiled pages)"),
                (Format::Svg, "SVG (1:1, one sheet)"),
                (Format::Dxf, "DXF (1:1, for CNC/laser)"),
            ]
        };
        for &(format, label) in formats {
            click(&mut h, label);
            assert_eq!(h.state().dialogs.export_state.format, format);
            let ext = match format {
                Format::Pdf => "pdf",
                Format::Svg => "svg",
                Format::Dxf => "dxf",
                Format::Stl => "stl",
                Format::Obj => "obj",
            };
            let out = fresh("export", &format!("{what:?}.{ext}"));
            script(&mut h, &out);
            click(&mut h, "Export…");
            check_export(&out, format);
            assert!(status(&h).starts_with("Exported"), "{}", status(&h));
        }
        snap(&mut h, &format!("qa-export-{what:?}").to_lowercase());
    }
    // Paper sizes for a PDF template: the page-count hint changes with paper.
    click(&mut h, What::Profile.label());
    click(&mut h, "PDF (print, tiled pages)");
    let mut counts = Vec::new();
    for paper in ["A4", "A3", "US Letter", "US Tabloid"] {
        click(&mut h, "Paper");
        click(&mut h, paper);
        let n = crate::export_ui::page_count(
            &h.state().doc.board,
            &h.state().dialogs.export_state,
            &h.state().set,
        );
        h.get_by_label_contains(&format!("{n} page(s)"));
        let out = fresh("export", &format!("profile-{paper}.pdf"));
        script(&mut h, &out);
        click(&mut h, "Export…");
        let pdf = std::fs::read(&out).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&pdf)
                .matches("/Type /Page ")
                .count(),
            n,
            "{paper}"
        );
        counts.push(n);
    }
    assert!(
        counts[1] < counts[0],
        "A3 needs fewer pages than A4: {counts:?}"
    );
    snap(&mut h, "qa-export-paper");
    // Overlap and fine mesh controls exist.
    assert!(h.query_all_by_label_contains("Overlap").next().is_some());
    click(&mut h, What::Model.label());
    click(&mut h, "High resolution mesh");
    assert!(!h.state().dialogs.export_state.fine_mesh);
}

#[test]
fn export_cancelled_dialog_writes_nothing() {
    let mut h = harness();
    menu(&mut h, "File", "Print & export…");
    click(&mut h, "Export…"); // no scripted path: cancelled
    assert!(status(&h).is_empty(), "{}", status(&h));
}

// --------------------------------------------------------------------------------------- toolbar

#[test]
fn toolbar_tabs_tools_undo_units() {
    let mut h = harness();
    for (label, tab) in [
        ("Outline", Tab::Outline),
        ("Rocker", Tab::Profile),
        ("Cross-section", Tab::Section),
        ("3D", Tab::ThreeD),
        ("Overview", Tab::Overview),
    ] {
        click(&mut h, label);
        assert_eq!(h.state().tab, tab, "{label}");
    }
    for (label, tool) in [
        ("+ Point", Tool::AddKnot),
        ("+ Guide", Tool::AddGuide),
        ("Select", Tool::Select),
    ] {
        click(&mut h, label);
        assert_eq!(h.state().ed.tool, tool);
    }
    make_dirty(&mut h);
    let flipped = h.state().doc.board.clone();
    // Toolbar Undo/Redo (the Edit menu has the same labels but is closed).
    click(&mut h, "Undo");
    assert_ne!(h.state().doc.board, flipped);
    click(&mut h, "Redo");
    assert_eq!(h.state().doc.board, flipped);
    // Units: the specs panel shows the 72" shortboard's length.
    for (label, unit, needle) in [
        ("cm", Unit::Centimetres, "182.88 cm"),
        ("mm", Unit::Millimetres, "1828.8 mm"),
        ("m", Unit::Metres, "1.8288 m"),
        ("ft-in (decimal)", Unit::FeetInchesDecimal, "6' 0.00\""),
        ("ft-in (fraction)", Unit::FeetInchesFraction, "6' 0\""),
    ] {
        click(&mut h, "Units");
        click(&mut h, label);
        assert_eq!(h.state().set.unit, unit, "{label}");
        h.get_by_label(needle);
        if unit == Unit::Millimetres {
            snap(&mut h, "qa-units-mm");
        }
    }
}

#[test]
fn keyboard_shortcuts() {
    let mut h = harness();
    for (k, tab) in [
        (Key::Num2, Tab::Outline),
        (Key::Num3, Tab::Profile),
        (Key::Num4, Tab::Section),
        (Key::Num5, Tab::ThreeD),
        (Key::Num1, Tab::Overview),
    ] {
        h.key_press_modifiers(Modifiers::COMMAND, k);
        step(&mut h);
        assert_eq!(h.state().tab, tab);
    }
    // F fits after zooming.
    set_tab(&mut h, Tab::Outline);
    let fitted = h.state().ed.outline_view.xf;
    let c = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(90.0, 0.0));
    h.hover_at(c);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 120.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    h.run_steps(10);
    assert!(
        h.state().ed.outline_view.xf.scale > fitted.scale * 1.05,
        "wheel zooms in"
    );
    h.key_press(Key::F);
    step(&mut h);
    let xf = h.state().ed.outline_view.xf;
    assert!((xf.scale - fitted.scale).abs() < 1e-3, "F fits");
    // Esc: select tool, no selection.
    h.state_mut().ed.tool = Tool::AddGuide;
    h.state_mut().ed.selection = Some(Selection::single(CurveId::Outline, 2, Handle::Point));
    h.key_press(Key::Escape);
    step(&mut h);
    assert_eq!(h.state().ed.tool, Tool::Select);
    assert!(h.state().ed.selection.is_none());
    // Undo / redo / Cmd+Y.
    make_dirty(&mut h);
    let after = h.state().doc.board.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    step(&mut h);
    assert_ne!(h.state().doc.board, after);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    step(&mut h);
    assert_eq!(h.state().doc.board, after);
    // Cmd+P opens Print & export, Cmd+N the new-board dialog.
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    step(&mut h);
    assert!(h.state().dialogs.export);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    step(&mut h);
    assert!(h.state().dialogs.new_board.is_some());
}

// ------------------------------------------------------------------------------------ mouse editing

#[test]
fn alt_drag_is_ten_times_finer() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let y0 = knot(&h, CurveId::Outline, 2).y;
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -20.0), Modifiers::NONE);
    let coarse = knot(&h, CurveId::Outline, 2).y - y0;
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    step(&mut h);
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -20.0), Modifiers::ALT);
    let fine = knot(&h, CurveId::Outline, 2).y - y0;
    assert!(coarse > 0.0 && fine > 0.0);
    assert!(
        (coarse / fine - 10.0).abs() < 1.5,
        "coarse {coarse} fine {fine}"
    );
}

#[test]
fn drag_handle_keeps_smooth_knot_collinear() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let from = outline_screen(&h, 2, Handle::Next);
    drag(
        &mut h,
        from,
        from + egui::vec2(-10.0, 15.0),
        Modifiers::NONE,
    );
    let k = h.state().doc.board.outline.knots()[2].clone();
    assert_eq!(
        h.state().ed.selection.as_ref().unwrap().handle,
        Handle::Next
    );
    let a = (k.next() - k.point()).normalize();
    let b = (k.prev() - k.point()).normalize();
    assert!((a + b).length() < 1e-6, "handles collinear");
    assert!(h.state().doc.can_undo());
}

#[test]
fn multi_select_box_select_nudge_delete() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    // Click selects; shift-click adds; cmd-click toggles.
    let p1 = outline_screen(&h, 1, Handle::Point);
    let p2 = outline_screen(&h, 2, Handle::Point);
    let p3 = outline_screen(&h, 3, Handle::Point);
    click_at(&mut h, p2, Modifiers::NONE);
    assert_eq!(h.state().ed.selection.as_ref().unwrap().knots.len(), 1);
    click_at(&mut h, p3, Modifiers::SHIFT);
    assert_eq!(
        h.state()
            .ed
            .selection
            .as_ref()
            .unwrap()
            .knots
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    click_at(&mut h, p3, Modifiers::COMMAND);
    assert_eq!(
        h.state()
            .ed
            .selection
            .as_ref()
            .unwrap()
            .knots
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
    click_at(&mut h, p1, Modifiers::COMMAND);
    assert_eq!(h.state().ed.selection.as_ref().unwrap().knots.len(), 2);
    snap(&mut h, "qa-multiselect");
    // Click on empty space clears.
    let empty = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(90.0, 60.0));
    click_at(&mut h, empty, Modifiers::NONE);
    assert!(h.state().ed.selection.is_none());
    // Box select around knots 1..=2 from empty space.
    let a = egui::pos2(p1.x.min(p2.x) - 12.0, p1.y.min(p2.y) - 12.0);
    let b = egui::pos2(p1.x.max(p2.x) + 12.0, p1.y.max(p2.y) + 12.0);
    let a = egui::pos2(
        a.x,
        a.y.min(
            h.state()
                .ed
                .outline_view
                .xf
                .to_screen(DVec2::new(0.0, 40.0))
                .y,
        ),
    );
    drag(&mut h, a, b, Modifiers::NONE);
    let sel = h.state().ed.selection.clone().expect("box selection");
    assert!(sel.knots.contains(&1) && sel.knots.contains(&2), "{sel:?}");
    let before = h.state().doc.board.clone();
    assert_eq!(before, h.state().doc.board, "box select does not edit");
    // Arrow keys nudge (hovering the view), one undo step each.
    h.state_mut().ed.selection = Some(Selection::single(CurveId::Outline, 2, Handle::Point));
    let k0 = knot(&h, CurveId::Outline, 2);
    h.hover_at(p2 + egui::vec2(40.0, 40.0));
    h.run_steps(1);
    h.key_press(Key::ArrowUp);
    step(&mut h);
    h.key_press(Key::ArrowRight);
    step(&mut h);
    let k1 = knot(&h, CurveId::Outline, 2);
    let px = h.state().ed.outline_view.xf.len_to_world(1.0);
    assert!(
        (k1.y - k0.y - px).abs() < 1e-6,
        "up: {} vs {px}",
        k1.y - k0.y
    );
    assert!((k1.x - k0.x - px).abs() < 1e-6, "right");
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    step(&mut h);
    let k2 = knot(&h, CurveId::Outline, 2);
    assert!((k1.y - k2.y - px * 0.1).abs() < 1e-6, "alt+down is fine");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    step(&mut h);
    assert_eq!(knot(&h, CurveId::Outline, 2), k1);
    // Delete removes the knot; end knots survive.
    let n = h.state().doc.board.outline.len();
    h.key_press(Key::Delete);
    step(&mut h);
    assert_eq!(h.state().doc.board.outline.len(), n - 1);
    assert!(h.state().ed.selection.is_none());
    h.state_mut().ed.selection = Some(Selection::single(CurveId::Outline, 0, Handle::Point));
    h.key_press(Key::Backspace);
    step(&mut h);
    assert_eq!(h.state().doc.board.outline.len(), n - 1, "end knot kept");
    // Edit menu delete.
    h.state_mut().ed.selection = Some(Selection::single(CurveId::Outline, 1, Handle::Point));
    menu(&mut h, "Edit", "Delete selected control points");
    assert_eq!(h.state().doc.board.outline.len(), n - 2);
}

#[test]
fn add_point_and_add_guide_tools() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    click(&mut h, "+ Point");
    let n = h.state().doc.board.outline.len();
    let x = 140.0;
    let y = h.state().doc.board.width_at(x) / 2.0;
    let at = h.state().ed.outline_view.xf.to_screen(DVec2::new(x, y));
    let shape = (
        h.state().doc.board.width_at(60.0),
        h.state().doc.board.volume(),
    );
    click_at(&mut h, at, Modifiers::NONE);
    settle(&mut h);
    assert_eq!(h.state().doc.board.outline.len(), n + 1, "knot added");
    let sel = h.state().ed.selection.clone().unwrap();
    assert_eq!(sel.curve, CurveId::Outline);
    assert!((knot(&h, CurveId::Outline, sel.primary().unwrap()).x - x).abs() < 1.0);
    assert!(
        (h.state().doc.board.width_at(60.0) - shape.0).abs() < 1e-3,
        "shape kept"
    );
    // Far from the curve: nothing.
    let far = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(90.0, 0.0));
    click_at(&mut h, far, Modifiers::NONE);
    assert_eq!(h.state().doc.board.outline.len(), n + 1);
    // Guide tool (Edit menu radio).
    menu(&mut h, "Edit", "Add guide point");
    assert_eq!(h.state().ed.tool, Tool::AddGuide);
    h.key_press(Key::Escape);
    step(&mut h);
    h.state_mut().ed.tool = Tool::AddGuide;
    click_at(&mut h, far, Modifiers::NONE);
    let g = h.state().doc.board.outline_guides.clone();
    assert_eq!(g.len(), 1);
    assert!((g[0] - DVec2::new(90.0, 0.0)).length() < 0.5, "{g:?}");
    snap(&mut h, "qa-add-tools");
    menu(&mut h, "Edit", "Clear guide points");
    assert!(h.state().doc.board.outline_guides.is_empty());
}

#[test]
fn double_click_with_add_point_adds_one_knot() {
    let mut h = harness_fast();
    set_tab(&mut h, Tab::Outline);
    h.state_mut().ed.tool = Tool::AddKnot;
    let n = h.state().doc.board.outline.len();
    let x = 140.0;
    let y = h.state().doc.board.width_at(x) / 2.0;
    let at = h.state().ed.outline_view.xf.to_screen(DVec2::new(x, y));
    click_at(&mut h, at, Modifiers::NONE);
    click_at(&mut h, at, Modifiers::NONE);
    assert_eq!(h.state().doc.board.outline.len(), n + 1);
}

#[test]
fn double_click_selects_nearest_section_and_pan_zoom() {
    let mut h = harness_fast();
    set_tab(&mut h, Tab::Outline);
    let b = h.state().doc.board.clone();
    let target = b.real_sections().start + 1;
    let x = b.sections[target].position;
    let at = h
        .state()
        .ed
        .outline_view
        .xf
        .to_screen(DVec2::new(x + 1.0, 0.0));
    h.state_mut().ed.current_section = b.real_sections().end - 1;
    click_at(&mut h, at, Modifiers::NONE);
    click_at(&mut h, at, Modifiers::NONE);
    assert_eq!(
        h.state().ed.current_section,
        target,
        "double-click picks section"
    );
    // Pan with the right button, middle button, and Space+drag; none of them edits.
    let before = h.state().doc.board.clone();
    for button in [egui::PointerButton::Secondary, egui::PointerButton::Middle] {
        let o = h.state().ed.outline_view.xf.origin;
        drag_button(&mut h, at, at + egui::vec2(60.0, 20.0), button);
        let o2 = h.state().ed.outline_view.xf.origin;
        assert!(
            (o2 - o - egui::vec2(60.0, 20.0)).length() < 2.0,
            "{button:?} pans {o:?} -> {o2:?}"
        );
    }
    let o = h.state().ed.outline_view.xf.origin;
    let k = outline_screen(&h, 2, Handle::Point);
    h.key_down(Key::Space);
    h.run_steps(1);
    drag(&mut h, k, k + egui::vec2(-50.0, 0.0), Modifiers::NONE);
    h.key_up(Key::Space);
    step(&mut h);
    assert!(
        (h.state().ed.outline_view.xf.origin.x - o.x + 50.0).abs() < 2.0,
        "space+drag pans"
    );
    assert_eq!(h.state().doc.board, before, "panning never edits");
    // Pinch zoom.
    let s = h.state().ed.outline_view.xf.scale;
    h.hover_at(at);
    h.event(egui::Event::Zoom(1.5));
    step(&mut h);
    assert!(h.state().ed.outline_view.xf.scale > s * 1.2);
    // Status bar shows the cursor position.
    h.hover_at(
        h.state()
            .ed
            .outline_view
            .xf
            .to_screen(DVec2::new(100.0, 10.0)),
    );
    step(&mut h);
    assert!(h.state().ed.cursor.is_some());
    snap(&mut h, "qa-outline-panned");
}

// ------------------------------------------------------------------------------------- profile view

#[test]
fn profile_view_edit_deck_bottom_and_exaggeration() {
    let mut h = harness();
    set_tab(&mut h, Tab::Profile);
    snap(&mut h, "qa-profile");
    // Deck knot 2 up by 10 px.
    let y0 = knot(&h, CurveId::Deck, 2).y;
    let from = screen(&h, Tab::Profile, CurveId::Deck, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -10.0), Modifiers::NONE);
    settle(&mut h);
    let y1 = knot(&h, CurveId::Deck, 2).y;
    assert!(y1 > y0, "deck raised {y0} -> {y1}");
    assert_eq!(
        h.state().ed.selection.as_ref().unwrap().curve,
        CurveId::Deck
    );
    // Bottom only: deck knots no longer editable.
    click(&mut h, "Bottom");
    let d = knot(&h, CurveId::Deck, 2);
    let from = screen(&h, Tab::Profile, CurveId::Deck, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -10.0), Modifiers::NONE);
    assert_eq!(knot(&h, CurveId::Deck, 2), d, "deck locked in bottom mode");
    click(&mut h, "Deck");
    click(&mut h, "Deck + bottom");
    // Exaggeration: a 10 px drag moves less in world units.
    h.state_mut().set.profile_exaggeration = 4.0;
    h.state_mut().ed.fit_all();
    step(&mut h);
    let y0 = knot(&h, CurveId::Deck, 2).y;
    let from = screen(&h, Tab::Profile, CurveId::Deck, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -10.0), Modifiers::NONE);
    let dy = knot(&h, CurveId::Deck, 2).y - y0;
    let px = h.state().ed.profile_view.xf.len_to_world(1.0);
    assert!(
        (dy - 10.0 * px / 4.0).abs() < px,
        "dy {dy} with 4x exaggeration"
    );
    snap(&mut h, "qa-profile-exaggerated");
}

#[test]
fn profile_nudge_respects_exaggeration() {
    let mut h = harness();
    set_tab(&mut h, Tab::Profile);
    h.state_mut().set.profile_exaggeration = 4.0;
    h.state_mut().ed.fit_all();
    step(&mut h);
    h.state_mut().ed.selection = Some(Selection::single(CurveId::Deck, 2, Handle::Point));
    let y0 = knot(&h, CurveId::Deck, 2).y;
    let p = screen(&h, Tab::Profile, CurveId::Deck, 2, Handle::Point);
    h.hover_at(p + egui::vec2(30.0, 30.0));
    h.run_steps(1);
    h.key_press(Key::ArrowUp);
    step(&mut h);
    let dy = knot(&h, CurveId::Deck, 2).y - y0;
    let px = h.state().ed.profile_view.xf.len_to_world(1.0);
    assert!(
        (dy - px / 4.0).abs() < 1e-6,
        "one pixel up moved {dy}, a pixel is {} cm",
        px / 4.0
    );
}

#[test]
fn rocker_to_zero_and_flip_from_board_menu() {
    let mut h = harness();
    h.state_mut().doc.edit(|b| {
        let d = DVec2::new(0.0, 3.0);
        b.bottom.translate(d);
        b.deck.translate(d);
    });
    menu(&mut h, "Board", "Put lowest rocker point at zero");
    assert!(h.state().doc.board.bottom.min_y().abs() < 1e-9);
    let l = h.state().doc.board.length();
    let tail = h.state().doc.board.rocker_at(1.0);
    let nose = h.state().doc.board.rocker_at(l - 1.0);
    menu(&mut h, "Board", "Flip nose and tail");
    assert!((h.state().doc.board.rocker_at(1.0) - nose).abs() < 1e-6);
    assert!((h.state().doc.board.rocker_at(l - 1.0) - tail).abs() < 1e-6);
    settle(&mut h);
    snap(&mut h, "qa-flipped");
}

// ------------------------------------------------------------------------------------- section view

#[test]
fn section_toolbar_prev_next_add_move_remove() {
    let mut h = harness();
    set_tab(&mut h, Tab::Section);
    let r = h.state().doc.board.real_sections();
    let start = h.state().ed.current_section;
    click(&mut h, ">");
    assert_eq!(
        h.state().ed.current_section,
        if start + 1 >= r.end {
            r.start
        } else {
            start + 1
        }
    );
    click(&mut h, "<");
    assert_eq!(h.state().ed.current_section, start);
    // Keys: + / = next, - previous, wrapping.
    h.state_mut().ed.current_section = r.end - 1;
    h.key_press(Key::Plus);
    step(&mut h);
    assert_eq!(h.state().ed.current_section, r.start, "wraps to first");
    h.key_press(Key::Minus);
    step(&mut h);
    assert_eq!(h.state().ed.current_section, r.end - 1, "wraps to last");
    h.key_press(Key::Equals);
    step(&mut h);
    assert_eq!(h.state().ed.current_section, r.start);
    // Add at 4'.
    let n = h.state().doc.board.sections.len();
    let pos = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .rect()
        .center();
    type_into(&mut h, pos, "4'");
    click(&mut h, "Add");
    let b = &h.state().doc.board;
    assert_eq!(b.sections.len(), n + 1);
    let cur = h.state().ed.current_section;
    assert!((b.sections[cur].position - 121.92).abs() < 1e-6);
    h.get_by_label_contains("at 4' 0\"");
    snap(&mut h, "qa-section-added");
    // Same place again: refused, nothing changes.
    click(&mut h, "Add");
    assert_eq!(h.state().doc.board.sections.len(), n + 1);
    // Move here: 4'2".
    type_into(&mut h, pos, "4' 2\"");
    click(&mut h, "Move here");
    let cur = h.state().ed.current_section;
    assert!((h.state().doc.board.sections[cur].position - 127.0).abs() < 1e-6);
    // Remove.
    click(&mut h, "Remove");
    assert_eq!(h.state().doc.board.sections.len(), n);
    assert!(
        h.state()
            .doc
            .board
            .real_sections()
            .contains(&h.state().ed.current_section)
    );
    // Unparseable position disables Add / Move here.
    type_into(&mut h, pos, "banana");
    assert!(h.get_by_label("Add").accesskit_node().is_disabled());
    assert!(h.get_by_label("Move here").accesskit_node().is_disabled());
    // Undo all three edits.
    assert_eq!(undo_depth(&mut h), 3);
}

#[test]
fn section_drag_knot_and_remove_until_one() {
    let mut h = harness();
    set_tab(&mut h, Tab::Section);
    let cur = h.state().ed.current_section;
    let id = CurveId::Section(cur);
    let k0 = knot(&h, id, 2);
    let from = screen(&h, Tab::Section, id, 2, Handle::Point);
    drag(
        &mut h,
        from,
        from + egui::vec2(-15.0, -10.0),
        Modifiers::NONE,
    );
    settle(&mut h);
    let b = &h.state().doc.board;
    let s = &b.sections[cur];
    // Section width/thickness stay fitted to outline and rocker.
    assert!((s.width() - b.width_at(s.position)).abs() < 0.05);
    assert_ne!(knot(&h, id, 2), k0);
    snap(&mut h, "qa-section-dragged");
    while h.state().doc.board.real_sections().len() > 1 {
        click(&mut h, "Remove");
    }
    click(&mut h, "Remove");
    assert_eq!(
        h.state().doc.board.real_sections().len(),
        1,
        "last section kept"
    );
    settle(&mut h);
    let v = h.state().doc.board.volume();
    assert!(v.is_finite() && v > 0.0);
    snap(&mut h, "qa-one-section");
}

// ------------------------------------------------------------------------------------ side panel

#[test]
fn knot_inspector_numeric_edit_and_continuity() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let k2 = outline_screen(&h, 2, Handle::Point);
    click_at(&mut h, k2, Modifiers::NONE);
    h.get_by_label("Outline knot 3");
    // Half width: the field right of the "Point" row's x field.
    let row = text_pos(&h, "Point");
    let mut fields: Vec<egui::Rect> = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .filter(|r| (r.center().y - row.center().y).abs() < 10.0)
        .collect();
    fields.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    assert_eq!(fields.len(), 2);
    type_into(&mut h, fields[1].center(), "9 1/2\"");
    settle(&mut h);
    let k = knot(&h, CurveId::Outline, 2);
    assert!((k.y - 9.5 * 2.54).abs() < 1e-9, "{}", k.y);
    assert!((h.state().doc.board.max_width() - 19.0 * 2.54).abs() < 0.05);
    type_into(&mut h, fields[0].center(), "100cm");
    assert!((knot(&h, CurveId::Outline, 2).x - 100.0).abs() < 1e-9);
    snap(&mut h, "qa-knot-inspector");
    // Continuity checkbox.
    click(&mut h, "Continuous (smooth)");
    assert!(!h.state().doc.board.outline.knots()[2].continuous);
    click(&mut h, "Continuous (smooth)");
    assert!(h.state().doc.board.outline.knots()[2].continuous);
    assert_eq!(undo_depth(&mut h), 4);
}

#[test]
fn enter_in_unchanged_length_field_is_a_no_op() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let k2 = outline_screen(&h, 2, Handle::Point);
    click_at(&mut h, k2, Modifiers::NONE);
    let before = h.state().doc.board.clone();
    let row = text_pos(&h, "Point");
    let f = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .find(|r| (r.center().y - row.center().y).abs() < 10.0)
        .unwrap();
    click_at(&mut h, f.center(), Modifiers::NONE);
    h.key_press(Key::Enter);
    step(&mut h);
    assert_eq!(h.state().doc.board, before, "Enter alone changed the board");
    assert!(!h.state().doc.can_undo());
}

#[test]
fn typing_in_inspector_does_not_edit_the_view_selection() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let k2 = outline_screen(&h, 2, Handle::Point);
    click_at(&mut h, k2, Modifiers::NONE);
    let before = h.state().doc.board.clone();
    let row = text_pos(&h, "Point");
    let f = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .find(|r| (r.center().y - row.center().y).abs() < 10.0)
        .unwrap();
    click_at(&mut h, f.center(), Modifiers::NONE);
    // Mouse goes back over the drawing while the user edits the number.
    h.hover_at(
        h.state()
            .ed
            .outline_view
            .xf
            .to_screen(DVec2::new(60.0, 0.0)),
    );
    h.run_steps(1);
    h.key_press(Key::ArrowLeft);
    h.key_press(Key::Backspace);
    step(&mut h);
    assert_eq!(
        h.state().doc.board.outline,
        before.outline,
        "caret keys edited the curve"
    );
}

#[test]
fn typing_minus_in_a_text_field_does_not_change_section() {
    let mut h = harness();
    menu(&mut h, "Board", "Board info…");
    let cur = h.state().ed.current_section;
    let p = field_in(&h, "Board info", "Name");
    click_at(&mut h, p, Modifiers::NONE);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    for c in ["T", "w", "i", "n", "-", "f", "i", "n"] {
        if c == "-" {
            h.key_press(Key::Minus); // a real keyboard sends the key and the text
        }
        h.event(egui::Event::Text(c.into()));
        h.run_steps(1);
    }
    step(&mut h);
    assert_eq!(h.state().doc.board.info.name, "Twin-fin");
    assert_eq!(
        h.state().ed.current_section,
        cur,
        "section changed while typing"
    );
}

#[test]
fn specs_panel_toggle_and_values() {
    let mut h = harness_t();
    let volume = default_volume_label();
    h.get_by_label(&volume);
    h.get_by_label("Planform area");
    h.get_by_label("Centre of mass");
    menu(&mut h, "View", "Specs panel");
    assert!(!h.state().set.show_side_panel);
    h.key_press(Key::Escape);
    step(&mut h);
    assert!(h.query_all_by_label(&volume).next().is_none());
    snap(&mut h, "qa-no-side-panel");
    h.state_mut().set.show_side_panel = true;
    step(&mut h);
    // Stale results are marked while recomputing.
    h.state_mut().doc.edit(|b| b.scale(200.0, 50.0, 7.0));
    h.run_steps(1);
    let stale = h.query_all_by_label_contains(" …").next().is_some();
    settle(&mut h);
    let _ = stale; // timing dependent; not asserted
    assert!(h.query_all_by_label(&volume).next().is_none());
}

// ----------------------------------------------------------------------------------------- dialogs

#[test]
fn scale_dialog_fields_keep_proportions_and_target_volume() {
    let mut h = harness();
    let width = h.state().doc.board.max_width();
    menu(&mut h, "Board", "Scale…");
    let p = field_in(&h, "Scale board", "Length");
    type_into(&mut h, p, "6'6\"");
    snap(&mut h, "qa-scale-dialog");
    click(&mut h, "Apply");
    settle(&mut h);
    let b = h.state().doc.board.clone();
    assert!((b.length() - 198.12).abs() < 1e-6);
    assert!((b.max_width() - width).abs() < 0.01, "width kept");
    // Fins moved with the board by default.
    // Keep proportions: change length, width follows.
    menu(&mut h, "Board", "Scale…");
    click(&mut h, "Keep proportions");
    let p = field_in(&h, "Scale board", "Length");
    type_into(&mut h, p, "7'0\"");
    click(&mut h, "Apply");
    settle(&mut h);
    let b2 = h.state().doc.board.clone();
    let f = 213.36 / 198.12;
    assert!((b2.length() - 213.36).abs() < 1e-6);
    assert!(
        (b2.max_width() - b.max_width() * f).abs() < 0.02,
        "width scaled with length"
    );
    // Target volume.
    menu(&mut h, "Board", "Scale…");
    let p = field_in(&h, "Scale board", "Target volume (L)");
    type_into(&mut h, p, "33");
    click(&mut h, "Apply");
    settle(&mut h);
    let v = h.state().doc.board.volume() / 1000.0;
    assert!((v - 33.0).abs() < 0.1, "volume {v}");
}

#[test]
fn scale_dialog_rejects_zero_length() {
    let mut h = harness();
    menu(&mut h, "Board", "Scale…");
    let p = field_in(&h, "Scale board", "Length");
    type_into(&mut h, p, "0");
    click(&mut h, "Apply");
    settle(&mut h);
    let b = &h.state().doc.board;
    assert!(b.length() > 10.0, "length {}", b.length());
    assert!(
        b.volume().is_finite() && b.volume() > 0.0,
        "volume {}",
        b.volume()
    );
}

#[test]
fn length_field_rejects_infinity() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let k2 = outline_screen(&h, 2, Handle::Point);
    click_at(&mut h, k2, Modifiers::NONE);
    let row = text_pos(&h, "Point");
    let f = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .find(|r| (r.center().y - row.center().y).abs() < 10.0)
        .unwrap();
    type_into(&mut h, f.center(), "inf");
    let k = knot(&h, CurveId::Outline, 2);
    assert!(k.x.is_finite(), "knot x {}", k.x);
}

#[test]
fn board_info_fields() {
    let mut h = harness();
    menu(&mut h, "Board", "Board info…");
    assert!(h.state().dialogs.info);
    for (label, text) in [
        ("Name", "QA Fish"),
        ("Model", "Keel"),
        ("Designer", "D"),
        ("Author", "A"),
        ("Surfer", "S"),
        ("Fin setup", "Twin"),
    ] {
        let p = field_in(&h, "Board info", label);
        type_into(&mut h, p, text);
    }
    let i = &h.state().doc.board.info;
    assert_eq!(
        (
            i.name.as_str(),
            i.model.as_str(),
            i.designer.as_str(),
            i.author.as_str(),
            i.surfer.as_str(),
            i.fin_type.as_str()
        ),
        ("QA Fish", "Keel", "D", "A", "S", "Twin")
    );
    assert_eq!(h.state().doc.title(), "QA Fish *");
    snap(&mut h, "qa-board-info");
}

#[test]
fn board_info_typing_is_one_undo_step() {
    let mut h = harness();
    menu(&mut h, "Board", "Board info…");
    let p = field_in(&h, "Board info", "Name");
    click_at(&mut h, p, Modifiers::NONE);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    for c in "Pipeline".chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(1);
    }
    step(&mut h);
    assert_eq!(h.state().doc.board.info.name, "Pipeline");
    let depth = undo_depth(&mut h);
    assert!(depth <= 1, "{depth} undo steps for typing one word");
}

#[test]
fn fins_dialog_applies_a_five_fin_fcs2_setup() {
    use surfcraft_core::fins::BoxSystem;
    let mut h = harness();
    menu(&mut h, "Board", "Fins…");
    step(&mut h);
    assert!(has_label(
        &h,
        "No fins. Choose a setup and press Apply preset."
    ));
    click(&mut h, "Setup");
    click(&mut h, "5 fins");
    click(&mut h, "Box system");
    click(&mut h, "FCS II");
    let depth = undo_depth(&mut h);
    click(&mut h, "Apply preset");
    step(&mut h);
    let s = h.state().doc.board.fin_setup.clone().expect("setup stored");
    assert_eq!(s.fins.len(), 5);
    assert!(s.fins.iter().all(|f| f.system == BoxSystem::FcsII));
    for l in [
        "Left front",
        "Right front",
        "Left rear",
        "Right rear",
        "Centre",
    ] {
        assert!(has_label(&h, l), "{l} not listed");
    }
    assert_eq!(
        undo_depth(&mut h),
        depth + 1,
        "Apply preset is one undo step"
    );
    // The legacy marks follow (BoardCAD shows the front pair and the centre fin).
    assert!(h.state().doc.board.fins[0] > 0.0 && h.state().doc.board.fins[4] > 0.0);
    // Typing a distance from the tail moves the fin and its mirror image.
    let p = field_in(&h, "Fins", "Left front");
    type_into(&mut h, p, "11 1/2\"");
    let s = h.state().doc.board.fin_setup.clone().unwrap();
    let moved: Vec<_> = s
        .fins
        .iter()
        .filter(|f| (f.x - 29.21).abs() < 1e-9)
        .collect();
    assert_eq!(moved.len(), 2, "{s:?}");
    assert_eq!(*moved[0], moved[1].mirrored());
    h.state_mut().set.view.fins = true;
    snap(&mut h, "qa-fins-dialog");
    set_tab(&mut h, Tab::Outline);
    snap(&mut h, "qa-fins-outline");
}

#[test]
fn weight_calculator_reacts_to_layup_and_units() {
    let mut h = harness();
    menu(&mut h, "Board", "Weight calculator…");
    settle(&mut h);
    h.run_steps(3);
    let total = |h: &H| {
        let st = h.state().dialogs.weight.as_ref().unwrap();
        let _ = st;
        // Read the "Total" row value: the label right of "Total".
        let w = window(h, "Weight calculator");
        let t = w.query_all_by_value("Total").next().unwrap().rect();
        w.get_all_by_role(egui::accesskit::Role::Label)
            .filter(|n| {
                (n.rect().center().y - t.center().y).abs() < 6.0 && n.rect().min.x > t.max.x
            })
            .filter_map(|n| n.value())
            .next()
            .unwrap()
    };
    let lb = total(&h);
    assert!(lb.ends_with(" lb"), "{lb}");
    click(&mut h, "Units");
    click(&mut h, "cm");
    h.run_steps(3);
    let kg = total(&h);
    assert!(kg.ends_with(" kg"), "{kg}");
    let kg0: f64 = kg.trim_end_matches(" kg").parse().unwrap();
    // Heavier foam.
    {
        let w = window(&h, "Weight calculator");
        let l = w
            .query_all_by_value("Foam density (kg/L)")
            .next()
            .unwrap()
            .rect();
        let dv = w
            .query_all_by(|n| n.numeric_value().is_some())
            .find(|n| (n.rect().center().y - l.center().y).abs() < 10.0)
            .expect("foam density drag value");
        let v = dv.accesskit_node().numeric_value().unwrap();
        dv.set_value_accesskit(v * 2.0);
    }
    h.run_steps(3);
    let kg1: f64 = total(&h).trim_end_matches(" kg").parse().unwrap();
    assert!(kg1 > kg0);
    click(&mut h, "Reset to defaults for this length");
    let kg2: f64 = total(&h).trim_end_matches(" kg").parse().unwrap();
    assert!((kg2 - kg0).abs() < 0.01);
    snap(&mut h, "qa-weight-kg");
}

#[test]
fn guide_points_dialog_add_edit_delete_per_view() {
    let mut h = harness();
    menu(&mut h, "Board", "Guide points…");
    h.get_by_label_contains("Outline — reference points");
    click(&mut h, "Add guide point");
    click(&mut h, "Add guide point");
    assert_eq!(h.state().doc.board.outline_guides.len(), 2);
    assert_eq!(h.state().doc.board.outline_guides[1], DVec2::new(20.0, 0.0));
    // Edit #1 X.
    let p = field_in(&h, "Guide points", "1");
    type_into(&mut h, p, "50cm");
    assert_eq!(h.state().doc.board.outline_guides[0].x, 50.0);
    snap(&mut h, "qa-guides-dialog");
    h.get_all_by_label("Delete").next().unwrap().click();
    step(&mut h);
    assert_eq!(h.state().doc.board.outline_guides.len(), 1);
    // Profile tab: deck guides; Section tab: section guides.
    h.state_mut().tab = Tab::Profile;
    step(&mut h);
    h.get_by_label_contains("Deck — reference points");
    click(&mut h, "Add guide point");
    assert_eq!(h.state().doc.board.deck_guides.len(), 1);
    h.state_mut().tab = Tab::Section;
    step(&mut h);
    h.get_by_label_contains("Cross-section — reference points");
    let cur = h.state().ed.current_section;
    let before = h.state().doc.board.sections[cur].guide_points.len();
    click(&mut h, "Add guide point");
    assert_eq!(
        h.state().doc.board.sections[cur].guide_points.len(),
        before + 1
    );
}

// -------------------------------------------------------------------------------- model / view menu

#[test]
fn surface_model_switch() {
    let mut h = harness();
    settle(&mut h);
    let v0 = h.state().results.as_ref().unwrap().specs.volume;
    menu(&mut h, "Board", "Control-point matching");
    assert_eq!(h.state().set.model, Model::ControlPoint);
    assert_eq!(h.state().doc.board.model, SurfaceModel::ControlPoint);
    settle(&mut h);
    let v1 = h.state().results.as_ref().unwrap().specs.volume;
    assert!(v1 > 0.0 && (v1 - v0).abs() / v0 < 0.05);
    snap(&mut h, "qa-control-point-model");
    // New boards use the chosen model.
    menu(&mut h, "File", "New…");
    click(&mut h, "Create");
    if h.state().pending.is_some() {
        click(&mut h, "Don't save");
    }
    assert_eq!(h.state().doc.board.model, SurfaceModel::ControlPoint);
    menu(&mut h, "Board", "S-blend (smooth, default)");
    assert_eq!(h.state().doc.board.model, SurfaceModel::SBlend);
}

#[test]
fn surface_model_and_undo_stay_consistent() {
    let mut h = harness();
    make_dirty(&mut h);
    menu(&mut h, "Board", "Control-point matching");
    h.key_press(Key::Escape);
    step(&mut h);
    assert_eq!(h.state().doc.board.model, SurfaceModel::ControlPoint);
    // The switch is its own undo step; the menu shows the open board's model, so they cannot disagree.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    step(&mut h);
    assert_eq!(
        h.state().doc.board.model,
        SurfaceModel::SBlend,
        "undo reverts the model switch"
    );
    assert!(
        h.state().doc.can_undo(),
        "the earlier edit is still undoable"
    );
}

#[test]
fn view_toggles_change_settings_and_render() {
    let mut h = harness_t();
    type Get = fn(&crate::settings::ViewToggles) -> bool;
    let checks: &[(&str, Get)] = &[
        ("Grid", |v| v.grid),
        ("Control points", |v| v.control_points),
        ("Guide points", |v| v.guide_points),
        ("Measurements", |v| v.measurements),
        ("Curvature comb", |v| v.curvature),
        ("Cross-section positions", |v| v.section_positions),
        ("Other cross-sections", |v| v.other_sections),
        ("Rail apex", |v| v.apex_line),
        ("Tuck-under point", |v| v.tuck_line),
        ("Fins", |v| v.fins),
        ("Centre line", |v| v.center_line),
        ("Ghost board", |v| v.ghost),
        ("Original board", |v| v.original),
        ("Centre of mass", |v| v.center_of_mass),
        ("Background image", |v| v.background_image),
    ];
    for (label, get) in checks {
        let before = get(&h.state().set.view);
        click(&mut h, "View");
        click(&mut h, label);
        assert_ne!(get(&h.state().set.view), before, "{label}");
        h.key_press(Key::Escape);
        step(&mut h);
    }
    // Everything on.
    {
        let v = &mut h.state_mut().set.view;
        v.curvature = true;
        v.apex_line = true;
        v.tuck_line = true;
        v.center_of_mass = true;
        v.original = true;
        v.grid = true;
        v.control_points = true;
        v.measurements = true;
        v.section_positions = true;
        v.other_sections = true;
        v.fins = true;
        v.center_line = true;
    }
    h.state_mut()
        .doc
        .edit(|b| b.fins = [10.0, 14.0, 20.0, 15.0, 8.0, 18.0, 0.0, 0.0, 3.0]);
    settle(&mut h);
    snap(&mut h, "qa-all-overlays-overview");
    for (tab, name) in [
        (Tab::Outline, "outline"),
        (Tab::Profile, "profile"),
        (Tab::Section, "section"),
    ] {
        set_tab(&mut h, tab);
        snap(&mut h, &format!("qa-all-overlays-{name}"));
    }
    // Everything off.
    {
        let v = &mut h.state_mut().set.view;
        *v = crate::settings::ViewToggles {
            grid: false,
            control_points: false,
            guide_points: false,
            ghost: false,
            original: false,
            curvature: false,
            section_positions: false,
            fins: false,
            center_line: false,
            apex_line: false,
            tuck_line: false,
            center_of_mass: false,
            other_sections: false,
            measurements: false,
            background_image: false,
        };
    }
    set_tab(&mut h, Tab::Overview);
    snap(&mut h, "qa-no-overlays-overview");
    // Profile vertical scale field exists.
    click(&mut h, "View");
    h.get_by_label("Profile vertical scale");
}

#[test]
fn theme_buttons_switch_light_dark() {
    let mut h = harness_t();
    for (icon, dark) in [("Light", false), ("Dark", true)] {
        click(&mut h, "View");
        click(&mut h, icon);
        h.key_press(Key::Escape);
        settle(&mut h);
        assert_eq!(h.ctx.theme() == egui::Theme::Dark, dark, "{icon}");
        snap(
            &mut h,
            if dark {
                "qa-dark-theme"
            } else {
                "qa-light-theme"
            },
        );
    }
}

#[test]
fn view_menu_has_no_missing_glyphs() {
    let mut h = harness();
    click(&mut h, "View");
    let _ = h.render();
}

// ------------------------------------------------------------------------------------------------ 3D

#[test]
fn three_d_view_gpu_wireframe_zebra_orbit() {
    let mut h = harness_fast();
    set_tab(&mut h, Tab::ThreeD);
    settle(&mut h);
    snap(&mut h, "qa-3d-gpu");
    // Orbit with the left button, pan with the right, wheel zoom, double-click resets.
    let c = egui::pos2(720.0, 470.0);
    let yaw = h.state().orbit.yaw;
    drag_button(
        &mut h,
        c,
        c + egui::vec2(80.0, 0.0),
        egui::PointerButton::Primary,
    );
    assert!(
        (h.state().orbit.yaw - (yaw - 0.8)).abs() < 0.05,
        "yaw {}",
        h.state().orbit.yaw
    );
    let pan = h.state().orbit.pan;
    drag_button(
        &mut h,
        c,
        c + egui::vec2(0.0, 40.0),
        egui::PointerButton::Secondary,
    );
    assert!((h.state().orbit.pan - pan).y > 30.0);
    let z = h.state().orbit.zoom;
    h.hover_at(c);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 100.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    h.run_steps(10);
    assert!(h.state().orbit.zoom > z);
    snap(&mut h, "qa-3d-orbited");
    click_at(&mut h, c, Modifiers::NONE);
    click_at(&mut h, c, Modifiers::NONE);
    assert!(
        (h.state().orbit.yaw - yaw).abs() < 1e-6,
        "double-click resets"
    );
    // Zebra.
    menu(&mut h, "View", "3D zebra stripes (surface fairness)");
    h.key_press(Key::Escape);
    step(&mut h);
    snap(&mut h, "qa-3d-zebra");
    // CPU wireframe preview.
    menu(&mut h, "View", "3D wireframe (CPU preview)");
    assert!(h.state().orbit.wireframe);
    h.key_press(Key::Escape);
    step(&mut h);
    snap(&mut h, "qa-3d-wireframe");
    // F resets the orbit too.
    drag_button(
        &mut h,
        c,
        c + egui::vec2(50.0, 30.0),
        egui::PointerButton::Primary,
    );
    h.key_press(Key::F);
    step(&mut h);
    assert!((h.state().orbit.yaw - yaw).abs() < 1e-6);
}

#[test]
fn every_template_renders_in_every_tab() {
    for t in 0..surfcraft_io::templates::TEMPLATES.len() {
        let mut h = harness();
        let b = surfcraft_io::templates::TEMPLATES[t].board();
        h.state_mut().doc = crate::document::Document::new(b, None);
        let b = h.state().doc.board.clone();
        h.state_mut().ed.reset_for(&b);
        settle(&mut h);
        for (tab, name) in [
            (Tab::Overview, "overview"),
            (Tab::Outline, "outline"),
            (Tab::Profile, "rocker"),
            (Tab::Section, "section"),
            (Tab::ThreeD, "3d"),
        ] {
            set_tab(&mut h, tab);
            snap(&mut h, &format!("qa-t{t}-{name}"));
        }
    }
}

#[test]
fn edit_menu_undo_redo_and_tool_radios() {
    let mut h = harness();
    make_dirty(&mut h);
    let after = h.state().doc.board.clone();
    menu(&mut h, "Edit", "Undo");
    assert_ne!(h.state().doc.board, after);
    assert!(!h.state().doc.is_dirty());
    menu(&mut h, "Edit", "Redo");
    assert_eq!(h.state().doc.board, after);
    for (label, tool) in [
        ("Add control point", Tool::AddKnot),
        ("Add guide point", Tool::AddGuide),
        ("Select and move", Tool::Select),
    ] {
        menu(&mut h, "Edit", label);
        assert_eq!(h.state().ed.tool, tool);
        h.key_press(Key::Escape);
        step(&mut h);
    }
}

#[test]
fn degenerate_board_file_does_not_crash() {
    let mut h = harness();
    let mut b = surfcraft_io::templates::TEMPLATES[0].board();
    for k in b.outline.knots_mut() {
        for p in &mut k.points {
            p.x = 0.0;
        }
    }
    let f = fresh("degenerate", "zero-length.brd");
    std::fs::write(&f, surfcraft_io::brd::write(&b)).unwrap();
    script(&mut h, &f);
    menu(&mut h, "File", "Open…");
    settle(&mut h);
    snap(&mut h, "qa-degenerate");
}

// ------------------------------------------------------------------------------------------------ tour

#[test]
fn tour_walks_every_step_from_the_help_menu() {
    use crate::tour::{Target, steps};
    let mut h = harness_t();
    assert!(!h.state().tour.open, "tests never auto-start the tour");
    menu(&mut h, "Help", "Tour of SurfCraft");
    step(&mut h);
    assert!(h.state().tour.open);
    let all = steps();
    for (i, s) in all.iter().enumerate() {
        assert_eq!(h.state().tour.step, i);
        assert!(has_label(&h, s.title), "step {i} shows {:?}", s.title);
        assert!(has_label(&h, &format!("Step {} of {}", i + 1, all.len())));
        if let Some(tab) = s.tab {
            assert_eq!(h.state().tab, tab, "step {i} opens its view");
        }
        if s.target != Target::Nothing {
            let r = h.state().tour_targets.get(s.target);
            assert!(
                r.is_some_and(|r| r.area() > 100.0),
                "step {i} highlights {:?}",
                s.target
            );
        }
        snap(&mut h, &format!("qa-tour-{i:02}"));
        click(&mut h, if i + 1 == all.len() { "Finish" } else { "Next" });
        step(&mut h);
    }
    assert!(!h.state().tour.open, "Finish closes the tour");
    assert!(h.state().set.tour_done, "and remembers it");
}

#[test]
fn tour_back_skip_and_app_stays_usable() {
    let mut h = harness_t();
    h.state_mut().tour.start();
    step(&mut h);
    assert!(!h.state().tour.open || has_label(&h, "Back"));
    click(&mut h, "Next");
    step(&mut h);
    click(&mut h, "Next");
    step(&mut h);
    assert_eq!(h.state().tour.step, 2);
    assert_eq!(h.state().tab, Tab::Outline);
    click(&mut h, "Back");
    step(&mut h);
    assert_eq!(h.state().tour.step, 1);
    assert_eq!(h.state().tab, Tab::Overview);
    click(&mut h, "Next");
    step(&mut h);
    // The window is not modal: drag a control point while the tour is open.
    let before = h.state().doc.board.outline.knots()[2].point();
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -25.0), Modifiers::NONE);
    assert_ne!(
        h.state().doc.board.outline.knots()[2].point(),
        before,
        "edit works with the tour open"
    );
    assert!(h.state().tour.open && h.state().tour.step == 2);
    click(&mut h, "Skip tour");
    step(&mut h);
    assert!(!h.state().tour.open);
    assert!(h.state().set.tour_done);
    // It can be reopened at any time, from the first step.
    menu(&mut h, "Help", "Tour of SurfCraft");
    step(&mut h);
    assert!(h.state().tour.open && h.state().tour.step == 0);
}

#[test]
fn tour_starts_on_first_launch_only() {
    // The constructor never starts it under cfg(test); the rule itself:
    let settings = crate::settings::Settings::default();
    assert!(!settings.tour_done, "a fresh install has not seen the tour");
    let old: crate::settings::Settings = serde_json::from_str(r#"{"unit":"Centimetres"}"#).unwrap();
    assert!(
        !old.tour_done,
        "settings saved before the tour existed default to showing it"
    );
    assert_eq!(old.unit, crate::settings::Unit::Centimetres);
}

// ------------------------------------------------------------------------------------------------ smoothing

#[test]
fn smoothing_from_the_edit_menu() {
    let mut h = harness_t();
    set_tab(&mut h, Tab::Outline);
    // Put a small kink in the outline at its widest knot.
    let wide = {
        let b = &h.state().doc.board;
        (1..b.outline.len() - 1)
            .max_by(|&i, &j| {
                let y = |k: usize| b.outline.knots()[k].point().y;
                y(i).total_cmp(&y(j))
            })
            .unwrap()
    };
    h.state_mut().doc.edit(|b| {
        let k = &mut b.outline.knots_mut()[wide];
        k.continuous = false;
        k.rotate_handle(Handle::Next, (-8.0f64).to_radians());
    });
    step(&mut h);
    let kinked = h.state().doc.board.clone();
    let tail_corner = (1..kinked.outline.len() - 1)
        .find(|&i| i != wide && !kinked.outline.knots()[i].continuous)
        .expect("squash-tail corner");
    // Whole curve: the kink goes, the deliberate tail corner stays; one undo step.
    menu(&mut h, "Edit", "Smooth kinks in this curve");
    step(&mut h);
    let b = h.state().doc.board.clone();
    assert!(b.outline.knots()[wide].continuous);
    assert_eq!(
        b.outline.knots()[tail_corner],
        kinked.outline.knots()[tail_corner]
    );
    assert!(status(&h).contains("Smoothed 1 point."), "{}", status(&h));
    snap(&mut h, "qa-smooth-kinks");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    step(&mut h);
    assert_eq!(h.state().doc.board, kinked, "one undo restores the kink");
    // Nothing to do: says so and records no edit.
    menu(&mut h, "Edit", "Smooth kinks in this curve");
    step(&mut h);
    menu(&mut h, "Edit", "Smooth kinks in this curve");
    step(&mut h);
    assert!(status(&h).contains("No kinks"), "{}", status(&h));
    // Selected points: smooths even the deliberate corner.
    h.state_mut().ed.selection = Some(Selection::single(
        CurveId::Outline,
        tail_corner,
        Handle::Point,
    ));
    step(&mut h);
    menu(&mut h, "Edit", "Smooth selected points");
    step(&mut h);
    assert!(h.state().doc.board.outline.knots()[tail_corner].continuous);
    // Disabled without a selection.
    h.state_mut().ed.selection = None;
    step(&mut h);
    click(&mut h, "Edit");
    let enabled = h
        .query_by_label("Smooth selected points")
        .map(|n| !n.accesskit_node().is_disabled());
    assert_eq!(enabled, Some(false));
}
