//! The native `.brd` board file: a line-oriented text format of `pNN : value` fields and bracketed blocks for
//! the curves and the cross-sections (`docs/spec/file-formats.md` §3).
//!
//! Reading is tolerant: unknown fields are ignored, missing fields keep their defaults, blank lines inside
//! blocks are skipped and a file cut short yields what was read. Writing reproduces the layout the reference
//! program expects, plus the fields it reads but never wrote, so that a read → write → read round trip is
//! lossless.

use std::fmt::Write as _;

use surfcraft_core::{Board, BoardInfo, CrossSection, DVec2, Knot, MachineData, Spline};

use crate::Error;

pub use crate::partial::{Partial, read_partial, write_outline, write_profile, write_section};

// ------------------------------------------------------------------------------------------------ reading

/// Read a `.brd` file (plain or legacy-encrypted).
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    if crate::legacy::is_encrypted(bytes) {
        let plain = crate::legacy::decrypt(bytes)?;
        return read_text(&decode(&plain));
    }
    read_text(&decode(bytes))
}

/// Text of a file: UTF-8 (a leading byte-order mark is skipped), or Windows-1252 when the bytes are not
/// valid UTF-8.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
    }
}

fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// Lines of the text with their 1-based numbers; LF, CR LF and lone CR all end a line. The flag tells whether
/// the line was terminated (the last line of a file cut short is not).
pub(crate) fn lines(text: &str) -> Vec<(usize, &str, bool)> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut n = 0;
    while !rest.is_empty() {
        n += 1;
        match rest.find(['\n', '\r']) {
            Some(i) => {
                out.push((n, &rest[..i], true));
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
            }
            None => {
                out.push((n, rest, false));
                rest = "";
            }
        }
    }
    out
}

fn invalid(line: usize, what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!(".brd line {line}: {what}"))
}

/// A real number in any decimal syntax.
pub(crate) fn parse_real(s: &str) -> Option<f64> {
    let s = s.trim();
    match s {
        "NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => {
            let ok = !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'));
            if ok { s.parse().ok() } else { None }
        }
    }
}

pub(crate) fn parse_finite(s: &str) -> Option<f64> {
    parse_real(s).filter(|v| v.is_finite())
}

/// An integer, also accepted when written as a real with no fraction (`3.0`).
fn parse_int(s: &str) -> Option<i32> {
    let s = s.trim();
    if let Ok(v) = s.parse::<i32>() {
        return Some(v);
    }
    parse_real(s)
        .filter(|v| v.is_finite() && v.fract() == 0.0 && v.abs() <= i32::MAX as f64)
        .map(|v| v as i32)
}

fn parse_bool(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("true")
}

/// Fill `slots` from a `[a,b,c]` list; missing elements keep their value, extra ones are ignored.
fn parse_array(s: &str, slots: &mut [f64]) -> Result<(), String> {
    let inner = s
        .trim()
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or("expected a [..] list")?;
    if inner.trim().is_empty() {
        return Ok(());
    }
    for (i, item) in inner.split(',').enumerate() {
        let v = parse_real(item).ok_or_else(|| format!("bad number {:?}", item.trim()))?;
        if let Some(slot) = slots.get_mut(i) {
            *slot = v;
        }
    }
    Ok(())
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
}

/// `(cp [px,py,hpx,hpy,hnx,hny] continuous aux)`.
fn parse_knot(line: &str) -> Option<Knot> {
    let rest = line.trim().strip_prefix("(cp")?.trim_start();
    let rest = rest.strip_suffix(')')?;
    let open = rest.find('[')?;
    let close = rest.find(']')?;
    if open != 0 || close < open {
        return None;
    }
    let nums: Vec<f64> = rest[open + 1..close]
        .split(',')
        .map(parse_finite)
        .collect::<Option<_>>()?;
    let [px, py, hpx, hpy, hnx, hny] = nums[..] else {
        return None;
    };
    let flags: Vec<&str> = rest[close + 1..].split_whitespace().collect();
    let [c, aux] = flags[..] else {
        return None;
    };
    let mut k = Knot::new(
        DVec2::new(px, py),
        DVec2::new(hpx, hpy),
        DVec2::new(hnx, hny),
    );
    k.continuous = parse_bool(c);
    k.other = parse_bool(aux);
    Some(k)
}

/// `(gp [x,y])`.
fn parse_guide(line: &str) -> Option<DVec2> {
    let inner = line
        .trim()
        .strip_prefix("(gp")?
        .trim()
        .strip_suffix(')')?
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')?;
    let (x, y) = inner.split_once(',')?;
    Some(DVec2::new(parse_finite(x)?, parse_finite(y)?))
}

/// Splits `pNN : value` lines; `None` for anything else.
pub(crate) fn scalar(line: &str) -> Option<(u32, &str)> {
    let t = line.trim_start();
    let b = t.as_bytes();
    if b.len() < 3 || b[0] != b'p' || !b[1].is_ascii_digit() || !b[2].is_ascii_digit() {
        return None;
    }
    let rest = t[3..].trim_start_matches([' ', '\t']);
    let value = rest.strip_prefix(':')?;
    let code = (b[1] - b'0') as u32 * 10 + (b[2] - b'0') as u32;
    Some((code, value.trim()))
}

/// Line cursor over the file with block parsing helpers.
pub(crate) struct Reader<'a> {
    pub(crate) lines: Vec<(usize, &'a str, bool)>,
    pub(crate) pos: usize,
}

pub(crate) enum End {
    /// The block's closing `)` was read.
    Closed,
    /// The file ended inside the block.
    Truncated,
}

impl<'a> Reader<'a> {
    pub(crate) fn next(&mut self) -> Option<(usize, &'a str, bool)> {
        let l = self.lines.get(self.pos).copied();
        self.pos += 1;
        l
    }

    /// Knot lines and an optional guide block up to the block's closing `)`.
    pub(crate) fn curve(&mut self) -> Result<(Spline, Vec<DVec2>, End), Error> {
        let mut knots = Vec::new();
        let mut guides = Vec::new();
        loop {
            let Some((n, line, terminated)) = self.next() else {
                return Ok((Spline::from_knots(knots), guides, End::Truncated));
            };
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if t == ")" {
                return Ok((Spline::from_knots(knots), guides, End::Closed));
            }
            if t.starts_with("(cp") {
                match parse_knot(t) {
                    Some(k) => knots.push(k),
                    None if !terminated => {
                        return Ok((Spline::from_knots(knots), guides, End::Truncated));
                    }
                    None => return Err(invalid(n, format!("malformed control point {t:?}"))),
                }
            } else if t.starts_with("gps") {
                if let End::Truncated = self.guides(&mut guides)? {
                    return Ok((Spline::from_knots(knots), guides, End::Truncated));
                }
            } else if !terminated {
                return Ok((Spline::from_knots(knots), guides, End::Truncated));
            } else {
                return Err(invalid(n, format!("unexpected line {t:?} in a curve")));
            }
        }
    }

    fn guides(&mut self, out: &mut Vec<DVec2>) -> Result<End, Error> {
        loop {
            let Some((n, line, terminated)) = self.next() else {
                return Ok(End::Truncated);
            };
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if t == ")" {
                return Ok(End::Closed);
            }
            match parse_guide(t) {
                Some(p) => out.push(p),
                None if !terminated => return Ok(End::Truncated),
                None => return Err(invalid(n, format!("malformed guide point {t:?}"))),
            }
        }
    }

    /// Section blocks up to the closing `)` of the `p35` block.
    pub(crate) fn sections(&mut self, out: &mut Vec<CrossSection>) -> Result<(), Error> {
        loop {
            let Some((n, line, terminated)) = self.next() else {
                return Ok(());
            };
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if t == ")" {
                return Ok(());
            }
            let Some(rest) = t.strip_prefix("(p36") else {
                if !terminated {
                    return Ok(());
                }
                return Err(invalid(n, format!("unexpected line {t:?} in the sections")));
            };
            let pos_text = rest.split_whitespace().next().unwrap_or("");
            let Some(position) = parse_finite(pos_text) else {
                if !terminated {
                    return Ok(());
                }
                return Err(invalid(n, format!("bad section position {pos_text:?}")));
            };
            let (spline, guides, end) = self.curve()?;
            if !spline.is_empty() {
                let mut s = CrossSection::new(position, spline);
                s.guide_points = guides;
                out.push(s);
            }
            if let End::Truncated = end {
                return Ok(());
            }
        }
    }
}

/// Store one `pNN : value` field (not a block).
fn scalar_field(b: &mut Board, code: u32, value: &str) -> Result<(), &'static str> {
    let i = &mut b.info;
    let m = &mut b.machine;
    let int = |v: &str| parse_int(v).ok_or("expected a whole number, got");
    let real = |v: &str| parse_real(v).ok_or("expected a number, got");
    let arr = |v: &str, slots: &mut [f64]| {
        parse_array(v, slots).map_err(|_| "expected a list of numbers, got")
    };
    match code {
        7 => i.version = unescape(value),
        8 => i.name = unescape(value),
        9 => i.author = unescape(value),
        10 => i.blank_file = unescape(value),
        11 => m.deck_cuts = int(value)?,
        12 => m.bottom_cuts = int(value)?,
        13 => m.rail_cuts = int(value)?,
        14 => m.cutter_diameter = real(value)?,
        15 => m.blank_pivot = real(value)?,
        16 => m.board_pivot = real(value)?,
        17 => m.max_angle = real(value)?,
        18 => m.nose_margin = real(value)?,
        19 => m.nose_length = real(value)?,
        20 => m.tail_length = real(value)?,
        21 => m.delta_x_nose = real(value)?,
        22 => m.delta_x_tail = real(value)?,
        23 => m.delta_x_middle = real(value)?,
        24 => m.speed_to_tail = int(value)?,
        25 => m.stringer_speed = int(value)?,
        26 => m.regular_speed = int(value)?,
        27 => arr(value, &mut m.strut1)?,
        28 => arr(value, &mut m.strut2)?,
        29 => arr(value, &mut m.cutter_start)?,
        30 => arr(value, &mut m.blank_tail)?,
        31 => arr(value, &mut m.board_start)?,
        38 => m.unit_code = int(value)?,
        39 => m.nose_rocker_one_foot = real(value)?,
        40 => m.tail_rocker_one_foot = real(value)?,
        41 => m.show_original = parse_bool(value),
        42 => m.stringer_speed_bottom = int(value)?,
        43 => m.output_folder = unescape(value),
        44 => m.top_shoulder_angle = real(value)?,
        45 => i.designer = unescape(value),
        46 => m.top_shoulder_cuts = int(value)?,
        47 => m.bottom_rail_cuts = int(value)?,
        48 => i.surfer = unescape(value),
        49 => i.comments = unescape(value),
        50 => arr(value, &mut b.fins)?,
        51 => i.fin_type = unescape(value),
        52 => i.description = unescape(value),
        53 => m.security_level = int(value)?,
        54 => i.model = unescape(value),
        55 => i.aux1 = unescape(value),
        56 => i.aux2 = unescape(value),
        57 => i.aux3 = unescape(value),
        99 => m.tail_margin = real(value)?,
        // Derived values (p01-p06) and unknown codes are ignored.
        _ => {}
    }
    Ok(())
}

/// Parse `.brd` text into a board.
pub fn read_text(text: &str) -> Result<Board, Error> {
    let mut b = Board::default();
    let mut r = Reader {
        lines: lines(text),
        pos: 0,
    };
    let mut curves = [false; 3];
    while let Some((n, line, terminated)) = r.next() {
        let Some((code, value)) = scalar(line) else {
            continue;
        };
        match code {
            32..=34 => {
                if value != "(" {
                    return Err(invalid(
                        n,
                        format!("p{code}: expected \"(\", got {value:?}"),
                    ));
                }
                let (spline, guides, end) = r.curve()?;
                curves[(code - 32) as usize] = true;
                match code {
                    32 => (b.outline, b.outline_guides) = (spline, guides),
                    33 => (b.bottom, b.bottom_guides) = (spline, guides),
                    _ => (b.deck, b.deck_guides) = (spline, guides),
                }
                if let End::Truncated = end {
                    break;
                }
            }
            35 => {
                if value != "(" {
                    return Err(invalid(n, format!("p35: expected \"(\", got {value:?}")));
                }
                r.sections(&mut b.sections)?;
            }
            _ => match scalar_field(&mut b, code, value) {
                Ok(()) => {}
                // A value cut off by the end of the file is ignored.
                Err(_) if !terminated => break,
                Err(e) => return Err(invalid(n, format!("p{code:02}: {e} {value:?}"))),
            },
        }
    }
    for (present, name, s) in [
        (curves[0], "outline", &b.outline),
        (curves[1], "bottom", &b.bottom),
        (curves[2], "deck", &b.deck),
    ] {
        if !present {
            return Err(Error::Invalid(format!(
                ".brd: missing required curve ({name})"
            )));
        }
        if s.len() < 2 {
            return Err(Error::Invalid(format!(
                ".brd: the {name} needs at least two control points"
            )));
        }
    }
    b.sections.sort_by(|a, c| a.position.total_cmp(&c.position));
    finish(&mut b);
    Ok(b)
}

/// Processing every importer applies: end sections, continuity flags checked against the handles, editing
/// constraints.
pub fn finish(b: &mut Board) {
    b.ensure_end_sections();
    b.normalize_continuity(false);
    b.apply_locks();
}

// ------------------------------------------------------------------------------------------------ writing

/// A real number as the reference program writes it: shortest round-trip digits, plain notation for
/// magnitudes in `[1e-3, 1e7)` and computerised scientific notation (`1.0E-4`) otherwise, always with a
/// fraction digit. Non-finite values are written as `0.0` so that the file stays readable everywhere.
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".into();
    }
    let a = v.abs();
    if a == 0.0 || (1e-3..1e7).contains(&a) {
        let s = format!("{v}");
        if s.contains('.') { s } else { format!("{s}.0") }
    } else {
        let s = format!("{v:e}");
        let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        format!("{mantissa}E{exp}")
    }
}

fn escape(s: &str) -> String {
    s.trim()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
}

fn knot_line(out: &mut String, k: &Knot) {
    let [p, a, b] = k.points;
    let _ = writeln!(
        out,
        "(cp [{},{},{},{},{},{}] {} {})",
        num(p.x),
        num(p.y),
        num(a.x),
        num(a.y),
        num(b.x),
        num(b.y),
        k.continuous,
        k.other
    );
}

/// Knot lines and guide block of a curve (without the opening line and the closing `)`).
pub fn curve_body(out: &mut String, s: &Spline, guides: &[DVec2]) {
    for k in s.knots() {
        knot_line(out, k);
    }
    if !guides.is_empty() {
        out.push_str("gps : (\n");
        for g in guides {
            let _ = writeln!(out, "(gp [{},{}])", num(g.x), num(g.y));
        }
        out.push_str(")\n");
    }
}

/// A `(p36 …` section block.
pub fn section_block(out: &mut String, s: &CrossSection) {
    let _ = writeln!(out, "(p36 {}", num(s.position));
    curve_body(out, &s.spline, &s.guide_points);
    out.push_str(")\n");
}

fn array(v: &[f64]) -> String {
    let items: Vec<String> = v.iter().map(|&x| num(x)).collect();
    format!("[{}]", items.join(","))
}

/// Write a board as `.brd` text (UTF-8, LF line ends).
pub fn write(b: &Board) -> String {
    let mut o = String::with_capacity(8192);
    let i: &BoardInfo = &b.info;
    let m: &MachineData = &b.machine;
    let l = b.length();
    let mut field = |code: &str, value: String| {
        let _ = writeln!(o, "{code} : {value}");
    };
    field("p01", num(l));
    field("p02", num(b.length_over_curve()));
    field("p03", num(b.thickness_at(l / 2.0)));
    field("p04", num(b.width_at(l / 2.0)));
    let text = |o: &mut String, code: &str, v: &str, always: bool| {
        let v = escape(v);
        if !v.is_empty() || always {
            let _ = writeln!(o, "{code} : {v}");
        }
    };
    text(&mut o, "p07", &i.version, true);
    text(&mut o, "p08", &i.name, false);
    text(&mut o, "p09", &i.author, false);
    text(&mut o, "p45", &i.designer, false);
    text(&mut o, "p54", &i.model, false);
    text(&mut o, "p55", &i.aux1, false);
    text(&mut o, "p56", &i.aux2, false);
    text(&mut o, "p57", &i.aux3, false);
    text(&mut o, "p52", &i.description, false);
    text(&mut o, "p43", &m.output_folder, true);
    text(&mut o, "p10", &i.blank_file, false);
    let mut field = |code: &str, value: String| {
        let _ = writeln!(o, "{code} : {value}");
    };
    field("p11", m.deck_cuts.to_string());
    field("p12", m.bottom_cuts.to_string());
    field("p13", m.rail_cuts.to_string());
    field("p14", num(m.cutter_diameter));
    field("p15", num(m.blank_pivot));
    field("p16", num(m.board_pivot));
    field("p17", num(m.max_angle));
    field("p18", num(m.nose_margin));
    field("p99", num(m.tail_margin));
    field("p19", num(m.nose_length));
    field("p20", num(m.tail_length));
    field("p21", num(m.delta_x_nose));
    field("p22", num(m.delta_x_tail));
    field("p23", num(m.delta_x_middle));
    field("p24", m.speed_to_tail.to_string());
    field("p25", m.stringer_speed.to_string());
    field("p42", m.stringer_speed_bottom.to_string());
    field("p26", m.regular_speed.to_string());
    field("p44", num(m.top_shoulder_angle));
    field("p46", m.top_shoulder_cuts.to_string());
    field("p47", m.bottom_rail_cuts.to_string());
    field("p38", m.unit_code.to_string());
    field("p39", num(m.nose_rocker_one_foot));
    field("p40", num(m.tail_rocker_one_foot));
    field("p53", m.security_level.to_string());
    field("p41", m.show_original.to_string());
    text(&mut o, "p48", &i.surfer, false);
    text(&mut o, "p49", &i.comments, false);
    text(&mut o, "p51", &i.fin_type, false);
    let mut field = |code: &str, value: String| {
        let _ = writeln!(o, "{code} : {value}");
    };
    field("p50", array(&b.fins));
    field("p27", array(&m.strut1));
    field("p28", array(&m.strut2));
    field("p29", array(&m.cutter_start));
    field("p30", array(&m.blank_tail));
    field("p31", array(&m.board_start));
    for (code, s, g) in [
        ("p32", &b.outline, &b.outline_guides),
        ("p33", &b.bottom, &b.bottom_guides),
        ("p34", &b.deck, &b.deck_guides),
    ] {
        let _ = writeln!(o, "{code} : (");
        curve_body(&mut o, s, g);
        o.push_str(")\n");
    }
    o.push_str("p35 : (\n");
    for s in &b.sections {
        section_block(&mut o, s);
    }
    o.push_str(")\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_like_the_reference_program() {
        assert_eq!(num(5.0), "5.0");
        assert_eq!(num(100.0), "100.0");
        assert_eq!(num(0.1), "0.1");
        assert_eq!(num(-0.0), "-0.0");
        assert_eq!(num(1e7), "1.0E7");
        assert_eq!(num(-9.617417041187615E-4), "-9.617417041187615E-4");
        assert_eq!(num(1.0804142968407003e-14), "1.0804142968407003E-14");
        assert_eq!(num(0.001), "0.001");
        assert_eq!(num(f64::NAN), "0.0");
        for v in [1.0 / 3.0, 187.96, 1e-12, 123456789.0] {
            assert_eq!(parse_real(&num(v)), Some(v));
        }
    }

    #[test]
    fn lenient_knot_lines() {
        let k = parse_knot("(cp [ 1, 2 ,3,4,5,6]   true  false )").unwrap();
        assert_eq!(k.point(), DVec2::new(1.0, 2.0));
        assert!(k.continuous && !k.other);
        assert!(parse_knot("(cp [1,2,3] true false)").is_none());
        assert!(parse_knot("(cp [1,2,3,4,5,NaN] true false)").is_none());
    }

    #[test]
    fn scalar_keys() {
        assert_eq!(scalar("p08 : name"), Some((8, "name")));
        assert_eq!(scalar("p08 :"), Some((8, "")));
        assert_eq!(scalar("pXY : 1"), None);
        assert_eq!(scalar("p100 : 5"), None);
        assert_eq!(parse_int("3.0"), Some(3));
        assert_eq!(parse_int("3.5"), None);
    }

    #[test]
    fn text_escapes() {
        assert_eq!(escape(" a\r\nb\rc\n "), "a\\nb\\nc");
        assert_eq!(unescape("a\\nb"), "a\nb");
        assert_eq!(decode(b"caf\xe9 \x80"), "café €");
        assert_eq!(decode("\u{feff}x".as_bytes()), "x");
    }

    #[test]
    fn line_ends() {
        let l: Vec<&str> = lines("a\r\nb\rc\nd").iter().map(|x| x.1).collect();
        assert_eq!(l, ["a", "b", "c", "d"]);
    }

    /// Every measurement of a board the reader accepted must work (no panic, no hang).
    fn exercise(b: &Board) {
        use surfcraft_core::SurfaceModel;
        for model in [SurfaceModel::SBlend, SurfaceModel::ControlPoint] {
            let mut b = b.clone();
            b.model = model;
            let l = b.length();
            let _ = (
                b.max_width(),
                b.max_thickness_and_pos(),
                b.area(),
                b.volume(),
            );
            let _ = (b.center_of_mass(), b.moment_of_inertia(l / 2.0, 0.0));
            for x in [0.0, l / 3.0, l / 2.0, l] {
                let _ = b.surface_point(x, 0.5, -90.0, 360.0);
                let _ = b.surface_normal(x, 0.3, -90.0, 90.0);
                let _ = b.deck_at_xy(x, 1.0);
            }
            for i in b.real_sections() {
                let s = &b.sections[i];
                let _ = (s.release_angle(), s.tuck_radius(), s.width());
            }
            let _ = write(&b);
        }
    }

    const FIXTURE: &str = include_str!("../../../tests/fixtures/funboard.brd");

    #[test]
    fn truncated_and_corrupted_files_never_panic() {
        let bytes = FIXTURE.as_bytes();
        for cut in (0..bytes.len()).step_by(bytes.len() / 60) {
            if let Ok(b) = read(&bytes[..cut]) {
                exercise(&b);
            }
        }
        let mut seed = 7u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..40 {
            let mut b = bytes.to_vec();
            for _ in 0..1 + next() % 6 {
                let i = next() % b.len();
                b[i] = b"0123456789-.,()[]:e \n;#'"[next() % 24];
            }
            if let Ok(board) = read(&b) {
                exercise(&board);
            }
        }
    }

    #[test]
    fn degenerate_boards_never_panic() {
        let base = read_text(FIXTURE).unwrap();
        let mut zero_length = base.clone();
        for k in zero_length.outline.knots_mut() {
            for p in &mut k.points {
                p.x = 0.0;
            }
        }
        let mut one_section = base.clone();
        one_section.sections.truncate(2);
        let mut flat = base.clone();
        for k in flat.deck.knots_mut() {
            k.points = [DVec2::ZERO; 3];
        }
        for b in [zero_length, one_section, flat] {
            let again = read_text(&write(&b)).unwrap();
            exercise(&again);
        }
    }
}
