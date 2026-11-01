//! `.s3d` reader against synthetic Shape3D files written from the `.brd` fixtures.

mod common;

use common::*;
use surfcraft_core::{Board, DVec2, Spline};
use surfcraft_io::s3d;

/// Test-only Shape3D writer: exactly the layout `S3dReader` expects.
struct S3d(String);

impl S3d {
    fn point(&mut self, p: [f64; 3]) {
        self.0.push_str(&format!(
            "<Point3d><x>{:?}</x><y>{:?}</y><z>{:?}</z></Point3d>\n",
            p[0], p[1], p[2]
        ));
    }

    fn polygon(&mut self, tag: &str, points: &[[f64; 3]]) {
        self.0.push_str(&format!(
            "<{tag}><Polygone3d>\n<Nb_of_points>{}</Nb_of_points>\n<Symmetry_point>",
            points.len()
        ));
        self.point([0.0; 3]);
        self.0.push_str("</Symmetry_point>\n");
        for &p in points {
            self.point(p);
        }
        self.0.push_str(&format!("</Polygone3d></{tag}>\n"));
    }

    /// `to3d` maps SurfCraft 2D coordinates to the curve's plane.
    fn curve(
        &mut self,
        tag: &str,
        spline: &Spline,
        guides: &[DVec2],
        to3d: impl Fn(DVec2) -> [f64; 3],
    ) {
        let knots = spline.knots();
        self.0
            .push_str(&format!("<{tag}>\n<Bezier3d>\n<Degree>3</Degree>\n"));
        let pts = |i: usize| knots.iter().map(|k| to3d(k.points[i])).collect::<Vec<_>>();
        self.polygon("Control_points", &pts(0));
        self.polygon("Tangents_1", &pts(1));
        self.polygon("Tangents_2", &pts(2));
        for (i, k) in knots.iter().enumerate() {
            let t = if k.continuous { 1 } else { 0 };
            self.0.push_str(&format!(
                "<Tangent_type_point_{i}>{t}</Tangent_type_point_{i}>\n"
            ));
        }
        self.0.push_str(&format!(
            "<Number_of_guides>{}</Number_of_guides>\n",
            guides.len()
        ));
        for &g in guides {
            self.0.push_str("<Guide>");
            self.point(to3d(g));
            self.0.push_str("</Guide>\n");
        }
        self.0.push_str(&format!("</Bezier3d>\n</{tag}>\n"));
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn encode(board: &Board, with_deck: bool) -> String {
    let mut w = S3d(String::new());
    w.0.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Shape3d_design>\n<Version>8</Version>\n<Board>\n");
    w.0.push_str(&format!(
        "<Name>{}</Name>\n<Author>A &amp; B</Author>\n<Comment>{}</Comment>\n",
        escape(&board.info.name),
        "line one\nline two"
    ));
    w.curve("Outline", &board.outline, &board.outline_guides, |p| {
        [p.x, p.y, 0.0]
    });
    w.curve("Bottom", &board.bottom, &board.bottom_guides, |p| {
        [p.x, 0.0, p.y]
    });
    if with_deck {
        // Shape3D's deck stops at the tail and nose thickness; the reader adds the bottom's end points.
        let mut deck = board.deck.knots().to_vec();
        let (b0, b1) = (
            board.bottom.knots()[0].point(),
            board.bottom.knots().last().unwrap().point(),
        );
        if deck[0].point() == b0 {
            deck.remove(0);
        }
        if deck.last().unwrap().point() == b1 {
            deck.pop();
        }
        w.curve("Deck", &Spline::from_knots(deck), &board.deck_guides, |p| {
            [p.x, 0.0, p.y]
        });
    }
    // Shape3D has sections at both tips; the reader moves them to 0.2 cm from the ends.
    let real = &board.sections[board.real_sections()];
    let mut couples = vec![real[0].clone()];
    couples.extend(real.iter().cloned());
    couples.push(real[real.len() - 1].clone());
    for (i, s) in couples.iter().enumerate() {
        // Sections sit at rocker height in Shape3D.
        let (pos, lift) = (s.position, board.rocker_at(s.position));
        w.curve(&format!("Couples_{i}"), &s.spline, &s.guide_points, |p| {
            [pos, p.x, p.y + lift]
        });
    }
    w.0.push_str("</Board>\n</Shape3d_design>\n");
    w.0
}

#[test]
fn reads_synthetic_shape3d_files() {
    for name in BOARDS {
        let board = fixture_board(name);
        let xml = encode(&board, true);
        keep("s3d", &format!("{name}.s3d"), xml.as_bytes());
        let (got, warnings) = s3d::read_with_warnings(xml.as_bytes()).unwrap();
        assert!(warnings.is_empty());
        assert_same_shape(name, &board, &got, 1e-6, 0.02);

        // Outline, bottom and real sections are carried exactly.
        assert_eq!(got.outline.knots().len(), board.outline.len());
        for (a, b) in got.outline.knots().iter().zip(board.outline.knots()) {
            assert_eq!(a.points, b.points);
            assert_eq!(a.continuous, b.continuous);
        }
        assert_eq!(got.sections.len(), board.sections.len() + 2);
        for (a, b) in got.sections[2..got.sections.len() - 2]
            .iter()
            .zip(&board.sections[board.real_sections()])
        {
            assert_eq!(a.position, b.position);
            for (ka, kb) in a.spline.knots().iter().zip(b.spline.knots()) {
                for i in 0..3 {
                    assert!(
                        (ka.points[i] - kb.points[i]).length() < 1e-9,
                        "{name} section {}",
                        a.position
                    );
                }
            }
            assert_eq!(a.guide_points.len(), b.guide_points.len());
        }
        assert_eq!(got.sections[1].position, 0.2);
        assert_eq!(
            got.sections[got.sections.len() - 2].position,
            board.length() - 0.2
        );
        assert_eq!(got.outline_guides, board.outline_guides);
        assert_eq!(got.info.model, board.info.name);
        assert_eq!(got.info.designer, "A & B");
        assert_eq!(got.info.comments, "line one\nline two");
    }
}

#[test]
fn outline_ending_off_the_stringer_gets_end_knots() {
    let mut board = fixture_board("shortboard");
    let n = board.outline.len();
    // Drop the end knots on the stringer, as Shape3D outlines do.
    let knots = board.outline.knots_mut();
    knots.remove(n - 1);
    knots.remove(0);
    // Knots near the nose tip can be within 1 cm of the stringer; those stay the end.
    let tail_off = knots[0].point().y > 1.0;
    let nose_off = knots[n - 3].point().y > 1.0;
    assert!(tail_off);
    let got = s3d::read(encode(&board, true).as_bytes()).unwrap();
    let k = got.outline.knots();
    assert_eq!(k.len(), n - 2 + tail_off as usize + nose_off as usize);
    assert_eq!(k[0].point(), DVec2::ZERO);
    assert!(!k[1].continuous);
    assert_eq!(k[1].points[1].y, k[1].point().y * 2.0 / 3.0);
    if nose_off {
        assert_eq!(k[k.len() - 1].point().y, 0.0);
    }
}

#[test]
fn missing_deck_gives_placeholder_and_warning() {
    let board = fixture_board("shortboard");
    let (got, warnings) = s3d::read_with_warnings(encode(&board, false).as_bytes()).unwrap();
    assert_eq!(warnings, [s3d::NO_DECK_WARNING]);
    assert_eq!(got.deck.len(), 5);
    assert_eq!(got.deck.knots()[0].point(), got.bottom.knots()[0].point());
    assert!(got.thickness_at(got.length() / 2.0) > 5.0);
}

#[test]
fn dispatches_by_extension() {
    let board = fixture_board("funboard");
    let xml = encode(&board, true);
    assert!(surfcraft_io::read_any("S3D", xml.as_bytes()).is_ok());
    assert!(surfcraft_io::read_any(".s3d", xml.as_bytes()).is_ok());
    assert!(surfcraft_io::read_any("xyz", xml.as_bytes()).is_err());
}

#[test]
fn malformed_files_are_errors() {
    let board = fixture_board("funboard");
    let xml = encode(&board, true);
    assert_truncations_dont_panic(xml.as_bytes(), s3d::read);
    for bad in [
        "",
        "<a/>",
        "<Shape3d_design><Board/></Shape3d_design>",
        "\u{0}\u{1}\u{ff}garbage",
    ] {
        assert!(s3d::read(bad.as_bytes()).is_err(), "{bad:?}");
    }
    let broken = [
        ("<Nb_of_points>", "<Nb_of_points>99999999999"),
        ("<Nb_of_points>", "<Nb_of_points>-"),
        // The symmetry point is skipped, so break the first real one.
        (
            "</Symmetry_point>\n<Point3d><x>",
            "</Symmetry_point>\n<Point3d><x>abc",
        ),
        ("<Tangent_type_point_0>", "<Tangent_type_point_0>q"),
    ];
    for (from, to) in broken {
        let x = xml.replacen(from, to, 1);
        assert!(s3d::read(x.as_bytes()).is_err(), "{to}");
    }
    // Missing sections are tolerated: only the end dummies remain.
    let cut = xml.find("<Couples_0>").unwrap();
    let no_sections = format!("{}</Board></Shape3d_design>", &xml[..cut]);
    assert_eq!(s3d::read(no_sections.as_bytes()).unwrap().sections.len(), 2);
}
