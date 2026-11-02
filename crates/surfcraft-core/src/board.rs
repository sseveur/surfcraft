//! The board: outline, rocker and deck curves, cross-sections, fins and metadata, with the measurements shown to
//! the user.

use glam::DVec2;

use crate::knot::{Handle, Locks};
use crate::math::{ZERO, simpson, simpson_par};
use crate::section::CrossSection;
use crate::spline::Spline;
use crate::units::FOOT;

/// How the 3-D surface between cross-sections is built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SurfaceModel {
    /// Each neighbouring section is scaled to the local width and thickness and the two shapes are blended
    /// linearly by position (SurfCraft's default).
    #[default]
    SBlend,
    /// The two neighbouring sections are matched knot by knot and their control points are interpolated.
    ControlPoint,
}

/// Descriptive text of a board.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardInfo {
    pub name: String,
    pub author: String,
    pub designer: String,
    pub surfer: String,
    pub model: String,
    pub description: String,
    pub comments: String,
    pub fin_type: String,
    /// Path of the foam blank file used for machining.
    pub blank_file: String,
    /// Format version string carried by `.brd` files.
    pub version: String,
    /// Identifiers kept for external board databases.
    pub aux1: String,
    pub aux2: String,
    pub aux3: String,
}

impl Default for BoardInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            author: String::new(),
            designer: String::new(),
            surfer: String::new(),
            model: String::new(),
            description: String::new(),
            comments: String::new(),
            fin_type: String::new(),
            blank_file: String::new(),
            version: "V4.4".into(),
            aux1: String::new(),
            aux2: String::new(),
            aux3: String::new(),
        }
    }
}

/// Machining parameters stored in `.brd` files. SurfCraft does not interpret most of them; they are kept so
/// that files round-trip unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineData {
    pub deck_cuts: i32,
    pub bottom_cuts: i32,
    pub rail_cuts: i32,
    pub cutter_diameter: f64,
    pub blank_pivot: f64,
    pub board_pivot: f64,
    /// Deck-rail cut angle, degrees.
    pub max_angle: f64,
    pub nose_margin: f64,
    pub tail_margin: f64,
    pub nose_length: f64,
    pub tail_length: f64,
    pub delta_x_nose: f64,
    pub delta_x_tail: f64,
    pub delta_x_middle: f64,
    pub speed_to_tail: i32,
    pub stringer_speed: i32,
    pub stringer_speed_bottom: i32,
    pub regular_speed: i32,
    /// Top shoulder angle, degrees.
    pub top_shoulder_angle: f64,
    pub top_shoulder_cuts: i32,
    pub bottom_rail_cuts: i32,
    /// Supports: position, height, third value.
    pub strut1: [f64; 3],
    pub strut2: [f64; 3],
    pub cutter_start: [f64; 3],
    pub blank_tail: [f64; 3],
    pub board_start: [f64; 3],
    /// Display unit code written by other programs (opaque).
    pub unit_code: i32,
    pub security_level: i32,
    pub show_original: bool,
    pub nose_rocker_one_foot: f64,
    pub tail_rocker_one_foot: f64,
    pub output_folder: String,
}

impl Default for MachineData {
    fn default() -> Self {
        Self {
            deck_cuts: 0,
            bottom_cuts: 0,
            rail_cuts: 0,
            cutter_diameter: 0.0,
            blank_pivot: 0.0,
            board_pivot: 0.0,
            max_angle: 0.0,
            nose_margin: 0.0,
            tail_margin: 0.0,
            nose_length: 0.0,
            tail_length: 0.0,
            delta_x_nose: 0.0,
            delta_x_tail: 0.0,
            delta_x_middle: 0.0,
            speed_to_tail: 0,
            stringer_speed: 0,
            stringer_speed_bottom: 0,
            regular_speed: 0,
            top_shoulder_angle: 0.0,
            top_shoulder_cuts: 0,
            bottom_rail_cuts: 0,
            strut1: [0.0; 3],
            strut2: [0.0; 3],
            cutter_start: [0.0; 3],
            blank_tail: [0.0; 3],
            board_start: [0.0; 3],
            unit_code: 0,
            security_level: 0,
            show_original: true,
            nose_rocker_one_foot: 0.0,
            tail_rocker_one_foot: 0.0,
            output_folder: "c:\\machine".into(),
        }
    }
}

/// Indices into [`Board::fins`].
pub mod fin {
    pub const SIDE_REAR_X: usize = 0;
    pub const SIDE_REAR_Y: usize = 1;
    pub const SIDE_FRONT_X: usize = 2;
    pub const SIDE_FRONT_Y: usize = 3;
    pub const CENTER_REAR_X: usize = 4;
    pub const CENTER_FRONT_X: usize = 5;
    pub const CENTER_DEPTH: usize = 6;
    pub const SIDE_DEPTH: usize = 7;
    /// Side fin splay, degrees.
    pub const SPLAY: usize = 8;
}

/// Smallest distance (cm) kept between two cross-sections, and between a section and the board's ends.
pub const MIN_SECTION_GAP: f64 = 0.5;

/// Main dimensions, computed together (e.g. on a background thread).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Specs {
    pub length: f64,
    pub length_over_curve: f64,
    pub max_width: f64,
    pub max_width_pos: f64,
    pub center_width: f64,
    pub max_thickness: f64,
    pub max_thickness_pos: f64,
    pub center_thickness: f64,
    pub nose_rocker: f64,
    pub tail_rocker: f64,
    /// Planform area, cm².
    pub area: f64,
    /// Volume, cm³.
    pub volume: f64,
}

/// A surfboard in the Bezier representation. Lengths in cm; x runs from the tail (0) to the nose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    /// Half-width as a function of x.
    pub outline: Spline,
    /// Height of the bottom on the stringer (rocker).
    pub bottom: Spline,
    /// Height of the deck on the stringer.
    pub deck: Spline,
    pub outline_guides: Vec<DVec2>,
    pub bottom_guides: Vec<DVec2>,
    pub deck_guides: Vec<DVec2>,
    /// Sorted by position; the first and last entries are zero-size end sections at the tail and the nose.
    pub sections: Vec<CrossSection>,
    /// Fin marks, see [`fin`].
    pub fins: [f64; 9],
    /// The fin setup with boxes (`.brd` p60); `None` for boards that only have the legacy marks in `fins`.
    /// Kept in step with `fins` by [`Board::set_fin_setup`].
    pub fin_setup: Option<crate::fins::FinSetup>,
    pub info: BoardInfo,
    pub machine: MachineData,
    pub model: SurfaceModel,
    /// Foam blank chosen for this board and where the board sits in it (SurfCraft extension, `.brd` p63).
    pub blank: Option<crate::blank::BlankSetup>,
}

/// `x.clamp(lo, hi)` that tolerates an empty range (returns its middle) and NaN (returns `lo`).
pub(crate) fn clamp_soft(x: f64, lo: f64, hi: f64) -> f64 {
    if lo > hi {
        (lo + hi) / 2.0
    } else if x.is_nan() {
        lo
    } else {
        x.clamp(lo, hi)
    }
}

/// Upper bound on the samples of a metric search, so huge or degenerate boards stay responsive.
const MAX_SAMPLES: usize = 10_000;

impl Board {
    // ------------------------------------------------------------------------------------ primitive queries

    /// Board length: the largest x among the outline knot points (handles ignored).
    pub fn length(&self) -> f64 {
        let l = self
            .outline
            .knots()
            .iter()
            .map(|k| k.point().x)
            .fold(f64::NEG_INFINITY, f64::max);
        if l.is_finite() { l } else { 0.0 }
    }

    /// A board without an outline.
    pub fn is_empty(&self) -> bool {
        self.outline.is_empty()
    }

    /// Arc length of the bottom curve.
    pub fn length_over_curve(&self) -> f64 {
        self.bottom.length()
    }

    pub fn width_at(&self, x: f64) -> f64 {
        2.0 * self.outline.value_at(x)
    }

    /// Height of the bottom on the stringer.
    pub fn rocker_at(&self, x: f64) -> f64 {
        self.bottom.value_at(x)
    }

    /// Height of the deck on the stringer.
    pub fn deck_at(&self, x: f64) -> f64 {
        self.deck.value_at(x)
    }

    /// Thickness on the stringer.
    pub fn thickness_at(&self, x: f64) -> f64 {
        self.deck_at(x) - self.rocker_at(x)
    }

    pub fn max_width(&self) -> f64 {
        2.0 * self.outline.max_y()
    }

    /// Distance of the wide point from the tail.
    pub fn max_width_pos(&self) -> f64 {
        self.outline.x_of_max_y()
    }

    /// Largest stringer thickness and its distance from the tail, sampled every millimetre (at most
    /// 10 000 samples).
    pub fn max_thickness_and_pos(&self) -> (f64, f64) {
        let l = self.length();
        let (count, step) = if l * 10.0 > MAX_SAMPLES as f64 {
            (MAX_SAMPLES, l / MAX_SAMPLES as f64)
        } else {
            ((l * 10.0).floor().max(0.0) as usize, 0.1)
        };
        let mut best: Option<(f64, f64)> = None;
        for i in 0..count {
            let x = if step == 0.1 {
                i as f64 / 10.0
            } else {
                i as f64 * step
            };
            let t = self.thickness_at(x);
            if best.is_none_or(|b| t > b.0) && !t.is_nan() {
                best = Some((t, x));
            }
        }
        best.unwrap_or((0.0, 0.0))
    }

    pub fn max_thickness(&self) -> f64 {
        self.max_thickness_and_pos().0
    }

    pub fn center_width(&self) -> f64 {
        self.width_at(self.length() / 2.0)
    }

    pub fn center_thickness(&self) -> f64 {
        self.thickness_at(self.length() / 2.0)
    }

    /// Highest point of the bottom curve.
    pub fn max_rocker(&self) -> f64 {
        self.bottom.max_y()
    }

    /// Distance from the tail measured along the bottom curve to the station `x`.
    pub fn from_tail_over_bottom(&self, x: f64) -> f64 {
        self.bottom.length_to_x(x)
    }

    /// Distance from the nose measured along the bottom curve to the station `x`.
    pub fn from_nose_over_bottom(&self, x: f64) -> f64 {
        self.bottom.length() - self.bottom.length_to_x(x)
    }

    /// Station whose distance from the tail along the bottom is `d`.
    pub fn x_at_distance_over_bottom(&self, d: f64) -> f64 {
        self.bottom.point_at_length(d).x
    }

    /// Highest deck point within one foot of the tail, as `(x, height)`.
    pub fn max_deck_at_tail(&self) -> DVec2 {
        self.deck
            .max_y_in(0.0, FOOT)
            .unwrap_or(DVec2::new(0.0, self.deck_at(0.0)))
    }

    /// Highest deck point within one foot of the nose, as `(x, height)`.
    pub fn max_deck_at_nose(&self) -> DVec2 {
        let l = self.length();
        self.deck
            .max_y_in(l - FOOT, l)
            .unwrap_or(DVec2::new(l, self.deck_at(l)))
    }

    // ------------------------------------------------------------------------------------ sections

    /// Indices of the real (non-placeholder) cross-sections: everything but the first and last entry.
    pub fn real_sections(&self) -> std::ops::Range<usize> {
        let n = self.sections.len();
        if n == 0 { 0..0 } else { 1..(n - 1).max(1) }
    }

    /// Real section nearest to station `x` (earlier one on ties).
    pub fn nearest_section_index(&self, x: f64) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for k in self.real_sections() {
            let d = (self.sections[k].position - x).abs();
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, k));
            }
        }
        best.map(|b| b.1)
    }

    fn previous_index(&self, x: f64) -> Option<usize> {
        let k = self.nearest_section_index(x)?;
        Some(if self.sections[k].position >= x {
            k - 1
        } else {
            k
        })
    }

    fn next_index(&self, x: f64) -> Option<usize> {
        let k = self.nearest_section_index(x)?;
        Some(if self.sections[k].position < x {
            k + 1
        } else {
            k
        })
    }

    /// Position of the section (end sections included) at or before station `x`.
    pub fn previous_section_position(&self, x: f64) -> Option<f64> {
        self.previous_index(x).map(|k| self.sections[k].position)
    }

    /// Position of the section (end sections included) at or after station `x`.
    pub fn next_section_position(&self, x: f64) -> Option<f64> {
        self.next_index(x).map(|k| self.sections[k].position)
    }

    /// Real section shaping the board just behind station `x` (the first real one near the tail).
    pub fn previous_section_index(&self, x: f64) -> Option<usize> {
        let n = self.sections.len();
        self.previous_index(x).map(|k| k.clamp(1, n - 2))
    }

    /// Real section shaping the board just ahead of station `x` (the last real one near the nose).
    pub fn next_section_index(&self, x: f64) -> Option<usize> {
        let n = self.sections.len();
        self.next_index(x).map(|k| k.clamp(1, n - 2))
    }

    /// Cross-section at station `x` interpolated from its neighbours by knot matching, scaled to the local
    /// width and thickness. `None` without real sections or outside the board.
    pub fn interpolated_section(&self, x: f64) -> Option<CrossSection> {
        let n = self.sections.len();
        if n < 3 || x.is_nan() || x < 0.0 || x > self.length() {
            return None;
        }
        let mut k = self.nearest_section_index(x)?;
        if self.sections[k].position > x {
            k -= 1;
        }
        let k2 = k + 1;
        let (p1, p2) = (self.sections[k].position, self.sections[k2].position);
        let mut f = (x - p1) / (p2 - p1);
        if !f.is_finite() {
            f = 0.0;
        }
        let (mut k, mut k2) = (k.max(1), k2);
        if k2 > n - 2 {
            k = n - 2;
            k2 = k;
        }
        let spline = self.sections[k].blend(&self.sections[k2], f)?;
        let mut s = CrossSection::new(x, spline);
        s.scale_to(self.thickness_at(x).max(0.5), self.width_at(x).max(0.5));
        Some(s)
    }

    /// Whether a section other than `except` may sit at `x`: strictly inside the board and at least
    /// [`MIN_SECTION_GAP`] from every other section (end sections included).
    pub fn section_position_free(&self, except: Option<usize>, x: f64) -> bool {
        let l = self.length();
        x.is_finite()
            && x >= MIN_SECTION_GAP
            && x <= l - MIN_SECTION_GAP
            && self
                .sections
                .iter()
                .enumerate()
                .filter(|(i, _)| Some(*i) != except)
                .all(|(_, s)| (s.position - x).abs() >= MIN_SECTION_GAP)
    }

    /// Sort the sections by position (stable), keeping the end sections first and last.
    fn sort_sections(&mut self) {
        self.sections
            .sort_by(|a, b| a.position.total_cmp(&b.position));
    }

    /// Index of a section after sorting, found by identity of position and shape.
    fn index_of(&self, s: &CrossSection) -> usize {
        self.sections
            .iter()
            .position(|t| t.position == s.position && t.spline == s.spline)
            .unwrap_or(0)
    }

    /// Add a section at `x`, shaped by interpolation of its neighbours. Returns its index, or `None` when the
    /// position is not free (see [`Self::section_position_free`]).
    pub fn add_section(&mut self, x: f64) -> Option<usize> {
        if !self.section_position_free(None, x) {
            return None;
        }
        let mut s = match self.interpolated_section(x) {
            Some(s) => s,
            None => {
                let mut s = CrossSection::new(x, CrossSection::fallback_shape());
                s.scale_to(self.thickness_at(x).max(0.5), self.width_at(x).max(0.5));
                s
            }
        };
        s.position = x;
        s.guide_points.clear();
        if self.sections.len() < 2 {
            self.ensure_end_sections();
        }
        let at = self
            .sections
            .len()
            .saturating_sub(1)
            .max(1)
            .min(self.sections.len());
        self.sections.insert(at, s.clone());
        self.sort_sections();
        Some(self.index_of(&s))
    }

    /// Remove real section `i`; refused for the last remaining real section.
    pub fn remove_section(&mut self, i: usize) -> bool {
        let r = self.real_sections();
        if !r.contains(&i) || r.len() <= 1 {
            return false;
        }
        self.sections.remove(i);
        true
    }

    /// Move real section `i` to `x` and re-fit it there. Returns the section's new index, or `None` when the
    /// position is not free.
    pub fn move_section(&mut self, i: usize, x: f64) -> Option<usize> {
        if !self.real_sections().contains(&i) || !self.section_position_free(Some(i), x) {
            return None;
        }
        let mut s = self.sections.remove(i);
        s.position = x;
        s.scale_to(self.thickness_at(x), self.width_at(x));
        self.sections.insert(i, s.clone());
        self.sort_sections();
        Some(self.index_of(&s))
    }

    /// Make sure the zero-size end sections exist at the tail and the nose.
    pub fn ensure_end_sections(&mut self) {
        let l = self.length();
        if self
            .sections
            .first()
            .is_none_or(|s| !(s.position <= 0.0 && s.is_end_section()))
        {
            self.sections.insert(0, CrossSection::end(0.0));
        }
        if self
            .sections
            .last()
            .is_none_or(|s| !(s.position >= l && s.is_end_section()))
        {
            self.sections.push(CrossSection::end(l));
        }
    }

    /// Scale every real section to the outline width and stringer thickness at its position.
    pub fn fit_sections(&mut self) {
        for k in self.real_sections() {
            let x = self.sections[k].position;
            let (t, w) = (self.thickness_at(x), self.width_at(x));
            self.sections[k].scale_to(t, w);
        }
    }

    // ------------------------------------------------------------------------------------ constraints

    /// Install the editing constraints: fixed outline end points, deck and bottom end points movable
    /// vertically only and shared between the two curves, handles that may not point backwards, fixed section
    /// end points. The bottom's end points are snapped onto the deck's.
    pub fn apply_locks(&mut self) {
        if self.outline.len() < 2 {
            return;
        }
        let ends = |s: &mut Spline, axis: DVec2, first: Locks, last: Locks, every: [Locks; 2]| {
            let n = s.len();
            for (i, k) in s.knots_mut().iter_mut().enumerate() {
                k.locks = every;
                k.axis = DVec2::ONE;
                if i == 0 || i + 1 == n {
                    k.axis = axis;
                }
                if i == 0 {
                    k.locks[1] = k.locks[1] | first;
                }
                if i + 1 == n {
                    k.locks[0] = k.locks[0] | last;
                }
            }
        };
        let curve = [Locks::X_NOT_MORE, Locks::X_NOT_LESS];
        ends(
            &mut self.outline,
            DVec2::ZERO,
            Locks::Y_NOT_LESS,
            Locks::Y_NOT_LESS,
            curve,
        );
        let vertical = DVec2::new(0.0, 1.0);
        ends(&mut self.deck, vertical, Locks::NONE, Locks::NONE, curve);
        ends(&mut self.bottom, vertical, Locks::NONE, Locks::NONE, curve);
        for s in &mut self.sections {
            let n = s.spline.len();
            for (i, k) in s.spline.knots_mut().iter_mut().enumerate() {
                k.locks = [Locks::NONE; 2];
                k.axis = DVec2::ONE;
                if i == 0 || i + 1 == n {
                    k.axis = DVec2::ZERO;
                }
                if i == 0 {
                    k.locks[1] = Locks::X_NOT_LESS;
                }
                if i + 1 == n && n > 1 {
                    k.locks[0] = Locks::X_NOT_LESS;
                }
            }
        }
        // Tie the deck's and the bottom's end points together.
        if !self.deck.is_empty() && !self.bottom.is_empty() {
            for (di, bi) in [(0, 0), (self.deck.len() - 1, self.bottom.len() - 1)] {
                let p = self.deck.knots()[di].point();
                let k = &mut self.bottom.knots_mut()[bi];
                let d = p - k.point();
                if d != DVec2::ZERO {
                    k.translate(d);
                }
            }
        }
    }

    /// Clear the `continuous` flag of knots whose handles are not opposite each other; with `both_ways`, also
    /// set it where they are.
    pub fn normalize_continuity(&mut self, both_ways: bool) {
        let fix = |s: &mut Spline| {
            for k in s.knots_mut() {
                let c = k.is_geometrically_continuous();
                if !c {
                    k.continuous = false;
                } else if both_ways {
                    k.continuous = true;
                }
            }
        };
        fix(&mut self.outline);
        fix(&mut self.deck);
        fix(&mut self.bottom);
        for s in &mut self.sections {
            fix(&mut s.spline);
        }
    }

    /// Index of the knot tied to knot `i` of the deck (`deck = true`) or the bottom: the end knots of the two
    /// curves move together.
    pub fn partner(&self, deck: bool, i: usize) -> Option<usize> {
        let (this, other) = if deck {
            (&self.deck, &self.bottom)
        } else {
            (&self.bottom, &self.deck)
        };
        if this.is_empty() || other.is_empty() {
            return None;
        }
        if i == 0 {
            Some(0)
        } else if i + 1 == this.len() {
            Some(other.len() - 1)
        } else {
            None
        }
    }

    // ------------------------------------------------------------------------------------ integrals

    /// Planform area (cm²).
    pub fn area(&self) -> f64 {
        let l = self.length();
        simpson(|x| self.width_at(x), ZERO, l - ZERO, 10)
    }

    /// Volume (cm³) by integrating the section area along the board.
    pub fn volume(&self) -> f64 {
        if self.sections.len() < 3 {
            return 0.0;
        }
        let l = self.length();
        let v = simpson_par(|x| self.section_area_at(x, 10), 0.01, l - 0.01, 30);
        if v.is_finite() { v } else { 0.0 }
    }

    /// Distance of the centre of volume from the tail (cm); 0 without sections.
    pub fn center_of_mass(&self) -> f64 {
        if self.sections.len() < 3 {
            return 0.0;
        }
        let (a, b) = (0.01, self.length() - 0.01);
        let h = (b - a) / 10.0;
        let mut xs = Vec::with_capacity(21);
        let mut p = a;
        for _ in 0..10 {
            xs.push(p);
            xs.push(p + h / 2.0);
            p += h;
        }
        xs.push(p);
        let areas = self.areas_at(&xs);
        let (mut moment, mut mass) = (0.0, 0.0);
        for k in 0..10 {
            let (f0, f1, f2) = (areas[2 * k], areas[2 * k + 1], areas[2 * k + 2]);
            let i = (h / 6.0) * (f0 + 4.0 * f1 + f2);
            moment += (xs[2 * k] + h / 2.0) * i;
            mass += i;
        }
        let c = moment / mass;
        if c.is_finite() { c } else { 0.0 }
    }

    /// Section areas at several stations, computed in parallel; NaN counts as 0.
    fn areas_at(&self, xs: &[f64]) -> Vec<f64> {
        use rayon::prelude::*;
        xs.par_iter()
            .map(|&x| {
                let a = self.section_area_at(x, 10);
                if a.is_nan() { 0.0 } else { a }
            })
            .collect()
    }

    /// Moment of inertia (kg·m²) about the point `(x0, y0)` of the outline plane, for a uniform density of
    /// 0.1 kg/l (a 3 kg, 30 l board).
    pub fn moment_of_inertia(&self, x0: f64, y0: f64) -> f64 {
        if self.sections.len() < 3 {
            return 0.0;
        }
        let (a, b) = (0.01, self.length() - 0.01);
        let h = (b - a) / 30.0;
        let mut starts = Vec::with_capacity(30);
        let mut p = a;
        for _ in 0..30 {
            starts.push(p);
            p += h;
        }
        let mut xs = Vec::with_capacity(90);
        for &p in &starts {
            xs.extend([p, (p + (p + h)) / 2.0, p + h]);
        }
        let areas = self.areas_at(&xs);
        let mut inertia = 0.0;
        for (k, &p) in starts.iter().enumerate() {
            let (f0, f1, f2) = (areas[3 * k], areas[3 * k + 1], areas[3 * k + 2]);
            let litres = ((p + h - p) / 6.0) * (f0 + 4.0 * f1 + f2) / 1000.0;
            let r = ((p + h / 2.0 - x0).powi(2) + y0 * y0).sqrt() / 100.0;
            inertia += litres * 0.1 * r * r;
        }
        inertia
    }

    /// Section area at 21 evenly spaced stations (`i · L / 20`), for the volume distribution graph.
    pub fn volume_distribution(&self) -> Vec<(f64, f64)> {
        let l = self.length();
        let xs: Vec<f64> = (0..=20).map(|i| i as f64 * l / 20.0).collect();
        use rayon::prelude::*;
        xs.par_iter()
            .map(|&x| (x, self.section_area_at(x, 8)))
            .collect()
    }

    /// The main dimensions at once.
    pub fn specs(&self) -> Specs {
        let (max_thickness, max_thickness_pos) = self.max_thickness_and_pos();
        let l = self.length();
        Specs {
            length: l,
            length_over_curve: self.length_over_curve(),
            max_width: self.max_width(),
            max_width_pos: self.max_width_pos(),
            center_width: self.center_width(),
            max_thickness,
            max_thickness_pos,
            center_thickness: self.center_thickness(),
            nose_rocker: self.rocker_at(l - 0.005),
            tail_rocker: self.rocker_at(0.001),
            area: self.area(),
            volume: self.volume(),
        }
    }

    // ------------------------------------------------------------------------------------ operations

    /// Re-fit the sections after the outline changed.
    pub fn on_outline_changed(&mut self) {
        self.fit_sections();
    }

    /// After the deck or the bottom changed: put the lowest rocker point at zero and re-fit the sections.
    pub fn on_rocker_changed(&mut self) {
        self.zero_rocker();
        self.fit_sections();
    }

    /// Shift deck and bottom vertically so that the lowest point of the bottom is at height 0.
    pub fn zero_rocker(&mut self) {
        if self.bottom.segment_count() == 0 {
            return;
        }
        let m = self.bottom.min_y();
        if !m.is_finite() || m == 0.0 {
            return;
        }
        let d = DVec2::new(0.0, -m);
        self.bottom.translate(d);
        self.deck.translate(d);
    }

    /// Scale to a new length, maximum width and maximum thickness (rocker scaled with the thickness). Refused
    /// (board unchanged) when a factor is not a positive finite number.
    pub fn scale(&mut self, length: f64, width: f64, thickness: f64) {
        let kl = length / self.length();
        let kw = width / self.max_width();
        let kt = thickness / self.max_thickness();
        if ![kl, kw, kt].iter().all(|k| k.is_finite() && *k > 0.0) {
            return;
        }
        self.outline.scale(kl, kw);
        self.deck.scale(kl, kt);
        self.bottom.scale(kl, kt);
        self.scale_section_positions(kl, length);
        self.fit_sections();
    }

    fn scale_section_positions(&mut self, kl: f64, length: f64) {
        let n = self.sections.len();
        for (i, s) in self.sections.iter_mut().enumerate() {
            if i + 1 == n && n > 1 && s.is_end_section() {
                s.position = length;
            } else {
                s.position *= kl;
            }
        }
    }

    /// Scale to a new length, maximum width and maximum thickness keeping the bottom rocker's proportions to
    /// the length: the bottom scales with the length, the deck's interior knots are raised or lowered to give
    /// the new thickness, their handles turned to follow. Refused like [`Self::scale`].
    pub fn scale_preserving_rocker(&mut self, length: f64, width: f64, thickness: f64) {
        let l = self.length();
        let kl = length / l;
        let kw = width / self.max_width();
        let (t_old, xm) = self.max_thickness_and_pos();
        let kt = thickness / t_old;
        if ![kl, kw, kt].iter().all(|k| k.is_finite() && *k > 0.0) {
            return;
        }
        let dt = thickness - t_old;
        let n = self.deck.len();
        let above: Vec<f64> = self
            .deck
            .knots()
            .iter()
            .map(|k| {
                let x = clamp_soft(k.point().x, ZERO, l - ZERO);
                k.point().y - self.rocker_at(x)
            })
            .collect();
        self.outline.scale(kl, kw);
        self.deck.scale(kl, 1.0);
        self.bottom.scale(kl, kl);
        // Keep the deck's ends on the bottom's ends.
        for (di, bi) in [
            (0usize, 0usize),
            (n.saturating_sub(1), self.bottom.len().saturating_sub(1)),
        ] {
            if n == 0 || self.bottom.is_empty() {
                break;
            }
            let p = self.bottom.knots()[bi].point();
            let k = &mut self.deck.knots_mut()[di];
            let d = p - k.point();
            k.translate(d);
        }
        let phi = dt.atan2(xm);
        let l_new = self.length();
        for (i, &above) in above.iter().enumerate().take(n.saturating_sub(1)).skip(1) {
            let x = clamp_soft(self.deck.knots()[i].point().x, ZERO, l_new - ZERO);
            let rocker = self.rocker_at(x);
            let k = &mut self.deck.knots_mut()[i];
            let dy = above * kt - (k.point().y - rocker);
            k.translate(DVec2::new(0.0, dy));
            let turn = if xm != 0.0 { phi * (xm - x) / xm } else { 0.0 };
            k.rotate_handle(Handle::Next, turn);
            k.rotate_handle(Handle::Prev, turn);
        }
        self.scale_section_positions(kl, length);
        self.fit_sections();
    }

    /// Scale the fin marks: positions along the board by `rx`, offsets from the stringer by `ry`.
    pub fn scale_fins(&mut self, rx: f64, ry: f64) {
        if !rx.is_finite() || !ry.is_finite() {
            return;
        }
        for i in [
            fin::SIDE_REAR_X,
            fin::SIDE_FRONT_X,
            fin::CENTER_REAR_X,
            fin::CENTER_FRONT_X,
        ] {
            self.fins[i] *= rx;
        }
        for i in [fin::SIDE_REAR_Y, fin::SIDE_FRONT_Y] {
            self.fins[i] *= ry;
        }
        if let Some(mut s) = self.fin_setup.take() {
            s.scale(rx, ry);
            self.set_fin_setup(s);
        }
    }

    /// Swap nose and tail: mirror every curve, section position, guide point and fin mark along the length.
    /// Flipping twice gives back the same board.
    pub fn flip(&mut self) {
        let l = self.length();
        let mirror = |p: DVec2| DVec2::new(l - p.x, p.y);
        for s in [&mut self.outline, &mut self.deck, &mut self.bottom] {
            let knots = s.knots_mut();
            knots.reverse();
            for k in knots.iter_mut() {
                let [p, prev, next] = k.points;
                k.points = [mirror(p), mirror(next), mirror(prev)];
                k.locks.swap(0, 1);
            }
        }
        for g in [
            &mut self.outline_guides,
            &mut self.deck_guides,
            &mut self.bottom_guides,
        ] {
            for p in g.iter_mut() {
                *p = mirror(*p);
            }
        }
        self.sections.reverse();
        for s in &mut self.sections {
            s.position = l - s.position;
        }
        // Fin marks that are set (non-zero) are mirrored; the rear mark stays the one nearer the tail.
        let f = self.fins;
        let fx = |v: f64| l - v;
        if f[fin::SIDE_REAR_X] != 0.0 || f[fin::SIDE_FRONT_X] != 0.0 {
            self.fins[fin::SIDE_REAR_X] = fx(f[fin::SIDE_FRONT_X]);
            self.fins[fin::SIDE_REAR_Y] = f[fin::SIDE_FRONT_Y];
            self.fins[fin::SIDE_FRONT_X] = fx(f[fin::SIDE_REAR_X]);
            self.fins[fin::SIDE_FRONT_Y] = f[fin::SIDE_REAR_Y];
        }
        if f[fin::CENTER_REAR_X] != 0.0 || f[fin::CENTER_FRONT_X] != 0.0 {
            self.fins[fin::CENTER_REAR_X] = fx(f[fin::CENTER_FRONT_X]);
            self.fins[fin::CENTER_FRONT_X] = fx(f[fin::CENTER_REAR_X]);
        }
        if let Some(mut s) = self.fin_setup.take() {
            s.flip(l);
            self.set_fin_setup(s);
        }
        self.apply_locks();
    }
}
