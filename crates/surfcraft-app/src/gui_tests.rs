//! GUI tests: drive the real app headlessly with egui_kittest (clicks, drags, keys) and save screenshots.
//!
//! Screenshots go to `target/gui-shots/<name>.png` (override with `SURFCRAFT_GUI_SHOTS`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use surfcraft_core::{DVec2, Handle};

use crate::Args;
use crate::app::{SurfCraftApp, Tab};

pub(crate) type H = Harness<'static, SurfCraftApp>;

pub(crate) fn harness() -> H {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_max_steps(200)
        .wgpu()
        .build_eframe(|cc| SurfCraftApp::new(cc, Args::default()));
    h.state_mut().scripted_dialogs = Some(Default::default());
    settle(&mut h);
    h
}

/// Step until the background worker has caught up with the current board.
pub(crate) fn settle(h: &mut H) {
    let start = Instant::now();
    loop {
        h.run_steps(2);
        let app = h.state();
        let current = app
            .results
            .as_ref()
            .is_some_and(|r| r.revision == app.doc.revision);
        if current && !app.worker.is_busy() {
            h.run_steps(2);
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "background worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn shot_dir() -> PathBuf {
    std::env::var_os("SURFCRAFT_GUI_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/gui-shots"))
}

/// Render the window and save it as PNG.
pub(crate) fn snap(h: &mut H, name: &str) {
    // Software GPUs on CI (Windows WARP) can time out on huge scenes such as a 290k-move toolpath preview;
    // the screenshot is then skipped. Any other render error still fails.
    let img = match h.render() {
        Ok(img) => img,
        Err(e) if format!("{e:?}").contains("timed out") => {
            eprintln!("screenshot {name} skipped: {e:?}");
            return;
        }
        Err(e) => panic!("render: {e:?}"),
    };
    let dir = shot_dir();
    std::fs::create_dir_all(&dir).unwrap();
    img.save(dir.join(format!("{name}.png"))).unwrap();
}

pub(crate) fn set_tab(h: &mut H, tab: Tab) {
    h.state_mut().tab = tab;
    h.state_mut().ed.fit_all();
    h.run_steps(3);
}

#[test]
fn views_stay_centred_across_tabs_and_resizes() {
    let mut h = harness();
    // Fitted in the overview's small cell; then shown full size in its own tab without an explicit fit.
    let centre_err = |h: &H| {
        let app = h.state();
        let v = &app.ed.outline_view;
        let mid = DVec2::new(app.doc.board.length() / 2.0, 0.0);
        (v.xf.to_screen(mid) - v.rect.unwrap().center()).length()
    };
    for tab in [Tab::Outline, Tab::Overview, Tab::Outline] {
        h.state_mut().tab = tab;
        h.run_steps(3);
        assert!(centre_err(&h) < 1.0, "outline centred in {tab:?}");
    }
    let section_err = |h: &H| {
        let v = &h.state().ed.section_view;
        (v.xf.to_screen(DVec2::ZERO).x - v.rect.unwrap().center().x).abs()
    };
    h.state_mut().tab = Tab::Section;
    h.run_steps(3);
    assert!(section_err(&h) < 1.0, "section centred");
    // After a zoom the user's framing is kept but follows the view's centre.
    h.state_mut().tab = Tab::Outline;
    h.run_steps(3);
    let v = h.state().ed.outline_view;
    let at_centre = v.xf.to_world(v.rect.unwrap().center());
    let c = v.rect.unwrap().center();
    h.state_mut().ed.outline_view.xf.zoom_about(c, 2.0);
    h.state_mut().ed.outline_view.user_moved = true;
    h.state_mut().tab = Tab::Overview;
    h.run_steps(3);
    let v = h.state().ed.outline_view;
    let now = v.xf.to_world(v.rect.unwrap().center());
    assert!(
        (now - at_centre).length() < 0.5,
        "zoomed view keeps its centre"
    );
    assert!(v.user_moved);
    // Resizing the window re-fits an untouched view.
    h.state_mut().ed.fit_all();
    h.state_mut().tab = Tab::Outline;
    h.run_steps(3);
    h.set_size(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    assert!(centre_err(&h) < 1.0, "centred after resize");
}

/// Screen position of a knot handle in the outline view.
pub(crate) fn outline_screen(h: &H, knot: usize, which: Handle) -> Pos2 {
    let app = h.state();
    let p = app.doc.board.outline.knots()[knot].get(which);
    app.ed.outline_view.xf.to_screen(p)
}

pub(crate) fn drag(h: &mut H, from: Pos2, to: Pos2, modifiers: Modifiers) {
    h.hover_at(from);
    h.run_steps(1);
    h.event_modifiers(
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        },
        modifiers,
    );
    h.run_steps(1);
    let steps = 8;
    for i in 1..=steps {
        let p = from + (to - from) * (i as f32 / steps as f32);
        h.event_modifiers(egui::Event::PointerMoved(p), modifiers);
        h.run_steps(1);
    }
    h.event_modifiers(
        egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        },
        modifiers,
    );
    h.run_steps(2);
}

pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gui-tmp")
        .join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Specs panel volume of the default board (the built-in shortboard), as computed by the reference program
/// (BoardCAD 3.0.1, its golden file).
pub(crate) fn default_volume_label() -> String {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/shortboard.golden.json"
    ))
    .unwrap();
    let cm3 = golden["slinear"]["volume"].as_f64().unwrap();
    format!("{:.2} L", cm3 / 1000.0)
}

#[test]
fn launches_with_template_and_specs() {
    let mut h = harness();
    h.get_by_label(&default_volume_label());
    // The shortboard template is 72" long.
    h.get_by_label("6' 0\"");
    snap(&mut h, "overview");
}

#[test]
fn drag_outline_point_then_undo() {
    let mut h = harness();
    set_tab(&mut h, Tab::Outline);
    let before = h.state().doc.board.clone();
    let width_before = before.max_width();
    // Knot 2 is the wide point; drag it outward (up on screen = wider).
    let from = outline_screen(&h, 2, Handle::Point);
    drag(&mut h, from, from + egui::vec2(0.0, -15.0), Modifiers::NONE);
    settle(&mut h);
    let after = h.state().doc.board.max_width();
    assert!(
        after > width_before + 0.5,
        "width {width_before} -> {after}"
    );
    snap(&mut h, "outline-dragged");
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().doc.board, before, "undo restores the board");
    h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), egui::Key::Z);
    h.run_steps(2);
    assert!(
        (h.state().doc.board.max_width() - after).abs() < 1e-9,
        "redo"
    );
}

#[test]
fn export_spec_sheet_through_menu() {
    let mut h = harness();
    let out = temp_dir("export").join("sheet.pdf");
    let _ = std::fs::remove_file(&out);
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(out.clone());
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label_contains("Print & export").click();
    h.run_steps(2);
    h.get_by_label("Spec sheet (one page)").click();
    h.run_steps(2);
    snap(&mut h, "export-dialog");
    h.get_by_label("Export…").click();
    h.run_steps(2);
    let bytes = std::fs::read(&out).expect("spec sheet written");
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn weight_calculator_and_guide_points() {
    let mut h = harness();
    h.get_by_label("Board").click();
    h.run_steps(2);
    h.get_by_label("Weight calculator…").click();
    settle(&mut h);
    h.run_steps(3);
    h.get_by_label("Total");
    let total = h.state().dialogs.weight.as_ref().map(|_| ());
    assert!(total.is_some());
    snap(&mut h, "weight-calculator");

    let before = h.state().doc.board.outline_guides.len();
    h.get_by_label("Board").click();
    h.run_steps(2);
    h.get_by_label("Guide points…").click();
    h.run_steps(2);
    h.get_by_label("Add guide point").click();
    h.run_steps(2);
    assert_eq!(h.state().doc.board.outline_guides.len(), before + 1);
    snap(&mut h, "guide-points");
}

/// Drive the live window through the automation bridge the MCP server uses.
#[test]
fn automation_bridge_drives_the_window() {
    use serde_json::json;
    use surfcraft_session::automation::{Client, discover};

    let file = temp_dir("automation").join("endpoint.json");
    // SAFETY: the only test that sets this variable.
    unsafe { std::env::set_var("SURFCRAFT_AUTOMATION_FILE", &file) };
    let mut h = harness();
    h.state_mut().set.allow_automation = true; // as the Help menu checkbox does
    h.run_steps(2);
    let before = h.state().doc.board.clone();
    let ep = discover().expect("endpoint file published");

    let client = std::thread::spawn(move || {
        let mut c = Client::connect(&ep).unwrap();
        let mut out = Vec::new();
        out.push(c.call("status", &json!({})).unwrap());
        out.push(
            c.call(
                "scale_board",
                &json!({"length": "6'4\"", "volume_litres": 29.0}),
            )
            .unwrap(),
        );
        out.push(c.call("set_view", &json!({"view": "outline"})).unwrap());
        out.push(c.call("screenshot", &json!({})).unwrap());
        out.push(c.call("undo", &json!({})).unwrap());
        out.push(json!(
            c.call("set_view", &json!({"view": "banana"})).is_err()
        ));
        out
    });
    let start = Instant::now();
    while !client.is_finished() {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "bridge timed out"
        );
    }
    let out = client.join().unwrap();
    assert_eq!(out[0]["unsaved_changes"], false);
    assert!((out[1]["volume_litres"].as_f64().unwrap() - 29.0).abs() < 0.1);
    assert!(out[3]["png_base64"].as_str().unwrap().len() > 1000);
    assert_eq!(out[4]["undone"], true);
    assert_eq!(out[5], true);
    assert_eq!(h.state().doc.board, before, "undo restored the board");
    assert_eq!(h.state().tab, Tab::Outline);
    h.state_mut().set.allow_automation = false;
    h.run_steps(2);
    assert!(h.state().automation.is_none());
    assert!(discover().is_none(), "endpoint file withdrawn");
}

#[test]
fn machine_window_generates_and_saves_gcode() {
    let mut h = harness();
    h.get_by_label("Machine").click();
    h.run_steps(2);
    h.get_by_label("Machine (CAM)…").click();
    h.run_steps(3);
    h.get_by_label("Generate deck").click();
    let start = Instant::now();
    loop {
        h.run_steps(1);
        let st = &h.state().dialogs.cam_state;
        if st.program.is_some() || st.error.is_some() {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "toolpath generation timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        h.state().dialogs.cam_state.error.is_none(),
        "{:?}",
        h.state().dialogs.cam_state.error
    );
    h.run_steps(3);
    snap(&mut h, "machine-deck");
    let out = temp_dir("cam").join("deck.nc");
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(out.clone());
    h.get_by_label("Save G-code…").click();
    h.run_steps(2);
    let g = std::fs::read_to_string(&out).expect("G-code written");
    assert!(g.contains("G21") && g.contains("M2"));
}

/// Every interface language: menus, side panel, views and dialogs lay out without missing glyphs (the harness panics
/// on tofu). Screenshots: `lang-<code>-<what>.png`.
#[test]
fn every_language_renders_menus_and_dialogs() {
    use crate::app::Pending;
    use crate::dialogs::{NewBoard, Scale};
    use crate::edit::{CurveId, Selection};
    use crate::i18n::{self, Language, tr};

    let mut h = harness();
    let v = &mut h.state_mut().set.view;
    (v.apex_line, v.tuck_line, v.center_of_mass) = (true, true, true);
    for lang in Language::ALL {
        h.state_mut().set.language = Some(lang);
        h.run_steps(2);
        assert_eq!(i18n::language(), lang, "language applies on the next frame");
        let shot = |h: &mut H, what: &str| snap(h, &format!("lang-{}-{what}", lang.code()));

        for menu in ["File", "View", "Board", "Help"] {
            h.get_by_label(tr(menu)).click();
            h.run_steps(2);
            shot(&mut h, &menu.to_lowercase());
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
        }
        // Escape cleared the selection; select two knots so the side panel shows the knot inspector.
        h.state_mut().ed.selection = Some(Selection {
            curve: CurveId::Outline,
            knots: [1, 2].into(),
            handle: Handle::Point,
        });
        h.run_steps(2);
        shot(&mut h, "main");

        type Open = fn(&mut SurfCraftApp);
        let dialogs: [(&str, Open); 10] = [
            ("new-board", |a| a.dialogs.new_board = Some(NewBoard::new())),
            ("scale", |a| a.dialogs.scale = Some(Scale::of(&a.doc.board))),
            ("info", |a| a.dialogs.info = true),
            ("fins", |a| a.dialogs.fins = true),
            ("weight", |a| {
                a.dialogs.weight = Some(crate::tools::WeightState::new(&a.doc.board))
            }),
            ("guides", |a| a.dialogs.guides = true),
            ("export", |a| a.dialogs.export = true),
            ("cam", |a| a.dialogs.cam = true),
            ("about", |a| a.dialogs.about = true),
            ("unsaved", |a| a.pending = Some(Pending::Quit)),
        ];
        for (name, open) in dialogs {
            open(h.state_mut());
            h.run_steps(3);
            shot(&mut h, name);
            let app = h.state_mut();
            let d = &mut app.dialogs;
            (d.new_board, d.scale, d.weight) = (None, None, None);
            (d.info, d.fins, d.guides, d.export, d.cam, d.about) =
                (false, false, false, false, false, false);
            app.pending = None;
            h.run_steps(2);
        }
        set_tab(&mut h, Tab::Section);
        shot(&mut h, "section");
        set_tab(&mut h, Tab::Overview);
        // Every tour step (long translated texts must wrap inside the window).
        h.state_mut().tour.start();
        for i in 0..crate::tour::steps().len() {
            h.state_mut().tour.step = i;
            h.run_steps(2);
            if matches!(i, 0 | 6) {
                shot(&mut h, &format!("tour-{i}"));
            }
        }
        h.state_mut().tour.open = false;
        h.run_steps(2);
    }
    h.state_mut().set.language = Some(Language::En);
    h.run_steps(2);
    h.get_by_label("File");
}

#[allow(dead_code)]
pub(crate) fn world(p: DVec2) -> DVec2 {
    p
}

#[test]
fn new_board_on_an_unedited_board_shows_its_own_specs() {
    // Found while recording the "Your first board" tutorial: both documents started at revision 0, so the
    // worker's cached results for the template were shown as the new board's specs.
    let mut h = harness();
    let mut b = h.state().doc.board.clone();
    let (l, w, t) = (b.length(), b.max_width(), b.max_thickness());
    b.scale(l, w, t * 0.8);
    let litres = b.volume() / 1000.0;
    h.state_mut().replace_document(b, None);
    settle(&mut h);
    let shown = h.state().results.as_ref().unwrap().specs.volume / 1000.0;
    assert!(
        (shown - litres).abs() < 0.05,
        "specs {shown} L, board {litres} L"
    );
}
