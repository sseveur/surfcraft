//! Woodworking templates (`docs/spec/templates.md`, sections 2, 5, 6 and 7): sandwich construction (core
//! profile, rail, deck and bottom skins), hollow wooden board (stringer, ribs, rail strip, skins, tail and nose
//! pieces) and chambered planks.
//!
//! All geometry is in millimetres at 1:1 with y up; the option structs are in centimetres like the board model.
//! The rail line lies *inside* the outline (D4); the rib tabs, the rail-strip slots and the stringer holes all
//! use one station list (D3) and one joint computation (D11), so the parts fit together.

use std::cell::RefCell;
use std::rc::Rc;

use surfcraft_core::{Board, DVec2, Station, surface};

use crate::drawing::{Drawing, Item, Style};
use crate::templates::{
    HEADER, HEADER_SMALL, OVERRUN, board_name, finish, grid, grid_over_geometry, is_imperial,
    mirror_closed,
};

/// Woodworking templates as (name used by `by_name` and the `export` command, label shown in menus).
pub const TEMPLATES: [(&str, &str); 12] = [
    ("sandwich_profile", "Sandwich: profile (core)"),
    ("sandwich_rail", "Sandwich: rail"),
    ("sandwich_deck_skin", "Sandwich: deck skin"),
    ("sandwich_bottom_skin", "Sandwich: bottom skin"),
    ("hws_stringer", "Hollow wood: stringer"),
    ("hws_ribs", "Hollow wood: ribs"),
    ("hws_rail", "Hollow wood: rail strip"),
    ("hws_deck_skin", "Hollow wood: deck skin"),
    ("hws_bottom_skin", "Hollow wood: bottom skin"),
    ("hws_tail_piece", "Hollow wood: tail piece"),
    ("hws_nose_piece", "Hollow wood: nose piece"),
    ("chambered_planks", "Chambered wood: all planks"),
];

/// Length of the hollow-wood tail and nose pieces' stations, cm (9 in).
pub const HWS_PIECE_LENGTH: f64 = 22.86;

/// One inch and one foot in mm.
const IN: f64 = 25.4;
const FT: f64 = 304.8;
/// Finite-difference step for surface slopes (mm).
const SLOPE_STEP: f64 = 0.1;
/// Light grey reference outline (plank outer face).
const OUTER_FACE: Style = Style::solid(0.35, [200, 200, 200]);
/// Red alignment marks.
const MARK: Style = Style::solid(0.35, [255, 20, 20]);

/// Sandwich construction settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SandwichOptions {
    pub skin_thickness: f64,
    pub distance_to_rail: f64,
    pub tail_offset: f64,
    pub nose_offset: f64,
    pub flatten: bool,
}

impl Default for SandwichOptions {
    fn default() -> Self {
        SandwichOptions {
            skin_thickness: 0.3,
            distance_to_rail: 1.27,
            tail_offset: 2.0,
            nose_offset: 6.0,
            flatten: false,
        }
    }
}

/// Hollow wooden board settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HwsOptions {
    pub distance_from_rail: f64,
    pub skin_thickness: f64,
    pub frame_thickness: f64,
    pub webbing: f64,
    pub tail_offset: f64,
    pub nose_offset: f64,
}

impl Default for HwsOptions {
    fn default() -> Self {
        HwsOptions {
            distance_from_rail: 3.0,
            skin_thickness: 0.4,
            frame_thickness: 0.5,
            webbing: 1.5,
            tail_offset: 3.5,
            nose_offset: 3.5,
        }
    }
}

/// Chambered (plank) board settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChamberedOptions {
    pub grid: bool,
    /// Offset of the first plank's inner face from the centre.
    pub start_offset: f64,
    /// Offset where the planks end; `None` = half the maximum width.
    pub end_offset: Option<f64>,
    pub plank_thickness: f64,
    /// Minimum wall left above and below the chambers.
    pub shell_thickness: f64,
    pub chambering: bool,
    pub alignment_marks: bool,
}

impl Default for ChamberedOptions {
    fn default() -> Self {
        ChamberedOptions {
            grid: true,
            start_offset: 0.0,
            end_offset: None,
            plank_thickness: 2.54,
            shell_thickness: 0.8,
            chambering: true,
            alignment_marks: true,
        }
    }
}

/// The template called `name` in [`TEMPLATES`], built with default options, or `None` for an unknown name.
pub fn by_name(board: &Board, name: &str, fmt: &dyn Fn(f64) -> String) -> Option<Drawing> {
    let s = SandwichOptions::default();
    let h = HwsOptions::default();
    Some(match name {
        "sandwich_profile" => sandwich_profile(board, &s, fmt),
        "sandwich_rail" => sandwich_rail(board, &s, fmt),
        "sandwich_deck_skin" => sandwich_deck_skin(board, &s, fmt),
        "sandwich_bottom_skin" => sandwich_bottom_skin(board, &s, fmt),
        "hws_stringer" => hws_stringer(board, &h, fmt),
        "hws_ribs" => hws_ribs(board, &h, fmt),
        "hws_rail" => hws_rail(board, &h, fmt),
        "hws_deck_skin" => hws_deck_skin(board, &h, fmt),
        "hws_bottom_skin" => hws_bottom_skin(board, &h, fmt),
        "hws_tail_piece" => hws_tail_piece(board, &h, fmt),
        "hws_nose_piece" => hws_nose_piece(board, &h, fmt),
        "chambered_planks" => stack(
            &chambered_planks(board, &ChamberedOptions::default(), fmt),
            10.0,
        ),
        _ => return None,
    })
}

/// Points of every path drawn with `style`.
pub fn paths_with_style(d: &Drawing, style: Style) -> impl Iterator<Item = &[DVec2]> + '_ {
    d.items.iter().filter_map(move |item| match item {
        Item::Path {
            points, style: s, ..
        } if *s == style => Some(points.as_slice()),
        _ => None,
    })
}

/// Drawings stacked one above the other (the first on top), `gap` mm apart. Each drawing becomes a separate
/// piece (its own [`Drawing::parts`] are kept if it has any), so the tiler prints each on its own pages.
pub fn stack(drawings: &[Drawing], gap: f64) -> Drawing {
    let width = drawings.iter().map(|d| d.width).fold(0.0, f64::max);
    let height = drawings.iter().map(|d| d.height).sum::<f64>()
        + gap * drawings.len().saturating_sub(1) as f64;
    let mut out = Drawing::new(width, height);
    let mut top = height;
    for d in drawings {
        let offset = DVec2::new(0.0, top - d.height);
        out.place(d, offset);
        if d.parts.is_empty() {
            out.part(offset, offset + DVec2::new(d.width, d.height));
        } else {
            for &(a, b) in &d.parts {
                out.part(a + offset, b + offset);
            }
        }
        top -= d.height + gap;
    }
    out
}

// ============================================================================================ geometry

/// Prepared deck and bottom curves of one station.
struct Slice {
    deck: Station,
    bottom: Station,
}

/// Height (cm, with rocker) of the station curve where its lateral coordinate equals `y` (cm), by a bracketed
/// secant (Illinois) search on the curve parameter. Outside the curve's lateral range the nearer end is used.
fn height_at_lateral(st: &Station, y: f64) -> f64 {
    let f = |s: f64| st.point(s).y - y;
    let (mut a, mut b) = (0.0, 1.0);
    let (mut fa, mut fb) = (f(a), f(b));
    if !(fa.is_finite() && fb.is_finite()) {
        return st.point(0.0).z;
    }
    if fa == 0.0 {
        return st.point(a).z;
    }
    if fb == 0.0 {
        return st.point(b).z;
    }
    if fa.signum() == fb.signum() {
        return st.point(if fa.abs() < fb.abs() { a } else { b }).z;
    }
    let mut side = 0;
    let mut c = 0.5;
    for _ in 0..80 {
        c = (a * fb - b * fa) / (fb - fa);
        if !c.is_finite() {
            c = 0.5 * (a + b);
        }
        let fc = f(c);
        if fc.abs() < 1e-9 || (b - a).abs() < 1e-12 {
            break;
        }
        if fc.signum() == fb.signum() {
            b = c;
            fb = fc;
            if side == -1 {
                fa *= 0.5;
            }
            side = -1;
        } else {
            a = c;
            fa = fc;
            if side == 1 {
                fb *= 0.5;
            }
            side = 1;
        }
    }
    st.point(c).z
}

/// Board surface queries in millimetres (section 1.4 of the spec), with a small cache of prepared stations.
pub(crate) struct Surf<'a> {
    board: &'a Board,
    /// Board length, mm.
    pub len: f64,
    cache: RefCell<Vec<(u64, Rc<Slice>)>>,
}

impl<'a> Surf<'a> {
    pub fn new(board: &'a Board) -> Self {
        Surf {
            board,
            len: board.length() * 10.0,
            cache: RefCell::new(Vec::new()),
        }
    }

    fn slice(&self, x: f64) -> Rc<Slice> {
        let key = x.to_bits();
        if let Some((_, s)) = self.cache.borrow().iter().find(|(k, _)| *k == key) {
            return s.clone();
        }
        let xc = x / 10.0;
        let s = Rc::new(Slice {
            deck: self.board.station(xc, surface::DECK.0, surface::DECK.1),
            bottom: self.board.station(xc, 90.0, 270.0),
        });
        let mut c = self.cache.borrow_mut();
        if c.len() >= 8 {
            c.remove(0);
        }
        c.push((key, s.clone()));
        s
    }

    /// Outline half-width `o(x)`.
    pub fn o(&self, x: f64) -> f64 {
        self.board.outline.value_at(x / 10.0) * 10.0
    }

    /// Unit tangent of the outline at `x`, oriented from tail to nose.
    pub fn tangent(&self, x: f64) -> DVec2 {
        if self.board.outline.segment_for_x(x / 10.0).is_some() {
            let a = self.board.outline.tangent_at(x / 10.0);
            let t = DVec2::new(a.sin(), a.cos());
            if t.is_finite() {
                return t;
            }
        }
        // Numerical fallback from the outline as a function of x.
        let h = 0.5;
        let (x0, x1) = ((x - h).max(0.0), (x + h).min(self.len));
        DVec2::new(x1 - x0, self.o(x1) - self.o(x0)).normalize_or(DVec2::X)
    }

    /// Rail-line point at distance `d` inside the outline for outline station `x0` (2.1).
    pub fn rail(&self, x0: f64, d: f64) -> DVec2 {
        let t = self.tangent(x0);
        DVec2::new(x0, self.o(x0)) + d * DVec2::new(t.y, -t.x)
    }

    /// Whether a plan point lies on the board: `y ≥ 0`, inside the outline, `x` within the length.
    pub fn inside(&self, p: DVec2) -> bool {
        p.is_finite() && p.y >= 0.0 && p.x >= 0.0 && p.x <= self.len && p.y <= self.o(p.x) + 1e-9
    }

    /// Station-plane rail-line point: the outline station `x0` whose rail point at distance `d` has `R_x = x`,
    /// and that point.
    pub fn rail_at_plane(&self, x: f64, d: f64) -> (f64, DVec2) {
        let f = |x0: f64| self.rail(x0, d).x - x;
        let (mut a, mut b) = (
            (x - d.abs() - 1.0).max(0.0),
            (x + d.abs() + 1.0).min(self.len),
        );
        let (fa, fb) = (f(a), f(b));
        if fa.signum() == fb.signum() {
            let x0 = if fa.abs() < fb.abs() { a } else { b };
            return (x0, self.rail(x0, d));
        }
        for _ in 0..60 {
            let m = 0.5 * (a + b);
            if f(m).signum() == fa.signum() {
                a = m;
            } else {
                b = m;
            }
        }
        let x0 = 0.5 * (a + b);
        (x0, self.rail(x0, d))
    }

    /// Raw deck and bottom heights from the station curves (cm in the stations, mm here), no checks.
    fn raw(&self, x: f64, y: f64) -> (f64, f64) {
        let s = self.slice(x);
        (
            height_at_lateral(&s.deck, y / 10.0) * 10.0,
            height_at_lateral(&s.bottom, y / 10.0) * 10.0,
        )
    }

    /// Deck and bottom heights `D(x, y)`, `B(x, y)`; `None` off the board.
    pub fn heights(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !self.inside(DVec2::new(x, y)) {
            return None;
        }
        if y == 0.0 {
            let xc = x / 10.0;
            return Some((
                self.board.deck_at(xc) * 10.0,
                self.board.rocker_at(xc) * 10.0,
            ));
        }
        Some(self.raw(x, y))
    }

    /// Skin-compensated core heights `D_t`, `B_t` (2.2): each surface moved inwards by `t / cos α` of its own
    /// slope. `None` off the board.
    pub fn core(&self, x: f64, y: f64, t: f64) -> Option<(f64, f64)> {
        let (d, b) = self.heights(x, y)?;
        if t == 0.0 {
            return Some((d, b));
        }
        let h = SLOPE_STEP;
        let here = self.raw(x, y);
        // Lengthwise slope: backwards, or forwards where that would leave the board.
        let gx = if x - h >= 0.0 && y <= self.o(x - h) {
            let p = self.raw(x - h, y);
            ((here.0 - p.0) / h, (here.1 - p.1) / h)
        } else if x + h <= self.len && y <= self.o(x + h) {
            let p = self.raw(x + h, y);
            ((p.0 - here.0) / h, (p.1 - here.1) / h)
        } else {
            (0.0, 0.0)
        };
        let o = self.o(x);
        let gy = if y - h >= 0.0 {
            let p = self.raw(x, y - h);
            ((here.0 - p.0) / h, (here.1 - p.1) / h)
        } else if y + h <= o {
            let p = self.raw(x, y + h);
            ((p.0 - here.0) / h, (p.1 - here.1) / h)
        } else {
            (0.0, 0.0)
        };
        let kd = (1.0 + gx.0 * gx.0 + gy.0 * gy.0).sqrt();
        let kb = (1.0 + gx.1 * gx.1 + gy.1 * gy.1).sqrt();
        let (dt, bt) = (d - t * kd, b + t * kb);
        (dt.is_finite() && bt.is_finite()).then_some((dt, bt))
    }

    /// Height of the rail apex (outward normal 88° from vertical) at `x`, with rocker.
    pub fn apex(&self, x: f64) -> f64 {
        self.board.station(x / 10.0, -90.0, 88.0).point(1.0).z * 10.0
    }

    /// The raw half cross-section at `x` (lateral, height with rocker), deck centre round the rail to the
    /// bottom centre, `n + 1` points.
    pub fn half_section(&self, x: f64, n: usize) -> Vec<DVec2> {
        let st = self
            .board
            .station(x / 10.0, surface::WHOLE.0, surface::WHOLE.1);
        (0..=n)
            .map(|k| {
                let p = st.point(k as f64 / n as f64);
                DVec2::new(p.y, p.z) * 10.0
            })
            .filter(|p| p.is_finite())
            .collect()
    }

    /// Thickness at the middle of the board (flattening reference).
    fn centre_thickness(&self) -> f64 {
        self.board.thickness_at(self.len / 20.0) * 10.0
    }

    /// Flattening shift at `x` (2.4): subtract to make the deck centreline level at the centre thickness.
    fn flatten_shift(&self, x: f64) -> f64 {
        self.board.deck_at(x / 10.0) * 10.0 - self.centre_thickness()
    }
}

/// `n + 1` equally spaced values from `a` to `b`, `n = max(1, floor((b − a) / step))`.
fn steps(a: f64, b: f64, step: f64) -> Vec<f64> {
    let n = ((b - a) / step).floor().max(1.0) as usize;
    (0..=n).map(|j| a + (b - a) * j as f64 / n as f64).collect()
}

/// Indices of the first maximal run of `true` (2.3).
fn first_run(valid: &[bool]) -> Option<(usize, usize)> {
    let start = valid.iter().position(|v| *v)?;
    let len = valid[start..].iter().take_while(|v| **v).count();
    Some((start, start + len - 1))
}

/// Closed polygon: `top` forwards, `bot` backwards, over the samples `u`.
fn band_polygon(u: &[f64], top: &[f64], bot: &[f64]) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = u.iter().zip(top).map(|(u, z)| DVec2::new(*u, *z)).collect();
    pts.extend(u.iter().zip(bot).rev().map(|(u, z)| DVec2::new(*u, *z)));
    pts
}

/// Closed rectangle of thickness `th` about the segment `a`–`b`.
fn band(a: DVec2, b: DVec2, th: f64) -> Vec<DVec2> {
    let dir = (b - a).normalize_or(DVec2::X);
    let n = DVec2::new(-dir.y, dir.x) * (th / 2.0);
    vec![a - n, b - n, b + n, a + n]
}

/// The half path followed by its mirror image about `u = 0`, closed (axis points not repeated).
fn mirror_closed_u(half: &[DVec2]) -> Vec<DVec2> {
    let swap = |p: &DVec2| DVec2::new(p.y, p.x);
    let swapped: Vec<DVec2> = half.iter().map(swap).collect();
    mirror_closed(&swapped).iter().map(swap).collect()
}

/// Closed profile at lateral offset `y0` (2.5), every 3 mm from `xa` to `xb`, over the first valid run.
fn closed_profile(
    s: &Surf,
    y0: f64,
    t: f64,
    flatten: bool,
    xa: f64,
    xb: f64,
) -> Option<Vec<DVec2>> {
    let xs = steps(xa, xb, 3.0);
    let hs: Vec<Option<(f64, f64)>> = xs
        .iter()
        .map(|&x| {
            let (top, bot) = s.core(x, y0, t)?;
            let sh = if flatten { s.flatten_shift(x) } else { 0.0 };
            let (top, bot) = (top - sh, bot - sh);
            (bot < top).then_some((top, bot))
        })
        .collect();
    let valid: Vec<bool> = hs.iter().map(Option::is_some).collect();
    let (i0, i1) = first_run(&valid)?;
    let u = &xs[i0..=i1];
    let top: Vec<f64> = hs[i0..=i1].iter().map(|h| h.unwrap().0).collect();
    let bot: Vec<f64> = hs[i0..=i1].iter().map(|h| h.unwrap().1).collect();
    Some(band_polygon(u, &top, &bot))
}

/// Lightening hole (2.6): `samples` points from `a` over `span` (20 of 20 steps short of the end for webbing,
/// 21 points to the end for chambers); `heights(c)` gives the already reduced (top, bottom) at coordinate `c`.
fn hole(
    a: f64,
    span: f64,
    to_end: bool,
    heights: impl Fn(f64) -> Option<(f64, f64)>,
) -> Option<Vec<DVec2>> {
    if span <= 0.0 {
        return None;
    }
    let n = if to_end { 21 } else { 20 };
    let cs: Vec<f64> = (0..n).map(|k| a + k as f64 * span / 20.0).collect();
    let hs: Vec<Option<(f64, f64)>> = cs
        .iter()
        .map(|&c| heights(c).filter(|(t, b)| t >= b))
        .collect();
    let valid: Vec<bool> = hs.iter().map(Option::is_some).collect();
    let (i0, i1) = first_run(&valid)?;
    if i1 == i0 {
        return None;
    }
    let top: Vec<f64> = hs[i0..=i1].iter().map(|h| h.unwrap().0).collect();
    let bot: Vec<f64> = hs[i0..=i1].iter().map(|h| h.unwrap().1).collect();
    Some(band_polygon(&cs[i0..=i1], &top, &bot))
}

/// Zero-height baseline from 5 mm before `a` to 5 mm past `b` (guide).
fn baseline(d: &mut Drawing, a: f64, b: f64) {
    d.line(
        DVec2::new(a - OVERRUN, 0.0),
        DVec2::new(b + OVERRUN, 0.0),
        Style::GUIDE,
    );
}

// ============================================================================================ rail line

/// One shared sampling of the rail line at distance `d` from outline station `xa` to `xb` (≈ 5 mm steps),
/// with core heights, the developed length and the first valid run (2.1, 5.2, 6.5).
struct RailStrip {
    x0: Vec<f64>,
    /// Developed length from the first sample.
    cum: Vec<f64>,
    heights: Vec<Option<(f64, f64)>>,
    run: Option<(usize, usize)>,
}

impl RailStrip {
    fn new(s: &Surf, d: f64, t: f64, flatten: bool, xa: f64, xb: f64) -> RailStrip {
        let x0 = steps(xa, xb, 5.0);
        let pts: Vec<DVec2> = x0.iter().map(|&x| s.rail(x, d)).collect();
        let mut cum = vec![0.0; pts.len()];
        for k in 1..pts.len() {
            let step = (pts[k] - pts[k - 1]).length();
            cum[k] = cum[k - 1] + if step.is_finite() { step } else { 0.0 };
        }
        let heights: Vec<Option<(f64, f64)>> = pts
            .iter()
            .map(|&r| {
                if !s.inside(r) {
                    return None;
                }
                let (top, bot) = s.core(r.x, r.y, t)?;
                let sh = if flatten { s.flatten_shift(r.x) } else { 0.0 };
                (top >= bot).then_some((top - sh, bot - sh))
            })
            .collect();
        let valid: Vec<bool> = heights.iter().map(Option::is_some).collect();
        let run = first_run(&valid);
        RailStrip {
            x0,
            cum,
            heights,
            run,
        }
    }

    /// Developed length (from the first valid sample) at outline station `x0`, interpolated.
    fn u(&self, x0: f64) -> f64 {
        let n = self.x0.len();
        let origin = self.run.map_or(0.0, |(i, _)| self.cum[i]);
        if n < 2 {
            return -origin;
        }
        let (a, b) = (self.x0[0], self.x0[n - 1]);
        let f = ((x0 - a) / (b - a) * (n - 1) as f64).clamp(0.0, (n - 1) as f64);
        let i = (f.floor() as usize).min(n - 2);
        let w = f - i as f64;
        self.cum[i] * (1.0 - w) + self.cum[i + 1] * w - origin
    }

    /// Outline-station range covered by the valid run.
    fn range(&self) -> Option<(f64, f64)> {
        self.run.map(|(i, j)| (self.x0[i], self.x0[j]))
    }

    fn polygon(&self) -> Option<Vec<DVec2>> {
        let (i0, i1) = self.run?;
        if i1 == i0 {
            return None;
        }
        let u: Vec<f64> = (i0..=i1).map(|k| self.cum[k] - self.cum[i0]).collect();
        let top: Vec<f64> = (i0..=i1).map(|k| self.heights[k].unwrap().0).collect();
        let bot: Vec<f64> = (i0..=i1).map(|k| self.heights[k].unwrap().1).collect();
        Some(band_polygon(&u, &top, &bot))
    }

    /// Developed length of the valid run.
    fn length(&self) -> f64 {
        self.run.map_or(0.0, |(i, j)| self.cum[j] - self.cum[i])
    }
}

/// Developed skin template (5.3 / 5.4): half-width over the surface out to the rail line at distance `d`,
/// against the length along the deck (or bottom) centreline.
fn skin(board: &Board, d: f64, deck: bool, header: &str, size: f64) -> Drawing {
    let s = Surf::new(board);
    let mut pts: Vec<DVec2> = Vec::new();
    let mut first_ax: Option<f64> = None;
    let centre = if deck { &board.deck } else { &board.bottom };
    let mut x0 = 0.0;
    while x0 < s.len {
        let r = s.rail(x0, d);
        x0 += 10.0;
        if !s.inside(r) {
            if pts.is_empty() {
                continue;
            }
            break;
        }
        let ax = centre.length_to_x(r.x / 10.0) * 10.0;
        let origin = *first_ax.get_or_insert(ax);
        let mut v = 0.0;
        let mut prev: Option<f64> = None;
        let mut ok = true;
        for k in 0..=10 {
            let y = r.y * k as f64 / 10.0;
            let Some((top, bot)) = s.heights(r.x, y) else {
                ok = false;
                break;
            };
            let z = if deck { top } else { bot };
            if let Some(p) = prev {
                v += ((r.y / 10.0).powi(2) + (z - p).powi(2)).sqrt();
            }
            prev = Some(z);
        }
        if !ok || !v.is_finite() {
            break;
        }
        pts.push(DVec2::new(ax - origin, v));
    }
    let mut dr = Drawing::default();
    if let (Some(a), Some(b)) = (pts.first().copied(), pts.last().copied()) {
        // Stringer edge (reference), then the skin outline closed down to it at both ends.
        dr.line(
            DVec2::new(a.x - OVERRUN, 0.0),
            DVec2::new(b.x + OVERRUN, 0.0),
            Style::GUIDE,
        );
        let mut cut = vec![DVec2::new(a.x, 0.0)];
        cut.extend(pts.iter().copied());
        cut.push(DVec2::new(b.x, 0.0));
        dr.path(cut, Style::CUT);
    }
    finish(
        dr,
        &[format!("Board file: {} {header}", board_name(board))],
        size,
    )
}

// ============================================================================================ sandwich

pub fn sandwich_profile(
    board: &Board,
    opts: &SandwichOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    let s = Surf::new(board);
    let mut d = Drawing::default();
    baseline(&mut d, 0.0, s.len);
    if let Some(p) = closed_profile(
        &s,
        0.0,
        opts.skin_thickness * 10.0,
        opts.flatten,
        0.0,
        s.len,
    ) {
        d.closed(p, Style::CUT);
    }
    grid_over_geometry(&mut d, is_imperial(fmt));
    finish(
        d,
        &[format!("Board file: {} Profile", board_name(board))],
        HEADER_SMALL,
    )
}

pub fn sandwich_rail(
    board: &Board,
    opts: &SandwichOptions,
    _fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    let s = Surf::new(board);
    let strip = RailStrip::new(
        &s,
        opts.distance_to_rail * 10.0,
        opts.skin_thickness * 10.0,
        opts.flatten,
        opts.tail_offset * 10.0,
        s.len - opts.nose_offset * 10.0,
    );
    let mut d = Drawing::default();
    if let Some(p) = strip.polygon() {
        baseline(&mut d, 0.0, strip.length());
        d.closed(p, Style::CUT);
    }
    finish(
        d,
        &[format!("Board file: {} Rail template", board_name(board))],
        HEADER_SMALL,
    )
}

pub fn sandwich_deck_skin(
    board: &Board,
    opts: &SandwichOptions,
    _fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    skin(
        board,
        opts.distance_to_rail * 10.0,
        true,
        "Deck skin template",
        HEADER_SMALL,
    )
}

pub fn sandwich_bottom_skin(
    board: &Board,
    opts: &SandwichOptions,
    _fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    skin(
        board,
        opts.distance_to_rail * 10.0,
        false,
        "Bottom skin template",
        HEADER_SMALL,
    )
}

// ============================================================================================ hollow wood

/// Hollow-wood options in mm.
#[derive(Clone, Copy, Debug)]
struct Hws {
    d: f64,
    t: f64,
    f: f64,
    w: f64,
    et: f64,
    en: f64,
}

impl Hws {
    fn new(o: &HwsOptions) -> Hws {
        Hws {
            d: o.distance_from_rail * 10.0,
            t: o.skin_thickness * 10.0,
            f: o.frame_thickness * 10.0,
            w: o.webbing * 10.0,
            et: o.tail_offset * 10.0,
            en: o.nose_offset * 10.0,
        }
    }
}

/// Rib stations (cm from the tail), shared by the stringer, ribs and rail strip.
pub fn hws_rib_stations(board: &Board) -> Vec<f64> {
    let l = board.length() * 10.0;
    let n = ((l - 9.0 * IN) / FT).floor();
    if n.is_nan() || n < 1.0 {
        return Vec::new();
    }
    (1..=n as usize).map(|k| k as f64 * FT / 10.0).collect()
}

/// Where a rib meets the rail strip: computed once and used by both the rib's tab and the strip's slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibJoint {
    /// Station (mm from the tail).
    pub x: f64,
    /// Outline station of the station-plane rail-line point.
    pub outline_station: f64,
    /// Lateral position of the rail strip's outer face (rail line at `d`) in the station plane: the tab tip.
    pub outer: f64,
    /// Lateral position of the rail strip's inner face (rail line at `d + f`) in the station plane: where the
    /// rib body ends.
    pub inner: f64,
    /// Centre height of the tab and slot (mm, with rocker).
    pub mid: f64,
    /// Height of the tab and slot (mm).
    pub height: f64,
    /// Width of the slot along the strip: the rib thickness measured along the rail line, `f / cos β`.
    pub slot_width: f64,
}

/// The rib-to-rail-strip joint at station `x` (cm), `None` where the rail line misses the core there.
pub fn hws_rib_joint(board: &Board, x: f64, opts: &HwsOptions) -> Option<RibJoint> {
    rib_joint(&Surf::new(board), x * 10.0, &Hws::new(opts))
}

fn rib_joint(s: &Surf, x: f64, o: &Hws) -> Option<RibJoint> {
    let (x0, r) = s.rail_at_plane(x, o.d);
    if !s.inside(r) || (r.x - x).abs() > 0.05 {
        return None;
    }
    let (top, bot) = s.core(r.x, r.y, o.t)?;
    if top <= bot {
        return None;
    }
    let (_, ri) = s.rail_at_plane(x, o.d + o.f);
    let inner = if ri.is_finite() && (ri.x - x).abs() <= 0.05 && ri.y < r.y {
        ri.y
    } else {
        r.y - o.f
    };
    let cos_b = s.tangent(x0).x.abs().max(0.2);
    Some(RibJoint {
        x,
        outline_station: x0,
        outer: r.y,
        inner,
        mid: 0.5 * (top + bot),
        height: ((top - bot) / 4.0).max(5.0),
        slot_width: o.f / cos_b,
    })
}

/// A tail or nose plate (6.7, 6.8): its plate line and its five sections along the outline stations.
#[derive(Clone, Debug)]
struct Plate {
    /// Plate-line end points `(x, z)`: tail end and front (tail plate) or rear and nose end (nose plate).
    a: DVec2,
    b: DVec2,
    alpha: f64,
    /// Outline-station boundaries of the five sections, from the tail side.
    bounds: [f64; 6],
    /// True for the nose plate.
    nose: bool,
}

impl Plate {
    fn z(&self, x: f64) -> f64 {
        self.a.y + (x - self.a.x) * self.alpha.tan()
    }

    /// Tab sections (1 and 3) as outline-station ranges.
    fn tabs(&self) -> [(f64, f64); 2] {
        [
            (self.bounds[1], self.bounds[2]),
            (self.bounds[3], self.bounds[4]),
        ]
    }
}

/// Section boundaries over `[a, b]`: 1 in, 1 in, rest, 1 in, 1 in when the piece is longer than 5 in, otherwise
/// five equal sections (D8).
fn plate_bounds(a: f64, b: f64) -> [f64; 6] {
    let len = b - a;
    let lens = if len > 5.0 * IN {
        [IN, IN, len - 4.0 * IN, IN, IN]
    } else {
        [len / 5.0; 5]
    };
    let mut out = [a; 6];
    for k in 0..5 {
        out[k + 1] = out[k] + lens[k];
    }
    out[5] = b;
    out
}

/// Mid-height of the core at the rail point `r`, if valid.
fn mid_height(s: &Surf, r: DVec2, t: f64) -> Option<f64> {
    if !s.inside(r) {
        return None;
    }
    let (top, bot) = s.core(r.x, r.y, t)?;
    (top >= bot).then_some(0.5 * (top + bot))
}

fn tail_plate(s: &Surf, o: &Hws) -> Option<Plate> {
    let front = 9.0 * IN;
    if front <= o.et {
        return None;
    }
    let n = (s.len / 5.0).floor().max(1.0);
    let step = s.len / n;
    let mut x0 = o.et;
    let a = loop {
        if x0 > front {
            return None;
        }
        let r = s.rail(x0, o.d);
        if let Some(z) = mid_height(s, r, o.t) {
            break DVec2::new(r.x, z);
        }
        x0 += step;
    };
    let r = s.rail(front, o.d);
    let b = DVec2::new(r.x, mid_height(s, r, o.t)?);
    Some(Plate {
        a,
        b,
        alpha: (b.y - a.y).atan2(b.x - a.x),
        bounds: plate_bounds(o.et, front),
        nose: false,
    })
}

fn nose_plate(s: &Surf, o: &Hws) -> Option<Plate> {
    let rear = s.len - 9.0 * IN;
    let end = s.len - o.en;
    if end <= rear || rear <= 0.0 {
        return None;
    }
    let r = s.rail(rear, o.d);
    let a = DVec2::new(r.x, mid_height(s, r, o.t)?);
    let n = ((s.len - o.et - o.en) / 5.0).floor().max(1.0);
    let step = (s.len - o.et - o.en) / n;
    let mut x0 = end;
    let b = loop {
        if x0 < rear {
            return None;
        }
        let r = s.rail(x0, o.d);
        if let Some(z) = mid_height(s, r, o.t) {
            break DVec2::new(r.x, z);
        }
        x0 -= step;
    };
    Some(Plate {
        a,
        b,
        alpha: (b.y - a.y).atan2(b.x - a.x),
        bounds: plate_bounds(rear, end),
        nose: true,
    })
}

pub fn hws_stringer(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let s = Surf::new(board);
    let o = Hws::new(opts);
    let mut d = Drawing::default();
    baseline(&mut d, 0.0, s.len);
    let (xa, xb) = (o.et, s.len - o.en);
    if let Some(p) = closed_profile(&s, 0.0, o.t, false, xa, xb) {
        d.closed(p, Style::CUT);
    }
    // Lightening holes centred on every half-foot mark; the ones on the rib stations let the ribs through.
    // Holes are kept clear of the tail and nose pieces (and of the stringer's ends).
    let k = (s.len / FT).floor() as usize;
    let span = FT / 2.0 - o.w;
    let (lo, hi) = ((9.0 * IN).max(xa), (s.len - 9.0 * IN).min(xb));
    for i in 0..2 * k {
        let a = (i + 1) as f64 * FT / 2.0 - span / 2.0;
        let h = hole(a, span, false, |x| {
            if x < lo || x > hi {
                return None;
            }
            let (top, bot) = s.core(x, 0.0, o.t)?;
            Some((top - o.w, bot + o.w))
        });
        if let Some(h) = h {
            d.closed(h, Style::CUT);
        }
    }
    // Tail piece joint: reference band along the plate, and the slot from 2 in behind the plate's front edge
    // to the stringer's tail end.
    if let Some(p) = tail_plate(&s, &o) {
        d.closed(band(p.a, p.b, o.f), Style::GUIDE);
        let tpl = 9.0 * IN - o.et;
        let x1 = p.b.x - 2.0 * IN;
        let x2 = (p.b.x - tpl).min(xa);
        if x1 > x2 {
            d.closed(
                band(DVec2::new(x1, p.z(x1)), DVec2::new(x2, p.z(x2)), o.f),
                Style::CUT,
            );
        }
    }
    if let Some(p) = nose_plate(&s, &o) {
        d.closed(band(p.a, p.b, o.f), Style::GUIDE);
        let npl = 9.0 * IN - o.en;
        let x1 = p.a.x + 2.0 * IN;
        let x2 = (p.a.x + npl).max(xb);
        if x2 > x1 {
            d.closed(
                band(DVec2::new(x1, p.z(x1)), DVec2::new(x2, p.z(x2)), o.f),
                Style::CUT,
            );
        }
    }
    grid_over_geometry(&mut d, is_imperial(fmt));
    finish(
        d,
        &[format!("Board file: {} Stringer", board_name(board))],
        HEADER,
    )
}

/// The rib at station `x` (cm): the full rib (both halves), its lightening holes and the raw cross-section
/// behind it.
pub fn hws_rib(board: &Board, x: f64, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let s = Surf::new(board);
    let o = Hws::new(opts);
    let x = x * 10.0;
    let mut d = Drawing::default();
    // Reference: the raw cross-section, both halves, closed along the centreline.
    let half = s.half_section(x, 300);
    if half.len() > 2 {
        let mut full = half.clone();
        full.extend(half.iter().rev().map(|p| DVec2::new(-p.x, p.y)));
        let (zlo, zhi) = half
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        d.line(DVec2::new(0.0, zlo), DVec2::new(0.0, zhi), Style::GUIDE);
        d.closed(full, Style::GUIDE);
    }
    if let Some(j) = rib_joint(&s, x, &o) {
        let (yr, yb) = (j.outer, j.inner);
        let core = |y: f64| s.core(x, y, o.t).filter(|(t, b)| t >= b);
        // Stringer pass-through: the rib is reduced to the stringer hole's height between ±f/2.
        let c0 = core(0.0);
        let cf = core(o.f / 2.0);
        if let (Some(c0), Some(cf)) = (c0, cf) {
            let notch_top = c0.0.min(cf.0) - o.w;
            let notch_bot = c0.1.max(cf.1) + o.w;
            let ys: Vec<f64> = (0..=150)
                .map(|j| o.f / 2.0 + j as f64 * (yb - o.f / 2.0) / 150.0)
                .collect();
            let edge: Vec<(f64, (f64, f64))> =
                ys.iter().filter_map(|&y| core(y).map(|c| (y, c))).collect();
            let mut path = vec![DVec2::new(0.0, notch_top), DVec2::new(o.f / 2.0, notch_top)];
            path.extend(edge.iter().map(|(y, c)| DVec2::new(*y, c.0)));
            let (zt, zb) = (j.mid + j.height / 2.0, j.mid - j.height / 2.0);
            path.extend([
                DVec2::new(yb, zt),
                DVec2::new(yr, zt),
                DVec2::new(yr, zb),
                DVec2::new(yb, zb),
            ]);
            path.extend(edge.iter().rev().map(|(y, c)| DVec2::new(*y, c.1)));
            path.extend([DVec2::new(o.f / 2.0, notch_bot), DVec2::new(0.0, notch_bot)]);
            d.closed(mirror_closed_u(&path), Style::CUT);
        }
        // Two lightening holes each side.
        let span = (yr - 1.5 * o.f - 3.0 * o.w) / 2.0;
        for a in [o.f / 2.0 + o.w, o.f / 2.0 + 2.0 * o.w + span] {
            if let Some(h) = hole(a, span, false, |y| core(y).map(|(t, b)| (t - o.w, b + o.w))) {
                d.closed(
                    h.iter().map(|p| DVec2::new(-p.x, p.y)).collect(),
                    Style::CUT,
                );
                d.closed(h, Style::CUT);
            }
        }
    }
    // Grid over the whole rib (both halves).
    if let Some((lo, hi)) = d.bounds() {
        grid(&mut d, lo, hi, is_imperial(fmt));
    }
    finish(
        d,
        &[
            format!("Board file: {} Crossection", board_name(board)),
            format!("Position: {}", fmt(x / 10.0)),
        ],
        HEADER,
    )
}

/// Every rib, each as its own piece.
pub fn hws_ribs(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    let ribs: Vec<Drawing> = hws_rib_stations(board)
        .iter()
        .map(|&x| hws_rib(board, x, opts, fmt))
        .collect();
    if ribs.is_empty() {
        return finish(
            Drawing::default(),
            &[format!(
                "Board file: {} Crossection: board too short for ribs",
                board_name(board)
            )],
            HEADER,
        );
    }
    stack(&ribs, 10.0)
}

pub fn hws_rail(board: &Board, opts: &HwsOptions, _fmt: &dyn Fn(f64) -> String) -> Drawing {
    let s = Surf::new(board);
    let o = Hws::new(opts);
    let strip = RailStrip::new(&s, o.d, o.t, false, o.et, s.len - o.en);
    let mut d = Drawing::default();
    if let (Some(p), Some((ra, rb))) = (strip.polygon(), strip.range()) {
        baseline(&mut d, 0.0, strip.length());
        d.closed(p, Style::CUT);
        let on_strip = |x0: f64| x0 >= ra - 1e-9 && x0 <= rb + 1e-9;
        // Lightening holes: 2w webs on the whole feet, w webs on the half feet, solid up to the first foot and
        // over the nose piece.
        let hole_span = FT / 2.0 - 1.5 * o.w;
        let clip = s.len - 9.0 * IN;
        if hole_span > 0.0 {
            let k = (s.len / FT).floor() as usize;
            'feet: for m in 1..=k {
                let foot = m as f64 * FT;
                for a in [foot + o.w, foot + 2.0 * o.w + hole_span] {
                    if a > rb || a > clip {
                        break 'feet;
                    }
                    let xs = steps(a, a + hole_span, 5.0);
                    let xs = &xs[..xs.len() - 1];
                    let mut u = Vec::new();
                    let mut top = Vec::new();
                    let mut bot = Vec::new();
                    let mut pinched = false;
                    for &x0 in xs {
                        let h = if on_strip(x0) && x0 <= clip {
                            let r = s.rail(x0, o.d);
                            s.inside(r)
                                .then(|| s.core(r.x, r.y, o.t))
                                .flatten()
                                .map(|(t, b)| (t - o.w, b + o.w))
                                .filter(|(t, b)| t >= b)
                        } else {
                            None
                        };
                        match h {
                            Some((t, b)) => {
                                u.push(strip.u(x0));
                                top.push(t);
                                bot.push(b);
                            }
                            None if !u.is_empty() => {
                                pinched = true;
                                break;
                            }
                            None => {}
                        }
                    }
                    if u.len() >= 2 {
                        d.closed(band_polygon(&u, &top, &bot), Style::CUT);
                    }
                    if pinched {
                        break 'feet;
                    }
                }
            }
        }
        // Rib slots at the shared stations, sized by the shared joint.
        for x in hws_rib_stations(board) {
            let Some(j) = rib_joint(&s, x * 10.0, &o) else {
                continue;
            };
            if !on_strip(j.outline_station) {
                continue;
            }
            let u = strip.u(j.outline_station);
            d.rect(
                DVec2::new(u - j.slot_width / 2.0, j.mid - j.height / 2.0),
                DVec2::new(u + j.slot_width / 2.0, j.mid + j.height / 2.0),
                Style::CUT,
            );
        }
        // Slots for the tabs of the tail and nose pieces.
        for p in [tail_plate(&s, &o), nose_plate(&s, &o)]
            .into_iter()
            .flatten()
        {
            for (a, b) in p.tabs() {
                // Where the strip ends inside a tab, the plate ends there too: cut the part that exists.
                let (a, b) = (a.max(ra), b.min(rb));
                if b - a < 1.0 {
                    continue;
                }
                let (ra, rb) = (s.rail(a, o.d), s.rail(b, o.d));
                let pa = DVec2::new(strip.u(a), p.z(ra.x));
                let pb = DVec2::new(strip.u(b), p.z(rb.x));
                d.closed(band(pa, pb, o.f), Style::CUT);
            }
        }
    }
    finish(
        d,
        &[format!("Board file: {} Rail", board_name(board))],
        HEADER,
    )
}

pub fn hws_deck_skin(board: &Board, opts: &HwsOptions, _fmt: &dyn Fn(f64) -> String) -> Drawing {
    skin(
        board,
        opts.distance_from_rail * 10.0,
        true,
        "Deck skin template",
        HEADER,
    )
}

pub fn hws_bottom_skin(board: &Board, opts: &HwsOptions, _fmt: &dyn Fn(f64) -> String) -> Drawing {
    skin(
        board,
        opts.distance_from_rail * 10.0,
        false,
        "Bottom skin template",
        HEADER,
    )
}

/// Half outline and lightening holes of a plate in its plan frame (`u` along the inclined plate, `v = y`).
fn plate_outline(s: &Surf, o: &Hws, p: &Plate) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
    let cos = p.alpha.cos().abs().max(0.2);
    let slot = 2.0 * IN / cos;
    let at = |x0: f64, dist: f64| -> Option<DVec2> {
        let r = s.rail(x0, dist);
        mid_height(s, r, o.t).map(|_| r)
    };
    let mut half: Vec<DVec2> = Vec::new();
    let mut x1: Option<f64> = None;
    let mut ended = false;
    'sections: for k in 0..5 {
        // Body sections follow the strip's inner face (d + f), tabs its outer face (d) (D21). Every section is
        // sampled up to its end, so the jog between body and tab lies exactly on the section boundary, where
        // the rail strip's slot starts.
        let dist = if k % 2 == 1 { o.d } else { o.d + o.f };
        for x0 in steps(p.bounds[k], p.bounds[k + 1], 5.0) {
            match at(x0, dist) {
                Some(r) => {
                    let origin = *x1.get_or_insert(r.x);
                    let u = (r.x - origin) / cos;
                    if half.is_empty() {
                        if p.nose {
                            half.extend([
                                DVec2::new(slot, 0.0),
                                DVec2::new(slot, o.f / 2.0),
                                DVec2::new(0.0, o.f / 2.0),
                            ]);
                        } else {
                            half.push(DVec2::ZERO);
                        }
                    }
                    half.push(DVec2::new(u, r.y));
                }
                None if !half.is_empty() => {
                    let u = half.last().unwrap().x;
                    half.push(DVec2::new(u, 0.0));
                    ended = true;
                    break 'sections;
                }
                None => {}
            }
        }
    }
    if !ended && let Some(&last) = half.last() {
        if p.nose {
            half.push(DVec2::new(last.x, 0.0));
        } else {
            half.extend([
                DVec2::new(last.x, o.f / 2.0),
                DVec2::new(last.x - slot, o.f / 2.0),
                DVec2::new(last.x - slot, 0.0),
            ]);
        }
    }
    // Lightening holes: 2w from each end, w between, inset w from the body edge and from the centreline.
    let mut holes = Vec::new();
    let origin = x1.unwrap_or(0.0);
    let (start, len) = (p.bounds[0], p.bounds[5] - p.bounds[0]);
    let span = (len - 5.0 * o.w) / 2.0;
    if span > 0.0 && x1.is_some() {
        for a in [start + 2.0 * o.w, start + 3.0 * o.w + span] {
            let pts: Vec<DVec2> = steps(a, a + span, 5.0)
                .into_iter()
                .map(|x0| s.rail(x0, o.d + o.f + o.w))
                .collect();
            let valid: Vec<bool> = pts.iter().map(|r| s.inside(*r) && r.y > o.w).collect();
            if let Some((i0, i1)) = first_run(&valid)
                && i1 > i0
            {
                let edge: Vec<DVec2> = pts[i0..=i1]
                    .iter()
                    .map(|r| DVec2::new((r.x - origin) / cos, r.y))
                    .collect();
                let mut h = vec![DVec2::new(edge[0].x, o.w)];
                h.extend(edge.iter().copied());
                h.push(DVec2::new(edge[edge.len() - 1].x, o.w));
                holes.push(h);
            }
        }
    }
    (half, holes)
}

fn plate_drawing(board: &Board, opts: &HwsOptions, nose: bool) -> Drawing {
    let s = Surf::new(board);
    let o = Hws::new(opts);
    let mut d = Drawing::default();
    let plate = if nose {
        nose_plate(&s, &o)
    } else {
        tail_plate(&s, &o)
    };
    if let Some(p) = plate {
        let (half, holes) = plate_outline(&s, &o, &p);
        if half.len() >= 3 {
            let (lo, hi) = half
                .iter()
                .fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
            d.line(
                DVec2::new(lo - OVERRUN, 0.0),
                DVec2::new(hi + OVERRUN, 0.0),
                Style::GUIDE,
            );
            d.closed(mirror_closed(&half), Style::CUT);
            for h in holes {
                d.closed(
                    h.iter().map(|q| DVec2::new(q.x, -q.y)).collect(),
                    Style::CUT,
                );
                d.closed(h, Style::CUT);
            }
        }
    }
    let name = if nose { "Nose piece" } else { "Tail piece" };
    finish(
        d,
        &[format!("Board file: {} {name}", board_name(board))],
        HEADER,
    )
}

pub fn hws_tail_piece(board: &Board, opts: &HwsOptions, _fmt: &dyn Fn(f64) -> String) -> Drawing {
    plate_drawing(board, opts, false)
}

pub fn hws_nose_piece(board: &Board, opts: &HwsOptions, _fmt: &dyn Fn(f64) -> String) -> Drawing {
    plate_drawing(board, opts, true)
}

// ============================================================================================ chambered

/// Inner-face offsets (cm from the centre) of the planks.
pub fn chambered_plank_offsets(board: &Board, opts: &ChamberedOptions) -> Vec<f64> {
    let half = board.max_width() / 2.0;
    let end = opts.end_offset.unwrap_or(half).min(half);
    let p = opts.plank_thickness;
    let mut out = vec![opts.start_offset];
    if p > 0.0 && p.is_finite() {
        let mut k = 1;
        loop {
            let y = opts.start_offset + k as f64 * p;
            if y >= end - 1e-9 || out.len() >= 1000 {
                break;
            }
            out.push(y);
            k += 1;
        }
    }
    out
}

/// The plank whose inner face is at `offset` (cm from the centre).
pub fn chambered_plank(
    board: &Board,
    offset: f64,
    opts: &ChamberedOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    let s = Surf::new(board);
    let y0 = offset * 10.0;
    let p = opts.plank_thickness * 10.0;
    let ts = opts.shell_thickness * 10.0;
    let y1 = y0 + p;
    let mut d = Drawing::default();
    baseline(&mut d, 0.0, s.len);
    if let Some(poly) = closed_profile(&s, y0, 0.0, false, 0.0, s.len) {
        d.closed(poly, Style::CUT);
    }
    if opts.chambering {
        if let Some(poly) = closed_profile(&s, y1, 0.0, false, 0.0, s.len) {
            d.closed(poly, OUTER_FACE);
        }
        // Chambers leave `ts` of wood above and below on both faces of the plank (vertical offsets).
        let k = (s.len / FT).floor() as usize;
        for i in 0..2 * k {
            let a = (i + 1) as f64 * FT / 2.0 + 10.0;
            let h = hole(a, FT / 2.0 - 20.0, true, |x| {
                let (t1, b1) = s.heights(x, y1)?;
                let (t0, b0) = s.heights(x, y0)?;
                Some((t1.min(t0) - ts, b1.max(b0) + ts))
            });
            if let Some(h) = h {
                d.closed(h, Style::CUT);
            }
        }
    }
    if opts.alignment_marks {
        let k = (s.len / FT).floor() as usize;
        for i in 1..k {
            let x = i as f64 * FT;
            let Some((top, bot)) = s.core(x, y0, ts) else {
                continue;
            };
            if top < bot || top - bot < 10.0 {
                continue;
            }
            let za = s.apex(x);
            if top - za < 5.0 || za - bot < 5.0 {
                continue;
            }
            d.line(DVec2::new(x - 5.0, za), DVec2::new(x + 5.0, za), MARK);
            d.line(DVec2::new(x, za - 5.0), DVec2::new(x, za + 5.0), MARK);
        }
    }
    if opts.grid {
        grid_over_geometry(&mut d, is_imperial(fmt));
    }
    finish(
        d,
        &[
            format!("Board file: {} Profile", board_name(board)),
            format!("Plank at {}", fmt(offset)),
        ],
        HEADER,
    )
}

/// One template per plank.
pub fn chambered_planks(
    board: &Board,
    opts: &ChamberedOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Vec<Drawing> {
    chambered_plank_offsets(board, opts)
        .into_iter()
        .map(|y| chambered_plank(board, y, opts, fmt))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_sections_add_up() {
        let b = plate_bounds(35.0, 228.6);
        assert!((b[1] - b[0] - IN).abs() < 1e-9 && (b[5] - b[4] - IN).abs() < 1e-9);
        assert_eq!(b[5], 228.6);
        // Short pieces: five equal sections totalling one piece length (D8).
        let b = plate_bounds(0.0, 100.0);
        assert!((b[3] - 60.0).abs() < 1e-9 && b[5] == 100.0);
    }

    #[test]
    fn first_run_skips_leading_invalid() {
        assert_eq!(first_run(&[false, true, true, false, true]), Some((1, 2)));
        assert_eq!(first_run(&[false, false]), None);
    }

    #[test]
    fn stack_marks_each_drawing_as_a_piece() {
        let a = Drawing::new(100.0, 50.0);
        let b = Drawing::new(80.0, 30.0);
        let s = stack(&[a, b], 10.0);
        assert_eq!((s.width, s.height), (100.0, 90.0));
        assert_eq!(s.parts.len(), 2);
        assert_eq!(s.parts[0], (DVec2::new(0.0, 40.0), DVec2::new(100.0, 90.0)));
        assert_eq!(s.parts[1], (DVec2::ZERO, DVec2::new(80.0, 30.0)));
    }
}
