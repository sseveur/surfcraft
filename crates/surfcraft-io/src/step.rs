//! Placeholder: STEP import and export are written by the file-format implementer; only the API used by callers is declared here.

use surfcraft_core::Board;

use crate::Error;

/// Read a STEP file written by SurfCraft or BoardCAD.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    let _ = bytes;
    unimplemented!("STEP import not written yet")
}

/// Write the board's Bezier curves as STEP.
pub fn write(board: &Board) -> String {
    let _ = board;
    unimplemented!("STEP export not written yet")
}
