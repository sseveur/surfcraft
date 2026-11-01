//! Board file formats: the native `.brd` (plain and legacy-encrypted), partial curve files, STEP, Shape3D
//! `.s3d` and `.srf`, plus SurfCraft's built-in boards.

pub mod brd;
pub mod legacy;
pub mod partial;
pub mod s3d;
pub mod srf;
pub mod step;
pub mod templates;
pub mod xml;

use surfcraft_core::Board;

/// A file that could not be read. The message names the format and, when known, the line or field.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("unsupported file type {0:?}")]
    Unsupported(String),
}

/// Read a board file by its extension (case-insensitive, with or without the leading dot): `brd`, `s3d`,
/// `srf`, `stp` or `step`.
pub fn read_any(extension: &str, bytes: &[u8]) -> Result<Board, Error> {
    let ext = extension.trim_start_matches('.').to_ascii_lowercase();
    match ext.as_str() {
        "brd" => brd::read(bytes),
        "s3d" => s3d::read(bytes),
        "srf" => srf::read(bytes),
        "stp" | "step" => step::read(bytes),
        _ => Err(Error::Unsupported(extension.to_owned())),
    }
}
