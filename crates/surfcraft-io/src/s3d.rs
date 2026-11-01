//! Placeholder: Shape3D `.s3d` import is written by the file-format implementer; only the API used by callers is declared here.

use surfcraft_core::Board;

use crate::Error;

/// Warning reported when the design has no deck curve.
pub const NO_DECK_WARNING: &str = "The file has no deck curve; a deck was made from the thickness.";

/// Read a Shape3D design.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    read_with_warnings(bytes).map(|(b, _)| b)
}

/// Read a Shape3D design, also returning warnings about what had to be made up.
pub fn read_with_warnings(bytes: &[u8]) -> Result<(Board, Vec<String>), Error> {
    let _ = bytes;
    unimplemented!(".s3d import not written yet")
}
