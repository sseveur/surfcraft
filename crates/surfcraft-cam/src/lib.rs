//! Placeholder: CNC toolpath generation is written by another implementer; only the API used by callers is declared here.
#![allow(unused_variables, dead_code)]

use std::sync::Arc;

use surfcraft_core::{Board, DVec3};

// ------------------------------------------------------------------------------------------------ errors

/// Why a program could not be generated.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("the board has no usable shape")]
    InvalidBoard,
    #[error("invalid setting: {0}")]
    InvalidConfig(String),
    #[error("the settings leave nothing to cut")]
    NothingToCut,
    #[error("the lengthwise resolution is too fine: the program would have too many points")]
    TooManyPoints,
    #[error("invalid STL cutter: {0}")]
    InvalidStl(String),
}

// ------------------------------------------------------------------------------------------------ settings

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

/// Bullnose end mill (lengths in cm).
#[derive(Clone, Debug, PartialEq)]
pub struct BullnoseCutter {
    pub diameter: f64,
    pub corner_radius: f64,
    /// Length of the cutting part above the tip.
    pub height: f64,
    /// Diameter of the shank above the cutting part (collision checks).
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

/// Cutter shape read from an ASCII STL (mm, tip at the origin, axis +Z).
#[derive(Clone, Debug, PartialEq)]
pub struct StlCutter {
    vertices: Vec<DVec3>,
}

impl StlCutter {
    pub fn from_ascii(text: &str, scale: DVec3) -> Result<StlCutter, Error> {
        unimplemented!("STL cutter loading not written yet")
    }

    /// (radius, lowest z, highest z) in mm.
    pub fn bounds(&self) -> (f64, f64, f64) {
        unimplemented!("STL cutter bounds not written yet")
    }

    /// The cutter vertex touching a surface with normal `n`.
    pub fn contact_vertex(&self, n: DVec3) -> DVec3 {
        unimplemented!("STL cutter contact search not written yet")
    }

    /// Reference search over every vertex.
    pub fn contact_vertex_brute_force(&self, n: DVec3) -> DVec3 {
        unimplemented!("STL cutter contact search not written yet")
    }

    /// Tool tip for a contact point and surface normal.
    pub fn offset(&self, contact: DVec3, normal: DVec3) -> DVec3 {
        unimplemented!("STL cutter offset not written yet")
    }
}

/// Tool tip position (and A angle on the rotating board) for a contact point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    pub pos: DVec3,
    pub a: Option<f64>,
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

impl Cutter {
    /// The bullnose cutter, also inside a rotating-board cutter.
    pub fn bullnose(&self) -> Option<&BullnoseCutter> {
        match self {
            Cutter::Bullnose(b) => Some(b),
            Cutter::RotatingBoard(inner) => inner.bullnose(),
            _ => None,
        }
    }

    /// Tool tip for `contact` with surface `normal` (reference frame, mm), keeping `stringer_width` (cm) clear of the
    /// centreline when given; `side` is +1 right, -1 left; `a` is the current A angle (degrees).
    pub fn tip(
        &self,
        contact: DVec3,
        normal: DVec3,
        stringer_width: Option<f64>,
        side: f64,
        a: Option<f64>,
    ) -> Tip {
        unimplemented!("cutter compensation not written yet")
    }
}

/// Two supports and a tail stop (mm, measured from the tail stop).
#[derive(Clone, Debug, PartialEq)]
pub struct SupportsConfig {
    pub tailstop_pos: f64,
    pub support1_from_tailstop: f64,
    pub support2_from_tailstop: f64,
    pub support1_height: f64,
    pub support2_height: f64,
}

impl Default for SupportsConfig {
    fn default() -> Self {
        SupportsConfig {
            tailstop_pos: 1000.0,
            support1_from_tailstop: 600.0,
            support2_from_tailstop: 1600.0,
            support1_height: 260.0,
            support2_height: 300.0,
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

impl HoldingSystem {
    /// Placement of one side of `board` (or of the `blank`, when given) on the machine.
    pub fn placement(&self, board: &Board, blank: Option<&Board>, side: BoardSide) -> Placement {
        unimplemented!("blank holding placement not written yet")
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
    pub stringer_width: f64,
    pub deck_outline_cut: bool,
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
    /// Extra material, cm.
    pub expand: f64,
    /// Wire kerf, mm.
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

// ------------------------------------------------------------------------------------------------ toolpaths

/// What a pass does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassKind {
    Approach,
    Stringer,
    Regular,
    Rail,
    Outline,
    Cleanup,
    SideChange,
    NoseCutOff,
    TailCutOff,
    WidthSplit,
    HotwireProfile,
    HotwireOutline,
}

/// Board half a pass runs on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Side {
    #[default]
    Right,
    Left,
    Center,
}

/// Contact point on the board surface (side frame, cm) with its outward unit normal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolPoint {
    pub pos: DVec3,
    pub normal: DVec3,
}

/// One pass of contact points.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub kind: PassKind,
    pub side: Side,
    /// Number of this pass among the passes of its kind and side.
    pub index: usize,
    /// Leading transition points before the pass proper.
    pub lead_in: usize,
    pub points: Vec<ToolPoint>,
}

/// All passes of one program, in cutting order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub passes: Vec<Pass>,
}

impl Toolpath {
    /// Every contact point, in order.
    pub fn points(&self) -> impl Iterator<Item = &ToolPoint> + '_ {
        self.passes.iter().flat_map(|p| p.points.iter())
    }

    /// Number of passes of `kind`, on `side` when given.
    pub fn count(&self, kind: PassKind, side: Option<Side>) -> usize {
        self.passes
            .iter()
            .filter(|p| p.kind == kind && side.is_none_or(|s| p.side == s))
            .count()
    }

    pub fn point_count(&self) -> usize {
        self.passes.iter().map(|p| p.points.len()).sum()
    }
}

/// Side frame (cm) to reference frame (mm): rotation `angle` (radians) about Y, then `offset` (cm), then `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub angle: f64,
    pub offset: DVec3,
    pub scale: f64,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            angle: 0.0,
            offset: DVec3::ZERO,
            scale: 10.0,
        }
    }
}

impl Placement {
    /// Side-frame point (cm) to the reference frame (mm).
    pub fn point(&self, p: DVec3) -> DVec3 {
        unimplemented!("placement transform not written yet")
    }

    /// Side-frame direction to the reference frame (unscaled).
    pub fn vector(&self, v: DVec3) -> DVec3 {
        unimplemented!("placement transform not written yet")
    }

    /// Reference-frame point (mm) back to the side frame (cm).
    pub fn inverse_point(&self, p: DVec3) -> DVec3 {
        unimplemented!("placement transform not written yet")
    }
}

/// One linear cutting move (machine frame, mm; feed mm/min; A degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub pos: DVec3,
    pub feed: f64,
    pub a: Option<f64>,
    /// Index of the toolpath pass the move belongs to.
    pub pass: Option<usize>,
}

/// One program step.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Comment(String),
    /// G-code text written verbatim (scripts, preamble).
    Raw(String),
    Home,
    SpindleOn,
    SpindleOff,
    /// Dwell, seconds.
    Dwell(f64),
    /// Vertical move to `z` (mm) at `feed` (mm/min).
    MoveZ {
        z: f64,
        feed: f64,
    },
    Move(Move),
    End,
}

/// Summary of a program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Passes, not counting the approach.
    pub passes: usize,
    /// Motion lines with a known position.
    pub points: usize,
    /// Path length, mm.
    pub cut_length: f64,
    /// Seconds.
    pub estimated_time: f64,
    pub passes_by_kind: Vec<(PassKind, usize)>,
}

/// A generated machine program.
#[derive(Clone, Debug)]
pub struct Program {
    pub side: BoardSide,
    pub toolpath: Toolpath,
    pub placement: Placement,
    /// Program zero in the reference frame, mm.
    pub cutter_offset: DVec3,
    pub commands: Vec<Command>,
}

impl Program {
    /// The cutting moves, in order.
    pub fn moves(&self) -> impl Iterator<Item = &Move> + '_ {
        self.commands.iter().filter_map(|c| match c {
            Command::Move(m) => Some(m),
            _ => None,
        })
    }

    pub fn to_gcode(&self) -> String {
        unimplemented!("G-code writer not written yet")
    }

    pub fn stats(&self) -> Stats {
        unimplemented!("program statistics not written yet")
    }

    /// Tool positions mapped back to board space (cm), for previews.
    pub fn tool_path_in_board_space(&self) -> Vec<DVec3> {
        unimplemented!("inverse tool mapping not written yet")
    }
}

// ------------------------------------------------------------------------------------------------ generation

/// Contact-point toolpath of one side.
pub fn toolpath(board: &Board, cfg: &MachineConfig, side: BoardSide) -> Result<Toolpath, Error> {
    unimplemented!("toolpath generation not written yet")
}

/// Milling program of one side.
pub fn generate(board: &Board, cfg: &MachineConfig, side: BoardSide) -> Result<Program, Error> {
    unimplemented!("program generation not written yet")
}

pub fn generate_deck(board: &Board, cfg: &MachineConfig) -> Result<Program, Error> {
    generate(board, cfg, BoardSide::Deck)
}

pub fn generate_bottom(board: &Board, cfg: &MachineConfig) -> Result<Program, Error> {
    generate(board, cfg, BoardSide::Bottom)
}

/// Two-axis hot-wire program.
pub fn generate_hotwire(
    board: &Board,
    cfg: &HotwireConfig,
    kind: HotwireKind,
) -> Result<Program, Error> {
    unimplemented!("hot-wire generation not written yet")
}
