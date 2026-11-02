//! Foam blanks: the dimensions a blank maker publishes, an approximate shape built from them, and the check
//! that a board fits inside one.
//!
//! A catalogue gives a handful of numbers (length, widest width and where it is, the widths 12" from each end,
//! the thickness at the centre, the nose and tail rocker and sometimes the rocker 12" / 24" from each end). The
//! blank's outline, bottom and deck are rebuilt from them as smooth, shape-preserving curves:
//!
//! * **Outline**: a monotone cubic through the tail tip, the tail width 12" up, the wide point, the nose width
//!   12" down and the nose tip, with rounded corners at both ends. Missing 12" widths are estimated from the
//!   width with typical blank proportions; the tip widths are extrapolated from the 12" widths.
//! * **Bottom (rocker)**: zero at a flat point and rising to the published nose and tail rocker. Between the
//!   flat point and the tips the rocker grows as a power of the distance (exponent 2.5, or the exponent that
//!   matches the innermost published station); published 12"/24" stations are passed through exactly. The flat
//!   point is placed where both sides have the same curvature scale, which puts it a little behind the middle
//!   on a typical blank.
//! * **Deck**: the bottom plus a thickness that is the published centre thickness at the middle and thins
//!   toward the ends (to [`TIP_THICKNESS`] of it at the tips, following a power of 2.5 of the distance from the
//!   middle), unless the thickness 12" from an end is published. Blanks are moulded this way, but the real taper
//!   varies between moulds: treat thickness clearances within a few millimetres of the tips with care.
//!
//! The fit check places the board in the blank (lengthwise offset, tilt in the profile plane, height) and
//! compares, at stations along the board, the board's outline with the blank's outline and the board's deck
//! and bottom on the stringer with the blank's. Lengths are centimetres, x runs from the tail.

use std::fmt::Write as _;

use glam::DVec2;

use crate::board::Board;
use crate::units::FOOT;

/// Thickness at the tips relative to the centre thickness, when the catalogue gives no end thickness.
pub const TIP_THICKNESS: f64 = 0.5;
/// Exponent of the default rocker curve (rocker grows as distance^ROCKER_POWER from the flat point).
const ROCKER_POWER: f64 = 2.5;
/// Exponent of the thickness taper.
const TAPER_POWER: f64 = 2.5;
/// Typical 12" widths relative to the widest width (used when the catalogue gives none).
pub const NOSE_12_RATIO: f64 = 0.62;
pub const TAIL_12_RATIO: f64 = 0.74;
/// Clearances above this (cm) count as touching, not as poking out: rounding of the published numbers.
const TOLERANCE: f64 = 1e-6;

/// A blank as a catalogue describes it. Lengths in cm.
#[derive(Clone, Debug, PartialEq)]
pub struct Blank {
    pub name: String,
    pub length: f64,
    /// Widest width.
    pub width: f64,
    /// Position of the widest width from the middle of the blank, positive toward the nose.
    pub wide_point: f64,
    /// Width 12" from the nose tip.
    pub nose_width: Option<f64>,
    /// Width 12" from the tail tip.
    pub tail_width: Option<f64>,
    /// Thickness at the middle.
    pub thickness: f64,
    /// Thickness on the stringer 12" from the nose / tail tip.
    pub nose_thickness: Option<f64>,
    pub tail_thickness: Option<f64>,
    /// Height of the bottom at the nose / tail tip above its lowest point.
    pub nose_rocker: f64,
    pub tail_rocker: f64,
    /// Rocker 12" and 24" from the nose tip.
    pub nose_rocker_12: Option<f64>,
    pub nose_rocker_24: Option<f64>,
    /// Rocker 12" and 24" from the tail tip.
    pub tail_rocker_12: Option<f64>,
    pub tail_rocker_24: Option<f64>,
}

impl Default for Blank {
    fn default() -> Self {
        Self {
            name: String::new(),
            length: 193.0,
            width: 56.0,
            wide_point: 0.0,
            nose_width: None,
            tail_width: None,
            thickness: 8.0,
            nose_thickness: None,
            tail_thickness: None,
            nose_rocker: 12.0,
            tail_rocker: 6.0,
            nose_rocker_12: None,
            nose_rocker_24: None,
            tail_rocker_12: None,
            tail_rocker_24: None,
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
        let all = [
            Some(self.length),
            Some(self.width),
            Some(self.wide_point),
            self.nose_width,
            self.tail_width,
            Some(self.thickness),
            self.nose_thickness,
            self.tail_thickness,
            Some(self.nose_rocker),
            Some(self.tail_rocker),
            self.nose_rocker_12,
            self.nose_rocker_24,
            self.tail_rocker_12,
            self.tail_rocker_24,
        ];
        if all.iter().flatten().any(|v| !v.is_finite()) {
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
        for w in [self.nose_width, self.tail_width].into_iter().flatten() {
            if w <= 0.0 || w > self.width {
                return Err("the 12\" widths must be positive and not wider than the blank".into());
            }
        }
        if !(1.0..=60.0).contains(&self.thickness) {
            return Err("thickness must be between 1 cm and 60 cm".into());
        }
        for t in [self.nose_thickness, self.tail_thickness]
            .into_iter()
            .flatten()
        {
            if t <= 0.0 || t > self.thickness {
                return Err(
                    "the 12\" thicknesses must be positive and not thicker than the centre".into(),
                );
            }
        }
        if self.nose_rocker < 0.0 || self.tail_rocker < 0.0 {
            return Err("rocker cannot be negative".into());
        }
        if self.nose_rocker.max(self.tail_rocker) > self.length / 3.0 {
            return Err("rocker is larger than a third of the length".into());
        }
        let check = |tip: f64, r12: Option<f64>, r24: Option<f64>| -> Result<(), String> {
            let mut last = tip;
            for r in [r12, r24].into_iter().flatten() {
                if r < 0.0 || r > last {
                    return Err(
                        "rocker must decrease from the tip to the 12\" and 24\" stations".into(),
                    );
                }
                last = r;
            }
            Ok(())
        };
        check(self.nose_rocker, self.nose_rocker_12, self.nose_rocker_24)?;
        check(self.tail_rocker, self.tail_rocker_12, self.tail_rocker_24)?;
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------- file format

/// Keys of the compact one-line form, in writing order.
const KEYS: [&str; 14] = [
    "L", "W", "WP", "NW", "TW", "T", "NT", "TT", "NR", "TR", "NR12", "NR24", "TR12", "TR24",
];

impl Blank {
    fn fields(&self) -> [Option<f64>; 14] {
        [
            Some(self.length),
            Some(self.width),
            Some(self.wide_point),
            self.nose_width,
            self.tail_width,
            Some(self.thickness),
            self.nose_thickness,
            self.tail_thickness,
            Some(self.nose_rocker),
            Some(self.tail_rocker),
            self.nose_rocker_12,
            self.nose_rocker_24,
            self.tail_rocker_12,
            self.tail_rocker_24,
        ]
    }

    fn fields_mut(&mut self) -> [FieldMut<'_>; 14] {
        [
            FieldMut::Req(&mut self.length),
            FieldMut::Req(&mut self.width),
            FieldMut::Req(&mut self.wide_point),
            FieldMut::Opt(&mut self.nose_width),
            FieldMut::Opt(&mut self.tail_width),
            FieldMut::Req(&mut self.thickness),
            FieldMut::Opt(&mut self.nose_thickness),
            FieldMut::Opt(&mut self.tail_thickness),
            FieldMut::Req(&mut self.nose_rocker),
            FieldMut::Req(&mut self.tail_rocker),
            FieldMut::Opt(&mut self.nose_rocker_12),
            FieldMut::Opt(&mut self.nose_rocker_24),
            FieldMut::Opt(&mut self.tail_rocker_12),
            FieldMut::Opt(&mut self.tail_rocker_24),
        ]
    }

    /// One-line form `L=… W=… … name=…` (cm, shortest round-trip digits; the name is last and runs to the end
    /// of the line). Used for the `.brd` field and the saved user blanks.
    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        for (k, v) in KEYS.iter().zip(self.fields()) {
            if let Some(v) = v {
                let _ = write!(s, "{k}={v} ");
            }
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
            nose_rocker: 0.0,
            tail_rocker: 0.0,
            ..Blank::default()
        };
        let mut seen = [false; 14];
        for tok in head.split_whitespace() {
            let Some((k, v)) = tok.split_once('=') else {
                continue;
            };
            let Some(i) = KEYS.iter().position(|key| *key == k) else {
                continue;
            };
            let v: f64 = v.parse().ok().filter(|v: &f64| v.is_finite())?;
            seen[i] = true;
            match &mut b.fields_mut()[i] {
                FieldMut::Req(f) => **f = v,
                FieldMut::Opt(f) => **f = Some(v),
            }
        }
        // Length, width and thickness.
        if !(seen[0] && seen[1] && seen[5]) {
            return None;
        }
        if !seen[2] {
            b.wide_point = 0.0;
        }
        Some(b)
    }
}

enum FieldMut<'a> {
    Req(&'a mut f64),
    Opt(&'a mut Option<f64>),
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

/// Monotone piecewise cubic (Fritsch-Carlson) through points with increasing x: never overshoots the data, so
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
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
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
                for k in 1..n - 1 {
                    if d[k - 1] * d[k] > 0.0 {
                        let w1 = 2.0 * h[k] + h[k - 1];
                        let w2 = h[k] + 2.0 * h[k - 1];
                        m[k] = (w1 + w2) / (w1 / d[k - 1] + w2 / d[k]);
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
            let h = half_fn(x).max(0.0);
            let z = rocker.at(x).max(0.0);
            half.push(h);
            bottom.push(z);
            deck.push(z + thick(x).max(0.0));
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

fn outline_curve(b: &Blank, l: f64) -> impl Fn(f64) -> f64 {
    let w2 = b.width / 2.0;
    let xw = (l / 2.0 + b.wide_point).clamp(0.25 * l, 0.75 * l);
    let ht = b.tail_width.unwrap_or(b.width * TAIL_12_RATIO) / 2.0;
    let hn = b.nose_width.unwrap_or(b.width * NOSE_12_RATIO) / 2.0;
    // Each side narrows from the wide point as a power of the distance, h = w2 - (w2 - tip) * u^p (u = 0 at the
    // wide point, 1 at the tip), with the exponent that passes through the 12" width. The tail tip width is
    // extrapolated from the slope between the wide point and the 12" station; the nose closes to a point.
    let (span_t, span_n) = (xw, l - xw);
    let tail_tip = if FOOT < xw - 2.0 {
        let s = (w2 - ht) / (xw - FOOT);
        (ht - 1.6 * s * FOOT).clamp(0.2 * w2, ht * 0.98)
    } else {
        0.5 * w2
    };
    let power = |h12: f64, tip: f64, span: f64, default: f64| {
        let u = (span - FOOT) / span;
        let r = (w2 - h12) / (w2 - tip);
        if u > 0.05 && r > 0.0 && r < 1.0 {
            (r.ln() / u.ln()).clamp(1.2, 8.0)
        } else {
            default
        }
    };
    let pt = power(ht, tail_tip, span_t, 3.0);
    let pn = power(hn, 0.0, span_n, 2.5);
    // Rounded tail corners: a quarter ellipse closes the outline over `rt`.
    let rt = (0.4 * tail_tip).clamp(1.0, 8.0).min(l / 4.0);
    move |x: f64| {
        let mut h = if x >= xw {
            w2 * (1.0 - ((x - xw) / span_n).clamp(0.0, 1.0).powf(pn))
        } else {
            w2 - (w2 - tail_tip) * ((xw - x) / span_t).clamp(0.0, 1.0).powf(pt)
        };
        if x < rt {
            let u = ((rt - x) / rt).min(1.0);
            h *= (1.0 - u * u).max(0.0).sqrt();
        }
        h
    }
}

/// Rocker curve and the position of its lowest point.
fn rocker_curve(b: &Blank, l: f64) -> (Pchip, f64) {
    let (nr, tr) = (b.nose_rocker.max(0.0), b.tail_rocker.max(0.0));
    let x0 = if nr + tr <= 0.0 {
        l / 2.0
    } else {
        let (a, c) = (tr.powf(1.0 / ROCKER_POWER), nr.powf(1.0 / ROCKER_POWER));
        l * a / (a + c)
    }
    .clamp(0.3 * l, 0.7 * l);
    let mut pts = vec![(x0, 0.0)];
    // One side: tip rocker, stations (distance from the tip, value), the span from the flat point to the tip,
    // and a mapping from "distance from the flat point" to x.
    let mut side =
        |tip: f64, stations: [(f64, Option<f64>); 2], span: f64, at: &dyn Fn(f64) -> f64| {
            pts.push((at(span), tip));
            // Stations that lie clearly between the flat point and the tip, innermost last.
            let given: Vec<(f64, f64)> = stations
                .iter()
                .filter_map(|&(d, v)| v.map(|v| (span - d, v)))
                .filter(|&(u, _)| u > 2.0 && u < span - 1.0)
                .collect();
            for &(u, v) in &given {
                pts.push((at(u), v));
            }
            let inner = given.iter().copied().min_by(|a, c| a.0.total_cmp(&c.0));
            // Exponent through the innermost station, else the default.
            let p = match inner {
                Some((u, v)) if tip > 0.0 && v > 0.0 && v < tip => {
                    ((v / tip).ln() / (u / span).ln()).clamp(1.2, 6.0)
                }
                _ => ROCKER_POWER,
            };
            let limit = inner.map_or(span, |(u, _)| u);
            for f in [0.2, 0.4, 0.6, 0.8] {
                let u = f * span;
                if u < limit - 2.0 {
                    let v = match inner {
                        // Scale so the power curve passes exactly through the station.
                        Some((ui, vi)) => vi * (u / ui).powf(p),
                        None => tip * (u / span).powf(p),
                    };
                    pts.push((at(u), v));
                }
            }
        };
    side(
        nr,
        [(FOOT, b.nose_rocker_12), (2.0 * FOOT, b.nose_rocker_24)],
        l - x0,
        &|u| x0 + u,
    );
    side(
        tr,
        [(FOOT, b.tail_rocker_12), (2.0 * FOOT, b.tail_rocker_24)],
        x0,
        &|u| x0 - u,
    );
    (Pchip::new(&pts), x0)
}

fn thickness_curve(b: &Blank, l: f64) -> impl Fn(f64) -> f64 {
    let t = b.thickness;
    let xm = l / 2.0;
    // Thickness fraction lost at the tip, per side: from the 12" thickness when given.
    let loss = |t12: Option<f64>, half: f64| -> f64 {
        match t12 {
            Some(t12) if half > FOOT + 1.0 => {
                let s = (half - FOOT) / half;
                ((1.0 - t12 / t) / s.powf(TAPER_POWER)).clamp(0.0, 0.9)
            }
            _ => 1.0 - TIP_THICKNESS,
        }
    };
    let (kn, kt) = (loss(b.nose_thickness, l - xm), loss(b.tail_thickness, xm));
    move |x: f64| {
        let (s, k) = if x >= xm {
            ((x - xm) / (l - xm), kn)
        } else {
            ((xm - x) / xm, kt)
        };
        t * (1.0 - k * s.clamp(0.0, 1.0).powf(TAPER_POWER))
    }
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
                [x, (board.width_at(x) / 2.0).max(0.0), r, d]
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

/// A blank that holds `board` with the given margins all round: length, widths at 12" and the widest, the
/// centre thickness and rockers taken from the board. Useful for tests and as a starting point for a new
/// blank.
pub fn blank_around(
    board: &Board,
    length_margin: f64,
    width_margin: f64,
    thickness_margin: f64,
) -> Blank {
    let l = board.length();
    Blank {
        name: board.info.name.clone(),
        length: l + 2.0 * length_margin,
        width: board.max_width() + 2.0 * width_margin,
        wide_point: board.max_width_pos() - l / 2.0,
        nose_width: Some(board.width_at(l - FOOT) + 2.0 * width_margin),
        tail_width: Some(board.width_at(FOOT) + 2.0 * width_margin),
        thickness: board.max_thickness() + 2.0 * thickness_margin,
        nose_thickness: None,
        tail_thickness: None,
        nose_rocker: board.rocker_at(l - 0.005),
        tail_rocker: board.rocker_at(0.001),
        nose_rocker_12: None,
        nose_rocker_24: None,
        tail_rocker_12: None,
        tail_rocker_24: None,
    }
}

// ------------------------------------------------------------------------------------------- library

/// Built-in blanks: published catalogue numbers (sources in `docs/blanks.md`). Lengths in inches as
/// published, converted to cm on use.
struct Entry {
    name: &'static str,
    /// Length (tip to tip), widest width, centre thickness, nose and tail rocker, inches.
    dims: [f64; 5],
}

const fn e(name: &'static str, dims: [f64; 5]) -> Entry {
    Entry { name, dims }
}

const LIBRARY: &[Entry] = &[
    // US Blanks (California): "tip-to-tip deck length", max width, max thickness, natural rocker N / T.
    e(
        "5'10\" fish (US Blanks 5'10\" RP)",
        [70.75, 24.875, 2.9375, 4.625, 1.9375],
    ),
    e(
        "6'0\" shortboard (US Blanks 6'0\" R)",
        [71.75, 21.1875, 3.0625, 4.4375, 2.125],
    ),
    e(
        "6'2\" shortboard (US Blanks 6'2\" P)",
        [73.875, 20.5, 2.5, 4.4375, 2.25],
    ),
    e(
        "6'4\" shortboard (US Blanks 6'4\" EA)",
        [75.5, 20.625, 2.75, 5.5, 2.0625],
    ),
    e(
        "6'5\" shortboard (US Blanks 6'5\" R)",
        [76.75, 21.75, 3.25, 4.6875, 2.25],
    ),
    e(
        "6'8\" fish / hybrid (US Blanks 6'8\" RP)",
        [80.75, 25.125, 3.0, 5.0, 2.125],
    ),
    e(
        "7'5\" funboard (US Blanks 7'5\" A)",
        [87.4375, 21.875, 3.0, 6.0, 2.125],
    ),
    e(
        "7'8\" step-up (US Blanks 7'8\" EA)",
        [92.1875, 21.875, 3.0, 6.1875, 2.625],
    ),
    e(
        "8'0\" mid-length (US Blanks 8'0\" H)",
        [95.875, 23.625, 3.3125, 4.625, 1.9375],
    ),
    e(
        "9'2\" gun (US Blanks 9'2\" A)",
        [110.0, 22.25, 3.5, 7.625, 2.75],
    ),
    e(
        "9'4\" longboard (US Blanks 9'4\" B)",
        [111.0625, 25.0625, 3.3125, 4.75, 3.5],
    ),
    e(
        "10'2\" longboard (US Blanks 10'2\" B)",
        [121.25, 25.1875, 3.5, 4.8125, 3.4375],
    ),
    // Generic sizes between the catalogue blanks above (interpolated, not a manufacturer's numbers).
    e(
        "Generic 5'8\" shortboard (estimate)",
        [69.0, 21.0, 2.75, 4.25, 2.0],
    ),
    e(
        "Generic 7'0\" step-up (estimate)",
        [84.5, 21.25, 3.0, 5.75, 2.375],
    ),
    e(
        "Generic 8'6\" mid-length (estimate)",
        [102.0, 23.5, 3.375, 5.0, 2.5],
    ),
    e(
        "Generic 9'8\" longboard (estimate)",
        [116.0, 25.0, 3.375, 4.75, 3.5],
    ),
];

/// The built-in blank library, shortest first.
pub fn library() -> Vec<Blank> {
    let inch = crate::units::INCH;
    let mut lib = LIBRARY
        .iter()
        .map(|en| {
            let [l, w, t, nr, tr] = en.dims;
            Blank {
                name: en.name.to_string(),
                length: l * inch,
                width: w * inch,
                wide_point: 0.0,
                nose_width: None,
                tail_width: None,
                thickness: t * inch,
                nose_thickness: None,
                tail_thickness: None,
                nose_rocker: nr * inch,
                tail_rocker: tr * inch,
                nose_rocker_12: None,
                nose_rocker_24: None,
                tail_rocker_12: None,
                tail_rocker_24: None,
            }
        })
        .collect::<Vec<_>>();
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
        Blank {
            name: "Test = blank; 6'4\" R".into(),
            nose_width: Some(30.0),
            tail_width: Some(38.0),
            nose_thickness: Some(6.0),
            tail_thickness: Some(6.5),
            nose_rocker_12: Some(5.0),
            nose_rocker_24: Some(2.0),
            tail_rocker_12: Some(2.5),
            tail_rocker_24: Some(0.8),
            wide_point: -2.5,
            ..Blank::default()
        }
    }

    #[test]
    fn compact_round_trip() {
        for b in [Blank::default(), full()] {
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
        // Unknown keys are skipped, missing required ones refuse the line.
        let b = Blank::from_compact("L=200 W=55 T=8 XYZ=4 name=x").unwrap();
        assert_eq!((b.length, b.name.as_str()), (200.0, "x"));
        assert!(Blank::from_compact("L=200 W=55 name=x").is_none());
        assert!(Blank::from_compact("L=abc W=55 T=8").is_none());
        assert!(Blank::from_compact("").is_none());
    }

    #[test]
    fn validation() {
        assert!(Blank::default().validate().is_ok());
        assert!(full().validate().is_ok());
        let bad = [
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
            Blank {
                nose_rocker_12: Some(20.0),
                ..Blank::default()
            },
            Blank {
                nose_width: Some(80.0),
                ..Blank::default()
            },
        ];
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
        // Thickness: the published centre value, thinner toward the tips, never negative.
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
        let s = BlankShape::new(&b);
        let l = b.length;
        assert!((s.half_width_at(l - FOOT) * 2.0 - 30.0).abs() < 0.05);
        assert!((s.half_width_at(FOOT) * 2.0 - 38.0).abs() < 0.05);
        assert!((s.bottom_at(l - FOOT) - 5.0).abs() < 0.01);
        assert!((s.bottom_at(l - 2.0 * FOOT) - 2.0).abs() < 0.01);
        assert!((s.bottom_at(FOOT) - 2.5).abs() < 0.01);
        assert!((s.bottom_at(2.0 * FOOT) - 0.8).abs() < 0.01);
        assert!((s.thickness_at(l - FOOT) - 6.0).abs() < 0.01);
        assert!((s.thickness_at(FOOT) - 6.5).abs() < 0.01);
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
        // Thicker: deck and bottom fail in the middle by the same amount (centred height).
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
        // Ends stay about where they were lengthwise; the fit gets worse than level.
        assert!((r.tail_spare - 3.0).abs() < 0.5 && (r.nose_spare - 3.0).abs() < 0.5);
        let level = fit(&b, &blank, &Placement::default());
        assert!(r.min_clearance() < level.min_clearance());
    }
}
