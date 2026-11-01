//! SurfCraft CAM: CNC toolpaths for surfboards (`docs/spec/cam.md`).
//!
//! * Surface splits: lengthwise passes in bands of the cross-section normal angle, for the deck and the bottom.
//! * Width splits: a simple raster across the width.
//! * Hot-wire profile and outline programs.
//!
//! A milling program is made in two steps. [`toolpath`] samples contact points on the board surface (board frame,
//! cm, outward normals). [`generate`] places them on the machine (blank holding system), turns the board on a
//! 4-axis machine, compensates the cutter, keeps the tool out of the board (drop-cutter verification of every
//! position and every straight move between them) and out of the supports and clamps, and writes LinuxCNC
//! G-code with statistics and an estimated machining time.

mod config;
mod cutter;
mod eval;
mod gouge;
mod holding;
mod program;
mod toolpath;

pub use config::{
    AxisScaling, BoardSide, BottomStart, BullnoseCutter, Cutter, DeckStart, EndClampsConfig,
    FlatCutter, GeneratorKind, HoldingSystem, HotwireConfig, HotwireKind, MachineAction,
    MachineConfig, OutlineDepth, SandwichCompensation, SupportsConfig,
};
pub use cutter::{StlCutter, Tip};
pub use holding::Placement;
pub use program::{Command, Move, Program, Stats};
pub use toolpath::{Pass, PassKind, Side, ToolPoint, Toolpath};

use surfcraft_core::Board;

/// Largest number of toolpath points a program may have (finer resolutions are refused).
pub(crate) const MAX_POINTS: usize = 20_000_000;

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

/// Contact-point toolpath of one side.
pub fn toolpath(board: &Board, cfg: &MachineConfig, side: BoardSide) -> Result<Toolpath, Error> {
    toolpath::plan(board, cfg, side).map(|p| p.toolpath)
}

/// Milling program of one side.
pub fn generate(board: &Board, cfg: &MachineConfig, side: BoardSide) -> Result<Program, Error> {
    let planned = toolpath::plan(board, cfg, side)?;
    program::milling(board, cfg, side, planned)
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
    toolpath::check_board(board)?;
    cfg.validate()?;
    let tp = toolpath::hotwire(board, cfg, kind);
    Ok(program::hotwire_program(board, cfg, kind, tp))
}
