//! STEP files in the "BCstep 1.0" layout (`docs/spec/file-formats.md` §6): an ISO 10303-21 file carrying a
//! bicubic NURBS approximation of the deck and the bottom for other CAD programs, and the exact Bezier board as
//! `BEZIER_CURVE` entities with special names.
//!
//! The reader is a real Part 21 parser (statements may span lines, comments, complex entities, escaped
//! strings) and rebuilds the board from the Bezier curves only. The writer produces the layout the reference
//! program reads: every point and curve statement on one line, the surfaces in the expected structure, ids
//! below 10 000.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use surfcraft_core::surface::{BOTTOM, DECK};
use surfcraft_core::{Board, CrossSection, DVec2, DVec3, Knot, Rail, Spline};

use crate::Error;
use crate::brd::num;

// ================================================================================================ Part 21

/// A parameter of an entity.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Ref(u64),
    Str(String),
    Num(f64),
    Enum(String),
    List(Vec<Value>),
    /// A typed parameter such as `LENGTH_MEASURE(0.01)`.
    Typed(String, Vec<Value>),
    /// `$`
    Unset,
    /// `*`
    Derived,
    /// `"…"` binary
    Binary,
}

/// One entity instance; a complex entity `( A(…) B(…) )` has several parts.
#[derive(Debug)]
struct Record {
    id: u64,
    parts: Vec<(String, Vec<Value>)>,
}

impl Record {
    /// Parameters of the part with this (upper-case) type name.
    fn part(&self, name: &str) -> Option<&[Value]> {
        self.parts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, p)| p.as_slice())
    }
}

/// Nesting limit for parameter lists: deeper files are refused instead of overflowing the stack.
const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    pos: usize,
}

fn err_at(text: &str, pos: usize, what: impl std::fmt::Display) -> Error {
    let pos = pos.min(text.len());
    let line = text.as_bytes()[..pos]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    Error::Invalid(format!("STEP line {line}: {what}"))
}

impl<'a> Parser<'a> {
    fn err(&self, what: impl std::fmt::Display) -> Error {
        err_at(self.text, self.pos, what)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    /// Skip white space and `/* … */` comments.
    fn ws(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C) => self.pos += 1,
                Some(b'/') if self.s.get(self.pos + 1) == Some(&b'*') => {
                    let start = self.pos;
                    match find(&self.s[self.pos + 2..], b"*/") {
                        Some(i) => self.pos += 2 + i + 2,
                        None => {
                            self.pos = start;
                            return Err(self.err("unterminated comment"));
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), Error> {
        self.ws()?;
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(format!("expected '{}', found {}", c as char, self.found())))
        }
    }

    fn found(&self) -> String {
        match self.text.get(self.pos..) {
            Some(rest) if !rest.is_empty() => {
                let snippet: String = rest.chars().take(12).collect();
                format!("{snippet:?}")
            }
            _ => "the end of the file".into(),
        }
    }

    /// `[A-Za-z_][A-Za-z0-9_-]*` (the dash admits `ISO-10303-21` and `END-ISO-10303-21`).
    fn keyword(&mut self) -> Result<String, Error> {
        self.ws()?;
        let start = self.pos;
        if !matches!(self.peek(), Some(c) if c.is_ascii_alphabetic() || c == b'_') {
            return Err(self.err(format!("expected a keyword, found {}", self.found())));
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.pos += 1;
        }
        Ok(self.text[start..self.pos].to_ascii_uppercase())
    }

    fn integer(&mut self) -> Result<u64, Error> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.text[start..self.pos]
            .parse()
            .map_err(|_| err_at(self.text, start, "bad entity number"))
    }

    fn number(&mut self) -> Result<f64, Error> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let digits = |p: &mut Self| {
            let s = p.pos;
            while matches!(p.peek(), Some(c) if c.is_ascii_digit()) {
                p.pos += 1;
            }
            p.pos - s
        };
        if digits(self) == 0 {
            return Err(err_at(self.text, start, "bad number"));
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            digits(self);
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if digits(self) == 0 {
                return Err(err_at(self.text, start, "bad number"));
            }
        }
        self.text[start..self.pos]
            .parse()
            .map_err(|_| err_at(self.text, start, "bad number"))
    }

    /// A `'…'` string (the opening quote is current), decoded.
    fn string(&mut self) -> Result<String, Error> {
        let start = self.pos;
        self.pos += 1;
        let mut raw = String::new();
        loop {
            let rest = &self.s[self.pos..];
            let Some(q) = rest.iter().position(|&b| b == b'\'') else {
                return Err(err_at(self.text, start, "unterminated string"));
            };
            raw.push_str(&self.text[self.pos..self.pos + q]);
            self.pos += q + 1;
            if self.peek() == Some(b'\'') {
                raw.push('\'');
                self.pos += 1;
            } else {
                return Ok(decode_string(&raw));
            }
        }
    }

    fn param(&mut self, depth: usize) -> Result<Value, Error> {
        self.ws()?;
        match self.peek() {
            Some(b'#') => {
                self.pos += 1;
                Ok(Value::Ref(self.integer()?))
            }
            Some(b'\'') => Ok(Value::Str(self.string()?)),
            Some(b'"') => {
                let start = self.pos;
                self.pos += 1;
                match self.s[self.pos..].iter().position(|&b| b == b'"') {
                    Some(i) => {
                        self.pos += i + 1;
                        Ok(Value::Binary)
                    }
                    None => Err(err_at(self.text, start, "unterminated binary")),
                }
            }
            Some(b'.') => {
                let start = self.pos;
                self.pos += 1;
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
                    self.pos += 1;
                }
                if self.peek() != Some(b'.') || self.pos == start + 1 {
                    return Err(err_at(self.text, start, "bad enumeration"));
                }
                self.pos += 1;
                Ok(Value::Enum(
                    self.text[start + 1..self.pos - 1].to_ascii_uppercase(),
                ))
            }
            Some(b'$') => {
                self.pos += 1;
                Ok(Value::Unset)
            }
            Some(b'*') => {
                self.pos += 1;
                Ok(Value::Derived)
            }
            Some(b'(') => Ok(Value::List(self.list(depth + 1)?)),
            Some(c) if c.is_ascii_digit() || c == b'+' || c == b'-' => {
                Ok(Value::Num(self.number()?))
            }
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.keyword()?;
                self.ws()?;
                if self.peek() != Some(b'(') {
                    return Err(self.err(format!("expected '(' after {name}")));
                }
                Ok(Value::Typed(name, self.list(depth + 1)?))
            }
            _ => Err(self.err(format!("expected a parameter, found {}", self.found()))),
        }
    }

    /// `( p, p, … )` (the opening parenthesis is current, after white space).
    fn list(&mut self, depth: usize) -> Result<Vec<Value>, Error> {
        if depth > MAX_DEPTH {
            return Err(self.err("parameters nested too deeply"));
        }
        self.expect(b'(')?;
        let mut out = Vec::new();
        self.ws()?;
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            out.push(self.param(depth)?);
            self.ws()?;
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b')') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {
                    return Err(self.err(format!("expected ',' or ')', found {}", self.found())));
                }
            }
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Decode the Part 21 string escapes: `\\`, `\X\hh` (ISO 8859-1), `\X2\…\X0\` (UTF-16), `\X4\…\X0\` (UTF-32),
/// `\S\c` (upper half of the code page); `\Pa\` page switches are dropped. Malformed escapes are kept as text.
fn decode_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let hex = |s: &str| u32::from_str_radix(s, 16).ok();
        if let Some(r) = rest.strip_prefix("\\\\") {
            out.push('\\');
            rest = r;
        } else if let Some(c) = rest
            .strip_prefix("\\X\\")
            .and_then(|r| r.get(..2))
            .and_then(hex)
            .and_then(char::from_u32)
        {
            out.push(c);
            rest = &rest[5..];
        } else if let Some((width, r)) = rest
            .strip_prefix("\\X2\\")
            .map(|r| (4, r))
            .or_else(|| rest.strip_prefix("\\X4\\").map(|r| (8, r)))
            && let Some(end) = r.find("\\X0\\")
            && end % width == 0
            && r[..end].bytes().all(|b| b.is_ascii_hexdigit())
        {
            let units: Vec<u32> = (0..end / width)
                .filter_map(|k| hex(&r[k * width..(k + 1) * width]))
                .collect();
            if width == 4 {
                let u16s: Vec<u16> = units.iter().map(|&u| u as u16).collect();
                out.extend(char::decode_utf16(u16s).map(|c| c.unwrap_or('\u{fffd}')));
            } else {
                out.extend(
                    units
                        .iter()
                        .map(|&u| char::from_u32(u).unwrap_or('\u{fffd}')),
                );
            }
            rest = &r[end + 4..];
        } else if let Some(r) = rest.strip_prefix("\\S\\")
            && let Some(c) = r.chars().next()
            && c.is_ascii()
        {
            out.push(char::from_u32(c as u32 + 128).unwrap_or('\u{fffd}'));
            rest = &r[1..];
        } else if rest.len() >= 4
            && rest.as_bytes()[1] == b'P'
            && rest.as_bytes()[3] == b'\\'
            && rest.is_char_boundary(4)
        {
            rest = &rest[4..];
        } else {
            out.push('\\');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// All entity instances of the file, in file order.
struct Model {
    records: Vec<Record>,
    by_id: HashMap<u64, usize>,
}

impl Model {
    fn parse(text: &str) -> Result<Model, Error> {
        let mut p = Parser {
            s: text.as_bytes(),
            text,
            pos: 0,
        };
        let mut records = Vec::new();
        let mut by_id = HashMap::new();
        let mut ended = false;
        loop {
            p.ws()?;
            let Some(c) = p.peek() else { break };
            if ended {
                // Anything after END-ISO-10303-21 is ignored, as the standard allows trailing data.
                break;
            }
            if c == b'#' {
                let start = p.pos;
                p.pos += 1;
                let id = p.integer()?;
                p.expect(b'=')?;
                p.ws()?;
                let mut parts = Vec::new();
                if p.peek() == Some(b'(') {
                    p.pos += 1;
                    loop {
                        p.ws()?;
                        if p.peek() == Some(b')') {
                            p.pos += 1;
                            break;
                        }
                        let name = p.keyword()?;
                        p.ws()?;
                        let params = p.list(1)?;
                        parts.push((name, params));
                    }
                    if parts.is_empty() {
                        return Err(err_at(text, start, "empty complex entity"));
                    }
                } else {
                    let name = p.keyword()?;
                    p.ws()?;
                    let params = p.list(1)?;
                    parts.push((name, params));
                }
                p.expect(b';')?;
                if by_id.insert(id, records.len()).is_some() {
                    return Err(err_at(text, start, format!("entity #{id} defined twice")));
                }
                records.push(Record { id, parts });
            } else {
                // Section keywords and header entities.
                let name = p.keyword()?;
                p.ws()?;
                if p.peek() == Some(b'(') {
                    p.list(1)?;
                }
                p.expect(b';')?;
                if name == "END-ISO-10303-21" {
                    ended = true;
                }
            }
        }
        if !ended {
            return Err(Error::Invalid(
                "STEP: the file is cut short (no END-ISO-10303-21; line)".into(),
            ));
        }
        Ok(Model { records, by_id })
    }

    fn get(&self, id: u64) -> Option<&Record> {
        self.by_id.get(&id).map(|&i| &self.records[i])
    }
}

// ================================================================================================ reading

/// Length unit of the file, as a conversion to centimetres.
#[derive(Clone, Copy, Debug)]
enum Unit {
    /// Divide by this.
    Div(f64),
    /// Multiply by this.
    Mul(f64),
}

impl Unit {
    fn to_cm(self, v: f64) -> f64 {
        match self {
            Unit::Div(d) => v / d,
            Unit::Mul(m) => v * m,
        }
    }
}

/// The length unit declared by the file: `SI_UNIT(prefix, .METRE.)` or a named conversion-based unit;
/// millimetres when none is found.
fn length_unit(model: &Model) -> Unit {
    for r in &model.records {
        if r.part("LENGTH_UNIT").is_none() {
            continue;
        }
        if let Some([prefix, Value::Enum(name), ..]) = r.part("SI_UNIT")
            && name == "METRE"
        {
            return match prefix {
                Value::Enum(p) => match p.as_str() {
                    "MILLI" => Unit::Div(10.0),
                    "CENTI" => Unit::Mul(1.0),
                    "DECI" => Unit::Mul(10.0),
                    "MICRO" => Unit::Div(10_000.0),
                    "KILO" => Unit::Mul(100_000.0),
                    _ => Unit::Div(10.0),
                },
                _ => Unit::Mul(100.0),
            };
        }
        if let Some([Value::Str(name), ..]) = r.part("CONVERSION_BASED_UNIT") {
            match name.to_ascii_uppercase().as_str() {
                "INCH" => return Unit::Mul(2.54),
                "FOOT" => return Unit::Mul(30.48),
                _ => {}
            }
        }
    }
    Unit::Div(10.0)
}

/// A named point (`CARTESIAN_POINT('name', (x, y[, z]))`), coordinates in the file's unit.
fn point(r: &Record) -> Option<(&str, DVec3)> {
    let [Value::Str(name), Value::List(c), ..] = r.part("CARTESIAN_POINT")? else {
        return None;
    };
    let mut v = [0.0; 3];
    if !(2..=3).contains(&c.len()) {
        return None;
    }
    for (slot, x) in v.iter_mut().zip(c) {
        let Value::Num(x) = x else { return None };
        *slot = *x;
    }
    Some((name, DVec3::from_array(v)))
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Outline,
    Bottom,
    Deck,
    Section,
}

impl Kind {
    /// The curve's plane: outline (X, Z), bottom and deck (X, Y), sections (Z, Y).
    fn project(self, p: DVec3) -> DVec2 {
        match self {
            Kind::Outline => DVec2::new(p.x, p.z),
            Kind::Bottom | Kind::Deck => DVec2::new(p.x, p.y),
            Kind::Section => DVec2::new(p.z, p.y),
        }
    }

    /// Whether an end knot flagged smooth really is, judged from its only stored handle `d` (relative to the
    /// knot). Outline and cross-section end knots lie on the stringer, where the curve continues as its mirror
    /// image on the other half of the board: that is smooth only if the handle crosses the centre line at right
    /// angles (within the 0.02 rad of the continuity test). Deck and bottom ends have no such constraint.
    fn smooth_end(self, d: DVec2) -> bool {
        let off_perpendicular = match self {
            Kind::Outline => d.x.abs().atan2(d.y.abs()),
            Kind::Section => d.y.abs().atan2(d.x.abs()),
            Kind::Bottom | Kind::Deck => return true,
        };
        d == DVec2::ZERO || off_perpendicular < 0.02
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Outline => "outline",
            Kind::Bottom => "bottom",
            Kind::Deck => "deck",
            Kind::Section => "section",
        }
    }
}

/// Continuity and auxiliary flags from an end point's name.
fn flags(name: &str) -> (bool, bool) {
    let n = name.to_ascii_lowercase();
    if n.contains("false false") {
        (false, false)
    } else if n.contains("false true") {
        (false, true)
    } else if n.contains("true false") {
        (true, false)
    } else {
        (true, true)
    }
}

/// A Bezier curve read from the file: knots and, for sections, the position.
struct Curve {
    kind: Kind,
    knots: Vec<Knot>,
    position: f64,
}

fn curve(model: &Model, r: &Record, kind: Kind, unit: Unit) -> Result<Curve, Error> {
    let what = |m: String| Error::Invalid(format!("STEP: {} curve #{}: {m}", kind.name(), r.id));
    let refs = match r.part("BEZIER_CURVE") {
        Some([_, _, Value::List(refs), ..]) => refs,
        _ => return Err(what("no control point list".into())),
    };
    let mut pts = Vec::with_capacity(refs.len());
    for v in refs {
        let Value::Ref(id) = v else {
            return Err(what("a control point is not a reference".into()));
        };
        let (name, p) = model
            .get(*id)
            .and_then(point)
            .ok_or_else(|| what(format!("#{id} is not a CARTESIAN_POINT")))?;
        let p = DVec3::new(unit.to_cm(p.x), unit.to_cm(p.y), unit.to_cm(p.z));
        if !p.is_finite() {
            return Err(what(format!("#{id} has a coordinate that is not a number")));
        }
        pts.push((name, p));
    }
    let m = pts.len();
    if m == 0 || (m - 1) % 3 != 0 {
        return Err(what(format!(
            "{m} control points; a piecewise cubic needs 3 per knot minus 2"
        )));
    }
    let position = pts[0].1.x;
    let pr = |i: usize| kind.project(pts[i].1);
    let mut knots = Vec::with_capacity(m.div_ceil(3));
    if m == 1 {
        // A one-point curve (the zero-size end sections).
        let p = DVec2::new(pts[0].1.z, pts[0].1.y);
        let mut k = Knot::new(p, p, p);
        (k.continuous, k.other) = flags(pts[0].0);
        knots.push(k);
    } else {
        for j in 0..=(m - 1) / 3 {
            let e = 3 * j;
            let point = pr(e);
            let prev = if e > 0 { Some(pr(e - 1)) } else { None };
            let next = if e + 1 < m { Some(pr(e + 1)) } else { None };
            let (mut continuous, other) = flags(pts[e].0);
            // The outer handles of the end knots are not stored: smooth end knots get their inner handle
            // mirrored through the point, others a handle on the point.
            if let Some(inner) = if e == 0 {
                next
            } else if e + 1 == m {
                prev
            } else {
                None
            } {
                continuous &= kind.smooth_end(inner - point);
            }
            let mirror = |h: Option<DVec2>| match h {
                Some(h) if continuous => 2.0 * point - h,
                _ => point,
            };
            let prev_h = prev.unwrap_or_else(|| mirror(next));
            let next_h = next.unwrap_or_else(|| mirror(prev));
            let mut k = Knot::new(point, prev_h, next_h);
            (k.continuous, k.other) = (continuous, other);
            knots.push(k);
        }
    }
    Ok(Curve {
        kind,
        knots,
        position,
    })
}

/// Read a STEP file written by SurfCraft or BoardCAD.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    let text = crate::brd::decode(bytes);
    let model = Model::parse(&text)?;
    let unit = length_unit(&model);

    let mut outline = None;
    let mut bottom = None;
    let mut deck = None;
    let mut sections: Vec<Curve> = Vec::new();
    for r in &model.records {
        let Some([Value::Str(label), ..]) = r.part("BEZIER_CURVE") else {
            continue;
        };
        let kind = match label.as_str() {
            "outline" => Kind::Outline,
            "bottom" => Kind::Bottom,
            "deck" => Kind::Deck,
            "section" => Kind::Section,
            _ => continue,
        };
        let c = curve(&model, r, kind, unit)?;
        match c.kind {
            Kind::Outline => outline = Some(c),
            Kind::Bottom => bottom = Some(c),
            Kind::Deck => deck = Some(c),
            Kind::Section => sections.push(c),
        }
    }
    let Some(outline) = outline else {
        return Err(Error::Invalid(
            "STEP: not a BoardCAD STEP file (no 'outline' BEZIER_CURVE)".into(),
        ));
    };
    let mut b = Board::default();
    let need = |c: Option<Curve>, name: &str| -> Result<Vec<Knot>, Error> {
        match c {
            Some(c) if c.knots.len() >= 2 => Ok(c.knots),
            Some(_) => Err(Error::Invalid(format!(
                "STEP: the {name} curve needs at least two knots"
            ))),
            None => Err(Error::Invalid(format!(
                "STEP: no '{name}' BEZIER_CURVE in the file"
            ))),
        }
    };
    b.outline = Spline::from_knots(need(Some(outline), "outline")?);
    b.bottom = Spline::from_knots(need(bottom, "bottom")?);
    b.deck = Spline::from_knots(need(deck, "deck")?);
    let mut sections: Vec<CrossSection> = sections
        .into_iter()
        .map(|c| CrossSection::new(c.position, Spline::from_knots(c.knots)))
        .collect();

    // Guide points, in ascending id order.
    let mut by_id: BTreeMap<u64, (String, DVec3)> = BTreeMap::new();
    for r in &model.records {
        if let Some((name, p)) = point(r) {
            let name = name.trim().to_ascii_lowercase();
            if name.ends_with("guide point") {
                by_id.insert(r.id, (name, p));
            }
        }
    }
    for (name, p) in by_id.into_values() {
        let p = DVec3::new(unit.to_cm(p.x), unit.to_cm(p.y), unit.to_cm(p.z));
        if !p.is_finite() {
            continue;
        }
        match name.as_str() {
            "outline guide point" => b.outline_guides.push(Kind::Outline.project(p)),
            "bottom guide point" => b.bottom_guides.push(Kind::Bottom.project(p)),
            "deck guide point" => b.deck_guides.push(Kind::Deck.project(p)),
            _ => {
                let n = name
                    .strip_prefix("cross section")
                    .and_then(|r| r.strip_suffix("guide point"))
                    .and_then(|n| n.trim().parse::<usize>().ok());
                if let Some(s) = n.and_then(|n| sections.get_mut(n)) {
                    s.guide_points.push(Kind::Section.project(p));
                }
            }
        }
    }
    sections.sort_by(|a, c| a.position.total_cmp(&c.position));
    b.sections = sections;

    if let Some(name) = model.records.iter().find_map(|r| match r.part("PRODUCT") {
        Some([Value::Str(name), ..]) => Some(name.clone()),
        _ => None,
    }) {
        b.info.name = name;
    }
    b.ensure_end_sections();
    // The reference program judged its flags with a test blind to mirror images about the vertical; judge them
    // again with the corrected one (demote only), as every other reader does.
    b.normalize_continuity(false);
    b.apply_locks();
    Ok(b)
}

// ================================================================================================ writing

/// Substrings that make the reference program's line-based reader treat a line as something it is not.
const TRIGGERS: [&str; 7] = [
    "CARTESIAN_TRANSFORMATION_OPERATOR",
    "CARTESIAN_POINT",
    "DIRECTION",
    "BEZIER_CURVE",
    "BOUNDED_SURFACE",
    "FILE_NAME",
    "END-ISO-10303-21",
];

/// A Part 21 string literal: quotes doubled, backslashes doubled, other characters outside printable ASCII as
/// `\X2\…\X0\`. Words that would mislead the reference program's line-based reader get their first letter
/// written as `\X\hh`.
fn step_string(s: &str) -> String {
    let mut hide = vec![false; s.len()];
    for t in TRIGGERS {
        for (i, _) in s.match_indices(t) {
            hide[i] = true;
        }
    }
    let mut out = String::from("'");
    let mut wide = Vec::new();
    let flush = |out: &mut String, wide: &mut Vec<u16>| {
        if !wide.is_empty() {
            out.push_str("\\X2\\");
            for u in wide.drain(..) {
                let _ = write!(out, "{u:04X}");
            }
            out.push_str("\\X0\\");
        }
    };
    for (i, c) in s.char_indices() {
        if (' '..='~').contains(&c) {
            flush(&mut out, &mut wide);
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ if hide[i] => {
                    let _ = write!(out, "\\X\\{:02X}", c as u32);
                }
                _ => out.push(c),
            }
        } else {
            let mut buf = [0u16; 2];
            wide.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut out, &mut wide);
    out.push('\'');
    out
}

/// Civil date and time (UTC) of a Unix timestamp, as `YYYY-MM-DDThh:mm:ss`.
fn iso_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days since 1970-01-01 to a proleptic Gregorian date.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Statements of the DATA section with consecutive ids.
struct Writer {
    out: String,
    next: u64,
}

impl Writer {
    fn take(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn point(&mut self, id: u64, name: &str, p: DVec3) {
        let _ = writeln!(
            self.out,
            "#{id} = CARTESIAN_POINT ( '{name}',  ( {}, {}, {}) );",
            num(p.x),
            num(p.y),
            num(p.z)
        );
    }
}

/// Rows along the board (tip rows included) and columns across it (odd) of the written surfaces.
const ROWS: usize = 33;
const COLS: usize = 15;

/// Control-point grids of the deck and the bottom surface in file coordinates (mm, Z inverted): rows from the
/// tail tip to the nose tip, the deck's columns from one rail apex over the deck to the other, the bottom's
/// from that apex under the board back to the first. Both tip rows collapse to the tip point.
fn surface_grids(b: &Board) -> [Vec<Vec<DVec3>>; 2] {
    let mm = |p: DVec3| DVec3::new(10.0 * p.x, 10.0 * p.z, -10.0 * p.y);
    let usable = b.outline.len() >= 2 && b.bottom.len() >= 2 && b.deck.len() >= 2;
    let tip = |k: Option<&Knot>| {
        let p = k.map_or(DVec2::ZERO, |k| k.point());
        DVec3::new(10.0 * p.x, 10.0 * p.y, 0.0)
    };
    let (tail, nose) = (tip(b.bottom.first()), tip(b.bottom.last()));
    let l = b.length();
    let h = (COLS / 2) as f64;
    let mut deck = Vec::with_capacity(ROWS);
    let mut bottom = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        if i == 0 || i + 1 == ROWS || !usable {
            let p = if i + 1 == ROWS { nose } else { tail };
            deck.push(vec![p; COLS]);
            bottom.push(vec![p; COLS]);
            continue;
        }
        let x = l * (1.0 - (std::f64::consts::PI * i as f64 / (ROWS - 1) as f64).cos()) / 2.0;
        let sd = b.station(x, DECK.0, DECK.1);
        let sb = b.station(x, BOTTOM.0, BOTTOM.1);
        // The y < 0 (right rail) half of an asymmetric board has its own stations.
        let (sd_r, sb_r) = if b.is_asymmetric() {
            (
                b.station_on(x, DECK.0, DECK.1, Rail::Right),
                b.station_on(x, BOTTOM.0, BOTTOM.1, Rail::Right),
            )
        } else {
            (sd.clone(), sb.clone())
        };
        let finite = |p: DVec3| if p.is_finite() { p } else { DVec3::ZERO };
        // Columns j = 0 … COLS-1 run over t = -1 … 1; the side is the sign of t.
        let row = |f: &dyn Fn(f64) -> DVec3| -> Vec<DVec3> {
            (0..COLS).map(|j| finite(f(j as f64 / h - 1.0))).collect()
        };
        deck.push(row(&|t| {
            let st = if t > 0.0 { &sd_r } else { &sd };
            let mut p = st.point(t.abs());
            p.y *= -t.signum();
            mm(p)
        }));
        bottom.push(row(&|t| {
            let st = if t < 0.0 { &sb_r } else { &sb };
            let mut p = st.point(1.0 - t.abs());
            p.y *= t.signum();
            mm(p)
        }));
    }
    [deck, bottom]
}

/// One `BOUNDED_SURFACE` complex entity with its control points.
fn surface(w: &mut Writer, label: &str, grid: &[Vec<DVec3>]) -> u64 {
    let id = w.take();
    let (rows, cols) = (grid.len(), grid.first().map_or(0, Vec::len));
    let first = w.next;
    let ids = |r: usize| -> String {
        (0..cols)
            .map(|c| format!("#{}", first + (r * cols + c) as u64))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mults = |n: usize| -> String {
        let mut v = vec!["4".to_string()];
        v.extend(std::iter::repeat_n("1".to_string(), n - 4));
        v.push("4".into());
        format!("( {})", v.join(", "))
    };
    let knots = |n: usize| -> String {
        let v: Vec<String> = (0..n - 2).map(|k| num(k as f64)).collect();
        format!("( {} )", v.join(", "))
    };
    let o = &mut w.out;
    let _ = writeln!(o, "#{id}=(\nBOUNDED_SURFACE()\nB_SPLINE_SURFACE(3,3,(");
    for r in 0..rows {
        let end = if r + 1 == rows { ")), " } else { "), " };
        let _ = writeln!(o, "({}{end}", ids(r));
    }
    let _ = writeln!(
        o,
        ".UNSPECIFIED.,.F.,.F.,.F.)\nB_SPLINE_SURFACE_WITH_KNOTS("
    );
    let _ = writeln!(o, "{},\n{},", mults(rows), mults(cols));
    let _ = writeln!(o, "{},\n{},", knots(rows), knots(cols));
    let _ = writeln!(
        o,
        ".UNSPECIFIED.)\nGEOMETRIC_REPRESENTATION_ITEM()\nRATIONAL_B_SPLINE_SURFACE(("
    );
    let weights = format!("({})", vec!["1.0"; cols].join(", "));
    for r in 0..rows {
        let end = if r + 1 == rows { "))) " } else { "), " };
        let _ = writeln!(o, "{}{end}", &weights[..weights.len() - 1]);
    }
    let _ = writeln!(o, "REPRESENTATION_ITEM('{label}')\nSURFACE()\n);");
    for row in grid {
        for &p in row {
            let pid = w.take();
            w.point(pid, "NONE", p);
        }
    }
    id
}

/// A `BEZIER_CURVE` with its control points and guide points; one id is left unused after it.
fn bezier(
    w: &mut Writer,
    label: &str,
    s: &Spline,
    to3d: &dyn Fn(DVec2) -> DVec3,
    guides: &[DVec3],
    guide_name: &str,
) {
    let id = w.take();
    let knots = s.knots();
    let n = knots.len();
    let mut pts: Vec<(String, DVec3)> = Vec::with_capacity(3 * n);
    for (i, k) in knots.iter().enumerate() {
        if i > 0 {
            pts.push(("NONE".into(), to3d(k.prev())));
        }
        pts.push((format!("{} {}", k.continuous, k.other), to3d(k.point())));
        if i + 1 < n {
            pts.push(("NONE".into(), to3d(k.next())));
        }
    }
    let refs: Vec<String> = (0..pts.len())
        .map(|i| format!("#{}", id + 1 + i as u64))
        .collect();
    let _ = writeln!(
        w.out,
        "#{id} = BEZIER_CURVE( '{label}',3,({}),.UNSPECIFIED.,.F.,.F.);",
        refs.join(",")
    );
    for (name, p) in pts {
        let pid = w.take();
        w.point(pid, &name, p);
    }
    for &g in guides {
        let pid = w.take();
        w.point(pid, guide_name, g);
    }
    w.take();
}

/// Write the board as STEP: NURBS deck and bottom surfaces plus the exact Bezier curves.
pub fn write(board: &Board) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    write_at(board, secs)
}

/// [`write`] with a fixed time stamp (seconds since 1970, UTC).
pub fn write_at(board: &Board, unix_secs: u64) -> String {
    let name = board.info.name.trim();
    let file_name = if name.is_empty() {
        "board.stp".to_owned()
    } else {
        format!("{name}.stp")
    };
    let mut w = Writer {
        out: String::with_capacity(64 * 1024),
        next: 200,
    };
    let _ = write!(
        w.out,
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION (( 'STEP AP203' ), '1' );\n\
         FILE_NAME ({},'{}',( '' ),( '' ),'BCstep 1.0','BoardCAD','');\n\
         FILE_SCHEMA (( 'CONFIG_CONTROL_DESIGN' ));\nENDSEC;\nDATA;\n",
        step_string(&file_name),
        iso_time(unix_secs)
    );

    let [deck_grid, bottom_grid] = surface_grids(board);
    let deck_id = surface(&mut w, "deck", &deck_grid);
    w.take();
    let bottom_id = surface(&mut w, "bottom", &bottom_grid);

    let op = w.take();
    let _ = writeln!(
        w.out,
        "#{op} = CARTESIAN_TRANSFORMATION_OPERATOR( '','','',#{}, #{}, #{},1.0);",
        op + 1,
        op + 2,
        op + 3
    );
    for (name, kind, p) in [
        ("x axis", "DIRECTION", DVec3::X),
        ("y axis", "DIRECTION", DVec3::Y),
        ("local origin", "CARTESIAN_POINT", DVec3::ZERO),
    ] {
        let id = w.take();
        let _ = writeln!(
            w.out,
            "#{id} = {kind} ( '{name}',  ( {}, {}, {}) );",
            num(p.x),
            num(p.y),
            num(p.z)
        );
    }

    let plan = |p: DVec2| DVec3::new(10.0 * p.x, 0.0, 10.0 * p.y);
    let side = |p: DVec2| DVec3::new(10.0 * p.x, 10.0 * p.y, 0.0);
    let guides =
        |g: &[DVec2], f: &dyn Fn(DVec2) -> DVec3| g.iter().map(|&p| f(p)).collect::<Vec<_>>();
    bezier(
        &mut w,
        "outline",
        &board.outline,
        &plan,
        &guides(&board.outline_guides, &plan),
        "outline guide point",
    );
    bezier(
        &mut w,
        "bottom",
        &board.bottom,
        &side,
        &guides(&board.bottom_guides, &side),
        "bottom guide point",
    );
    bezier(
        &mut w,
        "deck",
        &board.deck,
        &side,
        &guides(&board.deck_guides, &side),
        "deck guide point",
    );
    for (n, s) in board.sections.iter().enumerate() {
        let pos = s.position;
        let at = move |p: DVec2| DVec3::new(10.0 * pos, 10.0 * p.y, 10.0 * p.x);
        bezier(
            &mut w,
            "section",
            &s.spline,
            &at,
            &guides(&s.guide_points, &at),
            &format!("cross section {n} guide point"),
        );
    }

    let t = w.next;
    let _ = write!(
        w.out,
        "#{t} = PRODUCT_DEFINITION_SHAPE('','',#{});\n\
         #{} = PRODUCT_DEFINITION('design','',#{});\n\
         #{} = PRODUCT_DEFINITION_FORMATION('','',#{});\n\
         #{} = PRODUCT({}, 'Generated by SurfCraft','');\n\
         #{} =SHAPE_DEFINITION_REPRESENTATION(#{t},#{});\n",
        t + 1,
        t + 1,
        t + 2,
        t + 2,
        t + 3,
        t + 3,
        step_string(&board.info.name),
        t + 4,
        t + 26,
    );
    for (k, surf) in [(5, deck_id), (10, bottom_id)] {
        let f = t + k;
        let _ = write!(
            w.out,
            "#{f} = ADVANCED_FACE('',(#{}),#{surf},.F.);\n\
             #{} = FACE_BOUND('',#{},.T.);\n\
             #{} = VERTEX_LOOP('',#{});\n\
             #{} = VERTEX_POINT('',#{});\n\
             #{} = CARTESIAN_POINT('',(0.0,0.0,0.0));\n",
            f + 1,
            f + 1,
            f + 2,
            f + 2,
            f + 3,
            f + 3,
            f + 4,
            f + 4,
        );
    }
    let i = |k: u64| t + k;
    let _ = write!(
        w.out,
        "#{}=CLOSED_SHELL('',(#{},#{}));\n\
         #{}=MANIFOLD_SOLID_BREP('',#{});\n\
         #{}=DRAUGHTING_PRE_DEFINED_COLOUR('white');\n\
         #{}=FILL_AREA_STYLE_COLOUR('Plastic (White)',#{});\n\
         #{}=FILL_AREA_STYLE('Plastic (White)',(#{}));\n\
         #{}=SURFACE_STYLE_FILL_AREA(#{});\n\
         #{}=SURFACE_SIDE_STYLE('Plastic (White)',(#{}));\n\
         #{}=SURFACE_STYLE_USAGE(.BOTH.,#{});\n\
         #{}=PRESENTATION_STYLE_ASSIGNMENT((#{}));\n\
         #{}=STYLED_ITEM('',(#{}),#{});\n\
         #{}=MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',(#{}),#{});\n\
         #{}=ADVANCED_BREP_SHAPE_REPRESENTATION('ABSR',(#{}),#{});\n\
         #{} = ( GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{})) \
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{},#{},#{})) REPRESENTATION_CONTEXT('Context #1', \
         '3D Context with UNIT and UNCERTAINTY') );\n\
         #{} = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n\
         #{} = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );\n\
         #{} = ( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() );\n\
         #{} = UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.01),#{},'distance_accuracy_value','confusion accuracy');\n\
         ENDSEC;\nEND-ISO-10303-21;\n",
        i(15),
        i(5),
        i(10),
        i(16),
        i(15),
        i(17),
        i(18),
        i(17),
        i(19),
        i(18),
        i(20),
        i(19),
        i(21),
        i(20),
        i(22),
        i(21),
        i(23),
        i(22),
        i(24),
        i(23),
        i(16),
        i(25),
        i(24),
        i(27),
        i(26),
        i(16),
        i(27),
        i(27),
        i(31),
        i(28),
        i(29),
        i(30),
        i(28),
        i(29),
        i(30),
        i(31),
        i(28),
    );
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        for s in [
            "plain",
            "It's",
            "back\\slash",
            "Fish — ñ 波 🏄",
            "NEW DIRECTION of BEZIER_CURVE",
            "",
        ] {
            let lit = step_string(s);
            assert!(lit.is_ascii(), "{lit}");
            for t in TRIGGERS {
                assert!(!lit.contains(t), "{lit}");
            }
            let mut p = Parser {
                s: lit.as_bytes(),
                text: &lit,
                pos: 0,
            };
            assert_eq!(p.string().unwrap(), s);
        }
        assert_eq!(decode_string("a\\S\\Ab\\PA\\c\\X\\E9"), "a\u{c1}bcé");
        assert_eq!(
            decode_string("bad \\X2\\00E\\X0\\ end\\"),
            "bad \\X2\\00E\\X0\\ end\\"
        );
    }

    #[test]
    fn time_stamps() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00");
        assert_eq!(iso_time(1_793_575_783), "2026-11-01T23:29:43");
        assert_eq!(iso_time(951_782_400), "2000-02-29T00:00:00");
    }

    #[test]
    fn numbers_and_units() {
        let m = Model::parse(
            "ISO-10303-21;DATA;#1=( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT($,.METRE.) );\
             #2=CARTESIAN_POINT('p',(-1.5E+2,+2.,3));ENDSEC;END-ISO-10303-21;",
        )
        .unwrap();
        assert!(matches!(length_unit(&m), Unit::Mul(100.0)));
        let (_, p) = point(m.get(2).unwrap()).unwrap();
        assert_eq!(p, DVec3::new(-150.0, 2.0, 3.0));
        for bad in [
            "#1=A(1.0.0);END-ISO-10303-21;",
            "#1=A(.X);END-ISO-10303-21;",
            "#1=A(1e);END-ISO-10303-21;",
            "#1=A('x);END-ISO-10303-21;",
            "#1=A(1)\n#2=B(2);END-ISO-10303-21;",
            "#1=A();#1=B();END-ISO-10303-21;",
            "#1=();END-ISO-10303-21;",
            "#99999999999999999999999=A();END-ISO-10303-21;",
            "#1=A(();END-ISO-10303-21;",
            "#1=A();",
        ] {
            assert!(Model::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn writes_ids_below_ten_thousand_and_reads_back() {
        let board =
            crate::brd::read_text(include_str!("../../../tests/fixtures/longboard.brd")).unwrap();
        let text = write_at(&board, 0);
        assert!(text.contains("'1970-01-01T00:00:00'"));
        let m = Model::parse(&text).unwrap();
        assert!(m.records.iter().all(|r| r.id < 10_000));
        // Every reference resolves.
        fn refs(v: &Value, out: &mut Vec<u64>) {
            match v {
                Value::Ref(id) => out.push(*id),
                Value::List(l) | Value::Typed(_, l) => l.iter().for_each(|v| refs(v, out)),
                _ => {}
            }
        }
        let mut all = Vec::new();
        for r in &m.records {
            for (_, p) in &r.parts {
                p.iter().for_each(|v| refs(v, &mut all));
            }
        }
        assert!(all.iter().all(|id| m.get(*id).is_some()));
        // The surfaces: deck first column mirrors the bottom's first column, and the rows close at the tips.
        let surf = |label: &str| {
            m.records
                .iter()
                .find(|r| r.part("REPRESENTATION_ITEM") == Some(&[Value::Str(label.into())][..]))
                .unwrap()
        };
        let grid = |r: &Record| -> Vec<Vec<DVec3>> {
            let Some([_, _, Value::List(rows), ..]) = r.part("B_SPLINE_SURFACE") else {
                panic!()
            };
            rows.iter()
                .map(|row| {
                    let Value::List(ids) = row else { panic!() };
                    ids.iter()
                        .map(|v| {
                            let Value::Ref(id) = v else { panic!() };
                            point(m.get(*id).unwrap()).unwrap().1
                        })
                        .collect()
                })
                .collect()
        };
        let (deck, bottom) = (grid(surf("deck")), grid(surf("bottom")));
        assert_eq!((deck.len(), deck[0].len()), (ROWS, COLS));
        for (d, b) in deck.iter().zip(&bottom) {
            assert!((d[COLS - 1] - b[0]).length() < 1e-6);
            assert!((d[0] - DVec3::new(b[0].x, b[0].y, -b[0].z)).length() < 1e-6);
            assert!(d[COLS / 2].y >= b[COLS / 2].y - 1e-6);
        }
        // Width of the surfaces near the middle matches the outline.
        let mid = &bottom[ROWS / 2];
        let x = mid[0].x / 10.0;
        assert!((mid[0].z.abs() / 10.0 - board.width_at(x) / 2.0).abs() < 0.2);
    }

    #[test]
    fn random_input_never_panics() {
        let base = write_at(
            &crate::brd::read_text(include_str!("../../../tests/fixtures/shortboard.brd")).unwrap(),
            0,
        );
        let bytes = base.as_bytes();
        let mut seed = 5u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for cut in (0..bytes.len()).step_by(97) {
            assert!(read(&bytes[..cut]).is_err());
        }
        for _ in 0..300 {
            let mut v = bytes.to_vec();
            for _ in 0..1 + rnd() % 8 {
                let i = rnd() % v.len();
                let set = b"0123456789-+.,()#=$*'\"/\\E;: \nX";
                v[i] = set[rnd() % set.len()];
            }
            if let Ok(b) = read(&v) {
                let _ = (b.length(), b.volume());
            }
        }
        for len in [0, 1, 10, 100, 1000] {
            let junk: Vec<u8> = (0..len).map(|_| rnd() as u8).collect();
            let _ = read(&junk);
        }
    }
}
