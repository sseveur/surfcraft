//! `.srf` reader against synthetic files written from the `.brd` fixtures.

mod common;

use common::*;
use surfcraft_core::units::FOOT;
use surfcraft_core::{Board, CrossSection, DVec2, Spline};
use surfcraft_io::srf;

/// Test-only `.srf` writer: exactly the layout `SrfReader` expects.
struct Srf {
    buf: Vec<u8>,
    /// Board length, cm.
    l: f64,
}

impl Srf {
    fn zeros(&mut self, n: usize) {
        self.buf.extend(std::iter::repeat_n(0u8, n));
    }

    fn f32(&mut self, v: f64) {
        self.buf.extend((v as f32).to_le_bytes());
    }

    fn i16(&mut self, v: usize) {
        self.buf.extend(i16::try_from(v).unwrap().to_le_bytes());
    }

    /// SurfCraft (x, y) cm -> file metres from the nose.
    fn vec3(&mut self, p: DVec2) {
        self.f32((self.l - p.x) / 100.0);
        self.f32(p.y / 100.0);
        self.f32(0.0);
    }

    fn curve(&mut self, spline: &Spline, first: bool) {
        if !first {
            self.zeros(1);
        }
        self.i16(spline.len());
        for k in spline.knots().iter().rev() {
            self.vec3(k.points[0]);
            self.zeros(12);
            self.vec3(k.points[1]);
            self.vec3(k.points[2]);
            self.zeros(28);
        }
    }

    /// Cave points in section coordinates (cm, height above the section's bottom).
    fn caves(&mut self, caves: &[(f64, [DVec2; 4])], lift: impl Fn(f64) -> f64) {
        self.zeros(1);
        self.i16(caves.len());
        for &(pos, pts) in caves {
            let z = (self.l - pos) / 100.0;
            let p3 = |s: &mut Self, p: DVec2| {
                s.f32(p.x / 100.0);
                s.f32((p.y + lift(pos)) / 100.0);
                s.f32(z);
            };
            self.zeros(6);
            p3(self, pts[0]);
            self.zeros(12);
            p3(self, pts[1]);
            self.zeros(40);
            p3(self, pts[2]);
            self.zeros(24);
            p3(self, pts[3]);
            self.zeros(28);
        }
    }
}

/// Least-squares cubic through `samples` with fixed ends and end tangent directions (Schneider's method with a
/// few reparameterisation passes). Returns the two inner control points.
fn fit_cubic(samples: &[DVec2], d0: DVec2, d3: DVec2) -> (DVec2, DVec2) {
    let (p0, p3) = (samples[0], *samples.last().unwrap());
    let mut t: Vec<f64> = {
        let mut acc = vec![0.0];
        for w in samples.windows(2) {
            acc.push(acc.last().unwrap() + (w[1] - w[0]).length());
        }
        let total = *acc.last().unwrap();
        acc.iter().map(|a| a / total).collect()
    };
    let bez = |a: DVec2, b: DVec2, t: f64| {
        let u = 1.0 - t;
        p0 * u * u * u + a * 3.0 * u * u * t + b * 3.0 * u * t * t + p3 * t * t * t
    };
    let chord = (p3 - p0).length();
    let mut handles = (p0 + d0 * chord / 3.0, p3 + d3 * chord / 3.0);
    for _ in 0..8 {
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (q, &ti) in samples.iter().zip(&t) {
            let u = 1.0 - ti;
            let a1 = d0 * 3.0 * u * u * ti;
            let a2 = d3 * 3.0 * u * ti * ti;
            let rest = *q
                - (p0 * (u * u * u + 3.0 * u * u * ti) + p3 * (3.0 * u * ti * ti + ti * ti * ti));
            c00 += a1.dot(a1);
            c01 += a1.dot(a2);
            c11 += a2.dot(a2);
            x0 += rest.dot(a1);
            x1 += rest.dot(a2);
        }
        let det = c00 * c11 - c01 * c01;
        let (a, b) = if det.abs() > 1e-12 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (chord / 3.0, chord / 3.0)
        };
        let (a, b) = (a.max(chord * 0.01), b.max(chord * 0.01));
        handles = (p0 + d0 * a, p3 + d3 * b);
        // Newton step on each parameter towards its sample.
        for (q, ti) in samples.iter().zip(t.iter_mut()) {
            let (h1, h2) = handles;
            let u = 1.0 - *ti;
            let d1 =
                (h1 - p0) * 3.0 * u * u + (h2 - h1) * 6.0 * u * *ti + (p3 - h2) * 3.0 * *ti * *ti;
            let d2 = (h2 - h1 * 2.0 + p0) * 6.0 * u + (p3 - h2 * 2.0 + h1) * 6.0 * *ti;
            let diff = bez(h1, h2, *ti) - *q;
            let den = d1.dot(d1) + diff.dot(d2);
            if den.abs() > 1e-12 {
                *ti = (*ti - diff.dot(d1) / den).clamp(0.0, 1.0);
            }
        }
    }
    handles
}

/// A section as SRF bottom and deck caves: one cubic each side of the rail apex (widest point).
fn section_caves(s: &CrossSection) -> ([DVec2; 4], [DVec2; 4]) {
    const N: usize = 400;
    let pts: Vec<DVec2> = (0..=N)
        .map(|i| s.spline.point_by_s(i as f64 / N as f64))
        .collect();
    let apex = (0..=N)
        .max_by(|&a, &b| pts[a].x.total_cmp(&pts[b].x))
        .unwrap();
    let dir = |from: DVec2, to: DVec2| (to - from).normalize_or_zero();
    let bottom = &pts[..=apex];
    let deck = &pts[apex..];
    let (b1, b2) = fit_cubic(bottom, dir(pts[0], pts[1]), dir(pts[apex], pts[apex - 1]));
    let (d1, d2) = fit_cubic(deck, dir(pts[apex], pts[apex + 1]), dir(pts[N], pts[N - 1]));
    // Bottom cave: centre, centre handle, rail point, rail handle. Deck cave: deck centre, its handle, rail
    // point, rail handle (see the reader).
    ([pts[0], b1, pts[apex], b2], [pts[N], d2, pts[apex], d1])
}

fn encode(board: &Board) -> Vec<u8> {
    let l = board.length();
    let mut w = Srf { buf: Vec::new(), l };
    w.buf.extend(b"SRF2.0 ");
    w.buf.extend(board.info.name.as_bytes());
    w.buf.extend(b" *a comment@");
    w.zeros(11);
    w.f32(l / 100.0);
    let wide = board.max_width_pos();
    for x in [l - FOOT, wide, FOOT, 0.0] {
        w.f32(board.outline.value_at(x) / 100.0);
    }
    w.f32((l - wide) / 100.0);
    for x in [l, l - FOOT, FOOT, 0.0] {
        w.f32(board.rocker_at(x) / 100.0);
    }
    w.zeros(8 * 4);
    w.zeros(113);

    w.curve(&board.outline, true);
    w.curve(&board.bottom, false);
    // The deck in the file stops at the tail and nose thickness; the reader adds the bottom's end points.
    let mut deck = board.deck.knots().to_vec();
    if deck[0].point() == board.bottom.knots()[0].point() {
        deck.remove(0);
    }
    if deck.last().unwrap().point() == board.bottom.knots().last().unwrap().point() {
        deck.pop();
    }
    w.curve(&board.deck, false); // rail line: unused when every deck cave has a bottom cave
    w.curve(&Spline::from_knots(deck), false);
    w.curve(&board.bottom, false); // bottom rail line: same

    let (mut decks, mut bottoms) = (Vec::new(), Vec::new());
    for s in &board.sections[board.real_sections()] {
        let (b, d) = section_caves(s);
        bottoms.push((s.position, b));
        decks.push((s.position, d));
    }
    // Nose first, like the curves.
    decks.reverse();
    bottoms.reverse();
    w.caves(&decks, |x| board.rocker_at(x));
    w.caves(&bottoms, |x| board.rocker_at(x));
    w.buf
}

#[test]
fn reads_synthetic_srf_files() {
    for name in BOARDS {
        let board = fixture_board(name);
        let bytes = encode(&board);
        keep("srf", &format!("{name}.srf"), &bytes);
        let got = srf::read(&bytes).unwrap();
        assert_same_shape(name, &board, &got, 1e-3, 0.02);
        assert_eq!(got.sections.len(), board.sections.len());
        for i in got.real_sections() {
            let want = board.sections[i].position.clamp(0.5, board.length() - 0.5);
            assert!((got.sections[i].position - want).abs() < 1e-3);
        }
        for s in &got.sections[got.real_sections()] {
            // Both caves end on the rail apex here, so the rail knots are merged.
            assert_eq!(s.spline.len(), 3);
            assert!(s.spline.knots()[0].point().y.abs() < 1e-3);
        }
        assert_eq!(got.info.model, board.info.name);
        assert_eq!(got.info.comments, "a comment");
        assert_eq!(got.outline_guides.len(), 5);
    }
}

#[test]
fn deck_uses_its_own_point_count() {
    // 3.0.1 read the deck with the rocker's count; give them different counts.
    let mut board = fixture_board("shortboard");
    let knots = board.bottom.knots_mut();
    let k = knots[1].clone();
    knots.insert(1, k);
    let got = srf::read(&encode(&board)).unwrap();
    assert_eq!(got.bottom.len(), board.bottom.len());
    assert_eq!(got.deck.len(), board.deck.len());
}

#[test]
fn unmatched_caves_are_interpolated() {
    let board = fixture_board("funboard");
    let mut bytes = encode(&board);
    // Move every bottom cave 10 cm: deck caves then build their bottom halves by interpolation, and bottom
    // caves get their own sections with interpolated deck halves.
    let n_real = board.real_sections().len();
    let start = bytes.len() - n_real * 158;
    for i in 0..n_real {
        let at = start + i * 158 + 6 + 8; // first point's z
        let z = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        bytes[at..at + 4].copy_from_slice(&(z - 0.1).to_le_bytes());
    }
    let got = srf::read(&bytes).unwrap();
    assert!(got.sections.len() > board.sections.len());
    for s in &got.sections[got.real_sections()] {
        assert_eq!(s.spline.len(), 4);
        for k in s.spline.knots() {
            assert!(k.points.iter().all(|p| p.is_finite()));
        }
    }
    assert!(got.volume().is_finite());
}

#[test]
fn malformed_files_are_errors() {
    let bytes = encode(&fixture_board("funboard"));
    assert_truncations_dont_panic(&bytes, srf::read);
    for n in [0, 10, 200, 600, bytes.len() - 1] {
        assert!(srf::read(&bytes[..n]).is_err(), "{n}");
    }
    assert!(srf::read(b"no terminators").is_err());
    // Negative and oversized point counts.
    let header = bytes.iter().position(|&b| b == b'@').unwrap() + 1;
    let count_at = header + 11 + 4 * 18 + 113;
    for v in [-1i16, i16::MAX] {
        let mut b = bytes.clone();
        b[count_at..count_at + 2].copy_from_slice(&v.to_le_bytes());
        assert!(srf::read(&b).is_err());
    }
    // NaN coordinates.
    let mut b = bytes.clone();
    b[count_at + 2..count_at + 6].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(srf::read(&b).is_err());
    // Both cave lists empty: just the end sections.
    let caves_len = 2 * (3 + fixture_board("funboard").real_sections().len() * 158);
    let mut b = bytes[..bytes.len() - caves_len].to_vec();
    b.extend([0, 0, 0, 0, 0, 0]);
    let got = srf::read(&b).unwrap();
    assert_eq!(got.sections.len(), 2);
    assert!(surfcraft_io::read_any("SRF", &bytes).is_ok());
}
