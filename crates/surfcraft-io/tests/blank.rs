//! Foam blank: the `.brd` extension field p63 and the fit check on the fixture boards.

use surfcraft_core::blank::{self, FitView, Placement, blank_around, fit};
use surfcraft_core::{BlankSetup, Board};
use surfcraft_io::brd;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

fn fixture(name: &str) -> Board {
    brd::read(&std::fs::read(format!("{FIXTURES}/{name}.brd")).unwrap()).unwrap()
}

const BOARDS: [&str; 3] = ["shortboard", "funboard", "longboard"];

#[test]
fn files_without_p63_load_unchanged() {
    for name in BOARDS {
        let b = fixture(name);
        assert!(b.blank.is_none());
        let text = brd::write(&b);
        assert!(!text.contains("p63"), "{name}");
        let again = brd::read(text.as_bytes()).unwrap();
        assert_eq!(again, b, "{name}");
        assert_eq!(brd::write(&again), text);
    }
}

#[test]
fn p63_round_trip() {
    for name in BOARDS {
        let mut b = fixture(name);
        let mut blank = blank::library()[3].clone();
        blank.name = "Odd name = with; \"quotes\" and spaces".into();
        blank.tail_rocker_12 = Some(1.5);
        b.blank = Some(BlankSetup {
            blank,
            placement: Placement {
                tail_offset: Some(2.5),
                tilt: 0.75,
                lift: None,
            },
        });
        let text = brd::write(&b);
        assert_eq!(text.matches("\np63 : ").count(), 1, "{name}: one line");
        let again = brd::read(text.as_bytes()).unwrap();
        assert_eq!(again, b, "{name}");
        assert_eq!(brd::write(&again), text);
    }
}

#[test]
fn unreadable_p63_is_ignored() {
    let text = brd::write(&fixture("shortboard"));
    let text = text.replacen("p01 :", "p63 : garbage\np01 :", 1);
    let b = brd::read(text.as_bytes()).unwrap();
    assert!(b.blank.is_none());
}

#[test]
fn fixture_boards_fit_blanks_made_around_them() {
    for name in BOARDS {
        let b = fixture(name);
        let blank = blank_around(&b, 2.0, 1.5, 1.5);
        let r = fit(&b, &blank, &Placement::default());
        assert!(r.fits, "{name}: {:?}", r.problems);
        assert!(r.min_clearance() > 0.0);
        // Auto fit never does worse than the centred placement.
        let p = blank::auto_fit(&b, &blank);
        let a = fit(&b, &blank, &p);
        assert!(
            a.min_clearance() >= r.min_clearance() - 1e-9,
            "{name}: {} < {}",
            a.min_clearance(),
            r.min_clearance()
        );
    }
}

#[test]
fn fixture_boards_too_big_are_reported_where_they_fail() {
    for name in BOARDS {
        let b = fixture(name);
        let l = b.length();
        // Too short: the length fails at both ends by the missing length / 2.
        let short = blank_around(&b, -3.0, 1.5, 1.5);
        let r = fit(&b, &short, &Placement::default());
        assert!(!r.fits);
        assert!((r.tail_spare + 3.0).abs() < 0.05, "{name} {}", r.tail_spare);
        assert!(r.problems.iter().any(|p| p.view == FitView::Length));
        // Too narrow: the outline fails around the wide point by about the missing half-width.
        let narrow = blank_around(&b, 2.0, -1.0, 1.5);
        let r = fit(&b, &narrow, &Placement::default());
        assert!(!r.fits && r.outline.min < -0.5, "{name}: {}", r.outline.min);
        let wp = b.max_width_pos();
        assert!(
            (r.outline.at - wp).abs() < l * 0.3,
            "{name}: outline fails at {} (wide point {wp})",
            r.outline.at
        );
        // Too thin: deck and bottom fail somewhere along the middle, outline is clear.
        let thin = blank_around(&b, 2.0, 1.5, -1.0);
        let r = fit(&b, &thin, &Placement::default());
        assert!(!r.fits && r.deck.min < 0.0 && r.bottom.min < 0.0, "{name}");
        assert!(r.outline.min > 0.0);
        assert!(
            r.deck.at > l * 0.15 && r.deck.at < l * 0.85,
            "{name}: {}",
            r.deck.at
        );
    }
}
