//! Placeholder: printable design templates (outline, spin, profile, sections, spec sheet) are written by another implementer; only the API used by callers is declared here.
#![allow(unused_variables)]

use surfcraft_core::{Board, DVec2};

use crate::drawing::Drawing;

/// Options of the outline template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineOptions {
    /// Both halves (mirrored about the stringer) instead of the half outline.
    pub full: bool,
    /// Draw the measuring grid.
    pub grid: bool,
    /// Lengths measured along the rocker instead of flat.
    pub over_the_curve: bool,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        OutlineOptions {
            full: false,
            grid: true,
            over_the_curve: false,
        }
    }
}

/// 1:1 plan-shape template (mm). `fmt` formats a length given in cm for labels.
pub fn outline(board: &Board, opts: OutlineOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("outline template not written yet")
}

/// 1:1 spin template: tail half and turned-over nose half of the outline on one sheet.
pub fn spin(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("spin template not written yet")
}

/// 1:1 profile (rocker) template.
pub fn profile(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("profile template not written yet")
}

/// 1:1 templates of the designer cross-sections.
pub fn sections(board: &Board, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("cross-section templates not written yet")
}

/// One-page spec sheet scaled to a page of `page` size (mm), showing `volume` (litres as computed by the caller).
pub fn spec_sheet(board: &Board, volume: f64, page: DVec2, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("spec sheet not written yet")
}
