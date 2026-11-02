//! Video 1: "SurfCraft in 3 minutes", a tour of the main features on the built-in shortboard.
//!
//! Record: `cargo test -p surfcraft-app --release demo_overview -- --ignored --nocapture`
//! Output: `target/videos/overview/overview.mp4` (+ `.srt`, `preview.gif` for the README).

use eframe::egui::{self, Key, Modifiers, accesskit::Role};
use surfcraft_core::Handle;

use super::recorder::Recorder;
use crate::app::Tab;
use crate::edit::CurveId;

/// The project page shown on the end cards.
pub(super) const GITHUB: &str = "github.com/sseveur/surfcraft";

#[test]
#[ignore = "records a video (minutes); run with --ignored"]
fn demo_overview() {
    let mut rec = Recorder::new("overview");
    let board = rec.app().doc.board.clone();

    // ---------------------------------------------------------------------------------------- intro
    rec.narrate(
        "This is SurfCraft: free, open-source surfboard design software for Mac, Windows and Linux.",
    );
    rec.title_card("SurfCraft", "Surfboard design in 3 minutes", 5.5);

    rec.narrate(
        "It opens on a built-in six-foot shortboard. The overview shows the outline, the rocker, \
         a cross-section and the 3D board side by side.",
    );
    let (outline, profile, section) = {
        let ed = &rec.app().ed;
        (
            ed.outline_view.rect.unwrap(),
            ed.profile_view.rect.unwrap(),
            ed.section_view.rect.unwrap(),
        )
    };
    rec.move_to(outline.center());
    rec.hold(0.9);
    rec.move_to(profile.center());
    rec.hold(0.9);
    rec.move_to(section.center());
    rec.hold(0.9);
    rec.move_to(egui::pos2(section.center().x, profile.center().y));

    rec.narrate(
        "On the right, the specs: length, widths, thickness, rocker and volume, always up to date.",
    );
    rec.move_to("Length");
    rec.move_to_in(
        rec.rect("Volume").left_center() + egui::vec2(-4.0, 0.0),
        1.8,
    );

    rec.narrate("Feet and inches or metric: switch the units any time, and every value follows.");
    rec.click(rec.combo("ft-in (fraction)"));
    rec.click("cm");
    rec.hold(1.6);
    rec.click(rec.combo("cm"));
    rec.click("ft-in (fraction)");
    rec.hold(0.4);

    // ---------------------------------------------------------------------------------------- outline
    rec.narrate(
        "Let's shape. In the Outline view, grab the wide point and pull it out. \
         The width and the volume follow as you drag.",
    );
    let shape_at = rec.time();
    rec.click("Outline");
    rec.hold(0.3);
    let wide = rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Point);
    rec.drag(wide, wide + egui::vec2(0.0, -8.0), 1.6);
    rec.move_to(rec.rect("Wide point").center());
    rec.hold(0.5);
    rec.move_to(rec.rect("Volume").center());
    rec.hold(0.6);
    assert!(rec.app().doc.board.max_width() > board.max_width() + 0.5);

    rec.narrate(
        "Prefer numbers? Type the half width in inches, feet or centimetres: \
         ten and a quarter makes the board twenty and a half wide.",
    );
    let fields = rec.fields_on_row("Point");
    rec.fill(fields[1].center(), "10 1/4\"");
    rec.move_to(rec.rect("Wide point").center());
    rec.hold(0.6);
    assert!((rec.app().doc.board.max_width() - 20.5 * 2.54).abs() < 0.05);

    rec.narrate(
        "Untick Continuous and the point can hold a corner. \
         Drag a handle, and you have a kink.",
    );
    // The template's outline has one smooth point: the wide point (2).
    rec.click("Continuous (smooth)");
    let handle = rec.knot(Tab::Outline, CurveId::Outline, 2, Handle::Next);
    rec.drag(handle, handle + egui::vec2(0.0, 24.0), 1.4);
    rec.hold(0.4);
    assert!(!rec.app().doc.board.outline.knots()[2].continuous);

    rec.narrate("Edit, Smooth kinks puts it back to a fair curve, in one undo step.");
    rec.click("Edit");
    rec.click("Smooth kinks in this curve");
    rec.hold(1.0);
    assert!(rec.app().doc.board.outline.knots()[2].continuous);

    rec.narrate(
        "Every change is one undo step. Control Z steps back, Control Shift Z steps forward again.",
    );
    rec.key(Modifiers::COMMAND, Key::Z, Some("Ctrl+Z  (Cmd+Z on a Mac)"));
    rec.hold(1.2);
    assert!(!rec.app().doc.board.outline.knots()[2].continuous);
    rec.key(
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::Z,
        Some("Ctrl+Shift+Z  (Cmd+Shift+Z)"),
    );
    rec.hold(1.2);
    assert!(rec.app().doc.board.outline.knots()[2].continuous);

    // ---------------------------------------------------------------------------------------- rocker
    rec.narrate(
        "In the Rocker view, choose Bottom and lift the nose a touch. \
         The nose rocker in the specs goes up with it.",
    );
    rec.click("Rocker");
    rec.hold(0.3);
    rec.click("Bottom");
    let nose_rocker = rec.app().results.as_ref().unwrap().specs.nose_rocker;
    let last = rec.app().doc.board.bottom.len() - 1;
    let nose = rec.knot(Tab::Profile, CurveId::Bottom, last, Handle::Point);
    rec.drag(nose, nose + egui::vec2(0.0, -10.0), 1.4);
    rec.move_to(rec.rect("Nose rocker").center());
    rec.hold(0.8);
    let now = rec.app().results.as_ref().unwrap().specs.nose_rocker;
    assert!(now > nose_rocker, "nose rocker {nose_rocker} -> {now}");

    // ---------------------------------------------------------------------------------------- sections
    rec.narrate(
        "Cross-sections shape the bottom and the rails. \
         The arrows step through the sections along the board.",
    );
    rec.click("Cross-section");
    rec.hold(0.6);
    rec.click(">");
    rec.hold(0.8);
    rec.click("<");
    rec.hold(0.8);

    rec.narrate(
        "Shape, Bottom contours adds a single concave. Preview draws the result dashed, \
         and Apply makes it one undo step.",
    );
    rec.click("Shape");
    rec.click("Bottom contours…");
    rec.click("Preview");
    rec.hold(1.2);
    let before = rec.app().doc.board.clone();
    rec.click("Apply");
    rec.hold(0.6);
    assert_ne!(rec.app().doc.board, before, "contour applied");
    rec.close_window("Bottom contours");
    rec.hold(1.0);

    rec.narrate(
        "Shape, Rails: pick a sixty-forty rail, preview it, and apply it to the whole board.",
    );
    rec.click("Shape");
    rec.click("Rails…");
    rec.click("60/40");
    rec.click("Preview");
    rec.hold(1.0);
    let before = rec.app().doc.board.clone();
    rec.click("Apply");
    rec.hold(0.6);
    assert_ne!(rec.app().doc.board, before, "rails applied");
    rec.close_window("Rails");
    rec.hold(1.0);

    // ---------------------------------------------------------------------------------------- 3D
    rec.narrate(
        "In 3D, zebra stripes show how fair the surfaces are. \
         Drag to turn the board around, and scroll to zoom.",
    );
    rec.click("3D");
    rec.click("View");
    rec.click("3D zebra stripes (surface fairness)");
    rec.key(Modifiers::NONE, Key::Escape, None);
    let c = outline.center();
    rec.drag(c, c + egui::vec2(90.0, -25.0), 1.8);
    rec.drag(c + egui::vec2(90.0, -25.0), c + egui::vec2(20.0, 5.0), 1.6);
    rec.wheel(40.0, 0.8);
    rec.hold(0.8);

    // ---------------------------------------------------------------------------------------- fins
    rec.narrate("Board, Fins: a five-fin setup in FCS II boxes, placed in one click.");
    rec.click("Board");
    rec.click("Fins…");
    rec.click("Setup");
    rec.click("5 fins");
    rec.click("Box system");
    rec.click("FCS II");
    rec.click("Apply preset");
    rec.hold(1.0);
    assert_eq!(
        rec.app().doc.board.fin_setup.as_ref().map(|s| s.fins.len()),
        Some(5)
    );

    rec.narrate(
        "The boxes show in the outline. Zoom in with the wheel to check them; F fits the view again.",
    );
    rec.close_window("Fins");
    rec.click("Outline");
    rec.hold(0.4);
    let fin = {
        let app = rec.app();
        let f = &app.doc.board.fin_setup.as_ref().unwrap().fins[0];
        app.ed
            .outline_view
            .xf
            .to_screen(surfcraft_core::DVec2::new(f.x, f.y.abs()))
    };
    rec.move_to(fin);
    rec.wheel(200.0, 1.0);
    rec.hold(1.2);
    rec.key(Modifiers::NONE, Key::F, Some("F"));
    rec.hold(0.4);

    // ---------------------------------------------------------------------------------------- blank
    rec.narrate(
        "Board, Fit in blank checks the board against real foam blanks. \
         Pick a US Blanks six-five, press Auto fit, and it fits.",
    );
    rec.click("Board");
    rec.click("Fit in blank…");
    let picker = rec.last_of_role(Role::ComboBox);
    rec.click(picker.center());
    rec.click("6'5\" shortboard (US Blanks 6'5\" R)");
    rec.hold(0.8);
    rec.click("Auto fit");
    rec.hold(0.4);
    assert!(rec.has("Fits"), "the board fits after Auto fit");
    if let Some(r) = rec.try_rect("Fits") {
        rec.move_to(r.center());
    }
    rec.hold(1.0);
    rec.close_window("Fit in blank");
    rec.click("Rocker");
    rec.hold(1.2);

    // ---------------------------------------------------------------------------------------- CAM
    rec.narrate(
        "Machine turns the board into toolpaths for a shaping machine. \
         Generate the deck and check the cuts in the preview.",
    );
    rec.click("Machine");
    rec.click("Machine (CAM)…");
    rec.click("Generate deck");
    rec.wait_for(|app| {
        let st = &app.dialogs.cam_state;
        st.running.is_none() && (st.program.is_some() || st.error.is_some())
    });
    assert!(
        rec.app().dialogs.cam_state.program.is_some(),
        "deck toolpath"
    );
    rec.hold(2.5);
    rec.close_window("Machine (CAM)");

    // ---------------------------------------------------------------------------------------- export
    rec.narrate(
        "File, Print and export makes full-size templates, spec sheets, \
         3D models and files for CNC and laser cutters.",
    );
    rec.click("File");
    rec.click("Print & export…");
    rec.click("Outline template (full, 1:1)");
    rec.hold(0.6);
    rec.click("Paper");
    rec.click("A3");
    rec.hold(0.4);
    rec.click("Spec sheet (one page)");
    rec.hold(0.6);
    rec.click("3D model");
    rec.hold(1.0);
    rec.close_window("Print & export");

    // ---------------------------------------------------------------------------------------- end
    rec.narrate_as(
        &format!("SurfCraft is free and open source: {GITHUB}"),
        "SurfCraft is free and open source. Get it on GitHub. Happy shaping!",
    );
    rec.end_card("SurfCraft", GITHUB);
    let out = rec.finish();
    // README preview: the outline drag and the numeric entry (10 s).
    let gif = rec.preview_gif(&out, shape_at, 10.0, 10);
    eprintln!(
        "{} ({:.1} s), {}",
        out.mp4.display(),
        out.seconds,
        gif.display()
    );
}
