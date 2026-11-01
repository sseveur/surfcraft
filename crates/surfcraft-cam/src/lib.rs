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
pub(crate) const MAX_POINTS: usize = 5_000_000;

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

#[cfg(test)]
mod tests {
    use super::*;
    use surfcraft_core::DVec3;

    fn shortboard() -> Board {
        let bytes = std::fs::read(format!(
            "{}/../../tests/fixtures/shortboard.brd",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        surfcraft_io::brd::read(&bytes).unwrap()
    }

    #[test]
    fn placement_round_trips_and_flips() {
        let p = Placement {
            angle: 0.03,
            offset: DVec3::new(100.0, 0.0, 26.0),
            scale: 10.0,
            flip: true,
        };
        let q = DVec3::new(50.0, 12.0, 4.0);
        assert!((p.inverse_point(p.point(q)) - q).length() < 1e-12);
        let v = DVec3::new(0.2, 0.3, -0.9).normalize();
        assert!((p.inverse_vector(p.vector(v)) - v).length() < 1e-12);
        // Turned over: a downward normal points up on the machine.
        assert!(p.vector(DVec3::NEG_Z).z > 0.99);
    }

    #[test]
    fn bullnose_touches_the_contact() {
        let c = Cutter::Bullnose(BullnoseCutter::default());
        let p = DVec3::new(10.0, 20.0, 30.0);
        assert_eq!(c.offset(p, DVec3::Z), p);
        for tilt in [5.0f64, 30.0, 60.0, 90.0] {
            let n = DVec3::new(tilt.to_radians().sin(), 0.0, tilt.to_radians().cos());
            let tip = c.offset(p, n);
            // The corner tube centre is r along the normal from the contact, a flat radius from the axis.
            let tube = p + 5.0 * n;
            let rel = tube - tip;
            assert!(
                (rel.z - 5.0).abs() < 1e-9 && (rel.x + 7.7).abs() < 1e-9,
                "{tilt}: {rel}"
            );
        }
        let flat = Cutter::Flat(FlatCutter::default());
        let tip = flat.offset(p, DVec3::new(0.0, 0.6, 0.8));
        assert!((tip - (p + DVec3::new(0.0, 12.7, 0.0))).length() < 1e-9);
    }

    #[test]
    fn stl_keywords_any_case_and_every_vertex() {
        let text = "solid t\n FACET NORMAL 0 0 -1\n outer loop\n VERTEX 0 0 0\n Vertex 2 0 0\n vertex 0 2 1\n \
                    endloop\n endfacet\nendsolid t\n";
        let c = StlCutter::from_ascii(text, DVec3::new(2.0, 1.0, 1.0)).unwrap();
        let (r, lo, hi) = c.bounds();
        assert_eq!((r, lo, hi), (4.0, 0.0, 1.0));
        assert!(StlCutter::from_ascii("solid empty\nendsolid\n", DVec3::ONE).is_err());
    }

    #[test]
    fn board_fields_prefill_without_zeros() {
        let mut board = shortboard();
        let mut cfg = MachineConfig::default();
        cfg.prefill_from_board(&board);
        // The fixture stores no machining data: nothing changes.
        assert_eq!(cfg.cutting_speed, 80.0);
        assert_eq!(cfg.deck_cuts, 28);
        cfg.deck_cuts = 12;
        cfg.cutting_speed = 100.7;
        cfg.rail_speed = 300.0;
        cfg.write_to_board(&mut board);
        assert_eq!(board.machine.regular_speed, 100);
        let mut again = MachineConfig::default();
        again.prefill_from_board(&board);
        assert_eq!(again.deck_cuts, 12);
        assert_eq!(again.cutting_speed, 100.0);
        if let (HoldingSystem::Supports(a), HoldingSystem::Supports(b)) =
            (&again.holding, &cfg.holding)
        {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn negative_zero_is_written_as_zero() {
        let board = shortboard();
        let cfg = MachineConfig {
            lengthwise_resolution: 20.0,
            deck_cuts: 1,
            deck_rail_cuts: 0,
            ..Default::default()
        };
        let g = generate_deck(&board, &cfg).unwrap().to_gcode();
        assert!(!g.contains("-0.0000"));
        assert!(g.contains("(Cutter: Bullnose cutter, diameter 25.4 mm, corner radius 5.0 mm"));
    }
}
