//! Fin setups on the test boards: preset placement, the p60 line and the legacy p50 marks.

mod common;

use common::{BOARDS, FIXTURES, fixture_board};
use surfcraft_core::fins::{BoxSystem, FinSetup, Preset, Side};
use surfcraft_core::units::INCH;
use surfcraft_io::brd;
use surfcraft_io::templates::TEMPLATES;

fn boards() -> Vec<(String, surfcraft_core::Board)> {
    let mut v: Vec<_> = BOARDS
        .iter()
        .map(|n| (n.to_string(), fixture_board(n)))
        .collect();
    v.extend(TEMPLATES.iter().map(|t| (t.name.to_string(), t.board())));
    v
}

#[test]
fn presets_fit_every_board() {
    for (name, b) in boards() {
        for preset in Preset::ALL {
            for system in BoxSystem::ALL {
                let s = FinSetup::preset(preset, system, &b);
                let what = format!("{name} {preset:?} {system:?}");
                let expected = match preset {
                    Preset::Single => 1,
                    Preset::Twin => 2,
                    Preset::TwinTrailer | Preset::Thruster | Preset::TwoPlusOne => 3,
                    Preset::Quad => 4,
                    Preset::Five => 5,
                    Preset::Custom => 0,
                };
                assert_eq!(s.fins.len(), expected, "{what}");
                for (i, f) in s.fins.iter().enumerate() {
                    // Base and box inside the outline, clear of the tail.
                    let mut pts = vec![f.rear(), f.front()];
                    pts.extend(f.footprint().into_iter().flatten());
                    for p in pts {
                        assert!(
                            p.x > 0.0 && p.x < b.length(),
                            "{what}: fin {i} at x {}",
                            p.x
                        );
                        assert!(
                            p.y.abs() < b.outline.value_at(p.x),
                            "{what}: fin {i} point {p} outside the outline"
                        );
                    }
                    // Sides mirrored.
                    if f.side() != Side::Centre {
                        let j = s
                            .partner(i)
                            .unwrap_or_else(|| panic!("{what}: fin {i} alone"));
                        assert_eq!(s.fins[j], f.mirrored(), "{what}");
                        assert!((f.from_rail(&b) - 1.25 * INCH).abs() < 1e-9, "{what}");
                        assert!(f.toe > 0.0 && f.toe < 4.0, "{what}");
                    }
                }
                for f in &s.fins {
                    let want = match (f.side(), system) {
                        (Side::Centre, _) if preset == Preset::TwoPlusOne => BoxSystem::UsBox,
                        (Side::Left | Side::Right, BoxSystem::UsBox) => BoxSystem::None,
                        _ => system,
                    };
                    assert_eq!(f.system, want, "{what}");
                }
            }
        }
    }
}

#[test]
fn five_fins_have_quad_rears_behind_the_thruster_sides() {
    for (name, b) in boards() {
        let s = FinSetup::preset(Preset::Five, BoxSystem::FcsII, &b);
        let left: Vec<_> = s.fins.iter().filter(|f| f.side() == Side::Left).collect();
        let right: Vec<_> = s.fins.iter().filter(|f| f.side() == Side::Right).collect();
        let centre: Vec<_> = s.fins.iter().filter(|f| f.side() == Side::Centre).collect();
        assert_eq!((left.len(), right.len(), centre.len()), (2, 2, 1), "{name}");
        let (front, rear) = if left[0].x > left[1].x {
            (left[0], left[1])
        } else {
            (left[1], left[0])
        };
        assert!(rear.front().x < front.rear().x, "{name}: boxes overlap");
        assert!(
            centre[0].x < rear.x,
            "{name}: centre fin ahead of the rears"
        );
        // The thruster positions are the thruster preset's, the rears the quad preset's.
        let t = FinSetup::preset(Preset::Thruster, BoxSystem::FcsII, &b);
        let q = FinSetup::preset(Preset::Quad, BoxSystem::FcsII, &b);
        assert!(
            t.fins.contains(front) && t.fins.contains(centre[0]),
            "{name}"
        );
        assert!(q.fins.contains(rear), "{name}");
    }
}

#[test]
fn shortboard_thruster_matches_the_guides() {
    // 6'0" shortboard: sides 11" from the tail, centre 3 1/2", 1/4" toe over the base (docs/fins.md).
    let b = fixture_board("shortboard");
    assert!((b.length() - 72.0 * INCH).abs() < 1e-9);
    let s = FinSetup::preset(Preset::Thruster, BoxSystem::FcsII, &b);
    let side = s.fins.iter().find(|f| f.side() == Side::Left).unwrap();
    let centre = s.fins.iter().find(|f| f.side() == Side::Centre).unwrap();
    assert!((side.x - 11.0 * INCH).abs() < 1e-9);
    assert!((centre.x - 3.5 * INCH).abs() < 1e-9);
    assert!((side.toe_offset() - 0.25 * INCH).abs() < 1e-9);
}

#[test]
fn p60_round_trips_and_keeps_p50_in_step() {
    for (name, mut b) in boards() {
        for preset in Preset::ALL {
            let s = FinSetup::preset(preset, BoxSystem::Futures, &b);
            b.set_fin_setup(s.clone());
            let text = brd::write(&b);
            assert_eq!(text.matches("\np60 : ").count(), 1, "{name}");
            let back = brd::read(text.as_bytes()).unwrap();
            assert_eq!(back.fin_setup.as_ref(), Some(&s), "{name} {preset:?}");
            assert_eq!(back.fins, s.to_legacy(), "{name} {preset:?}");
            assert_eq!(brd::write(&back), text, "{name}: not idempotent");
        }
    }
}

#[test]
fn old_files_load_and_save_unchanged() {
    for name in BOARDS {
        let bytes = std::fs::read(format!("{FIXTURES}/{name}.brd")).unwrap();
        let b = brd::read(&bytes).unwrap();
        assert_eq!(b.fin_setup, None, "{name}");
        assert!(!brd::write(&b).contains("p60"), "{name}");
    }
    // Legacy marks only: the setup is derived from them, nothing new is written.
    let mut b = fixture_board("shortboard");
    b.fins = [27.94, 21.0, 39.0, 20.4, 7.6, 18.0, 11.0, 11.5, 6.0];
    let text = brd::write(&b);
    assert!(!text.contains("p60"));
    let back = brd::read(text.as_bytes()).unwrap();
    let s = back.fin_setup();
    assert_eq!(
        (s.preset, s.fins.len(), s.system),
        (Preset::Thruster, 3, BoxSystem::None)
    );
    for (a, w) in s.to_legacy().iter().zip(b.fins) {
        assert!((a - w).abs() < 1e-9);
    }
}

#[test]
fn unreadable_p60_falls_back_to_the_legacy_marks() {
    let b = fixture_board("shortboard");
    let text = brd::write(&b).replace("p50 : ", "p60 : 9 garbage\np50 : ");
    let back = brd::read(text.as_bytes()).unwrap();
    assert_eq!(back.fin_setup, None);
}

#[test]
fn scaling_and_flipping_keep_the_marks_in_step() {
    let mut b = fixture_board("shortboard");
    b.set_fin_setup(FinSetup::preset(Preset::Quad, BoxSystem::FcsI, &b));
    b.scale_fins(1.1, 0.9);
    assert_eq!(b.fins, b.fin_setup.as_ref().unwrap().to_legacy());
    let before = b.fin_setup.clone().unwrap();
    b.flip();
    assert_eq!(b.fins, b.fin_setup.as_ref().unwrap().to_legacy());
    b.flip();
    let after = b.fin_setup.clone().unwrap();
    for (f, g) in before.fins.iter().zip(&after.fins) {
        assert!(f.rear().distance(g.rear()) < 1e-9 && (f.toe - g.toe).abs() < 1e-9);
    }
}
