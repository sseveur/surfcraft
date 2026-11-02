//! Tutorial 6: "Asymmetric boards": Board > Asymmetric outline, which rail is drawn where, editing one rail, the
//! per-rail widths in the specs, the 3D board, fins placed from each rail, going back to symmetric, undo.
//!
//! Record: `cargo test -p surfcraft-app --release demo_asymmetric -- --ignored --nocapture`
//! Output: `target/videos/asymmetric/asymmetric.mp4` (+ `.srt`) and the screenshots of
//! `docs/tutorials/06-asymmetric.md` in `docs/tutorials/img/`.

use eframe::egui::{self, Pos2};
use surfcraft_core::{DVec2, Handle, Rail};

use super::overview::GITHUB;
use super::recorder::{Recorder, repo_root};
use crate::edit::CurveId;

/// Screenshot width for the written tutorial.
const STILL_WIDTH: u32 = 1280;

fn still(rec: &mut Recorder, name: &str) {
    let p = repo_root().join("docs/tutorials/img").join(name);
    rec.still(&p, STILL_WIDTH);
}

/// Screen position of a knot (or handle) in the Outline view: the left rail above the stringer, the right rail of
/// an asymmetric board below it.
fn rail_knot(rec: &Recorder, curve: CurveId, i: usize, which: Handle) -> Pos2 {
    let app = rec.app();
    let b = &app.doc.board;
    let p = curve.spline(b).expect("curve").knots()[i].get(which);
    let sign = curve.rail(b).map_or(1.0, Rail::sign);
    app.ed
        .outline_view
        .xf
        .to_screen(DVec2::new(p.x, p.y * sign))
}

fn half_width(rec: &Recorder, rail: Rail) -> f64 {
    rec.app().doc.board.max_half_width(rail).0
}

#[test]
#[ignore = "records a video (minutes); run with --ignored"]
fn demo_asymmetric() {
    let mut rec = Recorder::new("asymmetric");
    rec.app_mut().set.snap.enabled = false;

    rec.narrate(
        "Asymmetric boards. Your heel-side and toe-side turns are different, \
         so some shapers give each rail its own outline.",
    );
    rec.title_card("Asymmetric boards", "SurfCraft tutorial 6", 5.0);

    // ---------------------------------------------------------------------------------------- turn on
    rec.narrate(
        "Open the Outline view, then tick Board, Asymmetric outline. \
         The second rail starts as a copy, so the board does not change yet.",
    );
    rec.click("Outline");
    rec.hold(0.3);
    let symmetric = rec.app().doc.board.clone();
    rec.click("Board");
    rec.move_to("Asymmetric outline");
    rec.hold(0.8);
    still(&mut rec, "asym-menu.png");
    rec.click("Asymmetric outline");
    rec.hold(0.6);
    {
        let b = &rec.app().doc.board;
        assert!(b.is_asymmetric());
        assert_eq!(b.outline_left.as_ref(), Some(&symmetric.outline));
        assert!((b.volume() - symmetric.volume()).abs() < 1e-6);
    }

    rec.narrate(
        "Which rail is which? Stand on the deck, facing the nose. \
         Your left rail is drawn above the stringer, your right rail below it.",
    );
    let top = rail_knot(&rec, CurveId::OutlineLeft, 2, Handle::Point);
    let bottom = rail_knot(&rec, CurveId::Outline, 2, Handle::Point);
    assert!(top.y < bottom.y, "left rail above the stringer");
    rec.move_to(top);
    rec.hold(1.0);
    rec.move_to(bottom);
    rec.hold(1.0);
    still(&mut rec, "asym-rails.png");

    // ---------------------------------------------------------------------------------------- one rail
    rec.narrate(
        "For a regular-foot surfer the toes point at the right rail, so the left rail is the heel side. \
         Pull its tail in for a tighter backside turn.",
    );
    let right_before = rec.app().doc.board.outline.clone();
    let tail = rail_knot(&rec, CurveId::OutlineLeft, 1, Handle::Point);
    let w0 = half_width(&rec, Rail::Left);
    let tail0 = rec.app().doc.board.half_width_at(30.0, Rail::Left);
    rec.drag(tail, tail + egui::vec2(0.0, 22.0), 1.4);
    rec.hold(0.4);
    let tail1 = rec.app().doc.board.half_width_at(30.0, Rail::Left);
    assert!(tail1 < tail0 - 0.5, "heel-side tail {tail0} -> {tail1}");
    assert_eq!(
        rec.app().doc.board.outline,
        right_before,
        "the right rail does not move"
    );

    rec.narrate("And draw the heel-side wide point in a little. Only that rail moves.");
    let wide = rail_knot(&rec, CurveId::OutlineLeft, 2, Handle::Point);
    rec.drag(wide, wide + egui::vec2(0.0, 9.0), 1.2);
    rec.hold(0.5);
    assert!(half_width(&rec, Rail::Left) < w0 - 0.3);
    assert_eq!(rec.app().doc.board.outline, right_before);
    still(&mut rec, "asym-heel-side.png");

    // ---------------------------------------------------------------------------------------- specs
    rec.narrate(
        "The specs panel now gives each rail's own half width and where it is, \
         next to the total width. The volume adds the two halves.",
    );
    rec.move_to(rec.rect("Left half width").left_center() + egui::vec2(-4.0, 0.0));
    rec.hold(0.8);
    rec.move_to_in(
        rec.rect("Right half width").left_center() + egui::vec2(-4.0, 0.0),
        0.8,
    );
    rec.hold(0.6);
    rec.move_to_in(
        rec.rect("Volume").left_center() + egui::vec2(-4.0, 0.0),
        1.0,
    );
    assert!(half_width(&rec, Rail::Right) > half_width(&rec, Rail::Left) + 0.3);
    assert!(rec.app().doc.board.volume() < symmetric.volume());
    rec.hold(0.6);

    // ---------------------------------------------------------------------------------------- fins
    rec.narrate(
        "Board, Fins: apply a thruster. Each side fin is placed from its own rail, \
         so the two sit at different distances from the stringer.",
    );
    rec.click("Board");
    rec.click("Fins…");
    rec.click("Setup");
    rec.click("Thruster");
    rec.click("Apply preset");
    rec.hold(0.8);
    {
        let b = &rec.app().doc.board;
        let fins = &b.fin_setup.as_ref().expect("fins").fins;
        assert_eq!(fins.len(), 3);
        let left = fins.iter().find(|f| f.y > 0.1).expect("left fin");
        let right = fins.iter().find(|f| f.y < -0.1).expect("right fin");
        // Same distance from each rail, so different offsets from the stringer.
        let from_rail = |f: &surfcraft_core::Fin| b.half_width_at(f.x, Rail::of_y(f.y)) - f.y.abs();
        assert!((from_rail(left) - from_rail(right)).abs() < 0.05);
        assert!(
            right.y.abs() > left.y.abs() + 0.1,
            "{} vs {}",
            left.y,
            right.y
        );
    }
    rec.close_window("Fins");
    rec.hold(0.4);
    // Zoom in on the stringer between the side fins, keeping the tail in view.
    let fins_at = {
        let app = rec.app();
        let setup = app.doc.board.fin_setup.as_ref().unwrap();
        let x = setup.fins.iter().map(|f| f.x).fold(0.0, f64::max);
        app.ed.outline_view.xf.to_screen(DVec2::new(x, 0.0))
    };
    rec.move_to(fins_at + egui::vec2(-40.0, 0.0));
    rec.wheel(90.0, 1.0);
    rec.hold(1.0);
    still(&mut rec, "asym-fins.png");
    rec.key(egui::Modifiers::NONE, egui::Key::F, Some("F"));
    rec.hold(0.3);

    // ---------------------------------------------------------------------------------------- 3D
    rec.narrate(
        "In 3D, each half of the board follows its own rail. \
         Turn it over to see the fins under the tail.",
    );
    rec.click("3D");
    rec.hold(0.4);
    let c = rec.app().ed.outline_view.rect.unwrap().center();
    // Straight down from above, nose to the right (left rail at the top, as in the Outline view), then over to
    // the bottom.
    rec.drag(c, c + egui::vec2(-60.0, 105.0), 1.4);
    assert!(
        rec.app().orbit.yaw.abs() < 0.12 && rec.app().orbit.pitch > 1.3,
        "top view"
    );
    rec.hold(0.8);
    still(&mut rec, "asym-3d.png");
    rec.drag(c, c + egui::vec2(-40.0, -215.0), 2.0);
    rec.hold(1.2);
    assert!(rec.app().orbit.pitch < -0.3, "bottom view");

    // ---------------------------------------------------------------------------------------- off
    rec.narrate(
        "To go back to a symmetric board, untick Asymmetric outline \
         and choose which rail both sides should take.",
    );
    rec.click("Outline");
    rec.hold(0.3);
    let left = rec.app().doc.board.outline_left.clone().unwrap();
    let right = rec.app().doc.board.outline.clone();
    rec.click("Board");
    rec.click("Asymmetric outline");
    rec.hold(0.5);
    assert!(rec.app().dialogs.asym_keep);
    still(&mut rec, "asym-keep.png");
    rec.click("Keep the right rail (lower half)");
    rec.hold(0.8);
    {
        let b = &rec.app().doc.board;
        assert!(!b.is_asymmetric());
        assert_eq!(b.outline, right);
    }

    rec.narrate("Changed your mind? It is one undo step, and the heel-side rail is back.");
    rec.click("Undo");
    rec.hold(1.0);
    {
        let b = &rec.app().doc.board;
        assert!(b.is_asymmetric());
        assert_eq!(b.outline_left.as_ref(), Some(&left));
    }

    rec.narrate(
        "Asymmetric boards save as normal dot B R D files. \
         BoardCAD opens them too, but sees a symmetric board: the right rail, mirrored.",
    );
    rec.hold(0.5);

    rec.narrate_as(
        &format!("Next: reporting a bug. SurfCraft: {GITHUB}"),
        "That's asymmetric boards. Next time: how to report a bug.",
    );
    rec.end_card("Asymmetric boards", GITHUB);
    let out = rec.finish();
    eprintln!(
        "{} ({:.1} s), {}",
        out.mp4.display(),
        out.seconds,
        out.srt.display()
    );
}
