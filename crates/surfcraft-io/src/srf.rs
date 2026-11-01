//! `.srf` binary boards (`docs/spec/file-formats.md` §8): little-endian, 32-bit floats in metres with x
//! measured from the nose, curves stored nose to tail, and the cross-sections as deck and bottom "caves"
//! (half-sections from the stringer to the rail). Only read.

use surfcraft_core::units::FOOT;
use surfcraft_core::{Board, CrossSection, DVec2, Knot, Spline};

use crate::Error;

/// Size of a curve record and of a cave record.
const CURVE_RECORD: usize = 76;
const CAVE_RECORD: usize = 158;
/// Outline end knots farther than this from the stringer get an extra knot on it (cm).
const TIP_GAP: f64 = 0.3;
/// Deck and bottom caves closer than this belong to the same section (cm).
const SAME_STATION: f64 = 2.0;
/// Height of the short vertical rail handles of cave-built sections (cm).
const RAIL_STEP: f64 = 0.2;

fn invalid(what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!(".srf: {what}"))
}

/// `x.clamp(lo, hi)` that cannot panic on an empty range.
fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if lo > hi {
        (lo + hi) / 2.0
    } else {
        x.max(lo).min(hi)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize, what: &str) -> Result<&[u8], Error> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.bytes.len())
            .ok_or_else(|| {
                invalid(format!(
                    "the file ends inside the {what} (offset {})",
                    self.pos
                ))
            })?;
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// Bytes up to and including the first `stop`.
    fn until(&mut self, stop: u8, what: &str) -> Result<&[u8], Error> {
        let rest = &self.bytes[self.pos..];
        let i = rest
            .iter()
            .position(|&b| b == stop)
            .ok_or_else(|| invalid(format!("no {:?} ending the {what}", stop as char)))?;
        self.take(i + 1, what)
    }

    fn f32(&mut self, what: &str) -> Result<f64, Error> {
        let b = self.take(4, what)?;
        let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(invalid(format!("a number in the {what} is not finite")))
        }
    }

    /// Three floats at `offset` within a record, z ignored.
    fn xy_at(rec: &[u8], offset: usize, what: &str) -> Result<DVec2, Error> {
        let f = |o: usize| {
            let v = f32::from_le_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]) as f64;
            if v.is_finite() {
                Ok(v)
            } else {
                Err(invalid(format!("a coordinate in the {what} is not finite")))
            }
        };
        Ok(DVec2::new(f(offset)?, f(offset + 4)?))
    }

    fn z_at(rec: &[u8], offset: usize, what: &str) -> Result<f64, Error> {
        let o = offset + 8;
        let v = f32::from_le_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]) as f64;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(invalid(format!("a coordinate in the {what} is not finite")))
        }
    }

    /// A signed 16-bit record count, refused when negative or larger than the data left.
    fn count(&mut self, record: usize, what: &str) -> Result<usize, Error> {
        let b = self.take(2, what)?;
        let n = i16::from_le_bytes([b[0], b[1]]);
        let n = usize::try_from(n).map_err(|_| invalid(format!("negative {what} count {n}")))?;
        if n * record > self.bytes.len() - self.pos {
            return Err(invalid(format!(
                "{n} {what} records announced but the file ends before them"
            )));
        }
        Ok(n)
    }
}

/// Mapping from file coordinates (metres, x from the nose) to the model (cm, x from the tail).
#[derive(Clone, Copy)]
struct Map {
    /// Board length from the header, cm.
    l: f64,
}

impl Map {
    fn point(self, p: DVec2) -> DVec2 {
        DVec2::new(self.l - 100.0 * p.x, 100.0 * p.y)
    }

    fn station(self, z: f64) -> f64 {
        self.l - 100.0 * z
    }
}

/// A curve: `count` records, nose to tail in the file, returned tail to nose.
fn curve(c: &mut Cursor, map: Map, n: usize, what: &str) -> Result<Vec<Knot>, Error> {
    let mut knots = Vec::with_capacity(n);
    for _ in 0..n {
        let rec = c.take(CURVE_RECORD, what)?;
        let e = map.point(Cursor::xy_at(rec, 0, what)?);
        let a = map.point(Cursor::xy_at(rec, 24, what)?);
        let b = map.point(Cursor::xy_at(rec, 36, what)?);
        knots.push(Knot::new(e, a, b));
    }
    knots.reverse();
    Ok(knots)
}

/// A cave: centreline point A with its handle B, rail point C with its handle D (cm, heights on the rocker
/// datum) and the station.
#[derive(Clone, Copy)]
struct Cave {
    station: f64,
    a: DVec2,
    b: DVec2,
    c: DVec2,
    d: DVec2,
}

fn caves(c: &mut Cursor, map: Map, l: f64, what: &str) -> Result<Vec<Cave>, Error> {
    c.take(1, what)?;
    let n = c.count(CAVE_RECORD, what)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let rec = c.take(CAVE_RECORD, what)?;
        let p = |o: usize| Cursor::xy_at(rec, o, what).map(|p| p * 100.0);
        let station = map.station(Cursor::z_at(rec, 6, what)?);
        // A is on the centre line by definition: drop the rounding noise of the 32-bit lateral coordinate, which
        // would put the section's centre knots a hair off the stringer.
        let mut a = p(6)?;
        a.x = 0.0;
        out.push(Cave {
            station: clamp(station, 0.5, l - 0.5),
            a,
            b: p(30)?,
            c: p(82)?,
            d: p(118)?,
        });
    }
    Ok(out)
}

/// The caves around station `s` and the blend weight towards the second: scanning from the last record to
/// the first, caves below `s` become both neighbours, the first one at or above `s` the upper neighbour.
fn neighbours(caves: &[Cave], s: f64) -> Option<(Cave, Cave, f64)> {
    let first = *caves.first()?;
    let (mut before, mut after) = (first, first);
    for cave in caves.iter().rev() {
        if cave.station < s {
            before = *cave;
            after = *cave;
        } else {
            after = *cave;
            break;
        }
    }
    let r = (s - before.station) / (after.station - before.station);
    let r = if r.is_finite() {
        r.clamp(0.0, 1.0)
    } else {
        1.0
    };
    Some((before, after, r))
}

fn lerp(a: DVec2, b: DVec2, r: f64) -> DVec2 {
    (1.0 - r) * a + r * b
}

/// Insert a section keeping the list sorted (after sections at the same position).
fn insert_sorted(sections: &mut Vec<CrossSection>, s: CrossSection) {
    let i = sections.partition_point(|x| x.position <= s.position);
    sections.insert(i, s);
}

/// A section from its four knots (non-continuous); the two rail knots are merged when they coincide.
fn section(position: f64, mut knots: Vec<Knot>) -> CrossSection {
    if knots.len() == 4 && (knots[1].point() - knots[2].point()).length() < 1e-4 {
        let upper = knots.remove(2);
        knots[1].points[2] = upper.points[2];
    }
    CrossSection::new(position, Spline::from_knots(knots))
}

/// Value of a curve's x range clamp.
fn x_range(s: &Spline) -> Option<(f64, f64)> {
    let xs = s.knots().iter().map(|k| k.point().x);
    let lo = xs.clone().fold(f64::INFINITY, f64::min);
    let hi = xs.fold(f64::NEG_INFINITY, f64::max);
    (lo <= hi).then_some((lo, hi))
}

/// Text of a header part: Windows-1252.
fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| crate::brd::windows_1252(b)).collect()
}

/// Read an `.srf` binary board.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    let mut c = Cursor { bytes, pos: 0 };
    c.until(b' ', "version")?;
    let name = c.until(b'*', "name")?;
    let name = text(&name[..name.len().saturating_sub(2)]);
    let comments = c.until(b'@', "comments")?;
    let comments = text(&comments[..comments.len() - 1]);

    c.take(11, "header")?;
    let map = Map {
        l: 100.0 * c.f32("header")?,
    };
    let mut outline_m = [0.0; 4];
    for v in &mut outline_m {
        *v = 100.0 * c.f32("header")?;
    }
    let wide_point = 100.0 * c.f32("header")?;
    let mut rocker_m = [0.0; 4];
    for v in &mut rocker_m {
        *v = 100.0 * c.f32("header")?;
    }
    c.take(16 + 16 + 113, "header")?;

    let n = c.count(CURVE_RECORD, "outline")?;
    let mut outline = curve(&mut c, map, n, "outline")?;
    let mut rest = Vec::new();
    for what in ["rocker", "upper rail", "deck", "lower rail"] {
        c.take(1, what)?;
        let n = c.count(CURVE_RECORD, what)?;
        rest.push(curve(&mut c, map, n, what)?);
    }
    let [bottom, upper_rail, mut deck, lower_rail]: [Vec<Knot>; 4] = rest
        .try_into()
        .map_err(|_| invalid("internal: curve count"))?;

    // Outline, closed on the stringer at the tips.
    if let Some(first) = outline.first_mut() {
        let e = first.point();
        if e.y > TIP_GAP {
            first.continuous = false;
            first.points[1] = DVec2::new(0.0, e.y * 2.0 / 3.0);
            outline.insert(
                0,
                Knot::new(DVec2::ZERO, DVec2::ZERO, DVec2::new(0.0, e.y / 3.0)),
            );
        }
    }
    if let Some(last) = outline.last_mut() {
        let e = last.point();
        if e.y > TIP_GAP {
            last.continuous = false;
            last.points[2] = DVec2::new(e.x, e.y * 2.0 / 3.0);
            outline.push(Knot::new(
                DVec2::new(e.x, 0.0),
                DVec2::new(e.x, e.y / 3.0),
                DVec2::new(e.x, 0.0),
            ));
        }
    }
    for (what, k) in [("outline", &outline), ("rocker", &bottom)] {
        if k.len() < 2 {
            return Err(invalid(format!("the {what} needs at least two points")));
        }
    }
    if deck.is_empty() {
        return Err(invalid("the deck has no points"));
    }

    // Deck tie-in: the deck starts and ends on the bottom's end points.
    let tie = |b: &Knot, d: &mut Knot, at_start: bool| {
        let mut k = b.clone();
        let (e, ed) = (k.point(), d.point());
        let towards_deck = e + 0.3 * (ed - e);
        let towards_bottom = ed + 0.3 * (e - ed);
        if at_start {
            k.points[2] = towards_deck;
            d.points[1] = towards_bottom;
        } else {
            k.points[1] = towards_deck;
            d.points[2] = towards_bottom;
        }
        k
    };
    let head = tie(&bottom[0], &mut deck[0], true);
    let last = deck.len() - 1;
    let tail = tie(&bottom[bottom.len() - 1], &mut deck[last], false);
    deck.insert(0, head);
    deck.push(tail);

    let mut b = Board {
        outline: Spline::from_knots(outline),
        bottom: Spline::from_knots(bottom),
        deck: Spline::from_knots(deck),
        ..Board::default()
    };
    let upper_rail = Spline::from_knots(upper_rail);
    let lower_rail = Spline::from_knots(lower_rail);
    let l = b.length();

    // Guide points from the measurements, tail to nose.
    let stations = [0.0, FOOT, l - wide_point, l - FOOT, l];
    let outline_y = [outline_m[3], outline_m[2], outline_m[1], outline_m[0], 0.0];
    let rocker_y = [rocker_m[3], rocker_m[2], 0.0, rocker_m[1], rocker_m[0]];
    for i in 0..5 {
        b.outline_guides.push(DVec2::new(stations[i], outline_y[i]));
        b.bottom_guides.push(DVec2::new(stations[i], rocker_y[i]));
    }

    let deck_caves = caves(&mut c, map, l, "deck caves")?;
    let bottom_caves = caves(&mut c, map, l, "bottom caves")?;

    let mut sections = vec![CrossSection::end(0.0), CrossSection::end(l)];
    for dc in &deck_caves {
        let s = dc.station;
        let h = DVec2::new(0.0, b.bottom.value_at(s));
        let mut knots = Vec::with_capacity(4);
        match bottom_caves
            .iter()
            .find(|bc| (bc.station - s).abs() < SAME_STATION)
        {
            Some(bc) => {
                knots.push(Knot::new(bc.a - h, DVec2::ZERO, bc.b - h));
                knots.push(Knot::new(
                    bc.c - h,
                    bc.d - h,
                    bc.c - h + DVec2::new(0.0, RAIL_STEP),
                ));
            }
            None => {
                let (centre, rail) = match neighbours(&bottom_caves, s) {
                    Some((p, q, r)) => {
                        (lerp(p.b - p.a, q.b - q.a, r), lerp(p.d - p.c, q.d - q.c, r))
                    }
                    None => (DVec2::ZERO, DVec2::ZERO),
                };
                knots.push(Knot::new(DVec2::ZERO, DVec2::ZERO, centre));
                let e = DVec2::new(b.outline.value_at(s), lower_rail.value_at(s) - h.y);
                knots.push(Knot::new(e, e + rail, e + DVec2::new(0.0, RAIL_STEP)));
            }
        }
        knots.push(Knot::new(
            dc.c - h,
            dc.c - h - DVec2::new(0.0, RAIL_STEP),
            dc.d - h,
        ));
        knots.push(Knot::new(dc.a - h, dc.b - h, DVec2::ZERO));
        insert_sorted(&mut sections, section(s, knots));
    }
    for bc in &bottom_caves {
        let s = bc.station;
        let n = sections.len();
        if sections[1..n - 1]
            .iter()
            .any(|x| (x.position - s).abs() < SAME_STATION)
        {
            continue;
        }
        let h = DVec2::new(0.0, b.bottom.value_at(s));
        let k1 = Knot::new(bc.a - h, DVec2::ZERO, bc.b - h);
        let k2 = Knot::new(bc.c - h, bc.d - h, bc.c - h + DVec2::new(0.0, RAIL_STEP));
        let (u, v) = match neighbours(&deck_caves, s) {
            Some((p, q, r)) => (lerp(p.d - p.c, q.d - q.c, r), lerp(p.b - p.a, q.b - q.a, r)),
            None => (DVec2::ZERO, DVec2::ZERO),
        };
        let s2 = x_range(&upper_rail).map_or(s, |(lo, hi)| clamp(s, lo, hi));
        let e3 = DVec2::new(k2.point().x, upper_rail.value_at(s2) - h.y);
        let k3 = Knot::new(e3, e3 - DVec2::new(0.0, RAIL_STEP), e3 + u);
        let e4 = DVec2::new(0.0, b.deck.value_at(s2) - h.y);
        let k4 = Knot::new(e4, e4 + v, DVec2::ZERO);
        insert_sorted(&mut sections, section(s, vec![k1, k2, k3, k4]));
    }
    b.sections = sections;
    b.info.model = name;
    b.info.comments = comments;
    b.ensure_end_sections();
    b.normalize_continuity(true);
    b.apply_locks();
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small hand-made file: straight outline, rocker and deck, one deck and one bottom cave at the middle.
    fn sample() -> Vec<u8> {
        let mut v = b"V1 Test board *notes\x92@".to_vec();
        let f = |v: &mut Vec<u8>, x: f32| v.extend(x.to_le_bytes());
        v.extend([0; 11]);
        f(&mut v, 2.0);
        for x in [0.1, 0.2, 0.25, 0.1, 1.0, 0.05, 0.0, 0.0, 0.03] {
            f(&mut v, x);
        }
        v.extend([0; 32 + 113]);
        let curve = |v: &mut Vec<u8>, pts: &[(f32, f32)], first: bool| {
            if !first {
                v.push(0);
            }
            v.extend((pts.len() as i16).to_le_bytes());
            for &(x, y) in pts {
                for p in [(x, y), (0.0, 0.0), (x + 0.1, y), (x - 0.1, y)] {
                    v.extend(p.0.to_le_bytes());
                    v.extend(p.1.to_le_bytes());
                    v.extend(0f32.to_le_bytes());
                }
                v.extend([0; 28]);
            }
        };
        // Nose first.
        curve(&mut v, &[(0.0, 0.0), (1.0, 0.25), (2.0, 0.0)], true);
        curve(&mut v, &[(0.0, 0.05), (1.0, 0.0), (2.0, 0.03)], false);
        curve(&mut v, &[(1.0, 0.04)], false);
        curve(&mut v, &[(0.1, 0.07), (1.0, 0.06), (1.9, 0.08)], false);
        curve(&mut v, &[(1.0, 0.01)], false);
        let cave = |v: &mut Vec<u8>, pts: [(f32, f32); 4]| {
            v.push(0);
            v.extend(1i16.to_le_bytes());
            let mut rec = vec![0u8; CAVE_RECORD];
            for (o, (x, y)) in [6, 30, 82, 118].into_iter().zip(pts) {
                rec[o..o + 4].copy_from_slice(&x.to_le_bytes());
                rec[o + 4..o + 8].copy_from_slice(&y.to_le_bytes());
                rec[o + 8..o + 12].copy_from_slice(&1.0f32.to_le_bytes());
            }
            v.extend(rec);
        };
        cave(
            &mut v,
            [(0.0, 0.06), (0.1, 0.06), (0.25, 0.03), (0.25, 0.05)],
        );
        cave(&mut v, [(0.0, 0.0), (0.1, 0.0), (0.25, 0.03), (0.25, 0.01)]);
        v
    }

    #[test]
    fn reads_a_hand_made_file() {
        let b = read(&sample()).unwrap();
        assert_eq!(b.info.model, "Test board");
        assert_eq!(b.info.comments, "notes\u{2019}");
        assert_eq!(b.length(), 200.0);
        assert_eq!(b.outline.len(), 3);
        assert!((b.width_at(100.0) - 50.0).abs() < 1e-4);
        assert_eq!(b.deck.len(), 5);
        assert_eq!(b.deck.knots()[0].point(), b.bottom.knots()[0].point());
        assert_eq!(b.sections.len(), 3);
        let s = &b.sections[1];
        assert!((s.position - 100.0).abs() < 1e-4);
        // Rail knots merged: centre bottom, rail, centre deck.
        assert_eq!(s.spline.len(), 3);
        assert!((s.spline.knots()[2].point() - DVec2::new(0.0, 6.0)).length() < 1e-4);
        assert_eq!(b.outline_guides.len(), 5);
        assert!((b.outline_guides[2] - DVec2::new(100.0, 20.0)).length() < 1e-4);
        assert!(b.volume() > 0.0);
    }

    #[test]
    fn corrupted_files_never_panic() {
        let base = sample();
        for n in 0..base.len() {
            assert!(read(&base[..n]).is_err(), "{n}");
        }
        let mut seed = 21u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..2000 {
            let mut v = base.clone();
            for _ in 0..1 + rnd() % 6 {
                let i = rnd() % v.len();
                v[i] = rnd() as u8;
            }
            if let Ok(b) = read(&v) {
                let _ = (b.length(), b.volume(), b.max_width());
            }
        }
        for len in [0, 1, 10, 300, 3000] {
            let junk: Vec<u8> = (0..len).map(|_| rnd() as u8).collect();
            let _ = read(&junk);
        }
    }
}
