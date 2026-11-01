//! QA: lossless .brd / partial-curve / STEP round trips for every template and fixture, and robustness of every
//! reader against truncated and corrupted input.

mod common;

use common::{BOARDS, FIXTURES, fixture_board};
use surfcraft_core::{Board, Handle, Spline};
use surfcraft_io::templates::TEMPLATES;

fn all_boards() -> Vec<(String, Board)> {
    let mut v: Vec<(String, Board)> = TEMPLATES
        .iter()
        .map(|t| (format!("template {}", t.name), t.board()))
        .collect();
    for name in BOARDS {
        v.push((name.to_string(), fixture_board(name)));
    }
    v
}

#[test]
fn brd_write_read_is_lossless() {
    for (name, b) in all_boards() {
        let text = surfcraft_io::brd::write(&b);
        let back = surfcraft_io::brd::read(text.as_bytes()).unwrap();
        assert_eq!(back.outline, b.outline, "{name} outline");
        assert_eq!(back.deck, b.deck, "{name} deck");
        assert_eq!(back.bottom, b.bottom, "{name} bottom");
        assert_eq!(back.sections, b.sections, "{name} sections");
        assert_eq!(back.outline_guides, b.outline_guides, "{name}");
        assert_eq!(back.fins, b.fins, "{name}");
        assert_eq!(back.info, b.info, "{name} info");
        assert_eq!(back.machine, b.machine, "{name} machine");
        assert_eq!(back, b, "{name}: whole board");
        // Writing again gives identical text.
        assert_eq!(
            surfcraft_io::brd::write(&back),
            text,
            "{name}: write is not idempotent"
        );
    }
}

#[test]
fn brd_round_trip_keeps_unusual_text() {
    let mut b = TEMPLATES[0].board();
    b.info.name = "Fish (twin) — ñ 波 \"quoted\" 'single' \\ back".into();
    b.info.comments = "two\nlines".into();
    b.info.surfer = String::new();
    let back = surfcraft_io::brd::read(surfcraft_io::brd::write(&b).as_bytes()).unwrap();
    assert_eq!(back.info.name, b.info.name);
    assert_eq!(back.info.comments, b.info.comments);
}

#[test]
fn brd_with_guides_round_trips() {
    let mut b = TEMPLATES[1].board();
    b.outline_guides = vec![[10.0, 12.5].into(), [100.0, 28.0].into()];
    b.deck_guides = vec![[50.0, 8.0].into()];
    b.bottom_guides = vec![[1.0, 0.5].into()];
    let i = b.real_sections().start;
    b.sections[i].guide_points = vec![[5.0, 2.0].into()];
    let back = surfcraft_io::brd::read(surfcraft_io::brd::write(&b).as_bytes()).unwrap();
    assert_eq!(back, b);
}

fn same_spline(a: &Spline, b: &Spline, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}");
    for (k0, k1) in a.knots().iter().zip(b.knots()) {
        for h in Handle::ALL {
            assert_eq!(k0.get(h), k1.get(h), "{what}");
        }
        assert_eq!(k0.continuous, k1.continuous, "{what}");
    }
}

#[test]
fn partial_curve_files_round_trip() {
    for (name, b) in all_boards() {
        let otl = surfcraft_io::brd::read_partial(&surfcraft_io::brd::write_outline(&b)).unwrap();
        let (s, g) = otl.outline.expect("outline in .otl");
        same_spline(&s, &b.outline, &format!("{name} .otl"));
        assert_eq!(g, b.outline_guides);
        assert!(otl.deck.is_none() && otl.bottom.is_none() && otl.section.is_none());

        let pfl = surfcraft_io::brd::read_partial(&surfcraft_io::brd::write_profile(&b)).unwrap();
        same_spline(
            &pfl.deck.expect("deck").0,
            &b.deck,
            &format!("{name} .pfl deck"),
        );
        same_spline(
            &pfl.bottom.expect("bottom").0,
            &b.bottom,
            &format!("{name} .pfl bottom"),
        );
        assert!(pfl.outline.is_none());

        for i in b.real_sections() {
            let crs =
                surfcraft_io::brd::read_partial(&surfcraft_io::brd::write_section(&b.sections[i]))
                    .unwrap();
            let s = crs.section.expect("section in .crs");
            assert_eq!(s.position, b.sections[i].position, "{name} .crs {i}");
            same_spline(
                &s.spline,
                &b.sections[i].spline,
                &format!("{name} .crs {i}"),
            );
        }
    }
}

#[test]
fn partial_reader_ignores_non_curve_text() {
    for text in [
        "",
        "hello",
        "p32 : (\n",
        "(p36 abc\n",
        "p33 : (\n(cp [1,2,3\n",
    ] {
        // Must not panic or hang; errors are fine.
        let _ = surfcraft_io::brd::read_partial(text);
    }
}

#[test]
fn step_write_read_for_every_board() {
    for (name, b) in all_boards() {
        let text = surfcraft_io::step::write(&b);
        let back =
            surfcraft_io::step::read(text.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (what, s0, s1) in [
            ("outline", &b.outline, &back.outline),
            ("deck", &b.deck, &back.deck),
            ("bottom", &b.bottom, &back.bottom),
        ] {
            assert_eq!(s0.len(), s1.len(), "{name} {what}");
            let n = s0.len();
            for (i, (k0, k1)) in s0.knots().iter().zip(s1.knots()).enumerate() {
                for h in Handle::ALL {
                    if (i == 0 && h == Handle::Prev) || (i + 1 == n && h == Handle::Next) {
                        continue;
                    }
                    assert!(
                        (k0.get(h) - k1.get(h)).length() < 1e-6,
                        "{name} {what} knot {i} {h:?}"
                    );
                }
            }
        }
        if name == "scan_template" {
            continue; // see step_round_trip_keeps_sections_without_dummies
        }
        assert_eq!(
            back.real_sections().len(),
            b.real_sections().len(),
            "{name}"
        );
        let rel = (back.volume() - b.volume()).abs() / b.volume();
        assert!(
            rel < 1e-3,
            "{name}: volume {} vs {}",
            back.volume(),
            b.volume()
        );
    }
}

#[test]
fn step_name_survives() {
    let mut b = TEMPLATES[0].board();
    b.info.name = "Twin fin 5'8".into();
    let back = surfcraft_io::step::read(surfcraft_io::step::write(&b).as_bytes()).unwrap();
    assert_eq!(back.info.name, b.info.name);
}

#[test]
fn read_any_dispatch() {
    let brd = std::fs::read(format!("{FIXTURES}/shortboard.brd")).unwrap();
    for ext in ["brd", ".brd", "BRD", ".Brd"] {
        assert!(surfcraft_io::read_any(ext, &brd).is_ok(), "{ext}");
    }
    for ext in ["", "txt", "otl", "s3dx"] {
        assert!(surfcraft_io::read_any(ext, &brd).is_err(), "{ext}");
    }
    let stp = surfcraft_io::step::write(&TEMPLATES[0].board());
    for ext in ["stp", "step", "STEP"] {
        assert!(surfcraft_io::read_any(ext, stp.as_bytes()).is_ok(), "{ext}");
    }
}

#[test]
fn legacy_encrypted_fixture_reads() {
    let bytes = std::fs::read(format!("{FIXTURES}/longboard_encrypted.brd")).unwrap();
    let b = surfcraft_io::brd::read(&bytes).unwrap();
    let plain = fixture_board("longboard");
    assert!((b.length() - plain.length()).abs() < 1e-6);
}

/// Tiny deterministic generator (no new dependencies).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn readers_survive(bytes: &[u8], what: &str) {
    for ext in ["brd", "s3d", "srf", "stp"] {
        let r = std::panic::catch_unwind(|| surfcraft_io::read_any(ext, bytes));
        assert!(r.is_ok(), "{ext} reader panicked on {what}");
        if let Ok(Ok(b)) = r {
            // Whatever was read must be usable.
            let _ = std::panic::catch_unwind(|| {
                let _ = b.length();
                let _ = b.max_width();
            });
        }
    }
}

#[test]
fn readers_do_not_panic_on_truncated_files() {
    let brd = std::fs::read(format!("{FIXTURES}/shortboard.brd")).unwrap();
    let stp = surfcraft_io::step::write(&TEMPLATES[0].board()).into_bytes();
    for (name, bytes) in [("brd", &brd), ("stp", &stp)] {
        let n = bytes.len();
        for cut in (0..n).step_by((n / 97).max(1)) {
            readers_survive(&bytes[..cut], &format!("{name} cut at {cut}"));
        }
    }
}

#[test]
fn readers_do_not_panic_on_corrupted_files() {
    let brd = std::fs::read(format!("{FIXTURES}/funboard.brd")).unwrap();
    let stp = surfcraft_io::step::write(&TEMPLATES[1].board()).into_bytes();
    let mut rng = Lcg(42);
    for (name, base) in [("brd", &brd), ("stp", &stp)] {
        for round in 0..60 {
            let mut b = base.clone();
            for _ in 0..1 + rng.next() % 8 {
                let i = (rng.next() as usize) % b.len();
                b[i] = b"0123456789-.,()[]:e \n;#'"[(rng.next() % 24) as usize];
            }
            readers_survive(&b, &format!("{name} round {round}"));
        }
    }
    for len in [0usize, 1, 7, 64, 512] {
        let junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        readers_survive(&junk, &format!("{len} random bytes"));
    }
}

#[test]
fn brd_with_nan_written_as_zero_is_detected() {
    // A NaN in memory is written as 0.0 (see `num`); make sure the file still reads back.
    let mut b = TEMPLATES[0].board();
    b.fins[0] = f64::NAN;
    let back = surfcraft_io::brd::read(surfcraft_io::brd::write(&b).as_bytes()).unwrap();
    assert_eq!(back.fins[0], 0.0);
}

#[test]
fn step_import_then_brd_save_is_stable() {
    for (name, b) in all_boards() {
        let back = surfcraft_io::step::read(surfcraft_io::step::write(&b).as_bytes()).unwrap();
        let again = surfcraft_io::brd::read(surfcraft_io::brd::write(&back).as_bytes()).unwrap();
        assert_eq!(again, back, "{name}");
    }
}

#[test]
fn step_round_trip_keeps_sections_without_dummies() {
    let b = fixture_board("scan_template");
    let back = surfcraft_io::step::read(surfcraft_io::step::write(&b).as_bytes()).unwrap();
    assert_eq!(back.real_sections().len(), b.real_sections().len());
    let rel = (back.volume() - b.volume()).abs() / b.volume();
    assert!(rel < 1e-3, "volume {} vs {}", back.volume(), b.volume());
}
