//! DXF (AutoCAD R12 ASCII) in millimetres: polylines and text, readable by CNC/laser software and CAD tools.

use std::fmt::Write;

use crate::drawing::{Anchor, Drawing, Item};

fn pair(s: &mut String, code: i32, value: impl std::fmt::Display) {
    let _ = writeln!(s, "{code:>3}\n{value}");
}

/// R12 files are ASCII: transliterate common typography, escape the rest as `\U+XXXX` (AutoCAD convention).
fn ascii_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            '—' | '–' | '−' => out.push('-'),
            '’' | '‘' | '′' => out.push('\''),
            '“' | '”' | '″' => out.push('"'),
            '…' => out.push_str("..."),
            '·' => out.push('-'),
            c if c.is_ascii() => out.push(c),
            c => {
                let _ = write!(out, "\\U+{:04X}", c as u32);
            }
        }
    }
    out
}

pub fn write(d: &Drawing) -> String {
    let mut s = String::new();
    pair(&mut s, 0, "SECTION");
    pair(&mut s, 2, "HEADER");
    pair(&mut s, 9, "$ACADVER");
    pair(&mut s, 1, "AC1009");
    pair(&mut s, 9, "$INSUNITS");
    pair(&mut s, 70, 4); // millimetres
    pair(&mut s, 0, "ENDSEC");
    pair(&mut s, 0, "SECTION");
    pair(&mut s, 2, "ENTITIES");
    for item in &d.items {
        match item {
            Item::Path {
                points,
                closed,
                style,
            } => {
                let layer = if style.dash.is_some() { "GUIDE" } else { "CUT" };
                pair(&mut s, 0, "POLYLINE");
                pair(&mut s, 8, layer);
                pair(&mut s, 66, 1);
                pair(&mut s, 70, if *closed { 1 } else { 0 });
                for p in points {
                    pair(&mut s, 0, "VERTEX");
                    pair(&mut s, 8, layer);
                    pair(&mut s, 10, format!("{:.4}", p.x));
                    pair(&mut s, 20, format!("{:.4}", p.y));
                    pair(&mut s, 30, "0.0");
                }
                pair(&mut s, 0, "SEQEND");
                pair(&mut s, 8, layer);
            }
            Item::Text {
                at,
                size_mm,
                text,
                anchor,
                rotation,
            } => {
                pair(&mut s, 0, "TEXT");
                pair(&mut s, 8, "TEXT");
                pair(&mut s, 10, format!("{:.4}", at.x));
                pair(&mut s, 20, format!("{:.4}", at.y));
                pair(&mut s, 30, "0.0");
                pair(&mut s, 40, format!("{size_mm:.3}"));
                pair(&mut s, 1, ascii_text(text));
                if *rotation != 0.0 {
                    pair(&mut s, 50, format!("{rotation:.3}"));
                }
                let h = match anchor {
                    Anchor::Left => 0,
                    Anchor::Centre => 1,
                    Anchor::Right => 2,
                };
                if h != 0 {
                    pair(&mut s, 72, h);
                    pair(&mut s, 11, format!("{:.4}", at.x));
                    pair(&mut s, 21, format!("{:.4}", at.y));
                    pair(&mut s, 31, "0.0");
                }
            }
        }
    }
    pair(&mut s, 0, "ENDSEC");
    pair(&mut s, 0, "EOF");
    s
}
