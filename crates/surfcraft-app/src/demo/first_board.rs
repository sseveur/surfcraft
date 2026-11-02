//! Tutorial 1: "Your first board": a new board from a template at target dimensions with the volume advisor,
//! outline editing (select, drag, Alt fine drag, typed values in feet and inches), smoothing, undo/redo, save.
//!
//! Record: `cargo test -p surfcraft-app --release demo_first_board -- --ignored --nocapture`
//! Output: `target/videos/first-board/first-board.mp4` (+ `.srt`) and the screenshots of
//! `docs/tutorials/01-first-board.md` in `docs/tutorials/img/`.

use std::path::PathBuf;

use eframe::egui::{self, Key, Modifiers, accesskit::Role};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use surfcraft_core::Handle;

use super::overview::GITHUB;
use super::recorder::{Recorder, repo_root};
use crate::app::Tab;
use crate::edit::CurveId;

/// Screenshot width for the written tutorial.
const STILL_WIDTH: u32 = 1280;

fn still(rec: &mut Recorder, name: &str) {
    let p = repo_root().join("docs/tutorials/img").join(name);
    rec.still(&p, STILL_WIDTH);
}

/// Where the tutorial saves its board: a neutral temporary path (it shows in the status bar of the video).
fn save_path() -> PathBuf {
    let dir = if cfg!(unix) {
        PathBuf::from("/tmp/surfcraft-demo")
    } else {
        std::env::temp_dir().join("surfcraft-demo")
    };
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("my-first-board.brd");
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(dir.join("my-first-board.brd.bak"));
    p
}

#[test]
#[ignore = "records a video (minutes); run with --ignored"]
fn demo_first_board() {
    let mut rec = Recorder::new("first-board");

    rec.narrate(
        "Your first board. In two minutes: start from a template at your size, \
         let SurfCraft suggest the volume, then edit the outline.",
    );
    rec.title_card("Your first board", "SurfCraft tutorial 1", 5.0);

    // ---------------------------------------------------------------------------------------- new board
    rec.narrate("File, New. Start from the shortboard template.");
    rec.click("File");
    rec.click("New…");
    let t = &surfcraft_io::templates::TEMPLATES[0];
    rec.click(format!("{} — {}", t.name, t.description).as_str());
    rec.hold(0.4);

    rec.narrate(
        "Type the size you want, in feet and inches: six foot, nineteen and a half, \
         two and seven sixteenths.",
    );
    rec.fill(rec.field_in("New board", "Length"), "6'0\"");
    rec.fill(rec.field_in("New board", "Width"), "19 1/2\"");
    rec.fill(rec.field_in("New board", "Thickness"), "2 7/16\"");

    rec.narrate(
        "Not sure about the volume? Open Suggest volume, enter the rider's weight and level, \
         and use the suggestion.",
    );
    rec.click("Suggest volume");
    rec.hold(0.3);
    // Drag the weight down a little (half a pound per point) from the default 165 lb.
    let weight = rec.right_of("Rider weight", Role::SpinButton).center();
    rec.drag(weight, weight + egui::vec2(-10.0, 0.0), 0.8);
    rec.click(rec.combo("Intermediate"));
    rec.click("Advanced");
    rec.hold(0.5);
    let use_label = rec
        .h
        .query_all_by(|n| {
            n.label()
                .is_some_and(|l| l.starts_with("Use ") && l.ends_with(" L"))
        })
        .map(|n| n.accesskit_node().label().unwrap_or_default())
        .next()
        .expect("Use … L button");
    rec.click(use_label.as_str());
    rec.hold(0.6);
    still(&mut rec, "01-new-board.png");
    let litres: f64 = use_label
        .trim_start_matches("Use ")
        .trim_end_matches(" L")
        .parse()
        .unwrap();
    assert!(
        (24.0..34.0).contains(&litres),
        "a sensible shortboard volume: {litres} L"
    );

    rec.narrate(
        "Press Create. SurfCraft scales the template to your size \
         and adjusts the thickness to hit the volume.",
    );
    rec.click("Create");
    rec.hold(0.6);
    {
        let b = &rec.app().doc.board;
        assert!((b.length() - 72.0 * 2.54).abs() < 0.01);
        assert!((b.volume() / 1000.0 - litres).abs() < 0.1, "volume");
    }
    rec.hold(0.6);

    rec.narrate(&format!(
        "The specs panel confirms it: six foot by nineteen and a half, {litres:.1} litres."
    ));
    rec.move_to(rec.rect("Length").left_center() + egui::vec2(-4.0, 0.0));
    rec.move_to_in(
        rec.rect("Volume").left_center() + egui::vec2(-4.0, 0.0),
        1.6,
    );
    let shown = rec.app().results.as_ref().unwrap().specs.volume / 1000.0;
    assert!((shown - litres).abs() < 0.1, "specs show {shown} L");
    rec.hold(0.8);
    still(&mut rec, "02-created.png");

    // ---------------------------------------------------------------------------------------- select, drag
    rec.narrate(
        "Open the Outline view and click a point to select it. \
         The panel on the right shows where it is.",
    );
    rec.click("Outline");
    rec.hold(0.3);
    rec.click(rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Point));
    rec.hold(0.4);
    rec.move_to(rec.rect("Point").center());
    rec.hold(0.6);
    still(&mut rec, "03-select.png");

    rec.narrate("Scroll to zoom in on the curve, and press F to fit the whole board again.");
    rec.move_to(rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Point) + egui::vec2(0.0, 30.0));
    rec.wheel(160.0, 1.0);
    rec.hold(1.0);
    rec.key(Modifiers::NONE, Key::F, Some("F"));
    rec.hold(0.4);

    rec.narrate("Drag the point to reshape the curve. The width and the volume update as you go.");
    let p = rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Point);
    let w0 = rec.app().doc.board.max_width();
    rec.drag(p, p + egui::vec2(0.0, -10.0), 1.4);
    rec.hold(0.5);
    let coarse = rec.app().doc.board.max_width() - w0;
    assert!(coarse > 0.0);

    rec.narrate(
        "For fine work, hold Alt, or Option on a Mac, while you drag: \
         the point moves ten times less.",
    );
    let p = rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Point);
    let w1 = rec.app().doc.board.max_width();
    rec.badge("Alt / Option", 2.4);
    rec.drag_with(
        egui::PointerButton::Primary,
        Modifiers::ALT,
        p,
        p + egui::vec2(0.0, -10.0),
        1.6,
    );
    rec.hold(0.5);
    let fine = rec.app().doc.board.max_width() - w1;
    assert!(
        fine > 0.0 && fine < coarse / 3.0,
        "fine {fine} coarse {coarse}"
    );

    // ---------------------------------------------------------------------------------------- numbers
    rec.narrate(
        "Or type exact values. Feet and inches, fractions and centimetres all work: \
         here the wide point goes three feet from the tail, nine and seven eighths out.",
    );
    let fields = rec.fields_on_row("Point");
    rec.fill(fields[0].center(), "3'");
    let fields = rec.fields_on_row("Point");
    rec.fill(fields[1].center(), "9 7/8\"");
    rec.move_to(rec.rect("Wide point").center());
    rec.hold(0.8);
    {
        let k = rec.app().doc.board.outline.knots()[2].point();
        assert!((k.x - 36.0 * 2.54).abs() < 1e-6 && (k.y - 9.875 * 2.54).abs() < 1e-6);
    }
    still(&mut rec, "04-numbers.png");

    // ---------------------------------------------------------------------------------------- smooth
    rec.narrate(
        "Untick Continuous and the point can make a corner. Drag a handle, and the curve gets a kink.",
    );
    rec.click("Continuous (smooth)");
    let h = rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Next);
    rec.drag(h, h + egui::vec2(0.0, 26.0), 1.4);
    rec.hold(0.6);
    still(&mut rec, "05-kink.png");

    rec.narrate("Edit, Smooth kinks in this curve makes it fair again.");
    rec.click("Edit");
    rec.click("Smooth kinks in this curve");
    rec.hold(1.0);
    assert!(rec.app().doc.board.outline.knots()[2].continuous);

    // ---------------------------------------------------------------------------------------- undo
    rec.narrate(
        "Every edit is one undo step. Use Undo and Redo in the toolbar, \
         or Control Z and Control Shift Z.",
    );
    let depth = undo_depth(&mut rec);
    rec.click("Undo");
    rec.hold(0.7);
    rec.click("Undo");
    rec.hold(0.7);
    rec.click("Redo");
    rec.hold(0.7);
    assert_eq!(undo_depth(&mut rec), depth - 1);
    rec.key(Modifiers::COMMAND, Key::Z, Some("Ctrl+Z  (Cmd+Z)"));
    rec.hold(0.6);
    rec.key(
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::Z,
        Some("Ctrl+Shift+Z  (Cmd+Shift+Z)"),
    );
    rec.hold(0.6);
    assert_eq!(undo_depth(&mut rec), depth - 1);

    // ---------------------------------------------------------------------------------------- save
    rec.narrate(
        "Save with File, Save. Boards are BoardCAD dot B R D files, \
         so they also open in BoardCAD and on shaping machines that read it.",
    );
    let path = save_path();
    rec.app_mut()
        .scripted_dialogs
        .as_mut()
        .unwrap()
        .push_back(path.clone());
    rec.click("File");
    rec.click("Save");
    rec.hold(1.5);
    assert!(path.is_file(), "board saved");
    assert!(!rec.app().doc.is_dirty());
    still(&mut rec, "06-saved.png");

    rec.narrate_as(
        &format!("Next: rocker and cross-sections. SurfCraft: {GITHUB}"),
        "That's your first board. Next time: rocker and cross-sections.",
    );
    rec.end_card("Your first board", GITHUB);
    let out = rec.finish();
    eprintln!(
        "{} ({:.1} s), {}",
        out.mp4.display(),
        out.seconds,
        out.srt.display()
    );
}

/// Undo steps available (counted by undoing and redoing everything, which leaves the board as it was).
fn undo_depth(rec: &mut Recorder) -> usize {
    let doc = &mut rec.app_mut().doc;
    let mut n = 0;
    while doc.can_undo() {
        doc.undo();
        n += 1;
    }
    for _ in 0..n {
        doc.redo();
    }
    n
}
