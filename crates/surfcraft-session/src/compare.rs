//! Side-by-side measurements of two boards: the main specs plus width, thickness and rocker at regular stations
//! along the length (every foot, or every 30 cm in metric).
//!
//! Differences are `this - other`, so a positive difference means this board is bigger.

use surfcraft_core::Board;
use surfcraft_core::units::FOOT;

/// What a row measures. Stations are distances from the tail (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measure {
    Length,
    MaxWidth,
    WidePointFromTail,
    /// Signed: positive when the wide point is ahead of (nose side of) the centre.
    WidePointFromCentre,
    /// Width one foot (30.48 cm) from the nose.
    NoseWidth1ft,
    /// Width one foot from the tail.
    TailWidth1ft,
    CentreWidth,
    MaxThickness,
    ThickestFromTail,
    CentreThickness,
    NoseRocker,
    TailRocker,
    NoseRocker1ft,
    TailRocker1ft,
    NoseRocker2ft,
    TailRocker2ft,
    Area,
    Volume,
    CentreOfMass,
    StationWidth(f64),
    StationThickness(f64),
    StationRocker(f64),
}

/// How a value is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// cm
    Length,
    /// cm²
    Area,
    /// cm³
    Volume,
}

impl Measure {
    pub fn kind(self) -> Kind {
        match self {
            Measure::Area => Kind::Area,
            Measure::Volume => Kind::Volume,
            _ => Kind::Length,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub measure: Measure,
    /// `None` where the value does not exist (a station past this board's nose).
    pub this: Option<f64>,
    pub other: Option<f64>,
}

impl Row {
    /// `this - other`.
    pub fn diff(&self) -> Option<f64> {
        Some(self.this? - self.other?)
    }
}

/// Every precomputed value of one board that the comparison needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Measures {
    pub length: f64,
    pub values: Vec<(Measure, f64)>,
    stations: Vec<(f64, f64, f64, f64)>,
}

/// Station spacing: one foot in imperial units, 30 cm in metric.
pub fn station_step(imperial: bool) -> f64 {
    if imperial { FOOT } else { 30.0 }
}

/// Stations from the tail, `step` apart, strictly inside a board of `length`.
pub fn stations(length: f64, step: f64) -> Vec<f64> {
    if step <= 0.0 || length.is_nan() {
        return Vec::new();
    }
    (1..)
        .map(|k| k as f64 * step)
        .take_while(|&x| x < length - 1e-6)
        .collect()
}

/// Measure one board. Integrals (area, volume, centre of mass) take a few milliseconds.
pub fn measure(b: &Board, step: f64) -> Measures {
    let l = b.length();
    let (max_t, max_t_pos) = b.max_thickness_and_pos();
    let wide = b.max_width_pos();
    let values = vec![
        (Measure::Length, l),
        (Measure::MaxWidth, b.max_width()),
        (Measure::WidePointFromTail, wide),
        (Measure::WidePointFromCentre, wide - l / 2.0),
        (Measure::NoseWidth1ft, b.width_at(l - FOOT)),
        (Measure::TailWidth1ft, b.width_at(FOOT)),
        (Measure::CentreWidth, b.center_width()),
        (Measure::MaxThickness, max_t),
        (Measure::ThickestFromTail, max_t_pos),
        (Measure::CentreThickness, b.center_thickness()),
        (Measure::NoseRocker, b.rocker_at(l)),
        (Measure::TailRocker, b.rocker_at(0.0)),
        (Measure::NoseRocker1ft, b.rocker_at(l - FOOT)),
        (Measure::TailRocker1ft, b.rocker_at(FOOT)),
        (Measure::NoseRocker2ft, b.rocker_at(l - 2.0 * FOOT)),
        (Measure::TailRocker2ft, b.rocker_at(2.0 * FOOT)),
        (Measure::Area, b.area()),
        (Measure::Volume, b.volume()),
        (Measure::CentreOfMass, b.center_of_mass()),
    ];
    let stations = stations(l, step)
        .into_iter()
        .map(|x| (x, b.width_at(x), b.thickness_at(x), b.rocker_at(x)))
        .collect();
    Measures {
        length: l,
        values,
        stations,
    }
}

fn station_value(m: &Measures, x: f64, which: usize) -> Option<f64> {
    m.stations
        .iter()
        .find(|s| (s.0 - x).abs() < 1e-6)
        .map(|s| [s.1, s.2, s.3][which])
}

/// The comparison table of two measured boards (measured with the same station step).
pub fn rows(this: &Measures, other: &Measures, step: f64) -> Vec<Row> {
    let finite = |v: f64| v.is_finite().then_some(v);
    let mut out: Vec<Row> = this
        .values
        .iter()
        .zip(&other.values)
        .map(|(a, b)| Row {
            measure: a.0,
            this: finite(a.1),
            other: finite(b.1),
        })
        .collect();
    for x in stations(this.length.max(other.length), step) {
        for (which, measure) in [
            Measure::StationWidth(x),
            Measure::StationThickness(x),
            Measure::StationRocker(x),
        ]
        .into_iter()
        .enumerate()
        {
            out.push(Row {
                measure,
                this: station_value(this, x, which).and_then(finite),
                other: station_value(other, x, which).and_then(finite),
            });
        }
    }
    out
}

/// Measure both boards and build the table.
pub fn compare(this: &Board, other: &Board, step: f64) -> Vec<Row> {
    rows(&measure(this, step), &measure(other, step), step)
}

/// One CSV field, quoted when needed (feet-inch values contain `'` and `"`).
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The table as CSV: a header line and one line per row. `label` names a measure, `value` formats a value
/// (`signed` for differences); missing values are empty fields.
pub fn to_csv(
    header: [&str; 4],
    rows: &[Row],
    label: impl Fn(Measure) -> String,
    value: impl Fn(Kind, f64, bool) -> String,
) -> String {
    let mut s = header.map(csv_field).join(",");
    s.push('\n');
    for r in rows {
        let k = r.measure.kind();
        let cells = [
            label(r.measure),
            r.this.map(|v| value(k, v, false)).unwrap_or_default(),
            r.other.map(|v| value(k, v, false)).unwrap_or_default(),
            r.diff().map(|v| value(k, v, true)).unwrap_or_default(),
        ];
        s.push_str(&cells.map(|c| csv_field(&c)).join(","));
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(name: &str) -> Board {
        surfcraft_io::templates::TEMPLATES
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .board()
    }

    #[test]
    fn stations_every_foot_inside_the_board() {
        let s = stations(6.0 * FOOT + 1.0, FOOT);
        assert_eq!(s.len(), 6);
        assert!((s[5] - 6.0 * FOOT).abs() < 1e-9);
        // A station exactly at the nose is left out.
        assert_eq!(stations(3.0 * FOOT, FOOT).len(), 2);
        assert_eq!(stations(100.0, 30.0), vec![30.0, 60.0, 90.0]);
        assert!(stations(100.0, 0.0).is_empty());
    }

    #[test]
    fn same_board_has_zero_differences() {
        let b = template("Shortboard");
        let rows = compare(&b, &b, FOOT);
        assert_eq!(rows.len(), 19 + 3 * 5);
        for r in &rows {
            assert_eq!(r.diff(), Some(0.0), "{:?}", r.measure);
        }
    }

    #[test]
    fn scaled_board_differences_have_the_right_sign_and_size() {
        let a = template("Shortboard");
        let mut b = a.clone();
        let (l, w, t) = (a.length(), a.max_width(), a.max_thickness());
        b.scale(l, w + 2.0, t);
        let rows = compare(&a, &b, 30.0);
        let get = |m: Measure| rows.iter().find(|r| r.measure == m).copied().unwrap();
        let d = get(Measure::MaxWidth).diff().unwrap();
        assert!((d + 2.0).abs() < 0.01, "this is 2 cm narrower: {d}");
        assert!(get(Measure::Length).diff().unwrap().abs() < 1e-9);
        assert!(get(Measure::Volume).diff().unwrap() < 0.0);
        assert!(get(Measure::Area).diff().unwrap() < 0.0);
        // Every station width is smaller on `this`.
        let widths: Vec<Row> = rows
            .iter()
            .filter(|r| matches!(r.measure, Measure::StationWidth(_)))
            .copied()
            .collect();
        assert_eq!(widths.len(), stations(l, 30.0).len());
        assert!(widths.iter().all(|r| r.diff().unwrap() < 0.0));
        // Spec values agree with the board's own measurements.
        assert!((get(Measure::NoseWidth1ft).this.unwrap() - a.width_at(l - FOOT)).abs() < 1e-12);
        assert!((get(Measure::Volume).this.unwrap() - a.volume()).abs() < 1e-9);
        let c = get(Measure::WidePointFromCentre).this.unwrap();
        assert!((c - (a.max_width_pos() - l / 2.0)).abs() < 1e-12);
    }

    #[test]
    fn longer_board_has_stations_the_shorter_one_lacks() {
        let a = template("Shortboard");
        let b = template("Longboard");
        let rows = compare(&a, &b, FOOT);
        let past: Vec<&Row> = rows
            .iter()
            .filter(|r| matches!(r.measure, Measure::StationWidth(x) if x > a.length()))
            .collect();
        assert!(!past.is_empty());
        assert!(
            past.iter()
                .all(|r| r.this.is_none() && r.other.is_some() && r.diff().is_none())
        );
        assert!(
            rows.iter()
                .find(|r| r.measure == Measure::Length)
                .unwrap()
                .diff()
                .unwrap()
                < 0.0
        );
    }

    #[test]
    fn csv_quotes_feet_and_inches() {
        let rows = [
            Row {
                measure: Measure::Length,
                this: Some(FOOT * 6.0),
                other: Some(FOOT * 6.5),
            },
            Row {
                measure: Measure::StationWidth(30.0),
                this: None,
                other: Some(40.0),
            },
        ];
        let csv = to_csv(
            ["Measure", "This, board", "Other", "Difference"],
            &rows,
            |m| format!("{m:?}"),
            |_, v, signed| {
                if signed {
                    format!("{v:+.1}\"")
                } else {
                    format!("{v:.1}\"")
                }
            },
        );
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Measure,\"This, board\",Other,Difference");
        assert_eq!(lines[1], "Length,\"182.9\"\"\",\"198.1\"\"\",\"-15.2\"\"\"");
        assert_eq!(lines[2], "StationWidth(30.0),,\"40.0\"\"\",");
        assert_eq!(lines.len(), 3);
    }
}
