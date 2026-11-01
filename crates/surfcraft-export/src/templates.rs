//! Printable design templates: outline (half / full, flat / over the curve), spin, profile, the designer
//! cross-sections and the one-page spec sheet (`docs/spec/templates.md`, sections 3, 4 and 8).
//!
//! Every template except the spec sheet is drawn in millimetres at 1:1 with y up. The board model is in
//! centimetres; values are converted on the way in. Labels use the caller's length formatter, which takes cm.

use surfcraft_core::units::{FOOT, INCH};
use surfcraft_core::{Board, DVec2, Spline, fin, surface};

use crate::drawing::{Anchor, Drawing, Item, Style};
use crate::woodwork::stack;

/// One typographic point in millimetres.
pub(crate) const PT: f64 = 25.4 / 72.0;
/// Header text size (10 pt).
pub(crate) const HEADER: f64 = 10.0 * PT;
/// Header text size of the sandwich templates (6 pt).
pub(crate) const HEADER_SMALL: f64 = 6.0 * PT;
/// Blank space around a template's geometry (mm).
const MARGIN: f64 = 10.0;
/// How far reference lines (stringer, baseline) run past the ends of the board (mm).
pub(crate) const OVERRUN: f64 = 5.0;
/// Guide-point cross half-size (4 pt).
const GUIDE_CROSS: f64 = 4.0 * PT;

/// Options of the outline template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineOptions {
    /// Both halves (mirrored about the stringer) instead of the half outline.
    pub full: bool,
    /// Draw the measuring grid.
    pub grid: bool,
    /// Lengths measured along the rocker instead of flat.
    pub over_the_curve: bool,
    /// Draw the designer's outline guide points.
    pub guide_points: bool,
    /// Draw the fin positions.
    pub fins: bool,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        OutlineOptions {
            full: false,
            grid: true,
            over_the_curve: false,
            guide_points: false,
            fins: false,
        }
    }
}

// ------------------------------------------------------------------------------------------ shared helpers

/// Whether the caller's formatter writes imperial lengths (feet and inches).
pub(crate) fn is_imperial(fmt: &dyn Fn(f64) -> String) -> bool {
    let s = fmt(FOOT);
    s.contains('"') || s.contains('\'') || s.contains("in") || s.contains("ft")
}

/// The name printed after "Board file:".
pub(crate) fn board_name(board: &Board) -> String {
    let n = board.info.name.trim();
    if n.is_empty() {
        "untitled".into()
    } else {
        n.to_string()
    }
}

/// Points of a whole spline in mm, every Bezier segment sampled at about `step` mm (exact curve points).
pub(crate) fn spline_points(s: &Spline, step: f64) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = Vec::new();
    for seg in s.segments() {
        let len = seg.length() * 10.0;
        let n = if len.is_finite() {
            ((len / step).ceil() as usize).clamp(4, 2000)
        } else {
            4
        };
        let start = if pts.is_empty() { 0 } else { 1 };
        for k in start..=n {
            let p = seg.point(k as f64 / n as f64) * 10.0;
            if p.is_finite() {
                pts.push(p);
            }
        }
    }
    if pts.is_empty()
        && let Some(k) = s.first()
    {
        pts.push(k.point() * 10.0);
    }
    pts
}

/// The half path followed by its mirror image about `v = 0`, as one closed outline (points on the axis are
/// not repeated).
pub(crate) fn mirror_closed(half: &[DVec2]) -> Vec<DVec2> {
    let mut pts = half.to_vec();
    let mut back: Vec<DVec2> = half.iter().rev().map(|p| DVec2::new(p.x, -p.y)).collect();
    if back.first().is_some_and(|p| p.y.abs() < 1e-9) {
        back.remove(0);
    }
    if back.last().is_some_and(|p| p.y.abs() < 1e-9) {
        back.pop();
    }
    pts.extend(back);
    pts
}

/// Bounding box of the paths drawn so far.
pub(crate) fn bounds_or_zero(d: &Drawing) -> (DVec2, DVec2) {
    d.bounds().unwrap_or((DVec2::ZERO, DVec2::ZERO))
}

/// Measuring grid over the box `lo..hi` (template frame, mm): one line every 10 mm (metric) or inch
/// (imperial), on multiples of the spacing from the template origin, each running the full extent of the box.
/// Emphasis: every 10th / 50th / 100th metric line, every 6th / 12th imperial line.
pub(crate) fn grid(d: &mut Drawing, lo: DVec2, hi: DVec2, imperial: bool) {
    let sp = if imperial { INCH * 10.0 } else { 10.0 };
    if !(lo.is_finite() && hi.is_finite()) || hi.x - lo.x > 1e5 || hi.y - lo.y > 1e5 {
        return;
    }
    let level = |k: i64| -> usize {
        let k = k.abs();
        if imperial {
            if k % 12 == 0 {
                2
            } else if k % 6 == 0 {
                1
            } else {
                0
            }
        } else if k % 100 == 0 {
            3
        } else if k % 50 == 0 {
            2
        } else if k % 10 == 0 {
            1
        } else {
            0
        }
    };
    let style = |lvl: usize| {
        let grey = (190.0 * 0.7f64.powi(lvl as i32)).round() as u8;
        Style::solid(0.1 + 0.05 * lvl as f64, [grey; 3])
    };
    let eps = 1e-6;
    let (k0, k1) = (
        ((lo.x - eps) / sp).ceil() as i64,
        ((hi.x + eps) / sp).floor() as i64,
    );
    for k in k0..=k1 {
        let x = (k as f64 * sp).clamp(lo.x, hi.x);
        d.line(DVec2::new(x, lo.y), DVec2::new(x, hi.y), style(level(k)));
    }
    let (k0, k1) = (
        ((lo.y - eps) / sp).ceil() as i64,
        ((hi.y + eps) / sp).floor() as i64,
    );
    for k in k0..=k1 {
        let y = (k as f64 * sp).clamp(lo.y, hi.y);
        d.line(DVec2::new(lo.x, y), DVec2::new(hi.x, y), style(level(k)));
    }
}

/// Grid over the bounding box of everything drawn so far, from the template origin outwards (`u ≥ 0`).
pub(crate) fn grid_over_geometry(d: &mut Drawing, imperial: bool) {
    if let Some((lo, hi)) = d.bounds() {
        grid(d, DVec2::new(lo.x.max(0.0).min(hi.x), lo.y), hi, imperial);
    }
}

/// Lay a template out on its own sheet: shift the geometry so it starts `MARGIN` from the bottom-left corner,
/// size the drawing to it and print the header lines above it, top left.
pub(crate) fn finish(mut d: Drawing, header: &[String], size: f64) -> Drawing {
    let (lo, hi) = bounds_or_zero(&d);
    let ext = hi - lo;
    let line = size * 1.4;
    let head = if header.is_empty() {
        0.0
    } else {
        header.len() as f64 * line + size
    };
    let text_w = header
        .iter()
        .map(|t| t.chars().count() as f64 * size * 0.6)
        .fold(0.0, f64::max);
    let offset = DVec2::splat(MARGIN) - lo;
    let mut out = Drawing::new(
        (ext.x + 2.0 * MARGIN).max(text_w + 2.0 * MARGIN),
        ext.y + 2.0 * MARGIN + head,
    );
    out.place(&d, offset);
    out.parts = d
        .parts
        .drain(..)
        .map(|(a, b)| (a + offset, b + offset))
        .collect();
    for (i, t) in header.iter().enumerate() {
        let y = out.height - MARGIN - size - i as f64 * line;
        out.text(DVec2::new(MARGIN, y), size, t.clone(), Anchor::Left);
    }
    out
}

/// A cross of half-size `GUIDE_CROSS` at `p`.
fn guide_cross(d: &mut Drawing, p: DVec2) {
    d.cross(p, GUIDE_CROSS, Style::GUIDE);
}

/// Fin marks in the plan view (mm), each lengthwise position mapped through `map_x`.
fn draw_fins(d: &mut Drawing, board: &Board, map_x: &dyn Fn(f64) -> f64, style: Style) {
    let f = board.fins;
    let p = |x: f64, y: f64| DVec2::new(map_x(x * 10.0), y * 10.0);
    if f[fin::SIDE_REAR_X] != 0.0 || f[fin::SIDE_FRONT_X] != 0.0 {
        let (xr, yr, xf, yf) = (
            f[fin::SIDE_REAR_X],
            f[fin::SIDE_REAR_Y],
            f[fin::SIDE_FRONT_X],
            f[fin::SIDE_FRONT_Y],
        );
        d.line(p(xr, yr), p(xf, yf), style);
        d.line(p(xr, -yr), p(xf, -yf), style);
    }
    if f[fin::CENTER_FRONT_X] != 0.0 || f[fin::CENTER_REAR_X] != 0.0 {
        d.line(
            p(f[fin::CENTER_FRONT_X], 0.0),
            p(f[fin::CENTER_REAR_X], 0.0),
            style,
        );
    }
}

// ------------------------------------------------------------------------------------------ outline

/// 1:1 plan-shape template (mm). `fmt` formats a length given in cm for labels.
pub fn outline(board: &Board, opts: OutlineOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let mut d = Drawing::default();
    let l = board.length() * 10.0;
    let map_x: Box<dyn Fn(f64) -> f64> = if opts.over_the_curve {
        Box::new(|x: f64| board.from_tail_over_bottom((x / 10.0).clamp(0.0, l / 10.0)) * 10.0)
    } else {
        Box::new(|x: f64| x)
    };
    let half: Vec<DVec2> = if opts.over_the_curve {
        // Every 3 mm against the length along the rocker, starting on the centreline at the tail.
        let n = (l / 3.0).floor().max(0.0) as usize;
        let mut pts = vec![DVec2::ZERO];
        for j in 0..=n {
            let x = j as f64 * 3.0;
            pts.push(DVec2::new(
                map_x(x),
                board.outline.value_at(x / 10.0) * 10.0,
            ));
        }
        pts.retain(|p| p.is_finite());
        pts
    } else {
        spline_points(&board.outline, 2.0)
    };
    let end = if opts.over_the_curve { map_x(l) } else { l };
    // Stringer reference line, 5 mm past each end.
    d.line(
        DVec2::new(-OVERRUN, 0.0),
        DVec2::new(end + OVERRUN, 0.0),
        Style::GUIDE,
    );
    d.path(half.clone(), Style::CUT);
    if opts.full {
        d.path(
            half.iter().map(|p| DVec2::new(p.x, -p.y)).collect(),
            Style::CUT,
        );
    }
    if opts.grid {
        grid_over_geometry(&mut d, is_imperial(fmt));
    }
    if opts.guide_points {
        for g in &board.outline_guides {
            guide_cross(&mut d, DVec2::new(map_x(g.x * 10.0), g.y * 10.0));
            if opts.full {
                guide_cross(&mut d, DVec2::new(map_x(g.x * 10.0), -g.y * 10.0));
            }
        }
    }
    if opts.fins {
        draw_fins(&mut d, board, &*map_x, Style::GUIDE);
    }
    finish(
        d,
        &[format!("Board file: {} Outline", board_name(board))],
        HEADER,
    )
}

// ------------------------------------------------------------------------------------------ spin

/// 1:1 spin template: tail half and turned-over nose half of the outline on one sheet.
pub fn spin(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    spin_with(board, true, false, fmt)
}

/// Spin template with the grid and over-the-curve options.
pub fn spin_with(
    board: &Board,
    draw_grid: bool,
    over_the_curve: bool,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    let mut d = Drawing::default();
    let l = board.length() * 10.0;
    let m = l / 2.0;
    let o = |x: f64| board.outline.value_at(x / 10.0) * 10.0;
    let h = o(m);
    let (tail, nose, meet) = if over_the_curve {
        let a = |x: f64| board.from_tail_over_bottom(x / 10.0) * 10.0;
        let am = a(m);
        let n = (m / 3.0).floor().max(1.0) as usize;
        let mut tail = vec![DVec2::ZERO];
        let mut nose = Vec::new();
        for j in 0..=n {
            let x = m * j as f64 / n as f64;
            tail.push(DVec2::new(a(x), o(x)));
            let xn = m + x;
            nose.push(DVec2::new(a(xn) - am, h - o(xn)));
        }
        (tail, nose, am)
    } else {
        let n = (m / 2.0).ceil().max(1.0) as usize;
        let mut tail = Vec::with_capacity(n + 1);
        let mut nose = Vec::with_capacity(n + 1);
        for j in 0..=n {
            let u = m * j as f64 / n as f64;
            tail.push(DVec2::new(u, o(u)));
            nose.push(DVec2::new(u, h - o(u + m)));
        }
        (tail, nose, m)
    };
    // Centrelines of the two halves (the tail half's at v = 0, the turned-over nose half's at v = h) and the
    // turning point.
    d.line(
        DVec2::new(-OVERRUN, 0.0),
        DVec2::new(meet + OVERRUN, 0.0),
        Style::GUIDE,
    );
    d.line(
        DVec2::new(-OVERRUN, h),
        DVec2::new(meet + OVERRUN, h),
        Style::GUIDE,
    );
    d.line(DVec2::new(meet, 0.0), DVec2::new(meet, h), Style::GUIDE);
    d.path(
        tail.into_iter().filter(|p| p.is_finite()).collect(),
        Style::CUT,
    );
    d.path(
        nose.into_iter().filter(|p| p.is_finite()).collect(),
        Style::CUT,
    );
    if draw_grid {
        grid_over_geometry(&mut d, is_imperial(fmt));
    }
    finish(
        d,
        &[format!("Board file: {} Spin template", board_name(board))],
        HEADER,
    )
}

// ------------------------------------------------------------------------------------------ profile

/// 1:1 profile (rocker) template.
pub fn profile(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let mut d = Drawing::default();
    let l = board.length() * 10.0;
    // Zero-rocker baseline, 5 mm past each end.
    d.line(
        DVec2::new(-OVERRUN, 0.0),
        DVec2::new(l + OVERRUN, 0.0),
        Style::GUIDE,
    );
    d.path(spline_points(&board.deck, 2.0), Style::CUT);
    d.path(spline_points(&board.bottom, 2.0), Style::CUT);
    grid_over_geometry(&mut d, is_imperial(fmt));
    finish(
        d,
        &[format!("Board file: {} Profile", board_name(board))],
        HEADER,
    )
}

// ------------------------------------------------------------------------------------------ sections

/// 1:1 templates of the designer cross-sections, one piece per section.
pub fn sections(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let imperial = is_imperial(fmt);
    let mut pieces = Vec::new();
    for i in board.real_sections() {
        let sec = &board.sections[i];
        if sec.is_end_section() {
            continue;
        }
        let pts = spline_points(&sec.spline, 1.0);
        if pts.len() < 2 {
            continue;
        }
        let z_min = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let pts: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.x, p.y - z_min)).collect();
        let mut d = Drawing::default();
        // Centreline (the mirror axis) from the bottom centre to the deck centre.
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        d.line(DVec2::new(0.0, a.y), DVec2::new(0.0, b.y), Style::GUIDE);
        d.path(pts, Style::CUT);
        grid_over_geometry(&mut d, imperial);
        pieces.push(finish(
            d,
            &[
                format!("Board file: {}", board_name(board)),
                format!("Crossection {i} at {}", fmt(sec.position)),
            ],
            HEADER,
        ));
    }
    if pieces.is_empty() {
        return finish(
            Drawing::default(),
            &[
                format!("Board file: {}", board_name(board)),
                "No cross-sections".into(),
            ],
            HEADER,
        );
    }
    stack(&pieces, 10.0)
}

// ------------------------------------------------------------------------------------------ spec sheet

/// Today's date as `dd.MM.yyyy` (UTC).
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Civil date from days since 1970-01-01 (proleptic Gregorian calendar).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}.{month:02}.{year}")
}

/// Full interpolated cross-section at `x` cm (both halves, local coordinates without rocker), in cm.
fn full_section(board: &Board, x: f64) -> Vec<DVec2> {
    let st = board.station(x, surface::WHOLE.0, surface::WHOLE.1);
    let half: Vec<DVec2> = (0..=100)
        .map(|k| st.section_point(k as f64 / 100.0))
        .filter(|p| p.is_finite())
        .collect();
    let mut pts = half.clone();
    pts.extend(half.iter().rev().map(|p| DVec2::new(-p.x, p.y)));
    pts
}

/// Words of `text` wrapped to lines of at most `max` characters (newlines kept).
fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.lines() {
        let mut cur = String::new();
        for w in para.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > max {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        lines.push(cur);
    }
    lines
}

/// One-page spec sheet scaled to a page of `page` size (mm), showing `volume` (cm³, as returned by
/// [`Board::volume`]; printed in litres). The drawing is the page; it grows taller only if the content does
/// not fit.
pub fn spec_sheet(board: &Board, volume: f64, page: DVec2, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let margin = 10.0;
    let pw = (page.x - 2.0 * margin).max(50.0);
    let size = 10.0 * PT;
    let line = size * 1.2;
    let l_cm = board.length();
    let l = l_cm * 10.0;
    let s = if l > 0.0 { (pw - 10.0 * PT) / l } else { 1.0 };
    let x0 = margin + 5.0 * PT;
    let ts = FOOT; // tail and nose stations, cm from the ends
    let black = Style::solid(0.4, [0, 0, 0]);
    let thin = Style::solid(0.3, [0, 0, 0]);
    let grey = Style::solid(0.2, [128, 128, 128]);
    // Laid out with y measured downwards from the top of the page (stored negated) and shifted at the end.
    let mut d = Drawing::default();
    let mut texts: Vec<(DVec2, f64, String, Anchor)> = Vec::new();
    let mut put = |x: f64, y: f64, sz: f64, t: String, a: Anchor| {
        texts.push((DVec2::new(x, y), sz, t, a));
    };
    // Board point (cm) of a view whose zero line lies `y0` mm below the top of the page.
    let at = |x: f64, z: f64, y0: f64| DVec2::new(x0 + x * 10.0 * s, -(y0 - z * 10.0 * s));
    let col = |x: f64| x0 + x * 10.0 * s;

    let mut y = margin + 30.0 * PT;
    put(margin, y, 30.0 * PT, "SurfCraft".into(), Anchor::Left);
    y += line + 2.0 * PT;
    put(margin, y, size, "Spec sheet".into(), Anchor::Left);
    let col2 = margin + pw / 2.0;
    let info = &board.info;
    let rows = [
        (
            format!("Designer: {}", info.designer),
            format!("Date: {}", today()),
        ),
        (
            format!("Surfer: {}", info.surfer),
            format!("Model: {}", info.model),
        ),
        (
            format!(
                "Measurements: {} x {} x {}",
                fmt(l_cm),
                fmt(board.center_width()),
                fmt(board.center_thickness())
            ),
            format!("Volume: {:.2} l", volume / 1000.0),
        ),
        (format!("File: {}", board_name(board)), String::new()),
    ];
    for (a, b) in rows {
        y += line + 2.0 * PT;
        put(margin, y, size, a, Anchor::Left);
        if !b.is_empty() {
            put(col2, y, size, b, Anchor::Left);
        }
    }

    // Plan view: full outline, fins, stringer and the three station lines.
    let half_w = board.max_width() / 2.0;
    y += line;
    let yc = y + half_w * 10.0 * s;
    let outline_cm: Vec<DVec2> = spline_points(&board.outline, 5.0)
        .into_iter()
        .map(|p| p / 10.0)
        .collect();
    d.path(outline_cm.iter().map(|p| at(p.x, p.y, yc)).collect(), black);
    d.path(
        outline_cm.iter().map(|p| at(p.x, -p.y, yc)).collect(),
        black,
    );
    {
        let mut fins = Drawing::default();
        draw_fins(&mut fins, board, &|x| x, black);
        for item in fins.items {
            if let Item::Path { points, .. } = item {
                d.path(
                    points
                        .iter()
                        .map(|p| at(p.x / 10.0, p.y / 10.0, yc))
                        .collect(),
                    black,
                );
            }
        }
    }
    d.line(at(0.0, 0.0, yc), at(l_cm, 0.0, yc), grey);
    let stations = [ts, l_cm / 2.0, l_cm - ts];
    for &x in &stations {
        let o = board.outline.value_at(x);
        d.line(at(x, -o, yc), at(x, o, yc), grey);
    }
    y = yc + half_w * 10.0 * s + line + 5.0 * PT;
    for &x in &stations {
        put(col(x), y, size, fmt(board.width_at(x)), Anchor::Centre);
    }

    // Side view.
    let top = board.deck.max_y().max(board.max_rocker());
    let yb = y + top * 10.0 * s + 10.0 * PT;
    for curve in [&board.deck, &board.bottom] {
        d.path(
            spline_points(curve, 5.0)
                .iter()
                .map(|p| at(p.x / 10.0, p.y / 10.0, yb))
                .collect(),
            black,
        );
    }
    for &x in &stations {
        d.line(
            at(x, board.rocker_at(x), yb),
            at(x, board.deck_at(x), yb),
            grey,
        );
    }
    for x in [0.0, ts, l_cm - ts, l_cm] {
        d.line(at(x, 0.0, yb), at(x, board.rocker_at(x), yb), grey);
    }
    y = yb + line + 5.0 * PT;
    for &x in &stations {
        put(col(x), y, size, fmt(board.thickness_at(x)), Anchor::Centre);
    }
    y += line + 5.0 * PT;
    put(col(0.0), y, size, fmt(board.rocker_at(0.0)), Anchor::Left);
    put(col(ts), y, size, fmt(board.rocker_at(ts)), Anchor::Centre);
    put(
        col(l_cm - ts),
        y,
        size,
        fmt(board.rocker_at(l_cm - ts)),
        Anchor::Centre,
    );
    put(
        col(l_cm),
        y,
        size,
        fmt(board.rocker_at(l_cm)),
        Anchor::Right,
    );

    // Cross-sections at the same scale.
    let mut section = |x: f64, cx: f64, base: f64| {
        let pts: Vec<DVec2> = full_section(board, x)
            .iter()
            .map(|p| DVec2::new(cx + p.x * 10.0 * s, -(base - p.y * 10.0 * s)))
            .collect();
        d.closed(pts, thin);
    };
    let base = y + board.center_thickness() * 10.0 * s + 5.0 * PT;
    section(l_cm / 2.0, margin + pw / 2.0, base);
    y = base + line;
    put(margin + pw / 2.0, y, size, "Center".into(), Anchor::Centre);
    let base = y
        + line
        + 10.0 * PT
        + board
            .thickness_at(ts)
            .max(board.thickness_at(l_cm - ts))
            .max(0.0)
            * 10.0
            * s;
    section(ts, margin + pw / 3.0, base);
    section(l_cm - ts, margin + 2.0 * pw / 3.0, base);
    y = base + line;
    put(margin + pw / 3.0, y, size, "Tail".into(), Anchor::Centre);
    put(
        margin + 2.0 * pw / 3.0,
        y,
        size,
        "Nose".into(),
        Anchor::Centre,
    );

    // Fins block, only when there is fin data.
    let f = board.fins;
    let fin_type = info.fin_type.trim();
    if f[fin::SIDE_FRONT_X] != 0.0 || f[fin::CENTER_FRONT_X] != 0.0 || !fin_type.is_empty() {
        y += line + 5.0 * PT;
        put(margin, y, size, "Fins".into(), Anchor::Left);
        if !fin_type.is_empty() {
            y += line + 5.0 * PT;
            put(
                margin,
                y,
                size,
                format!("Fin type: {fin_type}"),
                Anchor::Left,
            );
        }
        if f[fin::CENTER_FRONT_X] != 0.0 {
            y += line + 5.0 * PT;
            put(
                margin,
                y,
                size,
                format!(
                    "Center fin: Front: {} Rear: {} Depth: {}",
                    fmt(f[fin::CENTER_FRONT_X]),
                    fmt(f[fin::CENTER_REAR_X]),
                    fmt(f[fin::CENTER_DEPTH])
                ),
                Anchor::Left,
            );
        }
        if f[fin::SIDE_FRONT_X] != 0.0 {
            y += line + 5.0 * PT;
            put(
                margin,
                y,
                size,
                format!(
                    "Side fins: Front: {} Rear: {} Depth: {} Toe-in: {} Splay: {:.1}°",
                    fmt(f[fin::SIDE_FRONT_X]),
                    fmt(f[fin::SIDE_REAR_X]),
                    fmt(f[fin::SIDE_DEPTH]),
                    fmt(f[fin::SIDE_FRONT_Y] - f[fin::SIDE_REAR_Y]),
                    f[fin::SPLAY]
                ),
                Anchor::Left,
            );
        }
        y += line + 10.0 * PT;
    }

    // Comments, word-wrapped to the printable width.
    y += line + 2.0 * PT;
    put(margin, y, size, "Comments:".into(), Anchor::Left);
    let max_chars = ((pw / (size * 0.5)).floor() as usize).max(10);
    for t in wrap(&info.comments, max_chars) {
        y += line + 2.0 * PT;
        put(margin, y, size, t, Anchor::Left);
    }

    let height = page.y.max(y + margin);
    let mut out = Drawing::new(page.x, height);
    out.place(&d, DVec2::new(0.0, height));
    for (p, sz, t, a) in texts {
        out.text(DVec2::new(p.x, height - p.y), sz, t, a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_closed_skips_axis_duplicates() {
        let half = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 5.0),
            DVec2::new(10.0, 5.0),
            DVec2::new(10.0, 0.0),
        ];
        let full = mirror_closed(&half);
        assert_eq!(full.len(), 6);
        assert_eq!(full[4], DVec2::new(10.0, -5.0));
    }

    #[test]
    fn grid_stays_inside_its_box() {
        let mut d = Drawing::default();
        grid(&mut d, DVec2::new(3.0, -7.0), DVec2::new(95.0, 41.0), false);
        let (lo, hi) = d.bounds().unwrap();
        assert_eq!((lo, hi), (DVec2::new(3.0, -7.0), DVec2::new(95.0, 41.0)));
        // Lines at x = 10 … 90 (9) and y = 0 … 40 (5).
        assert_eq!(d.items.len(), 9 + 5);
    }

    #[test]
    fn wrap_breaks_on_spaces_and_newlines() {
        assert_eq!(wrap("aa bb cc\ndd", 5), vec!["aa bb", "cc", "dd"]);
    }

    #[test]
    fn date_has_the_spec_format() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert_eq!(&t[2..3], ".");
        assert_eq!(&t[5..6], ".");
    }
}
