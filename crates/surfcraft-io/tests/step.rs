//! STEP writer/reader round trip and parser robustness.

mod common;

use common::*;
use surfcraft_core::{Board, Spline};
use surfcraft_io::step;

#[track_caller]
fn same_spline(what: &str, a: &Spline, b: &Spline) {
    assert_eq!(a.len(), b.len(), "{what}: knot count");
    let n = a.len();
    for (i, (ka, kb)) in a.knots().iter().zip(b.knots()).enumerate() {
        for j in 0..3 {
            // The format has no handle before the first knot or after the last: they read as the point, or as
            // the mirrored inner handle on smooth end knots. Neither affects the curve.
            if (i == 0 && j == 1) || (i == n - 1 && j == 2) {
                assert!(ka.points[j].is_finite());
                continue;
            }
            let d = (ka.points[j] - kb.points[j]).length();
            assert!(d < 1e-6, "{what} knot {i} point {j}: off by {d:e}");
        }
        assert_eq!(ka.continuous, kb.continuous, "{what} knot {i} continuous");
        assert_eq!(ka.other, kb.other, "{what} knot {i} other");
    }
}

#[track_caller]
fn same_points(what: &str, a: &[surfcraft_core::DVec2], b: &[surfcraft_core::DVec2]) {
    assert_eq!(a.len(), b.len(), "{what}");
    for (p, q) in a.iter().zip(b) {
        assert!((*p - *q).length() < 1e-6, "{what}");
    }
}

fn assert_round_trip(name: &str, board: &Board) {
    let text = step::write(board);
    keep("step", &format!("{name}.stp"), text.as_bytes());
    let got = step::read(text.as_bytes()).unwrap();
    same_spline(&format!("{name} outline"), &got.outline, &board.outline);
    same_spline(&format!("{name} bottom"), &got.bottom, &board.bottom);
    same_spline(&format!("{name} deck"), &got.deck, &board.deck);
    same_points(
        &format!("{name} outline guides"),
        &got.outline_guides,
        &board.outline_guides,
    );
    same_points(
        &format!("{name} bottom guides"),
        &got.bottom_guides,
        &board.bottom_guides,
    );
    same_points(
        &format!("{name} deck guides"),
        &got.deck_guides,
        &board.deck_guides,
    );
    // The zero-size end sections are added when missing (scan_template has none).
    let real = |b: &Board| -> Vec<surfcraft_core::CrossSection> {
        b.sections
            .iter()
            .filter(|s| s.spline.len() > 1)
            .cloned()
            .collect()
    };
    let (got_real, want_real) = (real(&got), real(board));
    assert_eq!(got_real.len(), want_real.len(), "{name} sections");
    // Sections come back exactly as written (no end sections added), like .brd.
    assert_eq!(
        got.sections.len(),
        board.sections.len(),
        "{name} section count"
    );
    for (a, b) in got_real.iter().zip(&want_real) {
        let what = format!("{name} section {}", b.position);
        assert!((a.position - b.position).abs() < 1e-6, "{what}");
        same_spline(&what, &a.spline, &b.spline);
        same_points(&what, &a.guide_points, &b.guide_points);
    }
    assert_eq!(got.info.name, board.info.name);
}

#[test]
fn write_read_round_trip() {
    for name in BOARDS {
        let mut board = fixture_board(name);
        // Exercise every guide kind and quoting.
        board.info.name = format!("{name} 'q'");
        board
            .deck_guides
            .push(surfcraft_core::DVec2::new(50.0, 7.0));
        board.sections[2]
            .guide_points
            .push(surfcraft_core::DVec2::new(3.0, 1.0));
        assert_round_trip(name, &board);
        // Geometry measures follow.
        let got = step::read(step::write(&board).as_bytes()).unwrap();
        let has_ends = board.sections[0].spline.len() == 1;
        assert_same_shape(name, &board, &got, 1e-6, if has_ends { 1e-9 } else { 0.02 });
    }
}

/// Written by the reference program (BoardCAD 3.0.1) from `shortboard.brd`.
#[test]
fn reads_file_written_by_the_reference_program() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/shortboard_bcstep.stp"
    ))
    .unwrap();
    let got = step::read(&bytes).unwrap();
    let board = fixture_board("shortboard");
    same_spline("outline", &got.outline, &board.outline);
    same_spline("bottom", &got.bottom, &board.bottom);
    same_spline("deck", &got.deck, &board.deck);
    assert_eq!(got.sections.len(), board.sections.len());
    for (a, b) in got.sections.iter().zip(&board.sections) {
        assert!((a.position - b.position).abs() < 1e-9);
        same_spline("section", &a.spline, &b.spline);
    }
    assert_same_shape("shortboard", &board, &got, 1e-9, 1e-9);
}

#[test]
fn file_has_surfcraft_structure() {
    let board = fixture_board("shortboard");
    let text = step::write(&board);
    assert!(text.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(text.trim_end().ends_with("ENDSEC;\nEND-ISO-10303-21;"));
    assert!(text.contains("'BCstep 1.0','BoardCAD'"));
    // The reference program expects each CARTESIAN_POINT and BEZIER_CURVE on one line.
    for line in text.lines() {
        if line.contains("CARTESIAN_POINT") || line.contains("BEZIER_CURVE") {
            assert!(line.ends_with(';'), "{line}");
        }
    }
    // One curve per section, the zero-size end sections included.
    assert_eq!(
        text.matches("BEZIER_CURVE( 'section'").count(),
        board.sections.len()
    );
    assert!(text.contains("REPRESENTATION_ITEM('deck')"));
    assert!(text.contains("REPRESENTATION_ITEM('bottom')"));
    // STEP reals always have a decimal point.
    assert!(!text.contains("e-"));
}

/// Hand-written file in the SurfCraft layout but with free formatting: entities across lines, comments,
/// complex entities, 2D points, no end sections.
const LOOSE: &str = "ISO-10303-21;
HEADER; FILE_NAME('a;b','',(''),(''),'','',''); ENDSEC;
DATA;
/* comment with BEZIER_CURVE('outline' ... ; */
#1 = CARTESIAN_POINT('false false', (0., 0., 0.));
#2=CARTESIAN_POINT('NONE',(100.,0.,50.));
#3 = CARTESIAN_POINT ( 'NONE' , ( 1000.0 , 0.0 ,
   50.0 ) ) ;
#4 = CARTESIAN_POINT('false false', (1000.0, 0.0, 0.0));
#5 = BEZIER_CURVE('outline', 3,
  (#1, #2,
   #3, #4), .UNSPECIFIED., .F., .F.);
#6 = CARTESIAN_POINT('true false', (0.0, 30.0));
#7 = CARTESIAN_POINT('NONE', (500.0, 0.0));
#8 = CARTESIAN_POINT('NONE', (500.0, 0.0));
#9 = CARTESIAN_POINT('true false', (1000.0, 30.0));
#10 = BEZIER_CURVE('bottom',3,(#6,#7,#8,#9),.UNSPECIFIED.,.F.,.F.);
#11 = CARTESIAN_POINT('true false', (0.0, 30.0));
#12 = CARTESIAN_POINT('NONE', (500.0, 80.0));
#13 = CARTESIAN_POINT('NONE', (500.0, 80.0));
#14 = CARTESIAN_POINT('true false', (1000.0, 30.0));
#15 = BEZIER_CURVE('deck',3,(#11,#12,#13,#14),.UNSPECIFIED.,.F.,.F.);
#16 = CARTESIAN_POINT('false false', (500.0, 0.0, 0.0));
#17 = CARTESIAN_POINT('NONE', (500.0, 0.0, 200.0));
#18 = CARTESIAN_POINT('NONE', (500.0, 0.0, 250.0));
#19 = CARTESIAN_POINT('false false', (500.0, 20.0, 250.0));
#20 = CARTESIAN_POINT('NONE', (500.0, 50.0, 250.0));
#21 = CARTESIAN_POINT('NONE', (500.0, 50.0, 200.0));
#22 = CARTESIAN_POINT('false false', (500.0, 50.0, 0.0));
#23 = BEZIER_CURVE('section',3,(#16,#17,#18,#19,#20,#21,#22),.UNSPECIFIED.,.F.,.F.);
#24 = CARTESIAN_POINT('Cross Section 0 Guide Point', (0.0, 10.0, 100.0));
#25 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );
#26 = PRODUCT('It''s a board', '', '');
#27 = UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-2),#25,'a','b');
ENDSEC;
END-ISO-10303-21;
";

#[test]
fn parses_free_formatting() {
    let board = step::read(LOOSE.as_bytes()).unwrap();
    assert_eq!(board.outline.len(), 2);
    assert_eq!(
        board.outline.knots()[0].points[2],
        surfcraft_core::DVec2::new(10.0, 5.0)
    );
    assert_eq!(board.length(), 100.0);
    assert_eq!(board.sections.len(), 3);
    let s = &board.sections[1];
    assert_eq!(s.position, 50.0);
    assert_eq!(s.spline.len(), 3);
    assert_eq!(
        s.spline.knots()[1].point(),
        surfcraft_core::DVec2::new(25.0, 2.0)
    );
    assert_eq!(s.guide_points, [surfcraft_core::DVec2::new(10.0, 1.0)]);
    assert_eq!(board.info.name, "It's a board");
    assert!(surfcraft_io::read_any("STP", LOOSE.as_bytes()).is_ok());
    assert!(surfcraft_io::read_any("step", LOOSE.as_bytes()).is_ok());
}

#[test]
fn malformed_files_are_errors() {
    let text = step::write(&fixture_board("funboard"));
    assert_truncations_dont_panic(text.as_bytes(), step::read);
    let deep = format!("#1 = A({}{});", "(".repeat(100_000), ")".repeat(100_000));
    for bad in [
        "",
        "ISO-10303-21;\nDATA;\nENDSEC;\nEND-ISO-10303-21;",
        "#1 = CARTESIAN_POINT('x', (1.0, 2.0, 3.0))",
        "#1 = CARTESIAN_POINT('x, (1.0, 2.0, 3.0));",
        "#1 = CARTESIAN_POINT('x', (1.0.0, 2.0, 3.0));",
        "#1 = BEZIER_CURVE('outline', 3, (), .U., .F., .F.);",
        "#1 = BEZIER_CURVE('outline', 3, (#2, #3), .U., .F., .F.);",
        "#1 = BEZIER_CURVE('outline', 3, (#9), .U., .F., .F.);",
        "/* unterminated",
        "#x = A();",
        &deep,
    ] {
        assert!(step::read(bad.as_bytes()).is_err(), "{bad:.60}");
    }
    // Curve point counts that are not 3n-2.
    let broken = LOOSE.replace("(#1, #2,\n   #3, #4)", "(#1, #2, #3)");
    assert!(step::read(broken.as_bytes()).is_err());
}
