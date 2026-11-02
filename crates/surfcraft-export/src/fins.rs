//! Fin placement on the printed templates: box footprints, fin centre lines and the measurement table.

use surfcraft_core::fins::{FinSetup, Side};
use surfcraft_core::{Board, DVec2};

use crate::drawing::{Anchor, Drawing, Style};

/// Line style of the box footprints (what is routed).
pub const BOX: Style = Style::solid(0.25, [0, 0, 0]);
/// Frame around the measurement table.
pub const FRAME: Style = Style::solid(0.15, [120, 120, 120]);
/// Fin base line.
pub const BASE: Style = Style::solid(0.5, [0, 0, 0]);
/// How far the centre lines run past the box ends (mm).
const CENTRE_LINE_OVERRUN: f64 = 15.0;
/// Trailing-edge mark half-size (mm).
const TRAILING_EDGE_CROSS: f64 = 4.0;
/// Plug centre cross half-size (mm).
const PLUG_CROSS: f64 = 2.0;

/// Draw the setup's fins in a plan view in mm: the base of each fin, its box footprint and its centre line
/// extended past the box, each lengthwise position mapped through `map_x` (mm to mm). With `half`, only the fins
/// on the stringer and on the side drawn above it.
pub fn draw(
    d: &mut Drawing,
    setup: &FinSetup,
    map_x: &dyn Fn(f64) -> f64,
    half: bool,
    guide: Style,
) {
    let p = |q: DVec2| DVec2::new(map_x(q.x * 10.0), q.y * 10.0);
    for f in &setup.fins {
        if half && f.side() == Side::Right {
            continue;
        }
        let (a, b) = f.centre_line(CENTRE_LINE_OVERRUN / 10.0);
        d.line(p(a), p(b), guide);
        d.line(p(f.rear()), p(f.front()), BASE);
        // The trailing edge: the point every distance is measured to (the FCS II jig's dot).
        d.cross(p(f.rear()), TRAILING_EDGE_CROSS, BASE);
        for poly in f.footprint() {
            d.closed(poly.into_iter().map(p).collect(), BOX);
        }
        if let Some(s) = f.system.spec()
            && s.has_plugs()
        {
            for &u in &s.plug_offsets {
                d.cross(p(f.to_board(u, 0.0)), PLUG_CROSS, guide);
            }
        }
    }
}

/// Cells of the measurement table: a header row, then one row per fin.
pub fn table_rows(
    board: &Board,
    setup: &FinSetup,
    fmt: &dyn Fn(f64) -> String,
) -> Vec<[String; 8]> {
    let mut rows = vec![[
        "Fin".to_string(),
        "From tail".into(),
        "From rail".into(),
        "Toe".into(),
        "Cant".into(),
        "Base".into(),
        "Depth".into(),
        "Box".into(),
    ]];
    for (i, f) in setup.fins.iter().enumerate() {
        let rail = if f.side() == Side::Centre {
            "on stringer".to_string()
        } else {
            fmt(f.from_rail(board))
        };
        let toe = if f.side() == Side::Centre {
            "-".to_string()
        } else {
            format!("{:.1}° ({})", f.toe, fmt(f.toe_offset()))
        };
        let cant = if f.side() == Side::Centre {
            "-".to_string()
        } else {
            format!("{:.1}°", f.cant)
        };
        rows.push([
            setup.role_label(i).to_string(),
            fmt(f.x),
            rail,
            toe,
            cant,
            fmt(f.base),
            fmt(f.depth),
            f.system.label().to_string(),
        ]);
    }
    rows
}

/// The title line of the fin table: "Fins: Thruster, FCS II".
pub fn title(setup: &FinSetup) -> String {
    format!("Fins: {}, {}", setup.preset.label(), setup.system.label())
}

/// Notes under the table: where the distances are measured, and how far to trust the box outlines.
pub const NOTES: [&str; 2] = [
    "From tail and from rail: to the fin's trailing edge (rear of the base, marked +), square to the stringer. \
     Toe: how much closer to the stringer the leading edge is.",
    "Box outlines show where the boxes go; their sizes are approximate. Align your jig on the + mark and the \
     centre line, and check against your boxes before routing.",
];

/// The measurement table, top-left corner at `top_left` (mm), text `size` mm high. Returns the height used.
pub fn draw_table(
    d: &mut Drawing,
    board: &Board,
    setup: &FinSetup,
    top_left: DVec2,
    size: f64,
    fmt: &dyn Fn(f64) -> String,
) -> f64 {
    let rows = table_rows(board, setup, fmt);
    let line = size * 1.5;
    // Column widths from the longest cell (about 0.6 em per character).
    let mut widths = [0.0f64; 8];
    for r in &rows {
        for (w, c) in widths.iter_mut().zip(r) {
            *w = w.max(c.chars().count() as f64 * size * 0.6 + 2.0 * size);
        }
    }
    let mut y = top_left.y - size;
    d.text(DVec2::new(top_left.x, y), size, title(setup), Anchor::Left);
    for r in &rows {
        y -= line;
        let mut x = top_left.x;
        for (w, c) in widths.iter().zip(r) {
            d.text(DVec2::new(x, y), size, c.clone(), Anchor::Left);
            x += w;
        }
    }
    for n in NOTES {
        y -= line;
        d.text(DVec2::new(top_left.x, y), size * 0.8, n, Anchor::Left);
    }
    // A frame around the table, which also makes it part of the drawing's extent for page layout.
    let text_w = NOTES
        .iter()
        .map(|n| n.chars().count() as f64 * size * 0.8 * 0.6)
        .fold(widths.iter().sum::<f64>(), f64::max);
    let lo = DVec2::new(top_left.x - size, y - size);
    d.rect(
        lo,
        DVec2::new(top_left.x + text_w + size, top_left.y),
        FRAME,
    );
    top_left.y - lo.y
}
