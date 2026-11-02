//! Board from measurements: rebuild a board measured with a tape (widths, rocker and thickness at stations, the
//! length and the tail shape) as fair, editable Bezier curves.
//!
//! Between the measured stations the shape of a built-in template is followed: its outline and thickness are
//! scaled by ratios interpolated between the stations (monotone cubic, so nothing overshoots), its rocker is
//! scaled to the nose and tail lift and corrected by the residuals at the measured stations. Each curve is then
//! fitted with few knots (cubic Hermite in x, so the handles sit at a third of the knot spacing) by least squares
//! with a fairness penalty, adding knots until the fit passes within half a millimetre of every measurement. The
//! template's cross-sections are copied at proportional stations and scaled to the new board.

use serde_json::{Value, json};
use surfcraft_core::units::{FOOT, INCH};
use surfcraft_core::{Board, DVec2, Knot, LengthUnit, Spline};
use surfcraft_io::templates::TEMPLATES;

use crate::by_numbers::tail_corner;
use crate::commands::{Context, ToolSpec};
use crate::document::Document;
use crate::fit::{self, Hermite, Pchip, Sample};
use crate::shaping::{self, ContourKind, ContourZone, RAIL_PRESETS, RailParams, RailSpec};

/// Bottom shapes offered for a board built from measurements (`None` in the table keeps the template's).
pub const BOTTOMS: [ContourKind; 4] = [
    ContourKind::Flat,
    ContourKind::SingleConcave,
    ContourKind::DoubleConcave,
    ContourKind::Vee,
];

/// Deepest bottom contour accepted (cm): one inch.
pub const MAX_BOTTOM_DEPTH: f64 = INCH;

/// Shortest and longest board accepted (cm): 3' to 15'.
pub const MIN_LENGTH: f64 = 3.0 * FOOT;
pub const MAX_LENGTH: f64 = 15.0 * FOOT;

/// Fit tolerance at the measured points (cm): half a millimetre.
const FIT_TOLERANCE: f64 = 0.05;

/// Tail shapes. Square, squash and swallow tails have a tail block (its width is the `tail 0` width); a swallow
/// only differs in the outline end width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TailShape {
    Pin,
    Round,
    #[default]
    Squash,
    Square,
    Swallow,
}

impl TailShape {
    pub const ALL: [TailShape; 5] = [
        TailShape::Pin,
        TailShape::Round,
        TailShape::Squash,
        TailShape::Square,
        TailShape::Swallow,
    ];

    /// Name in CSV files and commands.
    pub fn code(self) -> &'static str {
        match self {
            TailShape::Pin => "pin",
            TailShape::Round => "round",
            TailShape::Squash => "squash",
            TailShape::Square => "square",
            TailShape::Swallow => "swallow",
        }
    }

    /// English name (a translation key).
    pub fn label(self) -> &'static str {
        match self {
            TailShape::Pin => "Pin",
            TailShape::Round => "Round",
            TailShape::Squash => "Squash",
            TailShape::Square => "Square",
            TailShape::Swallow => "Swallow",
        }
    }

    pub fn from_code(s: &str) -> Option<TailShape> {
        let s = s.trim().to_ascii_lowercase();
        TailShape::ALL.into_iter().find(|t| t.code() == s)
    }

    /// Whether the outline ends in a tail block (a corner either side of a straight tail end).
    pub fn has_block(self) -> bool {
        matches!(
            self,
            TailShape::Squash | TailShape::Square | TailShape::Swallow
        )
    }
}

/// One line of the measurement table, as typed: values may carry units (`18 1/2"`, `47cm`); bare numbers are in
/// the table's unit. `position` is a distance from the tail, or `tail 12"`, `nose 12"`, `centre`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub position: String,
    pub width: String,
    pub rocker: String,
    pub thickness: String,
}

impl Row {
    pub fn at(position: &str) -> Self {
        Self {
            position: position.into(),
            ..Default::default()
        }
    }

    fn is_blank(&self) -> bool {
        [&self.position, &self.width, &self.rocker, &self.thickness]
            .iter()
            .all(|s| s.trim().is_empty())
    }
}

/// The measurement table as typed by the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub length: String,
    pub tail: TailShape,
    /// Index into the built-in templates; `None` picks the one closest in length.
    pub template: Option<usize>,
    pub rows: Vec<Row>,
    /// Bottom contour applied after the board is built; `None` keeps the template's bottom.
    pub bottom: Option<ContourKind>,
    /// Concave or vee depth as typed; empty for the usual depth of that bottom.
    pub bottom_depth: String,
    /// Index into the rail presets; `None` keeps the template's rails.
    pub rail: Option<usize>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            length: String::new(),
            tail: TailShape::default(),
            template: None,
            rows: [
                "tail 0",
                "tail 12\"",
                "tail 24\"",
                "centre",
                "nose 24\"",
                "nose 12\"",
                "nose 0",
            ]
            .map(Row::at)
            .to_vec(),
            bottom: None,
            bottom_depth: String::new(),
            rail: None,
        }
    }
}

/// A column of the table (for messages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Length,
    Position,
    Width,
    Rocker,
    Thickness,
    /// The bottom contour depth.
    Depth,
}

impl Column {
    /// English name (a translation key).
    pub fn label(self) -> &'static str {
        match self {
            Column::Length => "Length",
            Column::Position => "Position",
            Column::Width => "Width",
            Column::Rocker => "Rocker",
            Column::Thickness => "Thickness",
            Column::Depth => "Depth",
        }
    }
}

/// Something wrong with the measurements. Rows count from 1.
#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    /// A value that is not a length.
    Unreadable {
        row: usize,
        column: Column,
        text: String,
    },
    NoLength,
    /// Length outside [`MIN_LENGTH`]..[`MAX_LENGTH`].
    LengthOutOfRange,
    /// The station lies beyond the board's ends.
    OffBoard {
        row: usize,
    },
    /// Two rows at the same station.
    Duplicate {
        row: usize,
        other: usize,
    },
    /// A width or thickness of zero or less.
    NotPositive {
        row: usize,
        column: Column,
    },
    /// More than a metre wide or 30 cm thick.
    TooLarge {
        row: usize,
        column: Column,
    },
    /// Rocker is a height above a flat surface.
    NegativeRocker {
        row: usize,
    },
    /// Fewer than two widths between the ends.
    TooFewWidths,
    /// No thickness anywhere.
    NoThickness,
    /// The tail block is wider than the board just ahead of it.
    TailBlockTooWide,
    /// Bottom depth of zero or less, or over [`MAX_BOTTOM_DEPTH`].
    DepthOutOfRange,
    /// The bottom contour or rail could not be applied (the shaping tool's reason, in English).
    Shaping(String),
    /// A CSV line that is not a table row or a known setting.
    CsvLine {
        line: usize,
        text: String,
    },
}

impl Problem {
    /// English message (for the command interface; the window translates its own).
    pub fn message(&self) -> String {
        match self {
            Problem::Unreadable { row, column, text } => format!(
                "row {row}: cannot read the {} {text:?}",
                column.label().to_lowercase()
            ),
            Problem::NoLength => "enter the board length".into(),
            Problem::DepthOutOfRange => {
                "the bottom depth must be more than zero and at most 1\"".into()
            }
            Problem::Shaping(e) => format!("could not shape the bottom or rails: {e}"),
            Problem::LengthOutOfRange => "the length must be between 3' and 15'".into(),
            Problem::OffBoard { row } => format!("row {row}: the station is not on the board"),
            Problem::Duplicate { row, other } => {
                format!("row {row}: same station as row {other}")
            }
            Problem::NotPositive { row, column } => format!(
                "row {row}: the {} must be more than zero",
                column.label().to_lowercase()
            ),
            Problem::TooLarge { row, column } => format!(
                "row {row}: the {} is too large for a surfboard",
                column.label().to_lowercase()
            ),
            Problem::NegativeRocker { row } => format!(
                "row {row}: rocker is the height of the bottom above a flat surface and cannot be negative"
            ),
            Problem::TooFewWidths => {
                "give at least two widths between the tail and the nose".into()
            }
            Problem::NoThickness => "give at least one thickness".into(),
            Problem::TailBlockTooWide => {
                "the tail block is wider than the board ahead of it".into()
            }
            Problem::CsvLine { line, text } => format!("line {line}: cannot read {text:?}"),
        }
    }
}

/// A measured station inside the board.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    /// Distance from the tail (cm).
    pub x: f64,
    pub width: Option<f64>,
    pub rocker: Option<f64>,
    pub thickness: Option<f64>,
}

/// Checked measurements, in cm.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurements {
    pub length: f64,
    pub tail: TailShape,
    pub template: usize,
    /// The `tail 0` and `nose 0` rows: width at the tail end (tail block), lift and thickness at each end.
    pub tail_end: Station,
    pub nose_end: Station,
    /// Stations strictly between the ends, sorted.
    pub stations: Vec<Station>,
    /// Bottom contour and its depth (`None` = the usual depth for that bottom).
    pub bottom: Option<(ContourKind, Option<f64>)>,
    /// Rail shape applied to every section.
    pub rail: Option<RailSpec>,
}

/// Where a position text puts the station: `(from_tail, from_nose)` or the centre.
enum Pos {
    Tail(f64),
    Nose(f64),
    Centre,
}

fn parse_position(text: &str, unit: LengthUnit) -> Option<Pos> {
    let t = text.trim().to_ascii_lowercase();
    if matches!(t.as_str(), "centre" | "center" | "middle" | "c") {
        return Some(Pos::Centre);
    }
    for (prefix, nose) in [("nose", true), ("tail", false), ("n", true), ("t", false)] {
        if let Some(rest) = t.strip_prefix(prefix) {
            let rest = rest.trim();
            let d = if rest.is_empty() {
                0.0
            } else {
                unit.parse(rest)?
            };
            return Some(if nose { Pos::Nose(d) } else { Pos::Tail(d) });
        }
    }
    unit.parse(&t).map(Pos::Tail)
}

/// Template closest in length.
pub fn closest_template(length: f64) -> usize {
    (0..TEMPLATES.len())
        .min_by(|&a, &b| {
            let d = |i: usize| (TEMPLATES[i].board().length() - length).abs();
            d(a).total_cmp(&d(b))
        })
        .unwrap_or(0)
}

/// Check the table and turn it into measurements in cm. Every problem found is reported.
pub fn parse(table: &Table, unit: LengthUnit) -> Result<Measurements, Vec<Problem>> {
    let mut problems = Vec::new();
    let length = if table.length.trim().is_empty() {
        problems.push(Problem::NoLength);
        None
    } else {
        match unit.parse(&table.length) {
            Some(l) if (MIN_LENGTH..=MAX_LENGTH).contains(&l) => Some(l),
            Some(_) => {
                problems.push(Problem::LengthOutOfRange);
                None
            }
            None => {
                problems.push(Problem::Unreadable {
                    row: 0,
                    column: Column::Length,
                    text: table.length.clone(),
                });
                None
            }
        }
    };
    let depth = if table.bottom_depth.trim().is_empty() {
        None
    } else {
        match unit.parse(&table.bottom_depth) {
            Some(d) if d > 0.0 && d <= MAX_BOTTOM_DEPTH => Some(d),
            Some(_) => {
                problems.push(Problem::DepthOutOfRange);
                None
            }
            None => {
                problems.push(Problem::Unreadable {
                    row: 0,
                    column: Column::Depth,
                    text: table.bottom_depth.clone(),
                });
                None
            }
        }
    };
    let mut tail_end = Station {
        x: 0.0,
        width: None,
        rocker: None,
        thickness: None,
    };
    let mut nose_end = tail_end;
    let mut stations: Vec<(usize, Station)> = Vec::new();
    for (i, r) in table.rows.iter().enumerate() {
        let row = i + 1;
        if r.is_blank() {
            continue;
        }
        let mut value = |text: &str, column: Column| -> Option<f64> {
            if text.trim().is_empty() {
                return None;
            }
            let v = unit.parse(text);
            let limit = match column {
                Column::Width => 100.0,
                Column::Thickness => 30.0,
                _ => 100.0,
            };
            match v {
                None => problems.push(Problem::Unreadable {
                    row,
                    column,
                    text: text.into(),
                }),
                Some(v) if column == Column::Rocker && v < 0.0 => {
                    problems.push(Problem::NegativeRocker { row })
                }
                Some(v) if column != Column::Rocker && v <= 0.0 => {
                    problems.push(Problem::NotPositive { row, column })
                }
                Some(v) if v > limit => problems.push(Problem::TooLarge { row, column }),
                Some(v) => return Some(v),
            }
            None
        };
        let width = value(&r.width, Column::Width);
        let rocker = value(&r.rocker, Column::Rocker);
        let thickness = value(&r.thickness, Column::Thickness);
        let Some(pos) = parse_position(&r.position, unit) else {
            problems.push(Problem::Unreadable {
                row,
                column: Column::Position,
                text: r.position.clone(),
            });
            continue;
        };
        let Some(l) = length else { continue };
        let x = match pos {
            Pos::Tail(d) => d,
            Pos::Nose(d) => l - d,
            Pos::Centre => l / 2.0,
        };
        let s = Station {
            x,
            width,
            rocker,
            thickness,
        };
        if !(-0.05..=l + 0.05).contains(&x) {
            problems.push(Problem::OffBoard { row });
        } else if x < 0.5 {
            tail_end = Station { x: 0.0, ..s };
        } else if x > l - 0.5 {
            nose_end = Station { x: l, ..s };
        } else if let Some((other, _)) = stations.iter().find(|(_, o)| (o.x - x).abs() < 0.5) {
            problems.push(Problem::Duplicate { row, other: *other });
        } else {
            stations.push((row, s));
        }
    }
    stations.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    let stations: Vec<Station> = stations.into_iter().map(|(_, s)| s).collect();
    if length.is_some() && problems.is_empty() {
        if stations.iter().filter(|s| s.width.is_some()).count() < 2 {
            problems.push(Problem::TooFewWidths);
        }
        if stations.iter().all(|s| s.thickness.is_none()) {
            problems.push(Problem::NoThickness);
        }
        if let (Some(tb), Some(first)) = (
            tail_end.width.filter(|_| table.tail.has_block()),
            stations.iter().find(|s| s.width.is_some()),
        ) && tb > first.width.unwrap_or(f64::MAX)
        {
            problems.push(Problem::TailBlockTooWide);
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let length = length.unwrap_or_default();
    Ok(Measurements {
        length,
        tail: table.tail,
        template: table
            .template
            .filter(|&t| t < TEMPLATES.len())
            .unwrap_or_else(|| closest_template(length)),
        tail_end,
        nose_end: Station {
            x: length,
            ..nose_end
        },
        stations,
        bottom: table.bottom.map(|k| (k, depth)),
        rail: table.rail.and_then(|i| RAIL_PRESETS.get(i)).map(|p| p.1),
    })
}

// --------------------------------------------------------------------------------------------- building

/// How closely the built board matches the measurements: largest difference (cm) at a measured station.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FitReport {
    pub width: f64,
    pub rocker: f64,
    pub thickness: f64,
}

impl FitReport {
    pub fn worst(&self) -> f64 {
        self.width.max(self.rocker).max(self.thickness)
    }
}

/// Fit a curve on `[a, b]` to `target`, which must pass through `points` (weighted heavily). Starts with five
/// knots and adds more (up to nine) until every point is within [`FIT_TOLERANCE`] and the target within 1.5 mm.
fn fit_curve(
    a: f64,
    b: f64,
    target: &dyn Fn(f64) -> f64,
    points: &[(f64, f64)],
    start: Option<f64>,
    end: Option<f64>,
) -> Hermite {
    let span = b - a;
    let n = (span.ceil() as usize).max(20);
    let dense: Vec<Sample> = (0..=n)
        .map(|i| {
            let x = a + span * i as f64 / n as f64;
            Sample {
                x,
                y: target(x),
                w: span / n as f64,
            }
        })
        .collect();
    // Every knot count from five to nine and a range of fairness weights: of the fits that pass within
    // tolerance, the one with fewest inflections, then fewest knots, then closest to the template's shape.
    let lambda = 20.0 * (span / 200.0).powi(3);
    let mut samples = dense.clone();
    samples.extend(points.iter().map(|&(x, y)| Sample { x, y, w: 1000.0 }));
    // (good, inflections, knots, deviation)
    let mut best: Option<((bool, usize, usize, f64), Hermite)> = None;
    for count in 5..=9 {
        for mult in [1.0, 4.0, 16.0, 64.0, 256.0] {
            let xs = fit::stations(a, b, count);
            let Some(h) = fit::fit(&xs, &samples, start, end, lambda * mult) else {
                continue;
            };
            let at_points = points
                .iter()
                .map(|&(x, y)| (h.eval(x) - y).abs())
                .fold(0.0, f64::max);
            let overall = dense
                .iter()
                .map(|s| (h.eval(s.x) - s.y).abs())
                .fold(0.0, f64::max);
            let good = at_points <= FIT_TOLERANCE && overall <= 0.3;
            let infl = crate::by_numbers::inflections(&Spline::from_knots(h.knots()));
            let key = (good, infl, count, at_points.max(overall / 6.0));
            let better = best.as_ref().is_none_or(|(k, _)| match (key.0, k.0) {
                (true, false) => true,
                (false, true) => false,
                (true, true) => {
                    (key.1, key.2) < (k.1, k.2) || ((key.1, key.2) == (k.1, k.2) && key.3 < k.3)
                }
                (false, false) => key.3 < k.3,
            });
            if better {
                best = Some((key, h));
            }
        }
    }
    best.map(|b| b.1).unwrap_or_else(|| Hermite {
        xs: vec![a, b],
        ys: vec![start.unwrap_or(0.0), end.unwrap_or(0.0)],
        ms: vec![0.0, 0.0],
    })
}

/// `f` evaluated on the template, stretched to length `l` (x scaled by `k`), clamped away from the very ends.
fn on_template(tb: &Board, k: f64, f: impl Fn(&Board, f64) -> f64) -> impl Fn(f64) -> f64 {
    let lt = tb.length();
    move |x: f64| f(tb, (x / k).clamp(0.05, lt - 0.05))
}

fn smootherstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * u * (u * (6.0 * u - 15.0) + 10.0)
}

/// Build the board: fair curves through the measurements, sections copied from the template, then the rails and
/// the bottom contour if asked for (the bottom keeps its rail line and the foam is cut, so a concave or vee
/// lowers the stringer thickness and the volume). The fit report is taken before the bottom is shaped.
pub fn build(m: &Measurements) -> Result<(Board, FitReport), Problem> {
    let (mut b, report) = build_curves(m);
    if let Some(spec) = m.rail {
        shaping::apply_rails(&mut b, &RailParams::uniform(spec)).map_err(Problem::Shaping)?;
    }
    if let Some((kind, depth)) = m.bottom {
        let mut z = ContourZone::for_board(kind, &b);
        if let Some(d) = depth.filter(|_| kind != ContourKind::Flat) {
            z.depth = d;
            z.inner_depth = d / 2.0;
        }
        shaping::apply_contours(&mut b, &[z], shaping::DEFAULT_SPACING)
            .map_err(Problem::Shaping)?;
    }
    Ok((b, report))
}

/// The board with its fitted curves and the template's sections.
fn build_curves(m: &Measurements) -> (Board, FitReport) {
    let tb = TEMPLATES[m.template].board();
    let l = m.length;
    let k = l / tb.length();

    // ---- outline: half-width
    let corner_t = tail_corner(&tb.outline).map_or(0.0, |i| tb.outline.knots()[i].point().x);
    let safe = (corner_t + 5.0) * k;
    let raw = on_template(&tb, k, |b, x| b.outline.value_at(x));
    let slope = (raw(safe + 1.0) - raw(safe)).max(0.0);
    let ft = |x: f64| {
        if x >= safe {
            raw(x)
        } else {
            (raw(safe) - slope * (safe - x)).max(0.1)
        }
    };
    let widths: Vec<(f64, f64)> = m
        .stations
        .iter()
        .filter_map(|s| s.width.map(|w| (s.x, w / 2.0)))
        .collect();
    let ratio = Pchip::new(
        &widths
            .iter()
            .map(|&(x, h)| (x, h / ft(x).max(0.1)))
            .collect::<Vec<_>>(),
    );
    let base = |x: f64| ft(x) * ratio.eval(x);
    let x_first = widths.first().map_or(l / 4.0, |w| w.0);
    let (xc, y_start) = match m.tail {
        TailShape::Pin => (0.0, 0.0),
        TailShape::Round => {
            let rc = (0.25 * base(x_first)).clamp(2.0, 8.0);
            (rc, 0.7 * base(rc))
        }
        shape => {
            let xc = if shape == TailShape::Squash { 1.5 } else { 0.4 };
            let default = tail_corner(&tb.outline).map_or(0.6 * base(x_first), |i| {
                tb.outline.knots()[i].point().y / ft(x_first).max(0.1) * base(x_first)
            });
            (xc, m.tail_end.width.map_or(default, |w| w / 2.0))
        }
    };
    let xb = x_first.max(xc + 10.0);
    // Near the tail the template's shape gives way to the measured tail: a narrower tail is reached along a
    // curve that keeps bending the same way as the rest of the outline.
    let offset = y_start - base(xc);
    let outline_target = |x: f64| {
        let b = base(x);
        if x < xb {
            let u = ((x - xc) / (xb - xc)).clamp(0.0, 1.0);
            let fade = if offset < 0.0 {
                (1.0 - u) * (1.0 - u)
            } else {
                1.0 - smootherstep(u)
            };
            b + offset * fade
        } else {
            b
        }
    };
    let body = fit_curve(xc, l, &outline_target, &widths, Some(y_start), Some(0.0));
    let mut knots = body.knots();
    let n = knots.len();
    // Pointed nose: the next handle mirrors the previous one about the centre line, as on the templates.
    let nose = &mut knots[n - 1];
    nose.points[2] = DVec2::new(nose.prev().x, -nose.prev().y);
    nose.continuous = tb.outline.last().is_some_and(|k| k.continuous);
    let first = body.knots()[0].point();
    match m.tail {
        TailShape::Pin => {}
        TailShape::Round => {
            // Quarter-ellipse cap from the centre line, square to it, into the body's first knot.
            let c = 0.5523;
            let k0 = &mut knots[0];
            k0.points[1] = first - DVec2::new(c * xc, c * xc * body.ms[0]);
            k0.continuous = true;
            knots.insert(
                0,
                Knot::new(DVec2::ZERO, DVec2::ZERO, DVec2::new(0.0, c * y_start)),
            );
        }
        _ => {
            let k0 = &mut knots[0];
            k0.points[1] = DVec2::new(0.4 * xc, 0.9 * y_start);
            knots.insert(
                0,
                Knot::new(DVec2::ZERO, DVec2::ZERO, DVec2::new(0.0, 0.55 * y_start)),
            );
        }
    }
    let outline = Spline::from_knots(knots);

    // ---- rocker
    let rt = on_template(&tb, k, |b, x| b.rocker_at(x));
    let low =
        tb.bottom
            .knots()
            .iter()
            .map(|k| k.point())
            .fold(
                DVec2::new(0.0, f64::MAX),
                |a, p| if p.y < a.y { p } else { a },
            );
    let xl = low.x * k;
    let scale = |measured: Option<f64>, template: f64| match measured {
        Some(v) if template > 0.1 => v / template,
        _ => 1.0,
    };
    let s_tail = scale(m.tail_end.rocker, rt(0.0));
    let s_nose = scale(m.nose_end.rocker, rt(l));
    let rbase = |x: f64| rt(x) * if x < xl { s_tail } else { s_nose };
    let r_tail = m.tail_end.rocker.unwrap_or_else(|| rbase(0.0));
    let r_nose = m.nose_end.rocker.unwrap_or_else(|| rbase(l));
    let rockers: Vec<(f64, f64)> = m
        .stations
        .iter()
        .filter_map(|s| s.rocker.map(|r| (s.x, r)))
        .collect();
    let mut corr = vec![(0.0, r_tail - rbase(0.0))];
    corr.extend(rockers.iter().map(|&(x, r)| (x, r - rbase(x))));
    corr.push((l, r_nose - rbase(l)));
    let corr = Pchip::new(&corr);
    let rocker_target = |x: f64| rbase(x) + corr.eval(x);
    let bottom_fit = fit_curve(0.0, l, &rocker_target, &rockers, Some(r_tail), Some(r_nose));
    let bottom = Spline::from_knots(bottom_fit.knots());

    // ---- thickness and deck
    let tt = on_template(&tb, k, |b, x| b.thickness_at(x));
    let thick: Vec<(f64, f64)> = m
        .stations
        .iter()
        .filter_map(|s| s.thickness.map(|t| (s.x, t)))
        .collect();
    let q = Pchip::new(
        &thick
            .iter()
            .map(|&(x, t)| (x, t / tt(x).max(0.1)))
            .collect::<Vec<_>>(),
    );
    let t_tail = m
        .tail_end
        .thickness
        .unwrap_or_else(|| tt(0.0) * q.eval(0.0));
    let t_nose = m.nose_end.thickness.unwrap_or_else(|| tt(l) * q.eval(l));
    let thickness = |x: f64| tt(x) * q.eval(x);
    let deck_target = |x: f64| bottom_fit.eval(x) + thickness(x);
    let deck_points: Vec<(f64, f64)> = thick
        .iter()
        .map(|&(x, t)| (x, bottom_fit.eval(x) + t))
        .collect();
    let (d0, dl) = (r_tail + t_tail, r_nose + t_nose);
    let deck_fit = fit_curve(0.0, l, &deck_target, &deck_points, Some(d0), Some(dl));
    let mut dk = deck_fit.knots();
    // Vertical tail and nose ends down to the bottom.
    if t_tail > 0.05 {
        dk[0].points[1] = DVec2::new(0.0, d0 - 0.4 * t_tail);
        dk.insert(
            0,
            Knot::new(
                DVec2::new(0.0, r_tail),
                DVec2::new(0.0, r_tail),
                DVec2::new(0.0, r_tail + 0.5 * t_tail),
            ),
        );
    }
    if t_nose > 0.05 {
        let last = dk.len() - 1;
        dk[last].points[2] = DVec2::new(l, dl - 0.4 * t_nose);
        dk.push(Knot::new(
            DVec2::new(l, r_nose),
            DVec2::new(l, r_nose + 0.5 * t_nose),
            DVec2::new(l, r_nose),
        ));
    }
    let deck = Spline::from_knots(dk);

    // ---- assemble on the template
    let mut b = tb.clone();
    let w0 = tb.max_width();
    b.outline = outline;
    b.bottom = bottom;
    b.deck = deck;
    b.outline_guides.clear();
    b.bottom_guides.clear();
    b.deck_guides.clear();
    let ns = b.sections.len();
    for (i, s) in b.sections.iter_mut().enumerate() {
        s.position = if i + 1 == ns && ns > 1 {
            l
        } else {
            s.position * k
        };
        s.guide_points.clear();
    }
    b.scale_fins(k, b.max_width() / w0);
    b.info.name.clear();
    b.apply_locks();
    b.on_rocker_changed();
    b.apply_locks();
    let report = report(&b, m);
    (b, report)
}

/// Differences between the board and the measurements at the measured stations.
pub fn report(b: &Board, m: &Measurements) -> FitReport {
    let mut r = FitReport::default();
    for s in &m.stations {
        if let Some(w) = s.width {
            r.width = r.width.max((b.width_at(s.x) - w).abs());
        }
        if let Some(v) = s.rocker {
            r.rocker = r.rocker.max((b.rocker_at(s.x) - v).abs());
        }
        if let Some(t) = s.thickness {
            r.thickness = r.thickness.max((b.thickness_at(s.x) - t).abs());
        }
    }
    let l = m.length;
    if let Some(v) = m.tail_end.rocker {
        r.rocker = r.rocker.max((b.rocker_at(0.001) - v).abs());
    }
    if let Some(v) = m.nose_end.rocker {
        r.rocker = r.rocker.max((b.rocker_at(l - 0.005) - v).abs());
    }
    r
}

// --------------------------------------------------------------------------------------------- tables

/// Measure a board the way a shaper would: widths, rocker and thickness at the ends, 12" from each end and every
/// `step` from the tail, in `unit`.
pub fn sample(b: &Board, step: f64, unit: LengthUnit, fraction: u32) -> Table {
    let l = b.length();
    let f = |cm: f64| unit.format(cm, fraction);
    let mut rows = vec![Row {
        position: "tail 0".into(),
        width: tail_corner(&b.outline)
            .map(|i| f(2.0 * b.outline.knots()[i].point().y))
            .unwrap_or_default(),
        rocker: f(b.rocker_at(0.001)),
        thickness: String::new(),
    }];
    let mut xs: Vec<f64> = (1..)
        .map(|i| i as f64 * step)
        .take_while(|&x| x < l - 2.0)
        .collect();
    xs.push(l - FOOT);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    for x in xs {
        let position = if (x - (l - FOOT)).abs() < 1e-9 {
            format!("nose {}", f(FOOT))
        } else {
            f(x)
        };
        rows.push(Row {
            position,
            width: f(b.width_at(x)),
            rocker: f(b.rocker_at(x)),
            thickness: f(b.thickness_at(x)),
        });
    }
    rows.push(Row {
        position: "nose 0".into(),
        width: String::new(),
        rocker: f(b.rocker_at(l - 0.005)),
        thickness: String::new(),
    });
    Table {
        length: f(l),
        tail: if tail_corner(&b.outline).is_some() {
            TailShape::Squash
        } else {
            TailShape::Pin
        },
        template: None,
        rows,
        ..Default::default()
    }
}

/// Column order of [`Table::to_csv`].
pub const CSV_HEADER: &str = "position,width,rocker,thickness";

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', ';', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Split one CSV line (quotes doubled inside quoted fields).
fn csv_split(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if cur.trim().is_empty() => {
                cur.clear();
                quoted = true;
            }
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

impl Table {
    /// The table as CSV: settings lines (`length`, `tail`, `template`), then a header and one line per row.
    pub fn to_csv(&self) -> String {
        let mut s = String::from("# SurfCraft board measurements\n");
        s += &format!("length,{}\n", csv_field(&self.length));
        s += &format!("tail,{}\n", self.tail.code());
        s += &format!(
            "template,{}\n",
            self.template
                .and_then(|i| TEMPLATES.get(i))
                .map_or("auto".into(), |t| t.name.to_ascii_lowercase())
        );
        s += &format!(
            "bottom,{}\n",
            self.bottom.map_or("template", ContourKind::slug)
        );
        if !self.bottom_depth.trim().is_empty() {
            s += &format!("bottom depth,{}\n", csv_field(&self.bottom_depth));
        }
        s += &format!(
            "rail,{}\n",
            self.rail
                .and_then(|i| RAIL_PRESETS.get(i))
                .map_or("template", |p| p.0)
        );
        s += CSV_HEADER;
        s.push('\n');
        for r in self.rows.iter().filter(|r| !r.is_blank()) {
            s += &[&r.position, &r.width, &r.rocker, &r.thickness]
                .map(|f| csv_field(f))
                .join(",");
            s.push('\n');
        }
        s
    }

    /// Read a table written by [`Self::to_csv`] or a spreadsheet: comma or semicolon separated, lines starting
    /// with `#` ignored, `length` / `tail` / `template` settings lines, an optional header naming the columns
    /// (`position`, `width`, `rocker`, `thickness`, in any order), then one station per line.
    pub fn from_csv(text: &str) -> Result<Table, Problem> {
        let sep = if text.lines().any(|l| l.contains(';')) && !text.lines().any(|l| l.contains(','))
        {
            ';'
        } else {
            ','
        };
        let mut t = Table {
            rows: Vec::new(),
            ..Table::default()
        };
        let mut cols = [0usize, 1, 2, 3];
        for (i, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f = csv_split(line, sep);
            let key = f[0].to_ascii_lowercase();
            let names = ["position", "station", "width", "rocker", "thickness"];
            let is_header = f
                .iter()
                .any(|h| h.eq_ignore_ascii_case("position") || h.eq_ignore_ascii_case("station"))
                && f.iter()
                    .all(|h| h.is_empty() || names.iter().any(|n| h.eq_ignore_ascii_case(n)));
            let key = if is_header {
                "position".to_string()
            } else {
                key
            };
            let bad = || Problem::CsvLine {
                line: i + 1,
                text: line.to_string(),
            };
            let second = f.get(1).cloned().unwrap_or_default();
            match key.as_str() {
                "length" => t.length = second,
                "tail" | "tail shape" if TailShape::from_code(&second).is_some() => {
                    t.tail = TailShape::from_code(&second).unwrap_or_default()
                }
                "template" => {
                    let v = second.to_ascii_lowercase();
                    t.template = if v.is_empty() || v == "auto" {
                        None
                    } else {
                        Some(
                            TEMPLATES
                                .iter()
                                .position(|tp| tp.name.to_ascii_lowercase() == v)
                                .ok_or_else(bad)?,
                        )
                    };
                }
                "bottom" => {
                    let v = second.to_ascii_lowercase();
                    t.bottom = if v.is_empty() || v == "template" {
                        None
                    } else {
                        Some(
                            ContourKind::from_slug(&v)
                                .filter(|k| BOTTOMS.contains(k))
                                .ok_or_else(bad)?,
                        )
                    };
                }
                "bottom depth" | "depth" => t.bottom_depth = second,
                "rail" | "rails" => {
                    t.rail = if second.is_empty() || second.eq_ignore_ascii_case("template") {
                        None
                    } else {
                        Some(
                            RAIL_PRESETS
                                .iter()
                                .position(|p| p.0 == second)
                                .ok_or_else(bad)?,
                        )
                    };
                }
                "position" | "station" => {
                    // Columns may come in any order; missing ones stay empty.
                    for (c, name) in ["position", "width", "rocker", "thickness"]
                        .iter()
                        .enumerate()
                    {
                        let alt = if c == 0 { "station" } else { name };
                        cols[c] = f
                            .iter()
                            .position(|h| {
                                h.eq_ignore_ascii_case(name) || h.eq_ignore_ascii_case(alt)
                            })
                            .unwrap_or(usize::MAX);
                    }
                }
                _ => {
                    let get = |c: usize| f.get(cols[c]).cloned().unwrap_or_default();
                    t.rows.push(Row {
                        position: get(0),
                        width: get(1),
                        rocker: get(2),
                        thickness: get(3),
                    });
                }
            }
        }
        Ok(t)
    }
}

// --------------------------------------------------------------------------------------------- command

pub fn tool_spec() -> ToolSpec {
    let len = |d: &str| json!({"type": ["number", "string"], "description": format!("{d} (cm, or a string with units like \"18 1/2\\\"\" or \"47cm\")")});
    ToolSpec {
        name: "board_from_measurements",
        description: "Build a new board from tape measurements of a real one: length, tail shape and a list of \
                      stations with width, bottom rocker (height above a flat surface) and thickness. Positions \
                      are from the tail, or \"tail 12\\\"\", \"nose 12\\\"\", \"centre\"; \"tail 0\" / \"nose 0\" give \
                      the tail block width and the tail / nose lift. Curves are fitted fair through the points \
                      and a template's cross-sections are scaled onto it. Optionally a bottom contour (foam cut, \
                      rail line kept) and a rail preset are applied afterwards. Replaces the open board (unsaved).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "length": len("overall length"),
                "tail": {"type": "string", "enum": TailShape::ALL.map(TailShape::code)},
                "template": {"type": "string", "enum": ["auto", "shortboard", "funboard", "longboard"], "description": "Whose cross-sections and in-between shape to follow (default: closest in length)"},
                "stations": {"type": "array", "items": {"type": "object", "properties": {
                    "position": {"type": ["number", "string"]},
                    "width": len("width"), "rocker": len("bottom rocker"), "thickness": len("thickness")
                }, "required": ["position"]}},
                "bottom": {"type": "string", "enum": ["template", "flat", "single_concave", "double_concave", "vee"], "description": "Bottom contour (default: the template's bottom)"},
                "bottom_depth": len("concave or vee depth, at most 1 inch (default: the usual depth for that bottom)"),
                "rail": {"type": "string", "enum": ["template", "50/50", "60/40", "70/30", "80/20"], "description": "Rail preset (default: the template's rails)"},
                "discard_unsaved": {"type": "boolean"}
            },
            "required": ["length", "stations"]
        }),
    }
}

/// The `board_from_measurements` command.
pub fn command(doc: &mut Document, ctx: &mut Context, a: &Value) -> Result<Value, String> {
    let get = |k: &str| a.get(k).filter(|v| !v.is_null());
    if doc.is_dirty()
        && !get("discard_unsaved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(
            "the open board has unsaved changes: save it first or pass discard_unsaved: true"
                .into(),
        );
    }
    // Numbers are centimetres; strings keep their units (bare numbers in strings are centimetres too).
    let text = |v: Option<&Value>, k: &str| -> Result<String, String> {
        match v {
            None | Some(Value::Null) => Ok(String::new()),
            Some(Value::Number(n)) => Ok(format!("{}cm", n.as_f64().unwrap_or(f64::NAN))),
            Some(Value::String(s)) => Ok(s.clone()),
            Some(v) => Err(format!("{k}: expected a number or string, got {v}")),
        }
    };
    let tail = match get("tail") {
        None => TailShape::default(),
        Some(Value::String(s)) => TailShape::from_code(s).ok_or_else(|| {
            format!(
                "tail: unknown {s:?}, expected one of {}",
                TailShape::ALL.map(TailShape::code).join(", ")
            )
        })?,
        Some(v) => return Err(format!("tail: expected text, got {v}")),
    };
    let template = match get("template").and_then(Value::as_str) {
        None | Some("auto") => None,
        Some("shortboard") => Some(0),
        Some("funboard") => Some(1),
        Some("longboard") => Some(2),
        Some(o) => return Err(format!("unknown template {o:?}")),
    };
    let list = match get("stations") {
        Some(Value::Array(l)) if l.len() <= 200 => l,
        _ => return Err("stations: expected an array of up to 200 stations".into()),
    };
    let mut rows = Vec::new();
    for s in list {
        if !s.is_object() {
            return Err(format!("stations: expected objects, got {s}"));
        }
        rows.push(Row {
            position: text(s.get("position"), "position")?,
            width: text(s.get("width"), "width")?,
            rocker: text(s.get("rocker"), "rocker")?,
            thickness: text(s.get("thickness"), "thickness")?,
        });
    }
    let bottom = match get("bottom") {
        None => None,
        Some(Value::String(v)) if v == "template" => None,
        Some(Value::String(v)) => Some(
            ContourKind::from_slug(v)
                .filter(|k| BOTTOMS.contains(k))
                .ok_or_else(|| {
                    format!("bottom: unknown {v:?}, expected template, flat, single_concave, double_concave or vee")
                })?,
        ),
        Some(v) => return Err(format!("bottom: expected text, got {v}")),
    };
    let rail = match get("rail") {
        None => None,
        Some(Value::String(v)) if v == "template" => None,
        Some(Value::String(v)) => {
            Some(RAIL_PRESETS.iter().position(|p| p.0 == v).ok_or_else(|| {
                format!("rail: unknown {v:?}, expected template, 50/50, 60/40, 70/30 or 80/20")
            })?)
        }
        Some(v) => return Err(format!("rail: expected text, got {v}")),
    };
    let table = Table {
        length: text(get("length"), "length")?,
        tail,
        template,
        rows,
        bottom,
        bottom_depth: text(get("bottom_depth"), "bottom_depth")?,
        rail,
    };
    let m = parse(&table, LengthUnit::Centimetres).map_err(|ps| {
        ps.iter()
            .map(Problem::message)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let (mut b, report) = build(&m).map_err(|p| p.message())?;
    b.model = doc.board.model;
    *doc = Document::new(b, None);
    ctx.current_section = doc
        .board
        .nearest_section_index(doc.board.length() / 2.0)
        .unwrap_or(1);
    let mm = |cm: f64| (cm * 100.0).round() / 10.0;
    let mut out = crate::commands::summary(&doc.board, ctx);
    out["fit_mm"] = json!({"width": mm(report.width), "rocker": mm(report.rocker), "thickness": mm(report.thickness)});
    out["template"] = json!(TEMPLATES[m.template].name);
    Ok(out)
}

/// Distance between measured stations in [`sample`]: a foot, or six inches for a closer copy.
pub const STATION_STEP: f64 = FOOT;
pub const HALF_STEP: f64 = 6.0 * INCH;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::by_numbers::{inflections, worst_smooth_kink};

    fn rebuild(b0: &Board, step: f64, template: Option<usize>) -> (Board, Measurements, FitReport) {
        let mut t = sample(b0, step, LengthUnit::Centimetres, 16);
        t.template = template;
        let m = parse(&t, LengthUnit::Centimetres).unwrap_or_else(|e| panic!("{e:?}"));
        let (b, r) = build(&m).unwrap();
        (b, m, r)
    }

    #[test]
    fn round_trip_every_template() {
        for (i, tp) in TEMPLATES.iter().enumerate() {
            let b0 = tp.board();
            let (b, m, r) = rebuild(&b0, STATION_STEP, None);
            assert_eq!(m.template, i, "closest template");
            assert!(r.worst() <= 0.1, "{}: fit {r:?}", tp.name);
            let l = b0.length();
            assert!((b.length() - l).abs() < 1e-6);
            for s in &m.stations {
                let x = s.x;
                for (what, a, c) in [
                    ("width", b.width_at(x), b0.width_at(x)),
                    ("rocker", b.rocker_at(x), b0.rocker_at(x)),
                    ("thickness", b.thickness_at(x), b0.thickness_at(x)),
                ] {
                    assert!(
                        (a - c).abs() <= 0.2,
                        "{} {what} at {x:.1}: {a:.3} vs {c:.3}",
                        tp.name
                    );
                }
            }
            let (v0, v) = (b0.volume(), b.volume());
            assert!(
                (v - v0).abs() / v0 <= 0.03,
                "{}: volume {v} vs {v0}",
                tp.name
            );
            // Few knots, fair curves.
            for (s, s0, what) in [
                (&b.outline, &b0.outline, "outline"),
                (&b.bottom, &b0.bottom, "bottom"),
                (&b.deck, &b0.deck, "deck"),
            ] {
                assert!(s.len() <= 12, "{} {what}: {} knots", tp.name, s.len());
                assert!(
                    inflections(s) <= inflections(s0) + 1,
                    "{} {what}: {} inflections",
                    tp.name,
                    inflections(s)
                );
                assert!(worst_smooth_kink(s) < 0.5, "{} {what}", tp.name);
            }
        }
    }

    #[test]
    fn rebuild_with_another_template_still_matches_the_stations() {
        let b0 = TEMPLATES[0].board();
        let (b, m, r) = rebuild(&b0, HALF_STEP, Some(2));
        assert!(r.worst() <= 0.1, "fit {r:?}");
        for s in &m.stations {
            assert!((b.width_at(s.x) - b0.width_at(s.x)).abs() <= 0.2);
            assert!((b.thickness_at(s.x) - b0.thickness_at(s.x)).abs() <= 0.2);
        }
        let (v0, v) = (b0.volume(), b.volume());
        assert!((v - v0).abs() / v0 <= 0.08, "volume {v} vs {v0}");
    }

    #[test]
    fn sparse_imperial_measurements_make_a_fair_board() {
        let t = Table {
            length: "6'2\"".into(),
            tail: TailShape::Round,
            template: None,
            rows: vec![
                Row {
                    position: "tail 0".into(),
                    rocker: "2 1/4".into(),
                    ..Default::default()
                },
                Row {
                    position: "tail 12\"".into(),
                    width: "14 1/2".into(),
                    thickness: "2".into(),
                    ..Default::default()
                },
                Row {
                    position: "centre".into(),
                    width: "20 1/4".into(),
                    thickness: "2 5/8".into(),
                    rocker: "0".into(),
                },
                Row {
                    position: "nose 12\"".into(),
                    width: "15".into(),
                    thickness: "2 1/8".into(),
                    ..Default::default()
                },
                Row {
                    position: "nose 0".into(),
                    rocker: "5 1/4\"".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let m = parse(&t, LengthUnit::FeetInchesFraction).unwrap();
        let (b, r) = build(&m).unwrap();
        assert!(r.worst() <= 0.1, "{r:?}");
        assert!((b.length() - 74.0 * INCH).abs() < 1e-6);
        assert!((b.width_at(b.length() / 2.0) - 20.25 * INCH).abs() < 0.1);
        assert!((b.rocker_at(b.length() - 0.005) - 5.25 * INCH).abs() < 0.1);
        assert!(inflections(&b.outline) <= 1 && inflections(&b.bottom) <= 1);
        assert!(
            b.volume() > 20_000.0 && b.volume() < 60_000.0,
            "{}",
            b.volume()
        );
        // Round tail: the outline leaves the centre line square to it.
        let k0 = &b.outline.knots()[0];
        assert!(k0.next().x.abs() < 1e-9 && k0.next().y > 0.0);
    }

    #[test]
    fn friendly_validation() {
        let mut t = Table::default();
        assert_eq!(
            parse(&t, LengthUnit::Centimetres),
            Err(vec![Problem::NoLength])
        );
        t.length = "6'".into();
        t.rows[1].width = "abc".into();
        t.rows[2].thickness = "-1".into();
        t.rows[3].rocker = "-0.5".into();
        t.rows.push(Row {
            position: "nose 12\"".into(),
            width: "11".into(),
            ..Default::default()
        });
        t.rows.push(Row {
            position: "9'".into(),
            width: "11".into(),
            ..Default::default()
        });
        t.rows.push(Row {
            position: "here".into(),
            ..Default::default()
        });
        let e = parse(&t, LengthUnit::FeetInchesFraction).unwrap_err();
        assert!(
            e.contains(&Problem::Unreadable {
                row: 2,
                column: Column::Width,
                text: "abc".into()
            }),
            "{e:?}"
        );
        assert!(e.contains(&Problem::NotPositive {
            row: 3,
            column: Column::Thickness
        }));
        assert!(e.contains(&Problem::NegativeRocker { row: 4 }));
        assert!(e.contains(&Problem::Duplicate { row: 8, other: 6 }));
        assert!(e.contains(&Problem::OffBoard { row: 9 }));
        assert!(e.contains(&Problem::Unreadable {
            row: 10,
            column: Column::Position,
            text: "here".into()
        }));
        assert!(e.iter().all(|p| !p.message().is_empty()));
        // Only widths are missing: say what is needed.
        let mut t = Table {
            length: "6'".into(),
            ..Default::default()
        };
        t.rows[3].width = "19".into();
        let e = parse(&t, LengthUnit::FeetInchesFraction).unwrap_err();
        assert_eq!(e, vec![Problem::TooFewWidths, Problem::NoThickness]);
        // A tail block wider than the board ahead of it.
        t.rows[0].width = "20".into();
        t.rows[1].width = "14".into();
        t.rows[3].thickness = "2 1/2".into();
        assert_eq!(
            parse(&t, LengthUnit::FeetInchesFraction).unwrap_err(),
            vec![Problem::TailBlockTooWide]
        );
        t.length = "40'".into();
        assert_eq!(
            parse(&t, LengthUnit::FeetInchesFraction).unwrap_err(),
            vec![Problem::LengthOutOfRange]
        );
    }

    #[test]
    fn csv_round_trip_and_spreadsheet_input() {
        let mut t = sample(
            &TEMPLATES[1].board(),
            STATION_STEP,
            LengthUnit::FeetInchesFraction,
            16,
        );
        t.template = Some(2);
        let back = Table::from_csv(&t.to_csv()).unwrap();
        assert_eq!(back, t);
        let sheet = "\u{feff}Length;7'6\"\nTail;swallow\nThickness;Station;Width\n2 1/8;\"tail 12\"\"\";15\n2 3/4;centre;21\n";
        let t = Table::from_csv(sheet).unwrap();
        assert_eq!(t.length, "7'6\"");
        assert_eq!(t.tail, TailShape::Swallow);
        assert_eq!(
            t.rows[0],
            Row {
                position: "tail 12\"".into(),
                width: "15".into(),
                rocker: String::new(),
                thickness: "2 1/8".into()
            }
        );
        assert!(matches!(
            Table::from_csv("template,banana\n"),
            Err(Problem::CsvLine { line: 1, .. })
        ));
    }

    #[test]
    fn bottom_and_rail_options_shape_the_built_board() {
        let base = sample(
            &TEMPLATES[0].board(),
            STATION_STEP,
            LengthUnit::Centimetres,
            16,
        );
        let build_with = |bottom: Option<ContourKind>, depth: &str, rail: Option<usize>| {
            let t = Table {
                bottom,
                bottom_depth: depth.into(),
                rail,
                ..base.clone()
            };
            build(&parse(&t, LengthUnit::Centimetres).unwrap()).unwrap()
        };
        let (template, r0) = build_with(None, "", None);
        let (flat, r1) = build_with(Some(ContourKind::Flat), "", None);
        // The fit is reported on the measured curves, before shaping.
        assert_eq!(r0, r1);
        let v_flat = flat.volume();
        assert!(
            (v_flat - template.volume()).abs() / v_flat < 0.02,
            "flat keeps about the template's volume"
        );
        for (kind, depth) in [
            (ContourKind::SingleConcave, ""),
            (ContourKind::SingleConcave, "0.6cm"),
            (ContourKind::DoubleConcave, ""),
            (ContourKind::Vee, ""),
        ] {
            let (b, _) = build_with(Some(kind), depth, None);
            // Foam is cut, the rail line kept: less volume than the flat bottom.
            assert!(
                b.volume() < v_flat,
                "{kind:?} {depth}: {} vs {v_flat}",
                b.volume()
            );
            for s in &b.sections[b.real_sections()] {
                assert!(
                    !shaping::self_intersects(&s.spline),
                    "{kind:?} at {}",
                    s.position
                );
            }
            assert_eq!(b.length(), flat.length());
        }
        // A deeper concave takes more foam.
        let (shallow, _) = build_with(Some(ContourKind::SingleConcave), "0.2cm", None);
        let (deep, _) = build_with(Some(ContourKind::SingleConcave), "0.8cm", None);
        assert!(deep.volume() < shallow.volume());
        // Rails: every section is reshaped, fitted and free of loops.
        let (soft, _) = build_with(None, "", Some(0));
        let (hard, _) = build_with(None, "", Some(3));
        for b in [&soft, &hard] {
            for s in &b.sections[b.real_sections()] {
                let x = s.position;
                assert!(!shaping::self_intersects(&s.spline), "section at {x}");
                assert!(
                    s.spline
                        .knots()
                        .iter()
                        .all(|k| k.points.iter().all(|p| p.is_finite()))
                );
                assert!((s.width() - b.width_at(x)).abs() < 0.05, "width at {x}");
                assert!(
                    (s.center_thickness() - b.thickness_at(x)).abs() < 0.05,
                    "thickness at {x}"
                );
            }
            assert_ne!(b.sections, template.sections, "rails reshaped");
        }
        assert!(
            (hard.volume() - soft.volume()).abs() / soft.volume() > 0.001,
            "different rails, different volume"
        );
        // Depth checks.
        for bad in ["0", "3cm", "deep"] {
            let t = Table {
                bottom: Some(ContourKind::Vee),
                bottom_depth: bad.into(),
                ..base.clone()
            };
            let e = parse(&t, LengthUnit::Centimetres).unwrap_err();
            assert!(
                e == vec![Problem::DepthOutOfRange]
                    || matches!(
                        e[0],
                        Problem::Unreadable {
                            column: Column::Depth,
                            ..
                        }
                    ),
                "{bad}: {e:?}"
            );
        }
        // The choices survive a CSV round trip.
        let t = Table {
            bottom: Some(ContourKind::DoubleConcave),
            bottom_depth: "3/16\"".into(),
            rail: Some(2),
            ..base.clone()
        };
        assert_eq!(Table::from_csv(&t.to_csv()).unwrap(), t);
        assert!(Table::from_csv("bottom,channels\n").is_err());
        assert!(Table::from_csv("rail,90/10\n").is_err());
    }
}
