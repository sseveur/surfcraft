//! Foam blanks: the dimensions a blank maker publishes, an approximate shape built from them, and the check
//! that a board fits inside one.
//!
//! A catalogue gives the length, the widest width (and where it is), the thickness at the middle and the nose
//! and tail rocker; measurement drawings add the width, thickness and rocker at stations measured from each tip
//! (6", 12", 18", 24", ...). The blank's outline, bottom and deck are rebuilt from these as monotone cubics
//! through the published points (they never overshoot the data). Where a side has no station data, typical
//! stations are added, with proportions measured on the US Blanks drawings of the built-in library
//! (`docs/blanks.md`):
//!
//! * **Outline**: half-widths at the tip, 12" and 24" from the nose of 33 %, 100 % and 100 % - 30 % of the
//!   missing width relative to the 12" width, which is 70 % (nose) / 74 % (tail) of the widest width when not
//!   given. Ends without a measured tip width are extrapolated from the two outermost stations; both ends close
//!   with a rounded cap.
//! * **Bottom (rocker)**: zero at a flat point (the middle, or between the innermost rocker stations), rising
//!   to the nose and tail rocker as a power of the distance (2.7 toward the nose, 2.2 toward the tail, or the
//!   exponent through the innermost published station); published stations are matched exactly.
//! * **Deck**: bottom plus the thickness, the published thickness at the middle thinning to 37 % (nose) /
//!   43 % (tail) at the tips through 67 % / 69 % at 12" and 88 % / 87 % at 24" when no thickness stations are
//!   given.
//!
//! The fit check places the board in the blank (lengthwise offset, tilt in the profile plane, height) and
//! compares, at stations along the board, the board's outline with the blank's outline and the board's deck
//! and bottom on the stringer with the blank's. Lengths are centimetres, x runs from the tail.

use std::fmt::Write as _;

use glam::DVec2;

use crate::board::Board;
use crate::units::{FOOT, INCH};

/// Typical 12" widths relative to the widest width (used when the catalogue gives none).
pub const NOSE_12_RATIO: f64 = 0.70;
pub const TAIL_12_RATIO: f64 = 0.74;
/// Clearances above this (cm) count as touching, not as poking out.
const TOLERANCE: f64 = 1e-6;

/// Measurements at one station of a blank, measured from the nose or the tail tip. Lengths in cm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlankStation {
    /// Measured from the nose tip (else from the tail tip).
    pub from_nose: bool,
    /// Distance from that tip.
    pub dist: f64,
    pub width: Option<f64>,
    /// Thickness on the stringer.
    pub thickness: Option<f64>,
    /// Height of the bottom above its lowest point.
    pub rocker: Option<f64>,
}

/// A blank as a catalogue describes it. Lengths in cm.
#[derive(Clone, Debug, PartialEq)]
pub struct Blank {
    pub name: String,
    /// Length tip to tip.
    pub length: f64,
    /// Widest width.
    pub width: f64,
    /// Position of the widest width from the middle of the blank, positive toward the nose.
    pub wide_point: f64,
    /// Thickness at the middle (the thickest point).
    pub thickness: f64,
    /// Height of the bottom at the nose / tail tip above its lowest point.
    pub nose_rocker: f64,
    pub tail_rocker: f64,
    /// Measurements at stations (any order).
    pub stations: Vec<BlankStation>,
}

impl Default for Blank {
    fn default() -> Self {
        Self {
            name: String::new(),
            length: 193.0,
            width: 56.0,
            wide_point: 0.0,
            thickness: 8.0,
            nose_rocker: 12.0,
            tail_rocker: 6.0,
            stations: Vec::new(),
        }
    }
}

/// Which measurement of a station.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Width,
    Thickness,
    Rocker,
}

impl BlankStation {
    fn get(&self, m: Measure) -> Option<f64> {
        match m {
            Measure::Width => self.width,
            Measure::Thickness => self.thickness,
            Measure::Rocker => self.rocker,
        }
    }

    fn slot(&mut self, m: Measure) -> &mut Option<f64> {
        match m {
            Measure::Width => &mut self.width,
            Measure::Thickness => &mut self.thickness,
            Measure::Rocker => &mut self.rocker,
        }
    }
}

impl Blank {
    /// A measurement at the station `dist` from the nose or tail tip.
    pub fn station(&self, from_nose: bool, dist: f64, m: Measure) -> Option<f64> {
        self.stations
            .iter()
            .find(|s| s.from_nose == from_nose && (s.dist - dist).abs() < 1e-6)
            .and_then(|s| s.get(m))
    }

    /// Set or clear a measurement at a station (the station is added or removed as needed).
    pub fn set_station(&mut self, from_nose: bool, dist: f64, m: Measure, v: Option<f64>) {
        let i = self
            .stations
            .iter()
            .position(|s| s.from_nose == from_nose && (s.dist - dist).abs() < 1e-6);
        let i = match (i, v) {
            (Some(i), _) => i,
            (None, None) => return,
            (None, Some(_)) => {
                self.stations.push(BlankStation {
                    from_nose,
                    dist,
                    width: None,
                    thickness: None,
                    rocker: None,
                });
                self.stations.len() - 1
            }
        };
        *self.stations[i].slot(m) = v;
        let s = &self.stations[i];
        if s.width.is_none() && s.thickness.is_none() && s.rocker.is_none() {
            self.stations.remove(i);
        }
    }

    /// x (from the tail) of a station.
    pub fn station_x(&self, s: &BlankStation) -> f64 {
        if s.from_nose {
            self.length - s.dist
        } else {
            s.dist
        }
    }
}

/// Where the board sits in the blank.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    /// Distance from the blank's tail to the board's tail, cm. `None` centres the board lengthwise.
    pub tail_offset: Option<f64>,
    /// Rotation in the profile plane, degrees, positive lifts the nose. The board turns about its mid-length
    /// on its rocker baseline.
    pub tilt: f64,
    /// Height of the board's rocker baseline above the blank's, cm. `None` centres the board between the
    /// blank's deck and bottom (equal smallest clearance above and below).
    pub lift: Option<f64>,
}

/// The blank chosen for a board and where the board sits in it (stored in the `.brd` file, code p63).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlankSetup {
    pub blank: Blank,
    pub placement: Placement,
}

// ------------------------------------------------------------------------------------------- validation

impl Blank {
    /// Problems with the numbers, if any (human-readable, English).
    pub fn validate(&self) -> Result<(), String> {
        let main = [
            self.length,
            self.width,
            self.wide_point,
            self.thickness,
            self.nose_rocker,
            self.tail_rocker,
        ];
        let st = self
            .stations
            .iter()
            .flat_map(|s| [Some(s.dist), s.width, s.thickness, s.rocker])
            .flatten();
        if main.iter().copied().chain(st).any(|v| !v.is_finite()) {
            return Err("every dimension must be a number".into());
        }
        if !(30.0..=1000.0).contains(&self.length) {
            return Err("length must be between 30 cm and 10 m".into());
        }
        if !(5.0..=200.0).contains(&self.width) || self.width >= self.length {
            return Err("width must be between 5 cm and 2 m, and less than the length".into());
        }
        if self.wide_point.abs() > self.length / 4.0 {
            return Err(
                "the wide point must be within a quarter of the length from the middle".into(),
            );
        }
        if !(1.0..=60.0).contains(&self.thickness) {
            return Err("thickness must be between 1 cm and 60 cm".into());
        }
        if self.nose_rocker < 0.0 || self.tail_rocker < 0.0 {
            return Err("rocker cannot be negative".into());
        }
        if self.nose_rocker.max(self.tail_rocker) > self.length / 3.0 {
            return Err("rocker is larger than a third of the length".into());
        }
        for s in &self.stations {
            if s.dist < 0.0 || s.dist > self.length * 0.75 {
                return Err("stations must be on the blank".into());
            }
            if s.width.is_some_and(|w| w <= 0.0 || w > self.width * 1.0001) {
                return Err("station widths must be positive and not wider than the blank".into());
            }
            if s.thickness
                .is_some_and(|t| t <= 0.0 || t > self.thickness * 1.0001)
            {
                return Err(
                    "station thicknesses must be positive and not thicker than the middle".into(),
                );
            }
            if s.rocker.is_some_and(|r| r < 0.0) {
                return Err("rocker cannot be negative".into());
            }
        }
        // Rocker decreases from each tip toward the middle.
        for nose in [true, false] {
            let mut pts: Vec<(f64, f64)> = self
                .stations
                .iter()
                .filter(|s| s.from_nose == nose)
                .filter_map(|s| s.rocker.map(|r| (s.dist, r)))
                .collect();
            pts.push((
                0.0,
                if nose {
                    self.nose_rocker
                } else {
                    self.tail_rocker
                },
            ));
            pts.sort_by(|a, b| a.0.total_cmp(&b.0));
            if pts.windows(2).any(|w| w[1].1 > w[0].1 + 1e-9) {
                return Err("rocker must decrease from each tip toward the middle".into());
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------- file format

impl Blank {
    /// One-line form `L=… W=… WP=… T=… NR=… TR=… S=N,dist,width,thickness,rocker … name=…` (cm, shortest
    /// round-trip digits, empty for a missing station value; the name is last and runs to the end of the
    /// line). Used for the `.brd` field and the saved user blanks.
    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        for (k, v) in [
            ("L", self.length),
            ("W", self.width),
            ("WP", self.wide_point),
            ("T", self.thickness),
            ("NR", self.nose_rocker),
            ("TR", self.tail_rocker),
        ] {
            let _ = write!(s, "{k}={v} ");
        }
        let opt = |v: Option<f64>| v.map(|v| v.to_string()).unwrap_or_default();
        for st in &self.stations {
            let _ = write!(
                s,
                "S={},{},{},{},{} ",
                if st.from_nose { "N" } else { "T" },
                st.dist,
                opt(st.width),
                opt(st.thickness),
                opt(st.rocker)
            );
        }
        s.push_str("name=");
        s.push_str(&one_line(&self.name));
        s
    }

    /// Parse [`Self::to_compact`]. Unknown keys are ignored; length, width and thickness are required.
    pub fn from_compact(text: &str) -> Option<Blank> {
        let (head, name) = split_name(text);
        let mut b = Blank {
            name: name.trim().to_string(),
            wide_point: 0.0,
            nose_rocker: 0.0,
            tail_rocker: 0.0,
            ..Blank::default()
        };
        let mut seen = (false, false, false);
        let num = |v: &str| v.parse::<f64>().ok().filter(|v| v.is_finite());
        for tok in head.split_whitespace() {
            let Some((k, v)) = tok.split_once('=') else {
                continue;
            };
            match k {
                "L" => (b.length, seen.0) = (num(v)?, true),
                "W" => (b.width, seen.1) = (num(v)?, true),
                "T" => (b.thickness, seen.2) = (num(v)?, true),
                "WP" => b.wide_point = num(v)?,
                "NR" => b.nose_rocker = num(v)?,
                "TR" => b.tail_rocker = num(v)?,
                "S" => {
                    let f: Vec<&str> = v.split(',').collect();
                    let [end, dist, w, t, r] = f[..] else {
                        return None;
                    };
                    let opt = |s: &str| {
                        if s.is_empty() {
                            Some(None)
                        } else {
                            num(s).map(Some)
                        }
                    };
                    b.stations.push(BlankStation {
                        from_nose: match end {
                            "N" => true,
                            "T" => false,
                            _ => return None,
                        },
                        dist: num(dist)?,
                        width: opt(w)?,
                        thickness: opt(t)?,
                        rocker: opt(r)?,
                    });
                }
                _ => {}
            }
        }
        (seen.0 && seen.1 && seen.2).then_some(b)
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ").trim().to_string()
}

/// Split at the `name=` token (the name may contain anything, including `=` and spaces).
fn split_name(text: &str) -> (&str, &str) {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("name=") {
        return ("", rest);
    }
    match t.find(" name=") {
        Some(i) => (&t[..i], &t[i + 6..]),
        None => (t, ""),
    }
}

impl BlankSetup {
    /// The `.brd` value: the blank, then the placement keys, then the name.
    pub fn to_compact(&self) -> String {
        let b = self.blank.to_compact();
        let (head, name) = split_name(&b);
        let mut s = head.trim_end().to_string();
        let p = &self.placement;
        if let Some(v) = p.tail_offset {
            let _ = write!(s, " SH={v}");
        }
        if p.tilt != 0.0 {
            let _ = write!(s, " TI={}", p.tilt);
        }
        if let Some(v) = p.lift {
            let _ = write!(s, " LI={v}");
        }
        let _ = write!(s, " name={name}");
        s
    }

    pub fn from_compact(text: &str) -> Option<BlankSetup> {
        let blank = Blank::from_compact(text)?;
        let (head, _) = split_name(text);
        let mut placement = Placement::default();
        for tok in head.split_whitespace() {
            let Some((k, v)) = tok.split_once('=') else {
                continue;
            };
            let Some(v) = v.parse::<f64>().ok().filter(|v| v.is_finite()) else {
                continue;
            };
            match k {
                "SH" => placement.tail_offset = Some(v),
                "TI" => placement.tilt = v,
                "LI" => placement.lift = Some(v),
                _ => {}
            }
        }
        Some(BlankSetup { blank, placement })
    }
}

// ------------------------------------------------------------------------------------------- curves

/// Monotone piecewise cubic (Fritsch-Carlson, three-point slopes) through points with increasing x: never overshoots the data, so
/// monotone data gives a monotone curve.
struct Pchip {
    x: Vec<f64>,
    y: Vec<f64>,
    m: Vec<f64>,
}

impl Pchip {
    fn new(pts: &[(f64, f64)]) -> Self {
        let mut pts: Vec<(f64, f64)> = pts.to_vec();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
        let x: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let y: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let n = x.len();
        let mut m = vec![0.0; n];
        if n >= 2 {
            let h: Vec<f64> = (0..n - 1).map(|k| x[k + 1] - x[k]).collect();
            let d: Vec<f64> = (0..n - 1).map(|k| (y[k + 1] - y[k]) / h[k]).collect();
            if n == 2 {
                m = vec![d[0], d[0]];
            } else {
                // Three-point slopes (zero at a local extreme of the data), later limited for monotonicity.
                for k in 1..n - 1 {
                    if d[k - 1] * d[k] > 0.0 {
                        m[k] = (h[k] * d[k - 1] + h[k - 1] * d[k]) / (h[k - 1] + h[k]);
                    }
                }
                let end = |h0: f64, h1: f64, d0: f64, d1: f64| {
                    let s = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
                    if s * d0 <= 0.0 {
                        0.0
                    } else if d0 * d1 <= 0.0 && s.abs() > 3.0 * d0.abs() {
                        3.0 * d0
                    } else {
                        s
                    }
                };
                m[0] = end(h[0], h[1], d[0], d[1]);
                m[n - 1] = end(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
                // Fritsch-Carlson limiter: keeps each piece monotone (no overshoot).
                for k in 0..n - 1 {
                    if d[k] == 0.0 {
                        m[k] = 0.0;
                        m[k + 1] = 0.0;
                        continue;
                    }
                    let (a, b) = (m[k] / d[k], m[k + 1] / d[k]);
                    if a < 0.0 {
                        m[k] = 0.0;
                    }
                    if b < 0.0 {
                        m[k + 1] = 0.0;
                    }
                    let r = a * a + b * b;
                    if r > 9.0 {
                        let t = 3.0 / r.sqrt();
                        m[k] = t * a * d[k];
                        m[k + 1] = t * b * d[k];
                    }
                }
            }
        }
        Self { x, y, m }
    }

    fn at(&self, t: f64) -> f64 {
        let n = self.x.len();
        match n {
            0 => return 0.0,
            1 => return self.y[0],
            _ => {}
        }
        let t = t.clamp(self.x[0], self.x[n - 1]);
        let k = match self.x.partition_point(|&v| v <= t) {
            0 => 0,
            i => (i - 1).min(n - 2),
        };
        let h = self.x[k + 1] - self.x[k];
        let s = (t - self.x[k]) / h;
        let (s2, s3) = (s * s, s * s * s);
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        h00 * self.y[k] + h10 * h * self.m[k] + h01 * self.y[k + 1] + h11 * h * self.m[k + 1]
    }
}

/// Number of intervals of the sampled blank curves.
const SAMPLES: usize = 800;

/// The blank's outline (half-width), bottom and deck on the stringer, sampled densely from tail (x = 0) to
/// nose (x = length). Rocker is measured from the lowest point of the bottom (z = 0).
#[derive(Clone, Debug, PartialEq)]
pub struct BlankShape {
    pub length: f64,
    half: Vec<f64>,
    bottom: Vec<f64>,
    deck: Vec<f64>,
    /// Position of the lowest point of the bottom.
    pub flat_point: f64,
}

impl BlankShape {
    pub fn new(b: &Blank) -> Self {
        let l = b.length.max(1.0);
        let half_fn = outline_curve(b, l);
        let (rocker, flat_point) = rocker_curve(b, l);
        let thick = thickness_curve(b, l);
        let xs = (0..=SAMPLES).map(|i| l * i as f64 / SAMPLES as f64);
        let mut half = Vec::with_capacity(SAMPLES + 1);
        let mut bottom = Vec::with_capacity(SAMPLES + 1);
        let mut deck = Vec::with_capacity(SAMPLES + 1);
        for x in xs {
            let z = rocker.at(x).max(0.0);
            half.push(half_fn(x).max(0.0));
            bottom.push(z);
            deck.push(z + thick.at(x).max(0.0));
        }
        Self {
            length: l,
            half,
            bottom,
            deck,
            flat_point,
        }
    }

    fn lerp(&self, v: &[f64], x: f64) -> f64 {
        let f = (x / self.length).clamp(0.0, 1.0) * SAMPLES as f64;
        let i = (f.floor() as usize).min(SAMPLES - 1);
        let t = f - i as f64;
        v[i] + (v[i + 1] - v[i]) * t
    }

    /// Half-width at `x` (0 outside the blank).
    pub fn half_width_at(&self, x: f64) -> f64 {
        if !(0.0..=self.length).contains(&x) {
            return 0.0;
        }
        self.lerp(&self.half, x)
    }

    pub fn bottom_at(&self, x: f64) -> f64 {
        self.lerp(&self.bottom, x)
    }

    pub fn deck_at(&self, x: f64) -> f64 {
        self.lerp(&self.deck, x)
    }

    pub fn thickness_at(&self, x: f64) -> f64 {
        self.deck_at(x) - self.bottom_at(x)
    }

    /// Half outline as (x, half-width) points, tail to nose.
    pub fn outline_points(&self) -> Vec<DVec2> {
        self.points(&self.half)
    }

    pub fn bottom_points(&self) -> Vec<DVec2> {
        self.points(&self.bottom)
    }

    pub fn deck_points(&self) -> Vec<DVec2> {
        self.points(&self.deck)
    }

    fn points(&self, v: &[f64]) -> Vec<DVec2> {
        v.iter()
            .enumerate()
            .map(|(i, &y)| DVec2::new(self.length * i as f64 / SAMPLES as f64, y))
            .collect()
    }
}

/// Published values of one measurement on one side, as (distance from the tip, value), nearest the tip first.
/// Stations closer than 1 cm to `limit` (the distance from the tip to the middle or the wide point) or beyond
/// it are left out.
fn side_points(b: &Blank, nose: bool, m: Measure, limit: f64) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = b
        .stations
        .iter()
        .filter(|s| s.from_nose == nose && s.dist < limit - 1.0)
        .filter_map(|s| s.get(m).map(|v| (s.dist, v)))
        .collect();
    v.sort_by(|a, c| a.0.total_cmp(&c.0));
    v
}

/// Value at the tip extrapolated from the two stations nearest it, kept between `lo` and the nearest value.
fn extrapolate_tip(pts: &[(f64, f64)], lo: f64) -> f64 {
    match pts {
        [(d1, v1), (d2, v2), ..] => {
            let slope = (v2 - v1) / (d2 - d1);
            (v1 - slope * d1).clamp(lo.min(*v1), *v1)
        }
        [(_, v1)] => v1 * 0.5,
        [] => lo,
    }
}

fn outline_curve(b: &Blank, l: f64) -> impl Fn(f64) -> f64 + use<> {
    let w2 = b.width / 2.0;
    let xw = (l / 2.0 + b.wide_point).clamp(0.25 * l, 0.75 * l);
    let mut pts = vec![(xw, w2)];
    // Tip half-widths, and whether each was measured (else extrapolated: a round end).
    let mut tips = [(0.0, true); 2];
    for (k, nose) in [true, false].into_iter().enumerate() {
        let span = if nose { l - xw } else { xw };
        let x_of = |d: f64| if nose { l - d } else { d };
        let mut side: Vec<(f64, f64)> = side_points(b, nose, Measure::Width, span)
            .into_iter()
            .map(|(d, w)| (d, w / 2.0))
            .collect();
        if side.is_empty() {
            // Typical proportions (US Blanks drawings): 12" width, then the tip and 24" relative to it.
            let ratio = if nose { NOSE_12_RATIO } else { TAIL_12_RATIO };
            let h12 = w2 * ratio;
            let tip = h12 * if nose { 0.33 } else { 0.54 };
            for (d, h) in [
                (0.0, tip),
                (FOOT, h12),
                (2.0 * FOOT, h12 + 0.7 * (w2 - h12)),
            ] {
                if d < span - 1.0 {
                    side.push((d, h));
                }
            }
        }
        let measured = side.first().is_some_and(|p| p.0 <= 0.0);
        if !measured {
            let tip = extrapolate_tip(&side, 0.0);
            side.insert(0, (0.0, tip));
        }
        tips[k] = (side[0].1, measured);
        pts.extend(side.iter().map(|&(d, h)| (x_of(d), h)));
    }
    let p = Pchip::new(&pts);
    // Rounded ends: a quarter ellipse closes the outline over `r` at each tip. A measured tip width is the
    // width at the very end (only the corners are rounded); an extrapolated one makes a round end, at most to
    // the first station.
    let station_gap = |nose: bool| {
        side_points(b, nose, Measure::Width, l)
            .iter()
            .map(|s| s.0)
            .find(|&d| d > 0.5)
            .unwrap_or(FOOT)
    };
    let radius = |(tip, measured): (f64, bool), nose: bool| {
        if measured {
            tip.clamp(0.1, 1.5)
        } else {
            tip.clamp(0.5, 15.0).min(0.9 * station_gap(nose))
        }
    };
    let (rn, rt) = (radius(tips[0], true), radius(tips[1], false));
    move |x: f64| {
        let mut h = p.at(x);
        for (r, d) in [(rn, l - x), (rt, x)] {
            if d < r {
                let u = 1.0 - d.max(0.0) / r;
                h *= (1.0 - u * u).max(0.0).sqrt();
            }
        }
        h
    }
}

/// Rocker curve and the position of its lowest point.
fn rocker_curve(b: &Blank, l: f64) -> (Pchip, f64) {
    let (nr, tr) = (b.nose_rocker.max(0.0), b.tail_rocker.max(0.0));
    let nose = side_points(b, true, Measure::Rocker, l * 0.75);
    let tail = side_points(b, false, Measure::Rocker, l * 0.75);
    // Flat point: between the innermost stations of both sides, nearer the lower one (as for a parabola);
    // else the middle, kept clear of the stations.
    let inner_n = nose.last().map(|&(d, v)| (l - d, v));
    let inner_t = tail.last().copied();
    let x0 = match (inner_t, inner_n) {
        (Some((xa, va)), Some((xb, vb))) if xb > xa => {
            let (sa, sb) = (va.max(0.0).sqrt(), vb.max(0.0).sqrt());
            if sa + sb > 0.0 {
                xa + (xb - xa) * sa / (sa + sb)
            } else {
                (xa + xb) / 2.0
            }
        }
        _ => {
            let lo = inner_t.map_or(0.25 * l, |p| p.0 + 2.0);
            let hi = inner_n.map_or(0.75 * l, |p| p.0 - 2.0);
            (l / 2.0).clamp(lo.min(hi), hi.max(lo))
        }
    };
    let mut pts = vec![(x0, 0.0)];
    for (tip, side, span, power, at) in [
        (
            nr,
            &nose,
            l - x0,
            2.7,
            &(|u: f64| x0 + u) as &dyn Fn(f64) -> f64,
        ),
        (tr, &tail, x0, 2.2, &|u: f64| x0 - u),
    ] {
        pts.push((at(span), tip));
        // Stations as (distance from the flat point, value).
        let given: Vec<(f64, f64)> = side
            .iter()
            .map(|&(d, v)| (span - d, v))
            .filter(|&(u, _)| u > 0.5 && u < span)
            .collect();
        for &(u, v) in &given {
            pts.push((at(u), v));
        }
        let inner = given.iter().copied().min_by(|a, c| a.0.total_cmp(&c.0));
        // Power curve from the flat point to the innermost station (or the tip).
        let (u1, v1) = inner.unwrap_or((span, tip));
        if v1 > 0.0 {
            for f in [0.2, 0.4, 0.6, 0.8] {
                let u = f * u1;
                if u < u1 - 1.0 {
                    pts.push((at(u), v1 * f.powf(power)));
                }
            }
        }
    }
    (Pchip::new(&pts), x0)
}

fn thickness_curve(b: &Blank, l: f64) -> Pchip {
    let t = b.thickness;
    let xm = l / 2.0;
    let mut pts = vec![(xm, t)];
    for nose in [true, false] {
        let x_of = |d: f64| if nose { l - d } else { d };
        let mut side = side_points(b, nose, Measure::Thickness, xm);
        if side.is_empty() {
            // Typical taper (US Blanks drawings), as fractions of the middle thickness.
            let f = if nose {
                [0.37, 0.67, 0.88]
            } else {
                [0.43, 0.69, 0.87]
            };
            for (d, f) in [(0.0, f[0]), (FOOT, f[1]), (2.0 * FOOT, f[2])] {
                if d < xm - 1.0 {
                    side.push((d, t * f));
                }
            }
        }
        if side.first().is_none_or(|p| p.0 > 0.0) {
            let tip = extrapolate_tip(&side, 0.2 * t);
            side.insert(0, (0.0, tip));
        }
        pts.extend(side.iter().map(|&(d, v)| (x_of(d), v)));
    }
    Pchip::new(&pts)
}

// ------------------------------------------------------------------------------------------- fit

/// Which view a clearance belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FitView {
    /// Plan view, per side.
    Outline,
    Deck,
    Bottom,
    /// The board is longer than the blank at that end.
    Length,
}

/// Smallest clearance in one view and the board station (cm from the board's tail) where it occurs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clearance {
    pub min: f64,
    pub at: f64,
}

/// Clearances at one board station.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitSample {
    /// Board station, cm from the board's tail.
    pub x: f64,
    pub outline: f64,
    pub deck: f64,
    pub bottom: f64,
}

/// A stretch of the board that sticks out of the blank in one view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Problem {
    pub view: FitView,
    /// Board stations (cm from the tail) where it starts and ends.
    pub from: f64,
    pub to: f64,
    /// How far it sticks out at the worst point (positive cm) and where.
    pub depth: f64,
    pub at: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FitReport {
    /// Length of the board that was placed (its middle is the tilt pivot).
    pub board_length: f64,
    /// The placement used, with the tail offset and lift resolved to numbers.
    pub tail_offset: f64,
    pub tilt: f64,
    pub lift: f64,
    pub fits: bool,
    pub outline: Clearance,
    pub deck: Clearance,
    pub bottom: Clearance,
    /// Foam left beyond the board at each end (negative: the board is longer than the blank there).
    pub tail_spare: f64,
    pub nose_spare: f64,
    pub samples: Vec<FitSample>,
    pub problems: Vec<Problem>,
}

impl FitReport {
    /// Smallest of every clearance, including the foam left at the ends.
    pub fn min_clearance(&self) -> f64 {
        self.outline
            .min
            .min(self.deck.min)
            .min(self.bottom.min)
            .min(self.tail_spare)
            .min(self.nose_spare)
    }

    /// A point in the blank's frame (x from the blank's tail, height above its lowest bottom point) in the
    /// board's frame (x from the board's tail, height on the board's rocker baseline): for drawing the blank
    /// behind the board.
    pub fn to_board(&self, p: DVec2) -> DVec2 {
        let pivot = self.board_length / 2.0;
        let (sin, cos) = self.tilt.to_radians().sin_cos();
        let d = p - DVec2::new(pivot + self.tail_offset, self.lift);
        DVec2::new(pivot + d.x * cos + d.y * sin, -d.x * sin + d.y * cos)
    }

    /// The view with the smallest clearance.
    pub fn tightest(&self) -> (FitView, f64, f64) {
        let mut best = (FitView::Outline, self.outline.min, self.outline.at);
        for c in [
            (FitView::Deck, self.deck.min, self.deck.at),
            (FitView::Bottom, self.bottom.min, self.bottom.at),
            (FitView::Length, self.tail_spare, 0.0),
            (FitView::Length, self.nose_spare, f64::NAN),
        ] {
            if c.1 < best.1 {
                best = c;
            }
        }
        best
    }
}

/// The board sampled at stations, ready to be placed many times.
pub struct BoardSamples {
    pub length: f64,
    /// (x, half-width, bottom, deck) on the stringer.
    pts: Vec<[f64; 4]>,
}

impl BoardSamples {
    pub fn new(board: &Board) -> Self {
        let l = board.length().max(0.0);
        let n = ((l / 1.0).ceil() as usize).clamp(100, 1000);
        let pts = (0..=n)
            .map(|i| {
                let x = l * i as f64 / n as f64;
                let r = board.rocker_at(x);
                let d = board.deck_at(x).max(r);
                // The blank is symmetric: an asymmetric board needs room for its wider rail.
                let half = if board.is_asymmetric() {
                    crate::Rail::BOTH
                        .map(|rail| board.half_width_at(x, rail))
                        .into_iter()
                        .fold(f64::MIN, f64::max)
                } else {
                    board.width_at(x) / 2.0
                };
                [x, half.max(0.0), r, d]
            })
            .collect();
        Self { length: l, pts }
    }
}

/// Resolved tail offset for a placement.
pub fn tail_offset(blank_length: f64, board_length: f64, p: &Placement) -> f64 {
    p.tail_offset.unwrap_or((blank_length - board_length) / 2.0)
}

/// Board point (x, z) in blank coordinates for the placement (without lift).
fn place(x: f64, z: f64, pivot: f64, shift: f64, cos: f64, sin: f64) -> DVec2 {
    let (dx, dz) = (x - pivot, z);
    DVec2::new(pivot + shift + dx * cos - dz * sin, dx * sin + dz * cos)
}

/// Fit the board in the blank at a placement.
pub fn fit(board: &Board, blank: &Blank, placement: &Placement) -> FitReport {
    fit_samples(
        &BoardSamples::new(board),
        &BlankShape::new(blank),
        placement,
    )
}

/// [`fit`] with the board and the blank already sampled.
pub fn fit_samples(board: &BoardSamples, shape: &BlankShape, placement: &Placement) -> FitReport {
    let shift = tail_offset(shape.length, board.length, placement);
    let (sin, cos) = placement.tilt.to_radians().sin_cos();
    let pivot = board.length / 2.0;
    let inside = |x: f64| (0.0..=shape.length).contains(&x);
    // Clearances without lift; positions outside the blank are marked with their distance outside.
    let mut raw = Vec::with_capacity(board.pts.len());
    let (mut deck_min, mut bottom_min) = (f64::INFINITY, f64::INFINITY);
    for &[x, h, r, d] in &board.pts {
        let pb = place(x, r, pivot, shift, cos, sin);
        let pd = place(x, d, pivot, shift, cos, sin);
        let outline = shape.half_width_at(pb.x) - h;
        let outside = |p: DVec2| (-p.x).max(p.x - shape.length).max(0.0);
        let (deck, bottom) = if inside(pb.x) && inside(pd.x) {
            let deck = shape.deck_at(pd.x) - pd.y;
            let bottom = pb.y - shape.bottom_at(pb.x);
            deck_min = deck_min.min(deck);
            bottom_min = bottom_min.min(bottom);
            (Some(deck), Some(bottom))
        } else {
            (None, None)
        };
        let out = outside(pb).max(outside(pd));
        raw.push((x, outline, deck, bottom, out));
    }
    let lift = match placement.lift {
        Some(v) => v,
        None if deck_min.is_finite() => (deck_min - bottom_min) / 2.0,
        None => 0.0,
    };
    let samples: Vec<FitSample> = raw
        .iter()
        .map(|&(x, outline, deck, bottom, out)| FitSample {
            x,
            outline,
            deck: deck.map_or(-out, |d| d - lift),
            bottom: bottom.map_or(-out, |b| b + lift),
        })
        .collect();
    let first = board.pts.first().copied().unwrap_or_default();
    let last = board.pts.last().copied().unwrap_or_default();
    let tail_spare = place(first[0], first[2], pivot, shift, cos, sin)
        .x
        .min(place(first[0], first[3], pivot, shift, cos, sin).x);
    let nose_spare = shape.length
        - place(last[0], last[2], pivot, shift, cos, sin)
            .x
            .max(place(last[0], last[3], pivot, shift, cos, sin).x);
    let min_of = |f: fn(&FitSample) -> f64| {
        samples.iter().fold(
            Clearance {
                min: f64::INFINITY,
                at: 0.0,
            },
            |c, s| {
                if f(s) < c.min {
                    Clearance { min: f(s), at: s.x }
                } else {
                    c
                }
            },
        )
    };
    let outline = min_of(|s| s.outline);
    let deck = min_of(|s| s.deck);
    let bottom = min_of(|s| s.bottom);
    let mut problems = Vec::new();
    for (view, f) in [
        (
            FitView::Outline,
            (|s: &FitSample| s.outline) as fn(&FitSample) -> f64,
        ),
        (FitView::Deck, |s| s.deck),
        (FitView::Bottom, |s| s.bottom),
    ] {
        let mut open: Option<Problem> = None;
        for s in &samples {
            let v = f(s);
            if v < -TOLERANCE {
                let p = open.get_or_insert(Problem {
                    view,
                    from: s.x,
                    to: s.x,
                    depth: 0.0,
                    at: s.x,
                });
                p.to = s.x;
                if -v > p.depth {
                    p.depth = -v;
                    p.at = s.x;
                }
            } else if let Some(p) = open.take() {
                problems.push(p);
            }
        }
        problems.extend(open);
    }
    for (spare, at) in [(tail_spare, 0.0), (nose_spare, board.length)] {
        if spare < -TOLERANCE {
            problems.push(Problem {
                view: FitView::Length,
                from: at,
                to: at,
                depth: -spare,
                at,
            });
        }
    }
    let fits = problems.is_empty();
    FitReport {
        board_length: board.length,
        tail_offset: shift,
        tilt: placement.tilt,
        lift,
        fits,
        outline,
        deck,
        bottom,
        tail_spare,
        nose_spare,
        samples,
        problems,
    }
}

/// Largest tilt the automatic fit tries, degrees.
pub const MAX_AUTO_TILT: f64 = 6.0;

/// Placement that maximises the smallest clearance (outline, deck, bottom and the foam left at the ends),
/// searching the lengthwise offset and the tilt; the height is centred.
pub fn auto_fit(board: &Board, blank: &Blank) -> Placement {
    let bs = BoardSamples::new(board);
    let shape = BlankShape::new(blank);
    auto_fit_samples(&bs, &shape)
}

pub fn auto_fit_samples(board: &BoardSamples, shape: &BlankShape) -> Placement {
    let score = |off: f64, tilt: f64| {
        let p = Placement {
            tail_offset: Some(off),
            tilt,
            lift: None,
        };
        fit_samples(board, shape, &p).min_clearance()
    };
    let spare = shape.length - board.length;
    let centre = spare / 2.0;
    let (lo, hi) = if spare > 0.0 {
        (0.0, spare)
    } else {
        (centre - 5.0, centre + 5.0)
    };
    let mut best = (score(centre, 0.0), centre, 0.0);
    // Coarse grid.
    let (nx, nt) = (24, 24);
    for i in 0..=nx {
        let off = lo + (hi - lo) * i as f64 / nx as f64;
        for j in 0..=nt {
            let tilt = -MAX_AUTO_TILT + 2.0 * MAX_AUTO_TILT * j as f64 / nt as f64;
            let s = score(off, tilt);
            if s > best.0 {
                best = (s, off, tilt);
            }
        }
    }
    // Pattern search around the best grid point.
    let (mut so, mut st) = (
        ((hi - lo) / nx as f64).max(0.1),
        2.0 * MAX_AUTO_TILT / nt as f64,
    );
    while so > 0.01 || st > 0.005 {
        let mut improved = false;
        for (d_off, d_tilt) in [(so, 0.0), (-so, 0.0), (0.0, st), (0.0, -st)] {
            let (off, tilt) = (
                best.1 + d_off,
                (best.2 + d_tilt).clamp(-MAX_AUTO_TILT, MAX_AUTO_TILT),
            );
            let s = score(off, tilt);
            if s > best.0 + 1e-9 {
                best = (s, off, tilt);
                improved = true;
            }
        }
        if !improved {
            so /= 2.0;
            st /= 2.0;
        }
    }
    Placement {
        tail_offset: Some(best.1),
        tilt: best.2,
        lift: None,
    }
}

/// A blank that holds `board` with the given margins all round: the board's length, widths, thicknesses and
/// rocker at stations 6", 12", 18", 24", 36" and 48" from each end (where the board is long enough), plus the
/// margins, and the widths and thicknesses at the tips. Useful for tests and as a starting point for a new blank.
pub fn blank_around(
    board: &Board,
    length_margin: f64,
    width_margin: f64,
    thickness_margin: f64,
) -> Blank {
    let l = board.length();
    let lb = l + 2.0 * length_margin;
    let mut b = Blank {
        name: board.info.name.clone(),
        length: lb,
        width: board.max_width() + 2.0 * width_margin,
        wide_point: board.max_width_pos() - l / 2.0,
        thickness: board.max_thickness() + 2.0 * thickness_margin,
        nose_rocker: board.rocker_at(l - 0.005),
        tail_rocker: board.rocker_at(0.001),
        stations: Vec::new(),
    };
    // At the tips, the widest the board gets within 4 cm (rounded tail corners).
    let width = |x: f64, d: f64| {
        if d > 0.0 {
            return board.width_at(x);
        }
        (0..=20)
            .map(|i| {
                let t = 4.0 * i as f64 / 20.0;
                board.width_at(if x < l / 2.0 { x + t } else { x - t })
            })
            .fold(0.0, f64::max)
    };
    for nose in [true, false] {
        for inches in [0.0, 6.0, 12.0, 18.0, 24.0, 36.0, 48.0] {
            let d = inches * INCH;
            if d > l / 2.0 - 10.0 {
                break;
            }
            // Station on the blank, and the matching board station.
            let x = if nose {
                l - d + length_margin
            } else {
                d - length_margin
            };
            // Just inside the board at the tips (a square tail starts with a vertical edge).
            let x = x.clamp(0.5, l - 0.5);
            b.stations.push(BlankStation {
                from_nose: nose,
                dist: d,
                width: Some(width(x, d) + 2.0 * width_margin),
                thickness: Some((board.thickness_at(x) + 2.0 * thickness_margin).min(b.thickness)),
                // The tip rocker is the main field.
                rocker: (d > 0.0).then(|| board.rocker_at(x)),
            });
        }
    }
    b
}

// ------------------------------------------------------------------------------------------- library

/// Built-in blank: published catalogue numbers (sources in `docs/blanks.md`), inches as published.
struct Entry {
    name: &'static str,
    /// Length (tip to tip), widest width, wide point from the middle (+ toward the nose), thickness at the
    /// middle, nose rocker, tail rocker.
    dims: [f64; 6],
    /// Nose stations then tail stations: (distance from the tip, width, thickness, rocker); negative = not
    /// published.
    nose: &'static [[f64; 4]],
    tail: &'static [[f64; 4]],
}

const LIBRARY: &[Entry] = &[
    // US Blanks (California): tip-to-tip deck length, max width, max thickness and natural rocker from the
    // blank's page; stations from its "with Measurements" drawing.
    Entry {
        name: "5'10\" fish (US Blanks 5'10\" RP)",
        dims: [70.75, 24.875, 0.0, 2.9375, 4.625, 1.9375],
        nose: &[
            [0.0, 8.0, 1.25, 4.625],
            [6.0, 14.8125, 1.6875, 2.625],
            [12.0, 19.3125, 2.1875, 1.375],
            [18.0, 22.1875, 2.5625, 0.625],
            [24.0, 23.875, 2.75, 0.25],
        ],
        tail: &[
            [0.0, 14.0, 1.3125, 1.9375],
            [6.0, 17.75, 1.75, 1.3125],
            [12.0, 20.375, 2.1875, 0.875],
            [18.0, 22.5, 2.5, 0.5],
            [24.0, 23.75, 2.75, 0.25],
        ],
    },
    Entry {
        name: "6'0\" shortboard (US Blanks 6'0\" R)",
        dims: [71.75, 21.1875, 0.0, 3.0, 4.4375, 2.125],
        nose: &[
            [0.0, 5.0, 1.25, 4.4375],
            [6.0, 12.375, 1.625, 2.4375],
            [12.0, 16.375, 2.1875, 1.375],
            [18.0, 18.875, 2.5625, 0.75],
            [24.0, 20.4375, 2.8125, 0.3125],
        ],
        tail: &[
            [0.0, 8.5, 1.25, 2.125],
            [6.0, 13.75, 1.75, 1.4375],
            [12.0, 17.125, 2.25, 0.9375],
            [18.0, 19.25, 2.5625, 0.5],
            [24.0, 20.625, 2.8125, 0.25],
        ],
    },
    Entry {
        name: "6'2\" shortboard (US Blanks 6'2\" P)",
        dims: [73.875, 20.5, 0.0, 2.5, 4.4375, 2.25],
        nose: &[
            [0.0, 4.5, 0.75, 4.4375],
            [6.0, 10.0, 1.25, 2.5],
            [12.0, 14.0, 1.625, 1.5],
            [18.0, 16.75, 2.0, 0.8125],
            [24.0, 18.625, 2.25, 0.375],
        ],
        tail: &[
            [0.0, 6.125, 1.0, 2.25],
            [6.0, 11.875, 1.375, 1.625],
            [12.0, 15.4375, 1.875, 1.0625],
            [18.0, 17.875, 2.1875, 0.625],
            [24.0, 19.4375, 2.375, 0.3125],
        ],
    },
    Entry {
        name: "6'4\" shortboard (US Blanks 6'4\" EA)",
        dims: [75.5, 20.625, 0.0, 2.75, 5.5, 2.0625],
        nose: &[
            [0.0, 4.125, 0.9375, 5.5],
            [6.0, 10.125, 1.3125, 3.3125],
            [12.0, 14.1875, 1.75, 1.8125],
            [18.0, 17.125, 2.125, 0.9375],
            [24.0, 19.0, 2.4375, 0.4375],
        ],
        tail: &[
            [0.0, 8.0, 1.3125, 2.0625],
            [6.0, 12.4375, 1.6875, 1.4375],
            [12.0, 15.4375, 2.0, 0.9375],
            [18.0, 17.6875, 2.3125, 0.5625],
            [24.0, 19.25, 2.5, 0.3125],
        ],
    },
    Entry {
        name: "6'5\" shortboard (US Blanks 6'5\" R)",
        dims: [76.75, 21.75, 0.0, 3.25, 4.6875, 2.25],
        nose: &[
            [0.0, 6.25, 1.375, 4.6875],
            [6.0, 13.125, 1.9375, 2.5625],
            [12.0, 16.875, 2.375, 1.5625],
            [18.0, 19.25, 2.75, 0.875],
            [24.0, 20.75, 3.0, 0.375],
        ],
        tail: &[
            [0.0, 9.25, 1.375, 2.25],
            [6.0, 14.0, 2.0, 1.4375],
            [12.0, 17.0, 2.4375, 0.6875],
            [18.0, 19.125, 2.8125, 0.5625],
            [24.0, 20.625, 3.0625, 0.25],
        ],
    },
    Entry {
        name: "6'8\" fish / hybrid (US Blanks 6'8\" RP)",
        dims: [80.75, 25.125, 0.0, 3.0, 5.0, 2.125],
        nose: &[
            [0.0, 7.5, 1.25, 5.0],
            [6.0, 14.3125, 1.6875, 3.0625],
            [12.0, 18.5625, 2.125, 1.75],
            [18.0, 21.5, 2.4375, 0.9375],
            [24.0, 23.375, 2.6875, 0.4375],
        ],
        tail: &[
            [0.0, 13.75, 1.4375, 2.125],
            [6.0, 17.375, 1.8125, 1.5],
            [12.0, 20.0, 2.1875, 1.0625],
            [18.0, 21.9375, 2.5, 0.625],
            [24.0, 23.375, 2.75, 0.375],
        ],
    },
    Entry {
        name: "7'5\" funboard (US Blanks 7'5\" A)",
        dims: [87.4375, 21.875, 0.0, 3.0, 6.0, 2.125],
        nose: &[
            [0.0, 4.75, 1.0, 6.0],
            [6.0, 11.0, 1.5, 3.5625],
            [12.0, 15.1875, 2.0, 2.0625],
            [18.0, 18.0, 2.375, 1.1875],
            [24.0, 19.6875, 2.625, 0.6875],
            [36.0, 21.625, 2.9375, 0.125],
        ],
        tail: &[
            [0.0, 8.5, 1.375, 2.125],
            [6.0, 12.6875, 1.75, 1.625],
            [12.0, 15.375, 2.0625, 1.1875],
            [18.0, 17.5, 2.375, 0.8125],
            [24.0, 19.1875, 2.5625, 0.5],
            [36.0, 21.25, 2.875, 0.09375],
        ],
    },
    Entry {
        name: "7'8\" step-up (US Blanks 7'8\" EA)",
        dims: [92.1875, 21.875, 0.0, 3.0, 6.1875, 2.625],
        nose: &[
            [0.0, 3.1875, 1.0, 6.1875],
            [6.0, 8.875, 1.375, 4.0],
            [12.0, 12.75, 1.8125, 2.5],
            [18.0, 16.0, 2.25, 1.5],
            [24.0, 18.1875, 2.5625, 0.8125],
            [36.0, 21.0, 3.0, 0.125],
        ],
        tail: &[
            [0.0, 6.5625, 1.375, 2.625],
            [6.0, 11.375, 1.625, 1.9375],
            [12.0, 14.5, 2.0, 1.375],
            [18.0, 16.875, 2.3125, 1.0],
            [24.0, 18.8125, 2.5625, 0.625],
            [36.0, 21.125, 2.875, 0.125],
        ],
    },
    Entry {
        name: "8'0\" mid-length (US Blanks 8'0\" H)",
        dims: [95.875, 23.625, 0.0, 3.3125, 3.5625, 3.125],
        nose: &[
            [0.0, -1.0, 0.9375, 3.5625],
            [6.0, 15.375, 1.75, 2.625],
            [12.0, 19.0625, 2.125, 1.875],
            [18.0, 21.0625, 2.5, 1.25],
            [24.0, 22.1875, 2.8125, 0.75],
            [36.0, 23.4375, 3.1875, 0.1875],
        ],
        tail: &[
            [0.0, 6.75, 1.3125, 3.125],
            [6.0, 13.0, 1.75, 2.875],
            [12.0, 16.4375, 2.125, 1.75],
            [18.0, 18.8125, 2.4375, 1.25],
            [24.0, 20.5, 2.75, 0.8125],
            [36.0, 22.625, 3.1875, 0.25],
        ],
    },
    Entry {
        name: "9'2\" gun (US Blanks 9'2\" A)",
        dims: [110.0, 22.25, 0.0, 3.5, 7.625, 2.75],
        nose: &[
            [0.0, 4.0, 1.3125, 7.625],
            [6.0, 8.75, 1.6875, 5.375],
            [12.0, 12.625, 2.1875, 3.625],
            [18.0, 15.75, -1.0, 2.375],
            [24.0, 18.0, 3.0, 1.5625],
            [36.0, 21.0, 3.375, 0.5],
        ],
        tail: &[
            [0.0, 6.0, 1.375, 2.75],
            [6.0, 9.75, 1.6875, 2.125],
            [12.0, 12.75, 2.0625, 1.5625],
            [18.0, 15.125, 2.375, 1.25],
            [24.0, 17.125, 2.625, 0.875],
            [36.0, 20.0, 3.125, 0.375],
        ],
    },
    Entry {
        name: "9'4\" longboard (US Blanks 9'4\" B)",
        dims: [111.0625, 25.0625, 7.53125, 3.3125, 4.75, 3.5],
        nose: &[
            [0.0, -1.0, 1.3125, 4.75],
            [6.0, 17.0, 1.6875, 3.5],
            [12.0, 20.3125, 2.0, 2.5],
            [18.0, 22.1875, 2.375, 1.75],
            [24.0, 23.4375, 2.625, 1.1875],
            [36.0, 24.8125, 3.0, 0.4375],
            [48.0, 25.0625, 3.25, 0.0625],
        ],
        tail: &[
            [0.0, 7.5, 1.5, 3.5],
            [6.0, 13.25, 1.75, 2.75],
            [12.0, 16.3125, 2.125, 2.125],
            [18.0, 18.5625, 2.375, 1.5625],
            [24.0, 20.25, 2.625, 1.09375],
            [36.0, 22.625, 3.0, 0.4375],
            [48.0, 24.125, 3.25, 0.09375],
            [55.53125, 24.875, -1.0, -1.0],
        ],
    },
    Entry {
        name: "10'2\" longboard (US Blanks 10'2\" B)",
        dims: [121.25, 25.1875, 12.625, 3.5, 4.8125, 3.4375],
        nose: &[
            [0.0, -1.0, 1.375, 4.8125],
            [6.0, 17.5, 1.875, 3.625],
            [12.0, 20.75, 2.3125, 2.625],
            [18.0, 22.3125, 2.6875, 1.875],
            [24.0, 23.4375, 2.9375, 1.3125],
            [36.0, 24.625, 3.25, 0.5625],
            [48.0, 25.1875, 3.4375, 0.15625],
        ],
        tail: &[
            [0.0, 9.25, 1.3125, 3.4375],
            [6.0, 14.25, 1.8125, 2.5],
            [12.0, 17.5625, 2.25, 1.8125],
            [18.0, 19.8125, 2.625, 1.3125],
            [24.0, 21.375, 2.875, 0.9375],
            [36.0, 23.4375, 3.25, 0.375],
            [48.0, 24.5, 3.4375, 0.0625],
            [60.625, 25.125, -1.0, -1.0],
        ],
    },
    // Surfblanks Australia (Brookvale, NSW): max length, max width (and its position from the tail), max thickness
    // and natural rocker from the blank's drawing; stations as labelled on it (nose stations 12" / 24" from the
    // nose, tail stations from the tail).
    Entry {
        name: "6'4\" shortboard (Surfblanks Australia 6'4\" SB)",
        dims: [76.0, 22.5, 0.375, 2.75, 4.875, 1.75],
        nose: &[
            [0.0, 4.0, -1.0, -1.0],
            [6.0, -1.0, 1.5625, 2.625],
            [12.0, 16.3125, 2.0, 1.4375],
            [24.0, 20.9375, 2.5, 0.3125],
        ],
        tail: &[
            [0.0, 8.375, 0.5, -1.0],
            [6.0, -1.0, 1.5625, 1.4375],
            [12.0, 17.9375, 2.0, 1.1875],
            [24.0, 21.125, 2.5, 0.25],
        ],
    },
    Entry {
        name: "6'5\" fat fish (Surfblanks Australia 6'5\" FF)",
        dims: [78.0, 25.0, 2.1875, 3.75, 4.5, 2.5],
        nose: &[[0.0, 0.5, -1.0, -1.0], [12.0, 19.75, 2.6875, 1.5]],
        tail: &[[0.0, 10.5, -1.0, -1.0], [12.0, 19.5, 2.5625, 1.1875]],
    },
    Entry {
        name: "6'7\" shortboard (Surfblanks Australia 6'7\" SB)",
        dims: [80.0, 20.75, -1.125, 2.8125, 5.1875, 2.5],
        nose: &[
            [0.0, 4.0, -1.0, -1.0],
            [12.0, 13.5625, 1.875, 1.75],
            [24.0, 18.5, 2.5, 0.5],
        ],
        tail: &[
            [0.0, 8.0, -1.0, -1.0],
            [12.0, 15.9375, 2.0, 1.25],
            [24.0, 19.3125, 2.5625, 0.375],
        ],
    },
    Entry {
        name: "8'3\" mid-length (Surfblanks Australia 8'3\" LB)",
        dims: [99.5, 24.5, 3.8125, 3.5625, 5.375, 3.1875],
        nose: &[
            [0.0, -1.0, 0.625, -1.0],
            [12.0, 19.9375, 2.25, 2.375],
            [24.0, 23.125, 2.9375, 1.0],
        ],
        tail: &[
            [0.0, 6.3125, 0.625, -1.0],
            [12.0, 17.75, 2.25, 1.75],
            [24.0, 21.6875, 2.9375, 0.75],
        ],
    },
    Entry {
        name: "9'2\" longboard (Surfblanks Australia 9'2\" LB)",
        dims: [110.5, 24.3125, 7.25, 3.375, 4.0, 3.5],
        nose: &[
            [12.0, 19.5, 2.0, 1.875],
            [24.0, 22.5, 2.625, 0.8125],
            [48.0, 24.3125, -1.0, -1.0],
        ],
        tail: &[
            [0.0, 7.5, -1.0, -1.0],
            [12.0, 16.0625, 2.0, 1.875],
            [24.0, 19.75, 2.625, 0.9375],
            [48.0, 23.6875, -1.0, -1.0],
        ],
    },
    // Generic sizes between the catalogue blanks above (interpolated, not a manufacturer's numbers); their
    // stations come from the typical proportions.
    Entry {
        name: "Generic 5'8\" shortboard (estimate)",
        dims: [69.0, 21.0, 0.0, 2.75, 4.25, 2.0],
        nose: &[],
        tail: &[],
    },
    Entry {
        name: "Generic 7'0\" step-up (estimate)",
        dims: [84.5, 21.25, 0.0, 3.0, 5.75, 2.375],
        nose: &[],
        tail: &[],
    },
    Entry {
        name: "Generic 8'6\" mid-length (estimate)",
        dims: [102.0, 23.5, 0.0, 3.375, 4.75, 3.25],
        nose: &[],
        tail: &[],
    },
    Entry {
        name: "Generic 9'8\" longboard (estimate)",
        dims: [116.0, 25.0, 0.0, 3.375, 4.75, 3.5],
        nose: &[],
        tail: &[],
    },
];

/// The built-in blank library, shortest first.
pub fn library() -> Vec<Blank> {
    let opt = |v: f64| (v >= 0.0).then_some(v * INCH);
    let mut lib: Vec<Blank> = LIBRARY
        .iter()
        .map(|en| {
            let [l, w, wp, t, nr, tr] = en.dims;
            let stations = en
                .nose
                .iter()
                .map(|s| (true, s))
                .chain(en.tail.iter().map(|s| (false, s)))
                .map(|(from_nose, &[d, w, t, r])| BlankStation {
                    from_nose,
                    dist: d * INCH,
                    width: opt(w),
                    thickness: opt(t),
                    rocker: opt(r),
                })
                .collect();
            Blank {
                name: en.name.to_string(),
                length: l * INCH,
                width: w * INCH,
                wide_point: wp * INCH,
                thickness: t * INCH,
                nose_rocker: nr * INCH,
                tail_rocker: tr * INCH,
                stations,
            }
        })
        .collect();
    lib.sort_by(|a, b| a.length.total_cmp(&b.length));
    lib
}

/// A library blank by name (exact, else case-insensitive, else the unique name containing `name`).
pub fn find(name: &str) -> Option<Blank> {
    let lib = library();
    if let Some(b) = lib.iter().find(|b| b.name == name) {
        return Some(b.clone());
    }
    let low = name.to_lowercase();
    if let Some(b) = lib.iter().find(|b| b.name.to_lowercase() == low) {
        return Some(b.clone());
    }
    let hits: Vec<&Blank> = lib
        .iter()
        .filter(|b| b.name.to_lowercase().contains(&low))
        .collect();
    match hits[..] {
        [b] => Some(b.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spline::polyline;

    fn full() -> Blank {
        let mut b = Blank {
            name: "Test = blank; 6'4\" R".into(),
            wide_point: -2.5,
            ..Blank::default()
        };
        for (nose, m, d, v) in [
            (true, Measure::Width, FOOT, 30.0),
            (false, Measure::Width, FOOT, 38.0),
            (true, Measure::Thickness, FOOT, 6.0),
            (false, Measure::Thickness, FOOT, 6.5),
            (true, Measure::Rocker, FOOT, 5.0),
            (true, Measure::Rocker, 2.0 * FOOT, 2.0),
            (false, Measure::Rocker, FOOT, 2.5),
            (false, Measure::Rocker, 2.0 * FOOT, 0.8),
        ] {
            b.set_station(nose, d, m, Some(v));
        }
        b
    }

    #[test]
    fn stations_set_and_clear() {
        let mut b = full();
        assert_eq!(b.stations.len(), 4);
        assert_eq!(b.station(true, FOOT, Measure::Width), Some(30.0));
        b.set_station(true, FOOT, Measure::Width, None);
        b.set_station(true, FOOT, Measure::Thickness, None);
        assert_eq!(b.station(true, FOOT, Measure::Rocker), Some(5.0));
        b.set_station(true, FOOT, Measure::Rocker, None);
        assert_eq!(b.stations.len(), 3);
        b.set_station(true, 3.0, Measure::Width, None);
        assert_eq!(b.stations.len(), 3);
    }

    #[test]
    fn compact_round_trip() {
        let mut lib = library();
        lib.push(Blank::default());
        lib.push(full());
        for b in lib {
            let text = b.to_compact();
            assert!(!text.contains('\n'));
            assert_eq!(Blank::from_compact(&text), Some(b.clone()), "{text}");
            for placement in [
                Placement::default(),
                Placement {
                    tail_offset: Some(3.25),
                    tilt: -1.5,
                    lift: Some(0.4),
                },
            ] {
                let s = BlankSetup {
                    blank: b.clone(),
                    placement,
                };
                assert_eq!(BlankSetup::from_compact(&s.to_compact()), Some(s));
            }
        }
        // Unknown keys are skipped, missing required ones or broken stations refuse the line.
        let b = Blank::from_compact("L=200 W=55 T=8 XYZ=4 name=x").unwrap();
        assert_eq!((b.length, b.name.as_str()), (200.0, "x"));
        assert!(Blank::from_compact("L=200 W=55 name=x").is_none());
        assert!(Blank::from_compact("L=abc W=55 T=8").is_none());
        assert!(Blank::from_compact("L=200 W=55 T=8 S=X,1,2,3,4").is_none());
        assert!(Blank::from_compact("L=200 W=55 T=8 S=N,1,2").is_none());
        assert!(Blank::from_compact("").is_none());
    }

    #[test]
    fn validation() {
        assert!(Blank::default().validate().is_ok());
        assert!(full().validate().is_ok());
        let mut bad = vec![
            Blank {
                length: -1.0,
                ..Blank::default()
            },
            Blank {
                width: 0.0,
                ..Blank::default()
            },
            Blank {
                thickness: f64::NAN,
                ..Blank::default()
            },
        ];
        let mut b = Blank::default();
        b.set_station(true, FOOT, Measure::Rocker, Some(20.0));
        bad.push(b);
        let mut b = Blank::default();
        b.set_station(false, FOOT, Measure::Width, Some(80.0));
        bad.push(b);
        for b in bad {
            assert!(b.validate().is_err(), "{b:?}");
        }
    }

    fn check_shape(b: &Blank) {
        let s = BlankShape::new(b);
        let l = b.length;
        // Widest at the wide point, closed at both tips, never wider than published.
        let xw = l / 2.0 + b.wide_point;
        assert!(
            (s.half_width_at(xw) * 2.0 - b.width).abs() < 0.05,
            "{}",
            b.name
        );
        assert!(
            s.outline_points()
                .iter()
                .all(|p| p.y <= b.width / 2.0 + 1e-6)
        );
        assert!(s.half_width_at(l) < 1e-6 && s.half_width_at(0.0) < 1e-6);
        // Every published station is matched (widths away from the rounded tips).
        for st in &b.stations {
            let x = b.station_x(st);
            if let Some(w) = st.width.filter(|_| st.dist > 1.0) {
                let got = s.half_width_at(x) * 2.0;
                assert!(
                    (got - w).abs() < 0.05,
                    "{}: width {got} vs {w} at {x}",
                    b.name
                );
            }
            if let Some(t) = st.thickness {
                let got = s.thickness_at(x);
                assert!((got - t).abs() < 0.05, "{}: thickness {got} vs {t}", b.name);
            }
            if let Some(r) = st.rocker {
                let got = s.bottom_at(x);
                assert!(
                    (got - r).abs() < 0.05,
                    "{}: rocker {got} vs {r} at {x}",
                    b.name
                );
            }
        }
        // Rocker: published tips, zero at the flat point, monotone either side of it.
        assert!((s.bottom_at(l) - b.nose_rocker).abs() < 1e-6, "{}", b.name);
        assert!((s.bottom_at(0.0) - b.tail_rocker).abs() < 1e-6);
        assert!(s.bottom_at(s.flat_point).abs() < 0.01);
        let pts = s.bottom_points();
        for w in pts.windows(2) {
            if w[0].x >= s.flat_point {
                assert!(
                    w[1].y >= w[0].y - 1e-9,
                    "{}: rocker dips at {}",
                    b.name,
                    w[0].x
                );
            } else if w[1].x <= s.flat_point {
                assert!(
                    w[1].y <= w[0].y + 1e-9,
                    "{}: rocker rises at {}",
                    b.name,
                    w[0].x
                );
            }
        }
        // Thickness: the published middle value, thinner toward the tips, never negative.
        assert!((s.thickness_at(l / 2.0) - b.thickness).abs() < 1e-6);
        for x in [0.0, l * 0.1, l * 0.9, l] {
            let t = s.thickness_at(x);
            assert!(
                t > 0.0 && t < b.thickness,
                "{}: thickness {t} at {x}",
                b.name
            );
        }
    }

    #[test]
    fn shapes_pass_through_the_published_numbers() {
        check_shape(&Blank::default());
        let b = full();
        check_shape(&b);
        // Typical proportions when nothing is published.
        let d = Blank::default();
        let s = BlankShape::new(&d);
        let w12 = s.half_width_at(d.length - FOOT) * 2.0 / d.width;
        assert!((w12 - NOSE_12_RATIO).abs() < 1e-3, "{w12}");
        let t12 = s.thickness_at(FOOT) / d.thickness;
        assert!((t12 - 0.69).abs() < 1e-3, "{t12}");
    }

    #[test]
    fn library_entries_are_valid() {
        let lib = library();
        assert!(lib.len() >= 15, "{} blanks", lib.len());
        let mut names: Vec<&str> = lib.iter().map(|b| b.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lib.len(), "duplicate names");
        for b in &lib {
            b.validate().unwrap_or_else(|e| panic!("{}: {e}", b.name));
            assert!(b.length > 150.0 && b.length < 400.0, "{}", b.name);
            assert!(b.width > 45.0 && b.width < 70.0, "{}", b.name);
            assert!(b.thickness > 5.0 && b.thickness < 13.0, "{}", b.name);
            // Station widths and thicknesses grow from each tip toward the middle.
            for nose in [true, false] {
                for m in [Measure::Width, Measure::Thickness] {
                    let v = side_points(b, nose, m, b.length);
                    assert!(
                        v.windows(2).all(|w| w[1].1 >= w[0].1),
                        "{} {m:?} nose={nose}: {v:?}",
                        b.name
                    );
                }
            }
            check_shape(b);
            assert_eq!(find(&b.name).as_ref(), Some(b));
        }
        assert!(find("no such blank").is_none());
    }

    /// A simple board: polyline outline, rocker and deck through a few points.
    fn board(l: f64, w: f64, t: f64, nr: f64, tr: f64) -> Board {
        let mut b = Board {
            outline: polyline(&[
                (0.0, 0.0),
                (0.0, w * 0.3),
                (FOOT, w * 0.38),
                (l * 0.5, w / 2.0),
                (l - FOOT, w * 0.32),
                (l, 0.0),
            ]),
            bottom: polyline(&[(0.0, tr), (l * 0.4, 0.0), (l * 0.6, 0.0), (l, nr)]),
            deck: polyline(&[
                (0.0, tr),
                (0.0, tr + t * 0.3),
                (l * 0.5, t),
                (l, nr + t * 0.25),
                (l, nr),
            ]),
            ..Board::default()
        };
        b.info.name = "test".into();
        b
    }

    #[test]
    fn board_fits_a_blank_made_around_it() {
        let b = board(190.0, 50.0, 6.5, 11.0, 5.0);
        let blank = blank_around(&b, 3.0, 2.0, 1.0);
        blank.validate().unwrap();
        let r = fit(&b, &blank, &Placement::default());
        assert!(r.fits, "{:?}", r.problems);
        assert!((r.tail_spare - 3.0).abs() < 0.01 && (r.nose_spare - 3.0).abs() < 0.01);
        assert!(r.outline.min > 0.0 && r.deck.min > 0.0 && r.bottom.min > 0.0);
        // Centred vertically: equal smallest clearance above and below.
        assert!((r.deck.min - r.bottom.min).abs() < 1e-6);
    }

    #[test]
    fn too_big_boards_are_reported_in_the_right_view() {
        let b = board(190.0, 50.0, 6.5, 11.0, 5.0);
        let blank = blank_around(&b, 3.0, 2.0, 1.0);
        // Longer than the blank.
        let long = board(200.0, 50.0, 6.5, 11.0, 5.0);
        let r = fit(&long, &blank, &Placement::default());
        assert!(!r.fits);
        assert!(r.tail_spare < 0.0 && r.nose_spare < 0.0);
        assert!(r.problems.iter().any(|p| p.view == FitView::Length));
        // Wider: the outline fails near the wide point.
        let wide = board(190.0, 58.0, 6.5, 11.0, 5.0);
        let r = fit(&wide, &blank, &Placement::default());
        assert!(!r.fits);
        assert!(r.outline.min < 0.0);
        assert!((r.outline.at - 95.0).abs() < 25.0, "{}", r.outline.at);
        assert!(r.deck.min > 0.0);
        let p = r
            .problems
            .iter()
            .find(|p| p.view == FitView::Outline)
            .unwrap();
        assert!(p.from < 95.0 && p.to > 95.0 && (p.depth + r.outline.min).abs() < 1e-9);
        // Thicker: deck and bottom fail along the middle by the same amount (centred height).
        let thick = board(190.0, 50.0, 10.0, 11.0, 5.0);
        let r = fit(&thick, &blank, &Placement::default());
        assert!(!r.fits && r.deck.min < 0.0 && r.bottom.min < 0.0);
        assert!((r.deck.at - 95.0).abs() < 50.0, "{}", r.deck.at);
        assert!(r.outline.min > 0.0);
        // Much more nose rocker than the blank: it pokes out near the nose.
        let rockered = board(190.0, 50.0, 6.5, 22.0, 5.0);
        let r = fit(&rockered, &blank, &Placement::default());
        assert!(!r.fits);
        let (view, _, at) = r.tightest();
        assert!(matches!(view, FitView::Deck | FitView::Bottom));
        assert!(at > 120.0, "{view:?} at {at}");
    }

    #[test]
    fn auto_fit_improves_the_clearance() {
        // A board with more nose rocker than the blank, placed off-centre: tilting and shifting helps.
        let b = board(190.0, 50.0, 6.5, 14.0, 4.0);
        let mut blank = blank_around(&b, 4.0, 2.0, 1.5);
        blank.stations.retain(|s| s.dist > 40.0);
        blank.nose_rocker = 9.0;
        blank.tail_rocker = 7.0;
        let start = Placement {
            tail_offset: Some(1.0),
            tilt: 0.0,
            lift: None,
        };
        let before = fit(&b, &blank, &start).min_clearance();
        let p = auto_fit(&b, &blank);
        let after = fit(&b, &blank, &p).min_clearance();
        assert!(after > before + 0.1, "{before} -> {after} ({p:?})");
        let centred = fit(&b, &blank, &Placement::default()).min_clearance();
        assert!(after >= centred - 1e-9);
        assert!(p.tilt.abs() <= MAX_AUTO_TILT);
    }

    #[test]
    fn tilt_rotates_about_the_middle() {
        let b = board(190.0, 50.0, 6.5, 11.0, 5.0);
        let blank = blank_around(&b, 3.0, 2.0, 1.0);
        let tilted = Placement {
            tilt: 2.0,
            ..Placement::default()
        };
        let r = fit(&b, &blank, &tilted);
        // The blank's frame maps back onto the board's: the board's tail bottom lands on itself.
        let tail = DVec2::new(0.0, b.rocker_at(0.0));
        let pivot = r.board_length / 2.0;
        let (sin, cos) = 2.0f64.to_radians().sin_cos();
        let d = tail - DVec2::new(pivot, 0.0);
        let in_blank = DVec2::new(
            pivot + r.tail_offset + d.x * cos - d.y * sin,
            d.x * sin + d.y * cos + r.lift,
        );
        assert!((r.to_board(in_blank) - tail).length() < 1e-9);
        // Ends stay about where they were lengthwise; the fit gets worse than level.
        assert!((r.tail_spare - 3.0).abs() < 0.5 && (r.nose_spare - 3.0).abs() < 0.5);
        let level = fit(&b, &blank, &Placement::default());
        assert!(r.min_clearance() < level.min_clearance());
    }
}
