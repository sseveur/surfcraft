//! SVG output at true size (1 user unit = 1 mm).

use std::fmt::Write;

use crate::drawing::{Anchor, Drawing, Item};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn write(d: &Drawing) -> String {
    let (w, h) = (d.width, d.height);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w:.3}mm" height="{h:.3}mm" viewBox="0 0 {w:.3} {h:.3}">
<g fill="none" stroke-linecap="round" stroke-linejoin="round">"#
    );
    let y = |v: f64| h - v;
    for item in &d.items {
        match item {
            Item::Path {
                points,
                closed,
                style,
            } => {
                let [r, g, b] = style.rgb;
                let pts: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.3},{:.3}", p.x, y(p.y)))
                    .collect();
                let tag = if *closed { "polygon" } else { "polyline" };
                let dash = style
                    .dash
                    .map(|(a, b)| format!(r#" stroke-dasharray="{a} {b}""#))
                    .unwrap_or_default();
                let _ = writeln!(
                    s,
                    r#"<{tag} points="{}" stroke="rgb({r},{g},{b})" stroke-width="{:.3}"{dash}/>"#,
                    pts.join(" "),
                    style.width_mm
                );
            }
            Item::Text {
                at,
                size_mm,
                text,
                anchor,
                rotation,
            } => {
                let a = match anchor {
                    Anchor::Left => "start",
                    Anchor::Centre => "middle",
                    Anchor::Right => "end",
                };
                let rot = if *rotation != 0.0 {
                    format!(
                        r#" transform="rotate({:.3} {:.3} {:.3})""#,
                        -rotation,
                        at.x,
                        y(at.y)
                    )
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    r#"<text x="{:.3}" y="{:.3}" font-family="Helvetica, Arial, sans-serif" font-size="{:.3}" text-anchor="{a}" fill="black" stroke="none"{rot}>{}</text>"#,
                    at.x,
                    y(at.y),
                    size_mm,
                    esc(text)
                );
            }
        }
    }
    s.push_str("</g>\n</svg>\n");
    s
}
