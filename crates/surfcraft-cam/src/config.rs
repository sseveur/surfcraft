//! Machine settings (`cam.md` §3) and their validation (§3.14).

use std::sync::Arc;

use surfcraft_core::{Board, DVec3};

use crate::Error;
use crate::cutter::StlCutter;

/// Which side of the board a program machines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoardSide {
    #[default]
    Deck,
    Bottom,
}

/// Toolpath strategy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum GeneratorKind {
    /// Lengthwise passes spaced by cross-section angle bands (the main generator).
    #[default]
    SurfaceSplits,
    /// Simple raster of `splits` passes per side across the width.
    WidthSplits { splits: usize },
}

/// Action run before or after a program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MachineAction {
    Nothing,
    #[default]
    Home,
    HomeRef,
    /// Lift the tool by `MachineConfig::clear_height`.
    LiftClear,
    /// G-code text written verbatim.
    Script(String),
}

/// End of the board where the deck program starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeckStart {
    Nose,
    #[default]
    Tail,
}

/// End of the board where the bottom program starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BottomStart {
    #[default]
    Nose,
    Tail,
    /// Where the deck program finished.
    Previous,
}

/// Height of the deck program's outline pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutlineDepth {
    /// At the rail apex.
    #[default]
    RailApex,
    /// At the bottom band's outer angle.
    Bottom,
}

/// Bullnose end mill (lengths in cm).
#[derive(Clone, Debug, PartialEq)]
pub struct BullnoseCutter {
    pub diameter: f64,
    pub corner_radius: f64,
    /// Length of the cutting part above the tip.
    pub height: f64,
    /// Diameter of the shank above the cutting part. Collision checks treat the tool as an unbounded cylinder of
    /// the larger of the two diameters.
    pub shank_diameter: f64,
}

impl Default for BullnoseCutter {
    fn default() -> Self {
        BullnoseCutter {
            diameter: 2.54,
            corner_radius: 0.5,
            height: 6.5,
            shank_diameter: 2.54,
        }
    }
}

/// Flat end mill (lengths in cm).
#[derive(Clone, Debug, PartialEq)]
pub struct FlatCutter {
    pub radius: f64,
    pub height: f64,
}

impl Default for FlatCutter {
    fn default() -> Self {
        FlatCutter {
            radius: 1.27,
            height: 6.5,
        }
    }
}

/// Cutter model.
#[derive(Clone, Debug, PartialEq)]
pub enum Cutter {
    Bullnose(BullnoseCutter),
    Flat(FlatCutter),
    Stl(Arc<StlCutter>),
    /// 4-axis machine turning the board about X, with the inner cutter.
    RotatingBoard(Box<Cutter>),
}

impl Default for Cutter {
    fn default() -> Self {
        Cutter::Bullnose(BullnoseCutter::default())
    }
}

/// Two supports and a tail stop (mm, measured from the tail stop).
#[derive(Clone, Debug, PartialEq)]
pub struct SupportsConfig {
    /// X of the tail-stop plane.
    pub tailstop_pos: f64,
    pub support1_from_tailstop: f64,
    pub support2_from_tailstop: f64,
    /// Z of the top of support 1.
    pub support1_height: f64,
    pub support2_height: f64,
    /// Extent of each support post along X.
    pub support_length: f64,
    /// Height of the tail stop above the top of support 1 while machining, mm; 0 when the stop only positions the
    /// blank and is folded away (or below the blank) during the cut. The tool avoids it when it is above 0.
    pub tailstop_height: f64,
}

impl Default for SupportsConfig {
    fn default() -> Self {
        SupportsConfig {
            tailstop_pos: 1000.0,
            support1_from_tailstop: 600.0,
            support2_from_tailstop: 1600.0,
            support1_height: 260.0,
            support2_height: 300.0,
            support_length: 50.0,
            tailstop_height: 0.0,
        }
    }
}

/// Nose and tail clamps (cm).
#[derive(Clone, Debug, PartialEq)]
pub struct EndClampsConfig {
    pub clamp_length: f64,
    pub clamp_height: f64,
    pub clamp_width: f64,
    pub clamp_safe_zone: f64,
    pub clamp_overlap: f64,
    pub blank_vertical_offset: f64,
}

impl Default for EndClampsConfig {
    fn default() -> Self {
        EndClampsConfig {
            clamp_length: 80.0,
            clamp_height: 20.0,
            clamp_width: 20.0,
            clamp_safe_zone: 5.0,
            clamp_overlap: 8.0,
            blank_vertical_offset: 10.0,
        }
    }
}

/// How the blank is held on the machine.
#[derive(Clone, Debug, PartialEq)]
pub enum HoldingSystem {
    Supports(SupportsConfig),
    EndClamps(EndClampsConfig),
    RotatingBoard(EndClampsConfig),
}

impl Default for HoldingSystem {
    fn default() -> Self {
        HoldingSystem::Supports(SupportsConfig::default())
    }
}

/// Scale factors of the output words.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisScaling {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub f: f64,
}

impl Default for AxisScaling {
    fn default() -> Self {
        AxisScaling {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            a: 1.0,
            f: 1.0,
        }
    }
}

/// Undersize machining of a sandwich core (thicknesses and limits in cm).
#[derive(Clone, Debug, PartialEq)]
pub struct SandwichCompensation {
    pub deck: bool,
    pub deck_thickness: f64,
    /// No deck compensation within this plan distance of the outline (0 = off).
    pub deck_rail_limit: f64,
    pub bottom: bool,
    pub bottom_thickness: f64,
    pub bottom_rail_limit: f64,
    pub rail: bool,
    pub rail_thickness: f64,
}

impl Default for SandwichCompensation {
    fn default() -> Self {
        SandwichCompensation {
            deck: false,
            deck_thickness: 0.3,
            deck_rail_limit: 0.0,
            bottom: false,
            bottom_thickness: 0.3,
            bottom_rail_limit: 0.0,
            rail: false,
            rail_thickness: 0.3,
        }
    }
}

/// Milling job settings (lengths in cm, speeds in mm/s, angles in degrees).
#[derive(Clone, Debug)]
pub struct MachineConfig {
    pub generator: GeneratorKind,
    /// Use all CPU cores (the output is the same either way).
    pub parallel: bool,
    pub lengthwise_resolution: f64,
    pub deck_cuts: usize,
    pub deck_rail_cuts: usize,
    pub bottom_cuts: usize,
    pub bottom_rail_cuts: usize,
    pub deck_cleanup_cuts: usize,
    pub bottom_cleanup_cuts: usize,
    pub deck_stringer_cut: bool,
    pub bottom_stringer_cut: bool,
    pub stay_away_from_stringer: bool,
    /// Clearance kept from the stringer plane on each side when staying away from it.
    pub stringer_width: f64,
    pub deck_outline_cut: bool,
    pub deck_outline_depth: OutlineDepth,
    pub bottom_outline_cut: bool,
    pub cut_off_nose: bool,
    pub cut_off_tail: bool,
    pub deck_angle: f64,
    pub deck_rail_angle: f64,
    pub bottom_angle: f64,
    pub bottom_rail_angle: f64,
    /// Transition steps between band passes.
    pub move_between_steps: usize,
    pub side_change_steps: usize,
    pub cutting_speed: f64,
    pub stringer_speed: f64,
    pub rail_speed: f64,
    pub outline_speed: f64,
    /// Plunge feed as a fraction of the pass feed.
    pub plunge_speed_factor: f64,
    pub before_deck_cut: MachineAction,
    pub after_deck_cut: MachineAction,
    pub before_bottom_cut: MachineAction,
    pub after_bottom_cut: MachineAction,
    pub deck_start: DeckStart,
    pub bottom_start: BottomStart,
    pub clear_height: f64,
    /// Program zero in the reference frame.
    pub cutter_offset: DVec3,
    pub cutter: Cutter,
    pub holding: HoldingSystem,
    /// The foam blank, when one is loaded.
    pub blank: Option<Board>,
    pub scaling: AxisScaling,
    pub sandwich: SandwichCompensation,
    /// Largest allowed penetration of the tool into the board, mm.
    pub gouge_tolerance: f64,
    /// Clearance above the highest obstacle or toolpath point for retracts, mm.
    pub retract_clearance: f64,
    /// Largest horizontal spacing of the checks along a straight move, mm.
    pub linking_spacing: f64,
    /// Largest change of the A axis between two output points while cutting, degrees.
    pub max_a_step: f64,
}

impl Default for MachineConfig {
    fn default() -> Self {
        MachineConfig {
            generator: GeneratorKind::SurfaceSplits,
            parallel: true,
            lengthwise_resolution: 1.0,
            deck_cuts: 28,
            deck_rail_cuts: 7,
            bottom_cuts: 26,
            bottom_rail_cuts: 0,
            deck_cleanup_cuts: 2,
            bottom_cleanup_cuts: 2,
            deck_stringer_cut: true,
            bottom_stringer_cut: true,
            stay_away_from_stringer: true,
            stringer_width: 0.5,
            deck_outline_cut: true,
            deck_outline_depth: OutlineDepth::RailApex,
            bottom_outline_cut: true,
            cut_off_nose: false,
            cut_off_tail: false,
            deck_angle: 28.0,
            deck_rail_angle: 130.0,
            bottom_angle: 90.0,
            bottom_rail_angle: 90.0,
            move_between_steps: 5,
            side_change_steps: 10,
            cutting_speed: 80.0,
            stringer_speed: 254.0,
            rail_speed: 254.0,
            outline_speed: 254.0,
            plunge_speed_factor: 0.05,
            before_deck_cut: MachineAction::Home,
            after_deck_cut: MachineAction::Home,
            before_bottom_cut: MachineAction::Home,
            after_bottom_cut: MachineAction::Home,
            deck_start: DeckStart::Tail,
            bottom_start: BottomStart::Nose,
            clear_height: 20.0,
            cutter_offset: DVec3::ZERO,
            cutter: Cutter::default(),
            holding: HoldingSystem::default(),
            blank: None,
            scaling: AxisScaling::default(),
            sandwich: SandwichCompensation::default(),
            gouge_tolerance: 0.25,
            retract_clearance: 25.4,
            linking_spacing: 2.0,
            max_a_step: 2.0,
        }
    }
}

/// Hot-wire output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotwireKind {
    Profile,
    Outline,
}

/// Hot-wire settings.
#[derive(Clone, Debug, PartialEq)]
pub struct HotwireConfig {
    /// Samples per curve.
    pub splits: usize,
    /// Wire speed, mm/s.
    pub speed: f64,
    /// Extra material, cm: added to the deck of the profile, and split over both sides of the outline.
    pub expand: f64,
    /// Wire kerf, mm: the wire runs half of it outside the curve.
    pub kerf: f64,
    pub begin_at_nose: bool,
}

impl Default for HotwireConfig {
    fn default() -> Self {
        HotwireConfig {
            splits: 5000,
            speed: 3.0,
            expand: 0.0,
            kerf: 0.0,
            begin_at_nose: true,
        }
    }
}

// ------------------------------------------------------------------------------------------------ validation

fn bad(msg: impl Into<String>) -> Error {
    Error::InvalidConfig(msg.into())
}

fn finite(name: &str, v: f64) -> Result<f64, Error> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(bad(format!("{name} must be a number (got {v})")))
    }
}

fn positive(name: &str, v: f64) -> Result<(), Error> {
    if finite(name, v)? > 0.0 {
        Ok(())
    } else {
        Err(bad(format!("{name} must be greater than 0 (got {v})")))
    }
}

fn non_negative(name: &str, v: f64) -> Result<(), Error> {
    if finite(name, v)? >= 0.0 {
        Ok(())
    } else {
        Err(bad(format!("{name} must not be negative (got {v})")))
    }
}

fn within(name: &str, v: f64, lo: f64, hi: f64) -> Result<(), Error> {
    let v = finite(name, v)?;
    if (lo..=hi).contains(&v) {
        Ok(())
    } else {
        Err(bad(format!(
            "{name} must be between {lo} and {hi} (got {v})"
        )))
    }
}

impl Cutter {
    /// Refuse cutters that describe no real tool.
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            Cutter::Bullnose(b) => {
                positive("Cutter diameter", b.diameter)?;
                non_negative("Corner radius", b.corner_radius)?;
                positive("Cutter height", b.height)?;
                non_negative("Shank diameter", b.shank_diameter)?;
                if b.corner_radius > b.diameter / 2.0 + 1e-12 {
                    return Err(bad(format!(
                        "the corner radius ({} cm) is larger than the cutter radius ({} cm)",
                        b.corner_radius,
                        b.diameter / 2.0
                    )));
                }
                Ok(())
            }
            Cutter::Flat(f) => {
                positive("Cutter radius", f.radius)?;
                positive("Cutter height", f.height)
            }
            Cutter::Stl(s) => {
                let (r, lo, hi) = s.bounds();
                if !(r > 0.0 && hi > lo) {
                    return Err(bad("the STL cutter has no width or height"));
                }
                Ok(())
            }
            Cutter::RotatingBoard(inner) => match inner.as_ref() {
                Cutter::RotatingBoard(_) => {
                    Err(bad("a rotating board needs a plain cutter inside"))
                }
                c => c.validate(),
            },
        }
    }
}

impl MachineConfig {
    /// Whether the machine turns the board (4-axis).
    pub fn four_axis(&self) -> bool {
        matches!(self.cutter, Cutter::RotatingBoard(_))
    }

    /// Refuse impossible settings before generating `side`.
    pub fn validate(&self, side: BoardSide) -> Result<(), Error> {
        within(
            "Lengthwise resolution",
            self.lengthwise_resolution,
            1e-9,
            f64::MAX,
        )?;
        if self.lengthwise_resolution <= 0.0 {
            return Err(bad("Lengthwise resolution must be greater than 0"));
        }
        for (name, v) in [
            ("Cutting speed", self.cutting_speed),
            ("Stringer cutting speed", self.stringer_speed),
            ("Rail cutting speed", self.rail_speed),
            ("Outline cutting speed", self.outline_speed),
            ("Plunge speed factor", self.plunge_speed_factor),
            ("F scale", self.scaling.f),
        ] {
            positive(name, v)?;
        }
        for (name, v) in [
            ("X scale", self.scaling.x),
            ("Y scale", self.scaling.y),
            ("Z scale", self.scaling.z),
            ("A scale", self.scaling.a),
        ] {
            if finite(name, v)? == 0.0 {
                return Err(bad(format!("{name} must not be 0")));
            }
        }
        within("Clear height", self.clear_height, 0.0, 1000.0)?;
        within("Stringer width", self.stringer_width, 0.0, 10.0)?;
        for (name, v) in [
            ("X ref. offset", self.cutter_offset.x),
            ("Y ref. offset", self.cutter_offset.y),
            ("Z ref. offset", self.cutter_offset.z),
        ] {
            finite(name, v)?;
        }
        within("Gouge tolerance", self.gouge_tolerance, 0.001, 1.0)?;
        within("Retract clearance", self.retract_clearance, 5.0, 1000.0)?;
        within("Linking sample spacing", self.linking_spacing, 0.1, 10.0)?;
        within("Maximum A step", self.max_a_step, 0.1, 45.0)?;
        for (name, v) in [
            ("Deck definition angle", self.deck_angle),
            ("Deck rail definition angle", self.deck_rail_angle),
            ("Bottom definition angle", self.bottom_angle),
            ("Bottom rail definition angle", self.bottom_rail_angle),
        ] {
            within(name, v, 0.0, 180.0)?;
        }
        // Both programs are made from the same settings: refuse inconsistent bands for either side.
        let _ = side;
        if self.deck_rail_cuts > 0 && self.deck_angle > self.deck_rail_angle {
            return Err(bad(format!(
                "the deck angle ({}) is larger than the deck rail angle ({})",
                self.deck_angle, self.deck_rail_angle
            )));
        }
        if self.bottom_rail_cuts > 0 && self.bottom_rail_angle > self.bottom_angle {
            return Err(bad(format!(
                "the bottom rail angle ({}) is larger than the bottom angle ({})",
                self.bottom_rail_angle, self.bottom_angle
            )));
        }
        for (name, v) in [
            ("Deck cuts", self.deck_cuts),
            ("Deck rail cuts", self.deck_rail_cuts),
            ("Bottom cuts", self.bottom_cuts),
            ("Bottom rail cuts", self.bottom_rail_cuts),
        ] {
            if v > 1000 {
                return Err(bad(format!("{name} must be at most 1000 (got {v})")));
            }
        }
        for (name, v) in [
            ("Deck clean-up cuts", self.deck_cleanup_cuts),
            ("Bottom clean-up cuts", self.bottom_cleanup_cuts),
        ] {
            if v > 100 {
                return Err(bad(format!("{name} must be at most 100 (got {v})")));
            }
        }
        if self.move_between_steps == 0 || self.move_between_steps > 100 {
            return Err(bad("Transition steps must be between 1 and 100"));
        }
        if self.side_change_steps < 2 || self.side_change_steps > 100 {
            return Err(bad("Side change steps must be between 2 and 100"));
        }
        if let GeneratorKind::WidthSplits { splits } = self.generator
            && !(1..=1000).contains(&splits)
        {
            return Err(bad(format!(
                "Width splits must be between 1 and 1000 (got {splits})"
            )));
        }
        self.cutter.validate()?;
        match (&self.cutter, &self.holding) {
            (Cutter::RotatingBoard(_), HoldingSystem::RotatingBoard(_)) => {}
            (Cutter::RotatingBoard(_), _) => {
                return Err(bad(
                    "the rotating board cutter needs the rotating board holding system",
                ));
            }
            (_, HoldingSystem::RotatingBoard(_)) => {
                return Err(bad(
                    "the rotating board holding system needs the rotating board cutter",
                ));
            }
            _ => {}
        }
        match &self.holding {
            HoldingSystem::Supports(s) => {
                for (name, v) in [
                    ("Tailstop position", s.tailstop_pos),
                    ("Support 1 height", s.support1_height),
                    ("Support 2 height", s.support2_height),
                    ("Support length", s.support_length),
                    ("Tail-stop height", s.tailstop_height),
                ] {
                    non_negative(name, v)?;
                }
                positive("Support 1 from tailstop", s.support1_from_tailstop)?;
                finite("Support 2 from tailstop", s.support2_from_tailstop)?;
                if s.support2_from_tailstop <= s.support1_from_tailstop {
                    return Err(bad("support 2 must lie beyond support 1"));
                }
            }
            HoldingSystem::EndClamps(c) | HoldingSystem::RotatingBoard(c) => {
                positive("Clamp length", c.clamp_length)?;
                positive("Clamp height", c.clamp_height)?;
                positive("Clamp width", c.clamp_width)?;
                non_negative("Clamp safe zone", c.clamp_safe_zone)?;
                finite("Clamp overlap", c.clamp_overlap)?;
                finite("Blank vertical offset", c.blank_vertical_offset)?;
            }
        }
        let s = &self.sandwich;
        for (name, v) in [
            ("Deck compensation", s.deck_thickness),
            ("Deck rail limit", s.deck_rail_limit),
            ("Bottom compensation", s.bottom_thickness),
            ("Bottom rail limit", s.bottom_rail_limit),
            ("Rail compensation", s.rail_thickness),
        ] {
            non_negative(name, v)?;
        }
        if let Some(blank) = &self.blank
            && (blank.is_empty() || blank.length().is_nan() || blank.length() <= 1.0)
        {
            return Err(bad("the blank has no usable shape"));
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------ board fields

impl MachineConfig {
    /// Take the machining fields stored in a board file (`cam.md` §3.13). Fields left at 0 (the file default) do
    /// not overwrite settings: a 0 speed or cut count would give an `F0` or empty program.
    pub fn prefill_from_board(&mut self, board: &Board) {
        let m = &board.machine;
        let count = |v: i32, to: &mut usize| {
            if v > 0 {
                *to = v as usize;
            }
        };
        count(m.deck_cuts, &mut self.deck_cuts);
        count(m.top_shoulder_cuts, &mut self.deck_rail_cuts);
        count(m.bottom_cuts, &mut self.bottom_cuts);
        count(m.bottom_rail_cuts, &mut self.bottom_rail_cuts);
        let angle = |v: f64, to: &mut f64| {
            if v > 0.0 && v <= 180.0 {
                *to = v;
            }
        };
        angle(m.top_shoulder_angle, &mut self.deck_angle);
        angle(m.max_angle, &mut self.deck_rail_angle);
        if m.regular_speed > 0 {
            let v = f64::from(m.regular_speed);
            (self.cutting_speed, self.rail_speed, self.outline_speed) = (v, v, v);
        }
        if m.stringer_speed > 0 {
            self.stringer_speed = f64::from(m.stringer_speed);
        }
        if let HoldingSystem::Supports(s) = &mut self.holding {
            if m.strut1[0] > 0.0 && m.strut1[1] > 0.0 {
                (s.support1_from_tailstop, s.support1_height) = (m.strut1[0], m.strut1[1]);
            }
            // The file stores support 2 from the machine origin.
            if m.strut2[0] > s.tailstop_pos && m.strut2[1] > 0.0 {
                s.support2_from_tailstop = m.strut2[0] - s.tailstop_pos;
                s.support2_height = m.strut2[1];
            }
        }
    }

    /// Store the settings in the board's machining fields (`cam.md` §3.13; the regular speed is the cutting
    /// speed, speeds truncated to whole numbers).
    pub fn write_to_board(&self, board: &mut Board) {
        let m = &mut board.machine;
        let int = |v: usize| i32::try_from(v).unwrap_or(i32::MAX);
        m.deck_cuts = int(self.deck_cuts);
        m.top_shoulder_cuts = int(self.deck_rail_cuts);
        m.bottom_cuts = int(self.bottom_cuts);
        m.bottom_rail_cuts = int(self.bottom_rail_cuts);
        m.top_shoulder_angle = self.deck_angle;
        m.max_angle = self.deck_rail_angle;
        m.regular_speed = self.cutting_speed.trunc() as i32;
        m.stringer_speed = self.stringer_speed.trunc() as i32;
        if let HoldingSystem::Supports(s) = &self.holding {
            m.strut1[0] = s.support1_from_tailstop;
            m.strut1[1] = s.support1_height;
            m.strut2[0] = s.tailstop_pos + s.support2_from_tailstop;
            m.strut2[1] = s.support2_height;
        }
    }
}

impl HotwireConfig {
    pub fn validate(&self) -> Result<(), Error> {
        positive("Wire speed", self.speed)?;
        non_negative("Extra material", self.expand)?;
        non_negative("Kerf", self.kerf)?;
        if !(1..=1_000_000).contains(&self.splits) {
            return Err(bad(format!(
                "Samples per curve must be between 1 and 1000000 (got {})",
                self.splits
            )));
        }
        Ok(())
    }
}
