//! Placeholder: `.srf` import is written by the file-format implementer; only the API used by callers is declared here.

use surfcraft_core::Board;

use crate::Error;

/// Read an `.srf` binary board.
pub fn read(bytes: &[u8]) -> Result<Board, Error> {
    let _ = bytes;
    unimplemented!(".srf import not written yet")
}
