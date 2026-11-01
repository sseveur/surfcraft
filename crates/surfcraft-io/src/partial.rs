//! Placeholder: partial curve files (`.otl`, `.pfl`, `.crs`) are written by the file-format implementer; only the API used by callers is declared here.

use surfcraft_core::{Board, CrossSection, DVec2, Spline};

use crate::Error;

/// Curves found in a partial curve file, each with its guide points.
#[derive(Clone, Debug, Default)]
pub struct Partial {
    pub outline: Option<(Spline, Vec<DVec2>)>,
    pub bottom: Option<(Spline, Vec<DVec2>)>,
    pub deck: Option<(Spline, Vec<DVec2>)>,
    pub section: Option<CrossSection>,
}

/// Read an `.otl`, `.pfl` or `.crs` file.
pub fn read_partial(text: &str) -> Result<Partial, Error> {
    let _ = text;
    unimplemented!("partial curve import not written yet")
}

/// The outline as an `.otl` file.
pub fn write_outline(board: &Board) -> String {
    let _ = board;
    unimplemented!("partial curve export (.otl) not written yet")
}

/// Deck and bottom as a `.pfl` file.
pub fn write_profile(board: &Board) -> String {
    let _ = board;
    unimplemented!("partial curve export (.pfl) not written yet")
}

/// One cross-section as a `.crs` file.
pub fn write_section(section: &CrossSection) -> String {
    let _ = section;
    unimplemented!("partial curve export (.crs) not written yet")
}
