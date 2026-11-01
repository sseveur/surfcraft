//! QA of the Machine (CAM) window, driven through its widgets with egui_kittest: settings fields, holding system,
//! width splits, every job, stats text, preview, G-code export. Screenshots go to `target/gui-shots/cam-*.png`.

use std::time::{Duration, Instant};

use eframe::egui::{self, Modifiers, accesskit::Role};
use egui_kittest::kittest::Queryable as _;
use surfcraft_cam::{GeneratorKind, HoldingSystem, MachineAction};

use crate::cam_ui::Job;
use crate::gui_tests::{H, harness, snap, temp_dir};
use crate::settings::Unit;

fn open_machine(h: &mut H) {
    h.state_mut().set.unit = Unit::Centimetres;
    h.run_steps(1);
    h.get_by_label("Machine").click();
    h.run_steps(2);
    h.get_by_label("Machine (CAM)…").click();
    h.run_steps(3);
    assert!(h.state().dialogs.cam, "machine window open");
}

/// Centre y of the first node with this label.
fn label_y(h: &H, label: &str) -> f32 {
    h.get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no label {label:?}"))
        .rect()
        .center()
        .y
}

/// Set the drag value on the same grid row as `label` (accesskit SetValue, as a screen reader would).
fn set_spin(h: &mut H, label: &str, v: f64) {
    let y = label_y(h, label);
    {
        let node = h
            .get_all_by_role(Role::SpinButton)
            .find(|n| (n.rect().center().y - y).abs() < 6.0)
            .unwrap_or_else(|| panic!("no drag value next to {label:?}"));
        node.set_value_accesskit(v);
    }
    h.run_steps(2);
}

/// Type into the length field on the same row as `label`: focus, select all, type, Enter.
fn set_length(h: &mut H, label: &str, text: &str) {
    let y = label_y(h, label);
    {
        let node = h
            .get_all_by_role(Role::TextInput)
            .find(|n| (n.rect().center().y - y).abs() < 6.0)
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

fn click(h: &mut H, label: &str) {
    h.get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no widget {label:?}"))
        .click_accesskit();
    h.run_steps(2);
}

/// Click a Generate button and wait for the background job.
fn generate(h: &mut H, button: &str) -> Duration {
    let start = Instant::now();
    h.get_by_label(button).click();
    loop {
        h.run_steps(1);
        let st = &h.state().dialogs.cam_state;
        if st.running.is_none() && (st.program.is_some() || st.error.is_some()) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "{button}: timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let took = start.elapsed();
    h.run_steps(3);
    took
}

/// Pixels close to a colour in the last render.
fn count_colour(img: &image::RgbaImage, rgb: [u8; 3]) -> usize {
    img.pixels()
        .filter(|p| (0..3).all(|i| (p.0[i] as i32 - rgb[i] as i32).abs() <= 20))
        .count()
}

fn stats_text(h: &H) -> String {
    let (_, _, p) = h.state().dialogs.cam_state.program.as_ref().unwrap();
    let s = p.stats();
    format!(
        "{} passes · {} moves · {:.1} m cut · about {:.0} min",
        s.passes,
        s.points,
        s.cut_length / 1000.0,
        s.estimated_time / 60.0
    )
}

fn save_gcode(h: &mut H, name: &str) -> String {
    let out = temp_dir("cam-qa").join(name);
    let _ = std::fs::remove_file(&out);
    h.state_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(out.clone());
    h.get_by_label("Save G-code…").click();
    h.run_steps(3);
    let g = std::fs::read_to_string(&out).expect("G-code written");
    assert!(g.lines().any(|l| l == "G21") && g.trim_end().ends_with("M2"));
    assert!(!g.to_ascii_lowercase().contains("nan") && !g.contains("inf"));
    g
}

#[test]
fn cam_settings_through_widgets_then_every_job() {
    let mut h = harness();
    open_machine(&mut h);
    snap(&mut h, "cam-01-open");

    // Cuts, angles and speeds through their drag values.
    set_spin(&mut h, "Deck cuts", 12.0);
    set_spin(&mut h, "Deck rail cuts", 3.0);
    set_spin(&mut h, "Bottom cuts", 10.0);
    set_spin(&mut h, "Deck angle (°)", 30.0);
    set_spin(&mut h, "Cutting", 120.0);
    // Lengths through the unit-aware text fields (cm here).
    set_length(&mut h, "Lengthwise step", "2");
    set_length(&mut h, "Diameter", "3.2");
    set_length(&mut h, "Corner radius", "0.8");
    click(&mut h, "Stay away from stringer");
    {
        let c = &h.state().dialogs.cam_state.cfg;
        assert_eq!((c.deck_cuts, c.deck_rail_cuts, c.bottom_cuts), (12, 3, 10));
        assert_eq!(c.deck_angle, 30.0);
        assert_eq!(c.cutting_speed, 120.0);
        assert!(
            (c.lengthwise_resolution - 2.0).abs() < 1e-9,
            "{}",
            c.lengthwise_resolution
        );
        let b = c.cutter.bullnose().unwrap();
        assert!((b.diameter - 3.2).abs() < 1e-9, "{}", b.diameter);
        assert!((b.corner_radius - 0.8).abs() < 1e-9, "{}", b.corner_radius);
        assert!(!c.stay_away_from_stringer);
    }
    snap(&mut h, "cam-02-settings");

    // Deck with supports.
    let t = generate(&mut h, "Generate deck");
    assert!(
        h.state().dialogs.cam_state.error.is_none(),
        "{:?}",
        h.state().dialogs.cam_state.error
    );
    let text = stats_text(&h);
    h.get_by_label(&text);
    {
        let (job, _, p) = h.state().dialogs.cam_state.program.as_ref().unwrap();
        assert_eq!(*job, Job::Deck);
        // 12 deck + 3 rail per side as set in the window.
        let regular = p
            .toolpath
            .passes
            .iter()
            .filter(|p| p.kind == surfcraft_cam::PassKind::Regular)
            .count();
        assert_eq!(regular, 24);
    }
    let img = h.render().unwrap();
    let blue = count_colour(&img, [90, 170, 255]);
    assert!(
        blue > 2000,
        "regular passes drawn in the preview ({blue} px)"
    );
    snap(&mut h, "cam-03-deck");
    let g = save_gcode(&mut h, "deck.nc");
    assert!(g.contains("F7200"), "120 mm/s cutting speed = F7200");
    assert!(g.contains("diameter 32.0 mm, corner radius 8.0 mm"));
    println!("deck generated in {t:?}: {text}");

    // Bottom with end clamps.
    click(&mut h, "Nose and tail clamps");
    assert!(matches!(
        h.state().dialogs.cam_state.cfg.holding,
        HoldingSystem::EndClamps(_)
    ));
    let t = generate(&mut h, "Generate bottom");
    let text = stats_text(&h);
    h.get_by_label(&text);
    snap(&mut h, "cam-04-bottom-clamps");
    let g = save_gcode(&mut h, "bottom.nc");
    assert!(g.contains("(Holding system: End clamps"));
    println!("bottom generated in {t:?}: {text}");

    // Width splits.
    click(&mut h, "Width splits");
    set_spin(&mut h, "Passes per side", 6.0);
    assert_eq!(
        h.state().dialogs.cam_state.cfg.generator,
        GeneratorKind::WidthSplits { splits: 6 }
    );
    generate(&mut h, "Generate deck");
    {
        let (_, _, p) = h.state().dialogs.cam_state.program.as_ref().unwrap();
        // Centre + 5 per side + the approach.
        assert_eq!(p.toolpath.passes.len(), 12);
    }
    snap(&mut h, "cam-05-width-splits");

    // Hot-wire jobs with kerf and extra material.
    set_spin(&mut h, "Kerf (mm)", 1.5);
    set_length(&mut h, "Extra material", "0.5");
    set_spin(&mut h, "Wire speed (mm/s)", 4.0);
    {
        let hw = &h.state().dialogs.cam_state.hotwire;
        assert_eq!((hw.kerf, hw.speed), (1.5, 4.0));
        assert!((hw.expand - 0.5).abs() < 1e-9);
    }
    generate(&mut h, "Generate hot-wire profile");
    h.get_by_label(&stats_text(&h));
    snap(&mut h, "cam-06-hotwire-profile");
    let g = save_gcode(&mut h, "profile.nc");
    assert!(g.contains("F240"), "4 mm/s wire speed = F240");
    generate(&mut h, "Generate hot-wire outline");
    h.get_by_label(&stats_text(&h));
    snap(&mut h, "cam-07-hotwire-outline");
    save_gcode(&mut h, "outline.nc");

    // Reset restores the machine defaults.
    click(&mut h, "Reset to defaults");
    let c = &h.state().dialogs.cam_state.cfg;
    assert_eq!(
        (c.deck_cuts, c.generator.clone()),
        (28, GeneratorKind::SurfaceSplits)
    );
}

#[test]
fn cam_errors_busy_state_and_stale_warning() {
    let mut h = harness();
    open_machine(&mut h);
    // Nothing to cut: the error is shown in the window, not a panic.
    {
        let c = &mut h.state_mut().dialogs.cam_state.cfg;
        c.deck_cuts = 0;
        c.deck_rail_cuts = 0;
        c.deck_cleanup_cuts = 0;
        c.deck_stringer_cut = false;
        c.deck_outline_cut = false;
    }
    generate(&mut h, "Generate deck");
    h.get_by_label("the settings leave nothing to cut");
    snap(&mut h, "cam-08-error");
    click(&mut h, "Reset to defaults");
    set_length(&mut h, "Lengthwise step", "3");
    // While a job runs the buttons are disabled.
    h.get_by_label("Generate deck").click();
    h.run_steps(1);
    if h.state().dialogs.cam_state.running.is_some() {
        h.get_by_label("Generate bottom").click();
        h.run_steps(1);
        assert_eq!(h.state().dialogs.cam_state.running, Some(Job::Deck));
    }
    let start = Instant::now();
    while h.state().dialogs.cam_state.program.is_none() {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(10));
        assert!(start.elapsed() < Duration::from_secs(60));
    }
    h.run_steps(2);
    // Edit the board: the preview says it is stale.
    let doc = &mut h.state_mut().doc;
    doc.edit(|b| b.scale(200.0, b.max_width(), b.max_thickness()));
    h.run_steps(3);
    h.get_by_label("board changed since: regenerate");
    snap(&mut h, "cam-09-stale");
}

/// The finest resolution the window allows: generation time and frame time with the preview of ~290k moves.
#[test]
fn cam_finest_resolution_performance() {
    let mut h = harness();
    open_machine(&mut h);
    set_length(&mut h, "Lengthwise step", "0.01");
    let res = h.state().dialogs.cam_state.cfg.lengthwise_resolution;
    assert_eq!(res, 0.05, "clamped to 0.05 cm");
    let t = generate(&mut h, "Generate deck");
    let moves = h
        .state()
        .dialogs
        .cam_state
        .program
        .as_ref()
        .unwrap()
        .2
        .stats()
        .points;
    let start = Instant::now();
    h.run_steps(10);
    let frame = start.elapsed() / 10;
    let start = Instant::now();
    // A render timeout on a software GPU (CI) is reported, not failed; see `snap`.
    if let Err(e) = h.render() {
        assert!(format!("{e:?}").contains("timed out"), "render: {e:?}");
    }
    let render = start.elapsed();
    println!(
        "resolution 0.05 cm: {moves} moves, generated in {t:?}, UI step {frame:?} per frame, render {render:?}"
    );
    snap(&mut h, "cam-10-finest");
    assert!(moves > 200_000);
}

/// The "Clear height (mm)" field edits `MachineConfig::clear_height`, which is in centimetres: entering 50 mm gives
/// a 500 mm lift. (The lift-clear action itself is not selectable in the window, so the field also has no effect
/// on programs made there.)
#[test]
fn cam_bug_clear_height_units() {
    let mut h = harness();
    open_machine(&mut h);
    // Now a unit-aware length field: 5 cm must lift 50 mm.
    set_length(&mut h, "Clear height", "5cm");
    set_length(&mut h, "Lengthwise step", "5");
    h.state_mut().dialogs.cam_state.cfg.before_deck_cut = MachineAction::LiftClear;
    generate(&mut h, "Generate deck");
    let (_, _, p) = h.state().dialogs.cam_state.program.as_ref().unwrap();
    let g = p.to_gcode();
    let lift: f64 = g
        .lines()
        .find(|l| l.starts_with("G01 Z"))
        .and_then(|l| l[5..].split(' ').next())
        .and_then(|z| z.parse().ok())
        .unwrap();
    let first_z = p.moves().next().unwrap().pos.z;
    assert!(
        (lift - first_z - 50.0).abs() < 1.0,
        "lift {lift:.1} mm is {:.1} mm above the first point, expected 50",
        lift - first_z
    );
}

/// Shrinking the diameter below twice the corner radius leaves an impossible cutter (corner radius is only clamped
/// when the corner field itself is edited).
#[test]
fn cam_bug_corner_radius_not_reclamped() {
    let mut h = harness();
    open_machine(&mut h);
    set_length(&mut h, "Diameter", "0.6");
    let b = h
        .state()
        .dialogs
        .cam_state
        .cfg
        .cutter
        .bullnose()
        .unwrap()
        .clone();
    assert!(
        b.corner_radius <= b.diameter / 2.0,
        "diameter {} cm with corner radius {} cm",
        b.diameter,
        b.corner_radius
    );
}
