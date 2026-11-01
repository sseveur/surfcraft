//! Partial curve files (`docs/spec/file-formats.md` §5): `.otl` (outline), `.pfl` (bottom and deck) and `.crs`
//! (one cross-section), written in the `.brd` block syntax without any scalar field.
//!
//! Reading is strict where the reference program was broken: the whole file is parsed before anything is
//! returned, a block cut short or a malformed line is an error, and every curve needs at least two knots, so
//! a failed import never leaves a half-replaced board behind.

use surfcraft_core::{Board, CrossSection, DVec2, Spline};

use crate::Error;
use crate::brd::{End, Reader, curve_body, lines, parse_finite, scalar, section_block};

/// Curves found in a partial curve file, each with its guide points.
#[derive(Clone, Debug, Default)]
pub struct Partial {
    pub outline: Option<(Spline, Vec<DVec2>)>,
    pub bottom: Option<(Spline, Vec<DVec2>)>,
    pub deck: Option<(Spline, Vec<DVec2>)>,
    pub section: Option<CrossSection>,
}

fn invalid(what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("curve file: {what}"))
}

/// Clear the `continuous` flag of knots whose handles are not opposite (demote-only normalisation).
fn normalize(s: &mut Spline) {
    for k in s.knots_mut() {
        if !k.is_geometrically_continuous() {
            k.continuous = false;
        }
    }
}

/// A curve block that must be complete and hold at least two knots.
fn complete(
    what: &str,
    line: usize,
    (mut spline, guides, end): (Spline, Vec<DVec2>, End),
) -> Result<(Spline, Vec<DVec2>), Error> {
    if let End::Truncated = end {
        return Err(invalid(format!(
            "the {what} starting on line {line} is cut short (truncated file)"
        )));
    }
    if spline.len() < 2 {
        return Err(invalid(format!(
            "the {what} starting on line {line} needs at least two control points"
        )));
    }
    normalize(&mut spline);
    Ok((spline, guides))
}

/// The position of a `(p36 <position>` line.
fn section_position(n: usize, rest: &str) -> Result<f64, Error> {
    let text = rest.split_whitespace().next().unwrap_or("");
    parse_finite(text)
        .ok_or_else(|| invalid(format!("line {n}: bad cross-section position {text:?}")))
}

/// Read an `.otl`, `.pfl` or `.crs` file.
///
/// Whatever blocks the file holds are returned: `p32` (outline), `p33` (bottom), `p34` (deck), a bare
/// `(p36 …` section block, or a `p35` block holding exactly one cross-section (end sections aside). Other
/// lines outside the blocks are ignored. A file without any curve is an error.
pub fn read_partial(text: &str) -> Result<Partial, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Partial::default();
    let mut r = Reader {
        lines: lines(text),
        pos: 0,
    };
    while let Some((n, line, _)) = r.next() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(p36") {
            let position = section_position(n, rest)?;
            let (spline, guides) = complete("cross-section", n, r.curve()?)?;
            let mut s = CrossSection::new(position, spline);
            s.guide_points = guides;
            out.section = Some(s);
            continue;
        }
        let Some((code, value)) = scalar(t) else {
            continue;
        };
        let what = match code {
            32 => "outline",
            33 => "bottom",
            34 => "deck",
            35 => "cross-section list",
            _ => continue,
        };
        if value != "(" {
            return Err(invalid(format!(
                "line {n}: p{code}: expected \"(\", got {value:?}"
            )));
        }
        if code == 35 {
            out.section = Some(one_section(&mut r, n)?);
            continue;
        }
        let curve = complete(what, n, r.curve()?)?;
        match code {
            32 => out.outline = Some(curve),
            33 => out.bottom = Some(curve),
            _ => out.deck = Some(curve),
        }
    }
    if out.outline.is_none() && out.bottom.is_none() && out.deck.is_none() && out.section.is_none()
    {
        return Err(invalid(
            "no outline, profile or cross-section found (expected a p32, p33, p34 or (p36 block)",
        ));
    }
    Ok(out)
}

/// The content of a `p35` block, which must hold exactly one real cross-section.
fn one_section(r: &mut Reader<'_>, start: usize) -> Result<CrossSection, Error> {
    let mut found = Vec::new();
    loop {
        let Some((n, line, _)) = r.next() else {
            return Err(invalid(format!(
                "the cross-section list starting on line {start} is cut short (truncated file)"
            )));
        };
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t == ")" {
            break;
        }
        let Some(rest) = t.strip_prefix("(p36") else {
            return Err(invalid(format!(
                "line {n}: unexpected {t:?} in the cross-section list"
            )));
        };
        let position = section_position(n, rest)?;
        let (spline, guides, end) = r.curve()?;
        if let End::Truncated = end {
            return Err(invalid(format!(
                "the cross-section starting on line {n} is cut short (truncated file)"
            )));
        }
        // Zero-size end sections are skipped.
        if spline.len() > 1 {
            let (spline, guides) = complete("cross-section", n, (spline, guides, End::Closed))?;
            let mut s = CrossSection::new(position, spline);
            s.guide_points = guides;
            found.push(s);
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        k => Err(invalid(format!(
            "the cross-section list starting on line {start} holds {k} cross-sections; a .crs file holds one"
        ))),
    }
}

/// The outline as an `.otl` file.
pub fn write_outline(board: &Board) -> String {
    let mut o = String::from("p32 : (\n");
    curve_body(&mut o, &board.outline, &board.outline_guides);
    o.push_str(")\n");
    o
}

/// Deck and bottom as a `.pfl` file.
pub fn write_profile(board: &Board) -> String {
    let mut o = String::from("p33 : (\n");
    curve_body(&mut o, &board.bottom, &board.bottom_guides);
    o.push_str(")\np34 : (\n");
    curve_body(&mut o, &board.deck, &board.deck_guides);
    o.push_str(")\n");
    o
}

/// One cross-section as a `.crs` file.
pub fn write_section(section: &CrossSection) -> String {
    let mut o = String::new();
    section_block(&mut o, section);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORTBOARD: &str = include_str!("../../../tests/fixtures/shortboard.brd");

    fn board() -> Board {
        crate::brd::read_text(SHORTBOARD).unwrap()
    }

    #[test]
    fn reads_every_kind_of_block() {
        let b = board();
        let p = read_partial(&write_outline(&b)).unwrap();
        assert_eq!(p.outline.unwrap().0.len(), b.outline.len());
        assert!(p.bottom.is_none() && p.deck.is_none() && p.section.is_none());

        // Profile blocks in either order, with CR LF line ends and blank lines.
        let pfl = write_profile(&b);
        let (bottom, deck) = pfl.split_at(pfl.find("p34").unwrap());
        let swapped = format!("{deck}\n\n{bottom}").replace('\n', "\r\n");
        let p = read_partial(&swapped).unwrap();
        assert_eq!(p.bottom.unwrap().0.len(), b.bottom.len());
        assert_eq!(p.deck.unwrap().0.len(), b.deck.len());

        // Only one of them: the other stays unset.
        let p = read_partial(bottom).unwrap();
        assert!(p.bottom.is_some() && p.deck.is_none());

        // A p35 wrapper with end sections and one real section.
        let s = &b.sections[3];
        let wrapped = format!(
            "p35 : (\n{}{}{})\n",
            write_section(&b.sections[0]),
            write_section(s),
            write_section(b.sections.last().unwrap())
        );
        let got = read_partial(&wrapped).unwrap().section.unwrap();
        assert_eq!(got.position, s.position);
        for (a, k) in got.spline.knots().iter().zip(s.spline.knots()) {
            assert!(a.is_identical(k));
        }
    }

    #[test]
    fn guide_points_come_along() {
        let mut b = board();
        b.outline_guides = vec![DVec2::new(10.0, 12.5), DVec2::new(100.0, 28.0)];
        b.sections[2].guide_points = vec![DVec2::new(5.0, 2.0)];
        let p = read_partial(&write_outline(&b)).unwrap();
        assert_eq!(p.outline.unwrap().1, b.outline_guides);
        let p = read_partial(&write_section(&b.sections[2])).unwrap();
        assert_eq!(p.section.unwrap().guide_points, b.sections[2].guide_points);
    }

    #[test]
    fn kinked_knots_lose_their_continuity_flag() {
        let text = "p32 : (\n(cp [0,0,0,0,0,5] true false)\n(cp [100,25,50,25,150,25] true false)\n(cp [200,0,190,10,200,0] false false)\n)\n";
        let (s, _) = read_partial(text).unwrap().outline.unwrap();
        assert!(!s.knots()[0].continuous);
        assert!(s.knots()[1].continuous);
    }

    #[test]
    fn malformed_files_are_errors() {
        let b = board();
        let wrapped_two = format!(
            "p35 : (\n{}{})\n",
            write_section(&b.sections[2]),
            write_section(&b.sections[3])
        );
        for bad in [
            "",
            "hello",
            "p08 : name\n",
            "p32 : (\n",
            "p32 : (\n(cp [x",
            "p32 : (\n(cp [0,0,0,0,0,0] false false)\n)\n",
            "p32 : x\n",
            "p33 : (\n(cp [1,2,3\n",
            "p33 : (\n(cp [1,2,3,4,5,6] true false)\nnonsense\n)\n",
            "(p36 abc\n",
            "(p36 10\n(cp [0,0,0,0,1,0] true false)\n(cp [0,1,1,1,0,1] true false)\n",
            "p35 : (\n)\n",
            &wrapped_two,
        ] {
            assert!(read_partial(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn truncated_and_corrupted_files_never_panic() {
        let b = board();
        let mut seed = 11u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for text in [
            write_outline(&b),
            write_profile(&b),
            write_section(&b.sections[2]),
        ] {
            let bytes = text.as_bytes();
            for cut in 0..bytes.len() {
                let _ = read_partial(&String::from_utf8_lossy(&bytes[..cut]));
            }
            for _ in 0..200 {
                let mut v = bytes.to_vec();
                for _ in 0..1 + rnd() % 6 {
                    let i = rnd() % v.len();
                    v[i] = b"0123456789-.,()[]:e \n;#'p"[rnd() % 25];
                }
                if let Ok(p) = read_partial(&String::from_utf8_lossy(&v)) {
                    for (s, _) in [p.outline, p.bottom, p.deck].into_iter().flatten() {
                        assert!(s.len() >= 2);
                        let _ = (s.length(), s.max_y());
                    }
                }
            }
            let junk: Vec<u8> = (0..500).map(|_| rnd() as u8).collect();
            let _ = read_partial(&String::from_utf8_lossy(&junk));
        }
    }
}
