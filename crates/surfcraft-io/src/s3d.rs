//! Shape3D designs (`.s3d`, `docs/spec/file-formats.md` §7): XML with the outline, bottom, deck and
//! cross-sections as 3-D Bezier curves in centimetres. Only read.
//!
//! Shape3D's curves differ from the board model at the ends: its outline may stop off the stringer, its deck
//! stops at the tail and nose thickness, and its sections sit at their height on the rocker. The reader closes
//! the outline, ties the deck to the bottom's end points and moves the sections onto the bottom.

use std::collections::HashMap;

use surfcraft_core::{Board, CrossSection, DVec2, Knot, Spline};

use crate::Error;
use crate::xml::{Document, Id};

/// Warning reported when the design has no deck curve.
pub const NO_DECK_WARNING: &str = "The file has no deck curve; a deck was made from the thickness.";

/// Outline end knots farther than this from the stringer get an extra knot on it (cm).
const TIP_GAP: f64 = 1.0;

fn invalid(what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!(".s3d: {what}"))
}

/// Which two coordinates of the 3-D points make the curve's plane.
#[derive(Clone, Copy)]
enum Plane {
    /// Outline: (x, y).
    Xy,
    /// Bottom and deck: (x, z).
    Xz,
    /// Cross-sections: (y, z).
    Yz,
}

impl Plane {
    fn project(self, p: [f64; 3]) -> DVec2 {
        match self {
            Plane::Xy => DVec2::new(p[0], p[1]),
            Plane::Xz => DVec2::new(p[0], p[2]),
            Plane::Yz => DVec2::new(p[1], p[2]),
        }
    }
}

struct Reader<'a> {
    doc: &'a Document,
    warnings: Vec<String>,
}

/// A curve as read: knots, guide points and the x of its first control point.
struct Curve {
    knots: Vec<Knot>,
    guides: Vec<DVec2>,
    first_x: f64,
}

impl Reader<'_> {
    fn child(&self, parent: Id, name: &str, what: &str) -> Result<Id, Error> {
        self.doc
            .find(parent, name)
            .ok_or_else(|| invalid(format!("{what}: no <{name}> element")))
    }

    fn number(&self, id: Id, what: &str) -> Result<f64, Error> {
        let text = self.doc.text(id);
        let t = text.trim();
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                invalid(format!(
                    "{what}: <{}> is not a number: {t:?}",
                    self.doc.name(id)
                ))
            })
    }

    fn integer(&self, id: Id, what: &str) -> Result<i64, Error> {
        let text = self.doc.text(id);
        let t = text.trim();
        t.parse::<i64>()
            .ok()
            .or_else(|| {
                t.parse::<f64>()
                    .ok()
                    .filter(|v| v.fract() == 0.0 && v.abs() < 1e15)
                    .map(|v| v as i64)
            })
            .ok_or_else(|| {
                invalid(format!(
                    "{what}: <{}> is not a whole number: {t:?}",
                    self.doc.name(id)
                ))
            })
    }

    fn point(&self, id: Id, what: &str) -> Result<[f64; 3], Error> {
        let mut p = [0.0; 3];
        for (slot, axis) in p.iter_mut().zip(["x", "y", "z"]) {
            *slot = self.number(self.child(id, axis, what)?, what)?;
        }
        Ok(p)
    }

    /// The `Point3d` entries 1 ..= n of the polygon under `<tag>` (entry 0 is Shape3D's symmetry point).
    fn polygon(&self, bezier: Id, tag: &str, n: usize, what: &str) -> Result<Vec<[f64; 3]>, Error> {
        let what = format!("{what} {tag}");
        let poly = self.child(self.child(bezier, tag, &what)?, "Polygone3d", &what)?;
        let points = self.doc.find_all(poly, "Point3d");
        if points.len() < n + 1 {
            return Err(invalid(format!(
                "{what}: {n} points announced but {} found",
                points.len().saturating_sub(1)
            )));
        }
        points[1..=n]
            .iter()
            .map(|&p| self.point(p, &what))
            .collect()
    }

    /// The `Bezier3d` curve under `element`.
    fn curve(&mut self, element: Id, plane: Plane, what: &str) -> Result<Curve, Error> {
        let bez = self.child(element, "Bezier3d", what)?;
        let poly = self.child(self.child(bez, "Control_points", what)?, "Polygone3d", what)?;
        let count = self.integer(self.child(poly, "Nb_of_points", what)?, what)?;
        let n = usize::try_from(count)
            .map_err(|_| invalid(format!("{what}: negative point count {count}")))?;
        let ends = self.polygon(bez, "Control_points", n, what)?;
        let prevs = self.polygon(bez, "Tangents_1", n, what)?;
        let nexts = self.polygon(bez, "Tangents_2", n, what)?;
        // Index the tangent types once (looking each up would be quadratic in the point count).
        let mut types: HashMap<usize, Id> = HashMap::new();
        for d in self.doc.descendants(bez) {
            if let Some(i) = self
                .doc
                .name(d)
                .strip_prefix("Tangent_type_point_")
                .and_then(|i| i.parse().ok())
            {
                types.entry(i).or_insert(d);
            }
        }
        let mut knots = Vec::with_capacity(n);
        for i in 0..n {
            let tag = format!("Tangent_type_point_{i}");
            let continuous = match types.get(&i).copied() {
                Some(id) => self.integer(id, what)? != 0,
                None => {
                    self.warnings
                        .push(format!("{what}: no <{tag}>; the knot was taken as smooth"));
                    true
                }
            };
            let mut k = Knot::new(
                plane.project(ends[i]),
                plane.project(prevs[i]),
                plane.project(nexts[i]),
            );
            k.continuous = continuous;
            knots.push(k);
        }
        let n_guides = match self.doc.find(bez, "Number_of_guides") {
            Some(id) => usize::try_from(self.integer(id, what)?).unwrap_or(0),
            None => 0,
        };
        let mut guides = Vec::new();
        for g in self.doc.find_all(bez, "Guide").into_iter().take(n_guides) {
            let p = self.child(g, "Point3d", what)?;
            guides.push(plane.project(self.point(p, what)?));
        }
        Ok(Curve {
            knots,
            guides,
            first_x: ends.first().map_or(0.0, |p| p[0]),
        })
    }
}

fn text_of(doc: &Document, board: Id, name: &str) -> Option<String> {
    doc.find(board, name)
        .map(|id| doc.text(id).trim().to_owned())
}

/// Close an outline that stops off the stringer with knots on it at the tail and the nose.
fn close_outline(knots: &mut Vec<Knot>) {
    if let Some(first) = knots.first_mut() {
        let e = first.point();
        if e.y > TIP_GAP {
            first.continuous = false;
            first.points[1].y = e.y * 2.0 / 3.0;
            knots.insert(
                0,
                Knot::new(DVec2::ZERO, DVec2::ZERO, DVec2::new(0.0, e.y / 3.0)),
            );
        }
    }
    if let Some(last) = knots.last_mut() {
        let e = last.point();
        if e.y > TIP_GAP {
            last.continuous = false;
            last.points[2] = DVec2::new(e.x, e.y * 2.0 / 3.0);
            knots.push(Knot::new(
                DVec2::new(e.x, 0.0),
                DVec2::new(e.x, e.y / 3.0),
                DVec2::new(e.x, 0.0),
            ));
        }
    }
}

/// The rough deck used when the design only has a thickness curve.
fn placeholder_deck(bottom: &[Knot], l: f64) -> Vec<Knot> {
    let (Some(b0), Some(b1)) = (bottom.first(), bottom.last()) else {
        return Vec::new();
    };
    let mut k1 = b0.clone();
    k1.continuous = false;
    k1.points[0].y += 1.5;
    k1.points[2] = DVec2::new(20.0, k1.points[0].y - 0.5);
    let t = ((l / 30.48) - 5.0) / 4.0 + 2.125 * 2.54;
    let k2 = Knot::smooth(
        DVec2::new(l / 2.0, t),
        DVec2::new(l / 2.0 - 50.0, t),
        DVec2::new(l / 2.0 + 50.0, t),
    );
    let mut k3 = b1.clone();
    k3.continuous = false;
    k3.points[0].y += 1.5;
    let ey = k3.points[0].y;
    k3.points[1].y += ey / 2.0;
    k3.points[2].x = k3.points[0].x;
    k3.points[2].y += 0.8;
    vec![k1, k2, k3]
}

/// Start and end the deck on the bottom's end points, with handles half-way up the tail and nose.
fn tie_deck(deck: &mut Vec<Knot>, bottom: &[Knot]) {
    let (Some(b0), Some(b1), Some(d0), Some(d1)) =
        (bottom.first(), bottom.last(), deck.first(), deck.last())
    else {
        return;
    };
    let mut head = b0.clone();
    let e = head.point();
    head.points[2] = DVec2::new(e.x, (e.y + d0.point().y) / 2.0);
    let mut tail = b1.clone();
    let e = tail.point();
    tail.points[1] = DVec2::new(e.x, (e.y + d1.point().y) / 2.0);
    deck.insert(0, head);
    deck.push(tail);
}

/// Read a Shape3D design.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    read_with_warnings(bytes).map(|(b, _)| b)
}

/// Read a Shape3D design, also returning warnings about what had to be made up.
pub fn read_with_warnings(bytes: &[u8]) -> Result<(Board, Vec<String>), Error> {
    let text = crate::brd::decode(bytes);
    let doc = Document::parse(&text)?;
    let board_el = doc
        .find(Document::ROOT, "Board")
        .ok_or_else(|| invalid("not a Shape3D design (no <Board> element)"))?;
    let mut r = Reader {
        doc: &doc,
        warnings: Vec::new(),
    };
    let mut b = Board::default();

    let el = r.child(board_el, "Outline", "outline")?;
    let mut outline = r.curve(el, Plane::Xy, "outline")?;
    close_outline(&mut outline.knots);
    if outline.knots.len() < 2 {
        return Err(invalid("the outline needs at least two control points"));
    }
    b.outline = Spline::from_knots(outline.knots);
    b.outline_guides = outline.guides;
    let l = b.length();

    let el = r.child(board_el, "Bottom", "bottom")?;
    let bottom = r.curve(el, Plane::Xz, "bottom")?;
    if bottom.knots.len() < 2 {
        return Err(invalid("the bottom needs at least two control points"));
    }

    let (mut deck, deck_guides) = match doc.find(board_el, "Deck") {
        Some(el) => {
            let c = r.curve(el, Plane::Xz, "deck")?;
            if c.knots.is_empty() {
                return Err(invalid("the deck has no control points"));
            }
            (c.knots, c.guides)
        }
        None => {
            r.warnings.push(NO_DECK_WARNING.to_owned());
            (placeholder_deck(&bottom.knots, l), Vec::new())
        }
    };
    tie_deck(&mut deck, &bottom.knots);
    b.bottom = Spline::from_knots(bottom.knots);
    b.bottom_guides = bottom.guides;
    b.deck = Spline::from_knots(deck);
    b.deck_guides = deck_guides;

    let mut sections = Vec::new();
    for i in 0.. {
        let Some(el) = doc.find(board_el, &format!("Couples_{i}")) else {
            break;
        };
        let what = format!("cross-section {i}");
        let mut c = r.curve(el, Plane::Yz, &what)?;
        if c.knots.len() < 2 {
            return Err(invalid(format!("{what} needs at least two control points")));
        }
        // From the height on the rocker to the height above the bottom.
        let lift = DVec2::new(0.0, c.knots[0].point().y);
        for k in &mut c.knots {
            for p in &mut k.points {
                *p -= lift;
            }
        }
        for g in &mut c.guides {
            *g -= lift;
        }
        let mut s = CrossSection::new(c.first_x, Spline::from_knots(c.knots));
        s.guide_points = c.guides;
        sections.push(s);
    }
    if sections.is_empty() {
        r.warnings
            .push("The file has no cross-sections.".to_owned());
    }
    // Shape3D's tip sections lie on the tips.
    if let Some(s) = sections.first_mut() {
        s.position = 0.2;
    }
    if let Some(s) = sections.last_mut() {
        s.position = l - 0.2;
    }
    sections.sort_by(|a, c| a.position.total_cmp(&c.position));
    b.sections = sections;
    b.sections.insert(0, CrossSection::end(0.0));
    b.sections.push(CrossSection::end(l));

    let info = &mut b.info;
    if let Some(v) = text_of(&doc, board_el, "Name") {
        info.model = v;
    }
    if let Some(v) = text_of(&doc, board_el, "Author") {
        info.designer = v;
    }
    if let Some(v) = text_of(&doc, board_el, "Comment") {
        info.comments = v;
    }
    crate::brd::finish(&mut b);
    let warnings = r.warnings;
    Ok((b, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64, z: f64) -> String {
        format!("<Point3d><x>{x}</x><y>{y}</y><z>{z}</z></Point3d>")
    }

    /// A curve element from (point, prev, next) triples in 3-D.
    fn curve(tag: &str, knots: &[[[f64; 3]; 3]], types: &[i32]) -> String {
        let poly = |name: &str, j: usize| {
            let mut s = format!(
                "<{name}><Polygone3d><Nb_of_points>{}</Nb_of_points>{}",
                knots.len(),
                pt(0.0, 0.0, 0.0)
            );
            for k in knots {
                s += &pt(k[j][0], k[j][1], k[j][2]);
            }
            s + &format!("</Polygone3d></{name}>")
        };
        let mut s = format!(
            "<{tag}><Bezier3d>{}{}{}",
            poly("Control_points", 0),
            poly("Tangents_1", 1),
            poly("Tangents_2", 2)
        );
        for (i, t) in types.iter().enumerate() {
            s += &format!("<Tangent_type_point_{i}> {t} </Tangent_type_point_{i}>");
        }
        s + &format!("</Bezier3d></{tag}>")
    }

    fn design(with_deck: bool, tangents: bool) -> String {
        let t = |n: usize| if tangents { vec![0; n] } else { Vec::new() };
        let mut s = String::from("<Shape3d_design><Board>");
        s += &curve(
            "Outline",
            &[
                [[0.0, 5.0, 0.0], [0.0, 3.0, 0.0], [10.0, 8.0, 0.0]],
                [[100.0, 25.0, 0.0], [60.0, 25.0, 0.0], [140.0, 25.0, 0.0]],
                [[180.0, 3.0, 0.0], [170.0, 10.0, 0.0], [181.0, 1.0, 0.0]],
            ],
            &t(3),
        );
        s += &curve(
            "Bottom",
            &[
                [[0.0, 0.0, 5.0], [0.0, 0.0, 5.0], [40.0, 0.0, 1.0]],
                [[180.0, 0.0, 12.0], [140.0, 0.0, 4.0], [180.0, 0.0, 12.0]],
            ],
            &t(2),
        );
        if with_deck {
            s += &curve(
                "Deck",
                &[
                    [[0.0, 0.0, 7.0], [0.0, 0.0, 7.0], [40.0, 0.0, 8.0]],
                    [[180.0, 0.0, 14.0], [140.0, 0.0, 9.0], [180.0, 0.0, 14.0]],
                ],
                &t(2),
            );
        }
        for i in 0..3 {
            let x = 30.0 + 50.0 * i as f64;
            s += &curve(
                &format!("Couples_{i}"),
                &[
                    [[x, 0.0, 2.0], [x, -5.0, 2.0], [x, 5.0, 2.0]],
                    [[x, 20.0, 3.0], [x, 20.0, 2.5], [x, 20.0, 4.0]],
                    [[x, 0.0, 7.0], [x, 5.0, 7.0], [x, -5.0, 7.0]],
                ],
                &t(3),
            );
        }
        s + "<Name> M </Name></Board></Shape3d_design>"
    }

    #[test]
    fn assembles_a_board() {
        let (b, w) = read_with_warnings(design(true, true).as_bytes()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        // Outline closed at both tips (both ends were off the stringer).
        assert_eq!(b.outline.len(), 5);
        assert_eq!(b.outline.knots()[4].point(), DVec2::new(180.0, 0.0));
        assert_eq!(b.length(), 180.0);
        // Deck tied to the bottom.
        assert_eq!(b.deck.len(), 4);
        assert_eq!(b.deck.knots()[0].point(), b.bottom.knots()[0].point());
        assert_eq!(b.deck.knots()[0].next(), DVec2::new(0.0, 6.0));
        assert_eq!(b.deck.knots()[3].prev(), DVec2::new(180.0, 13.0));
        // Sections moved onto the bottom, tips moved inside, end sections added.
        let pos: Vec<f64> = b.sections.iter().map(|s| s.position).collect();
        assert_eq!(pos, [0.0, 0.2, 80.0, 179.8, 180.0]);
        assert_eq!(b.sections[2].spline.knots()[0].point(), DVec2::ZERO);
        assert_eq!(
            b.sections[2].spline.knots()[2].point(),
            DVec2::new(0.0, 5.0)
        );
        assert_eq!(b.info.model, "M");
        assert_eq!(b.info.designer, "");
        assert!(b.volume() > 0.0);
    }

    #[test]
    fn missing_pieces() {
        let (b, w) = read_with_warnings(design(false, true).as_bytes()).unwrap();
        assert_eq!(w, [NO_DECK_WARNING]);
        assert_eq!(b.deck.len(), 5);
        let (_, w) = read_with_warnings(design(true, false).as_bytes()).unwrap();
        assert_eq!(w.len(), 3 + 2 + 2 + 3 * 3);
        assert!(read(b"<Shape3d_design><Board><Outline/></Board></Shape3d_design>").is_err());
    }

    #[test]
    fn corrupted_files_never_panic() {
        let base = design(true, true);
        let bytes = base.as_bytes();
        let mut seed = 9u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for cut in 0..bytes.len() {
            let _ = read(&bytes[..cut]);
        }
        let set = b"<>/0123456789.-e xyzP_";
        for _ in 0..500 {
            let mut v = bytes.to_vec();
            for _ in 0..1 + rnd() % 5 {
                let i = rnd() % v.len();
                v[i] = set[rnd() % set.len()];
            }
            if let Ok(b) = read(&v) {
                let _ = (b.length(), b.volume(), b.max_width());
            }
        }
        for len in [0, 1, 50, 500] {
            let junk: Vec<u8> = (0..len).map(|_| rnd() as u8).collect();
            assert!(read(&junk).is_err());
        }
    }
}
