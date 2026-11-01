//! Placeholder: woodworking templates (sandwich, hollow wood, chambered planks) are written by another implementer; only the API used by callers is declared here.
#![allow(unused_variables)]

use surfcraft_core::{Board, DVec2};

use crate::drawing::{Drawing, Item, Style};

/// Woodworking templates as (name used by `by_name` and the `export` command, label shown in menus).
pub const TEMPLATES: [(&str, &str); 12] = [
    ("sandwich_profile", "Sandwich: profile (core)"),
    ("sandwich_rail", "Sandwich: rail"),
    ("sandwich_deck_skin", "Sandwich: deck skin"),
    ("sandwich_bottom_skin", "Sandwich: bottom skin"),
    ("hws_stringer", "Hollow wood: stringer"),
    ("hws_ribs", "Hollow wood: ribs"),
    ("hws_rail", "Hollow wood: rail strip"),
    ("hws_deck_skin", "Hollow wood: deck skin"),
    ("hws_bottom_skin", "Hollow wood: bottom skin"),
    ("hws_tail_piece", "Hollow wood: tail piece"),
    ("hws_nose_piece", "Hollow wood: nose piece"),
    ("chambered_planks", "Chambered wood: all planks"),
];

/// Length of the hollow-wood tail and nose pieces' stations, cm (9 in).
pub const HWS_PIECE_LENGTH: f64 = 22.86;

/// Sandwich construction settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SandwichOptions {
    pub skin_thickness: f64,
    pub distance_to_rail: f64,
    pub tail_offset: f64,
    pub nose_offset: f64,
    pub flatten: bool,
}

impl Default for SandwichOptions {
    fn default() -> Self {
        SandwichOptions {
            skin_thickness: 0.3,
            distance_to_rail: 1.27,
            tail_offset: 2.0,
            nose_offset: 6.0,
            flatten: false,
        }
    }
}

/// Hollow wooden board settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HwsOptions {
    pub distance_from_rail: f64,
    pub skin_thickness: f64,
    pub frame_thickness: f64,
    pub webbing: f64,
    pub tail_offset: f64,
    pub nose_offset: f64,
}

impl Default for HwsOptions {
    fn default() -> Self {
        HwsOptions {
            distance_from_rail: 3.0,
            skin_thickness: 0.4,
            frame_thickness: 0.5,
            webbing: 1.5,
            tail_offset: 3.5,
            nose_offset: 3.5,
        }
    }
}

/// Chambered (plank) board settings (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChamberedOptions {
    pub grid: bool,
    /// Offset of the first plank's inner face from the centre.
    pub start_offset: f64,
    /// Offset where the planks end; `None` = half the maximum width.
    pub end_offset: Option<f64>,
    pub plank_thickness: f64,
    /// Minimum wall left above and below the chambers.
    pub shell_thickness: f64,
    pub chambering: bool,
    pub alignment_marks: bool,
}

impl Default for ChamberedOptions {
    fn default() -> Self {
        ChamberedOptions {
            grid: true,
            start_offset: 0.0,
            end_offset: None,
            plank_thickness: 2.54,
            shell_thickness: 0.8,
            chambering: true,
            alignment_marks: true,
        }
    }
}

/// The template called `name` in [`TEMPLATES`], or `None` for an unknown name.
pub fn by_name(board: &Board, name: &str, fmt: &dyn Fn(f64) -> String) -> Option<Drawing> {
    if !TEMPLATES.iter().any(|(n, _)| *n == name) {
        return None;
    }
    unimplemented!("woodworking template {name} not written yet")
}

/// Points of every path drawn with `style`.
pub fn paths_with_style(d: &Drawing, style: Style) -> impl Iterator<Item = &[DVec2]> + '_ {
    d.items.iter().filter_map(move |item| match item {
        Item::Path {
            points, style: s, ..
        } if *s == style => Some(points.as_slice()),
        _ => None,
    })
}

/// Drawings stacked one above the other, `gap` mm apart.
pub fn stack(drawings: &[Drawing], gap: f64) -> Drawing {
    unimplemented!("drawing stacking not written yet")
}

pub fn sandwich_profile(
    board: &Board,
    opts: &SandwichOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    unimplemented!("sandwich core profile template not written yet")
}

pub fn sandwich_rail(
    board: &Board,
    opts: &SandwichOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    unimplemented!("sandwich rail template not written yet")
}

pub fn sandwich_deck_skin(
    board: &Board,
    opts: &SandwichOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    unimplemented!("sandwich deck skin template not written yet")
}

pub fn sandwich_bottom_skin(
    board: &Board,
    opts: &SandwichOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    unimplemented!("sandwich bottom skin template not written yet")
}

/// Rib stations (cm from the tail), shared by the stringer, ribs and rail strip.
pub fn hws_rib_stations(board: &Board) -> Vec<f64> {
    unimplemented!("hollow wood rib stations not written yet")
}

pub fn hws_stringer(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood stringer template not written yet")
}

/// The rib at station `x` (cm).
pub fn hws_rib(board: &Board, x: f64, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood rib template not written yet")
}

/// Every rib on one sheet.
pub fn hws_ribs(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood rib templates not written yet")
}

pub fn hws_rail(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood rail strip template not written yet")
}

pub fn hws_deck_skin(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood deck skin template not written yet")
}

pub fn hws_bottom_skin(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood bottom skin template not written yet")
}

pub fn hws_tail_piece(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood tail piece template not written yet")
}

pub fn hws_nose_piece(board: &Board, opts: &HwsOptions, fmt: &dyn Fn(f64) -> String) -> Drawing {
    unimplemented!("hollow wood nose piece template not written yet")
}

/// Inner-face offsets (cm from the centre) of the planks.
pub fn chambered_plank_offsets(board: &Board, opts: &ChamberedOptions) -> Vec<f64> {
    unimplemented!("chambered plank offsets not written yet")
}

/// The plank whose inner face is at `offset` (cm from the centre).
pub fn chambered_plank(
    board: &Board,
    offset: f64,
    opts: &ChamberedOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Drawing {
    unimplemented!("chambered plank template not written yet")
}

/// One template per plank.
pub fn chambered_planks(
    board: &Board,
    opts: &ChamberedOptions,
    fmt: &dyn Fn(f64) -> String,
) -> Vec<Drawing> {
    unimplemented!("chambered plank templates not written yet")
}
