//! Minimal PDF 1.4 writer: vector paths and Helvetica text, one page per [`Page`].

use std::fmt::Write;

use surfcraft_core::DVec2;

use crate::drawing::{Anchor, Drawing, Item, Page};

const PT_PER_MM: f64 = 72.0 / 25.4;

/// Approximate Helvetica advance width (em units) for alignment.
fn text_width_em(s: &str) -> f64 {
    s.chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.24,
            'f' | 't' | 'r' | ' ' | '(' | ')' | '/' | '-' => 0.32,
            'm' | 'w' | 'M' | 'W' => 0.85,
            c if c.is_ascii_uppercase() => 0.68,
            c if c.is_ascii_digit() => 0.556,
            _ => 0.52,
        })
        .sum()
}

/// Escape for a PDF literal string; characters outside Latin-1 become '?'.
fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    out.push(b'(');
    for c in s.chars() {
        let b = match c {
            '—' | '–' => b'-',
            '’' | '‘' => b'\'',
            '“' | '”' => b'"',
            c if (c as u32) < 256 => c as u32 as u8,
            _ => b'?',
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Whether an item can show inside `visible` (drawing coordinates, mm). Items entirely outside are left out of the
/// page, so a drawing tiled over many pages doesn't repeat every item on every page.
fn may_show(item: &Item, visible: (DVec2, DVec2)) -> bool {
    let (lo, hi) = visible;
    let (min, max, pad) = match item {
        Item::Path { points, style, .. } => {
            let Some(first) = points.first() else {
                return false;
            };
            let (min, max) = points
                .iter()
                .fold((*first, *first), |(a, b), p| (a.min(*p), b.max(*p)));
            (min, max, style.width_mm)
        }
        // Generous box: the text can extend its own width either side of the anchor, in any rotation.
        Item::Text {
            at, size_mm, text, ..
        } => {
            let r = (text_width_em(text) + 1.0) * size_mm;
            (*at, *at, r)
        }
    };
    min.x - pad <= hi.x && max.x + pad >= lo.x && min.y - pad <= hi.y && max.y + pad >= lo.y
}

fn draw(content: &mut Vec<u8>, d: &Drawing, visible: Option<(DVec2, DVec2)>) {
    let mut s = String::new();
    for item in &d.items {
        if visible.is_some_and(|v| !may_show(item, v)) {
            continue;
        }
        match item {
            Item::Path {
                points,
                closed,
                style,
            } => {
                let [r, g, b] = style.rgb.map(|c| c as f64 / 255.0);
                let _ = write!(
                    s,
                    "{} {} {} RG {} w ",
                    num(r),
                    num(g),
                    num(b),
                    num(style.width_mm * PT_PER_MM)
                );
                match style.dash {
                    Some((on, off)) => {
                        let _ =
                            write!(s, "[{} {}] 0 d ", num(on * PT_PER_MM), num(off * PT_PER_MM));
                    }
                    None => s.push_str("[] 0 d "),
                }
                for (i, p) in points.iter().enumerate() {
                    let _ = write!(
                        s,
                        "{} {} {} ",
                        num(p.x * PT_PER_MM),
                        num(p.y * PT_PER_MM),
                        if i == 0 { "m" } else { "l" }
                    );
                }
                s.push_str(if *closed { "s\n" } else { "S\n" });
            }
            Item::Text {
                at,
                size_mm,
                text,
                anchor,
                rotation,
            } => {
                let size = size_mm * PT_PER_MM;
                let w = text_width_em(text) * size;
                let dx = match anchor {
                    Anchor::Left => 0.0,
                    Anchor::Centre => -w / 2.0,
                    Anchor::Right => -w,
                };
                let (sin, cos) = rotation.to_radians().sin_cos();
                let (x, y) = (at.x * PT_PER_MM, at.y * PT_PER_MM);
                let _ = write!(
                    s,
                    "BT /F1 {} Tf 0 g {} {} {} {} {} {} Tm ",
                    num(size),
                    num(cos),
                    num(sin),
                    num(-sin),
                    num(cos),
                    num(x + dx * cos),
                    num(y + dx * sin)
                );
                content.extend_from_slice(s.as_bytes());
                s.clear();
                content.extend(pdf_string(text));
                content.extend_from_slice(b" Tj ET\n");
            }
        }
    }
    content.extend_from_slice(s.as_bytes());
}

/// Render pages of `drawing` to a PDF file.
pub fn write(drawing: &Drawing, pages: &[Page], title: &str) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1: catalog, 2: pages, 3: font, then per page: page object + content stream.
    let n = pages.len();
    let page_ids: Vec<usize> = (0..n).map(|i| 4 + 2 * i).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    for (i, page) in pages.iter().enumerate() {
        let mut content = Vec::new();
        let (cmin, cmax) = (page.clip_min * PT_PER_MM, page.clip_max * PT_PER_MM);
        let _ = writeln!(
            byte_writer(&mut content),
            "q {} {} {} {} re W n 1 0 0 1 {} {} cm 1 J 1 j",
            num(cmin.x),
            num(cmin.y),
            num(cmax.x - cmin.x),
            num(cmax.y - cmin.y),
            num(page.offset.x * PT_PER_MM),
            num(page.offset.y * PT_PER_MM)
        );
        // The part of the drawing this page shows, in drawing coordinates.
        let visible = (page.clip_min - page.offset, page.clip_max - page.offset);
        draw(&mut content, drawing, Some(visible));
        content.extend_from_slice(b"Q\nq 1 J 1 j\n");
        draw(&mut content, &page.furniture, None);
        content.extend_from_slice(b"Q\n");
        let size = page.paper * PT_PER_MM;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                num(size.x),
                num(size.y),
                page_ids[i] + 1
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend(content);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let info_id = objects.len() + 1;
    let mut info = b"<< /Producer (SurfCraft) /Title ".to_vec();
    info.extend(pdf_string(title));
    info.extend_from_slice(b" >>");
    objects.push(info);

    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").into_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info {info_id} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .into_bytes(),
    );
    out
}

/// `fmt::Write` adapter over a byte buffer.
fn byte_writer(buf: &mut Vec<u8>) -> impl Write + '_ {
    struct W<'a>(&'a mut Vec<u8>);
    impl Write for W<'_> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }
    W(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{Paper, Style, tile};
    use surfcraft_core::DVec2;

    #[test]
    fn writes_valid_structure() {
        let mut d = Drawing::new(300.0, 100.0);
        d.line(DVec2::ZERO, DVec2::new(300.0, 100.0), Style::CUT);
        d.text(
            DVec2::new(10.0, 10.0),
            4.0,
            "Board (6'2\") — test",
            Anchor::Left,
        );
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "test");
        let pdf = write(&d, &pages, "test");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let lossy = String::from_utf8_lossy(&pdf);
        assert_eq!(lossy.matches("/Type /Page ").count(), pages.len());
        // xref offsets (byte positions) point at "N 0 obj".
        let tail = String::from_utf8(pdf[pdf.len() - 40..].to_vec()).unwrap();
        let xref_at: usize = tail
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[xref_at..].starts_with(b"xref"));
        let table = String::from_utf8(pdf[xref_at..].to_vec()).unwrap();
        for (i, line) in table.lines().skip(3).take(3).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()));
        }
    }

    #[test]
    fn pages_only_carry_what_they_show() {
        // Two pieces far apart: each is printed on its own page, and no page carries the other piece.
        let mut d = Drawing::new(2000.0, 50.0);
        d.line(DVec2::new(0.0, 10.0), DVec2::new(100.0, 10.0), Style::CUT);
        d.text(DVec2::new(10.0, 20.0), 4.0, "FIRSTPIECE", Anchor::Left);
        d.line(DVec2::new(1900.0, 10.0), DVec2::new(2000.0, 10.0), Style::CUT);
        d.text(DVec2::new(1910.0, 20.0), 4.0, "LASTPIECE", Anchor::Left);
        d.part(DVec2::new(0.0, 0.0), DVec2::new(100.0, 50.0));
        d.part(DVec2::new(1900.0, 0.0), DVec2::new(2000.0, 50.0));
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "test");
        assert_eq!(pages.len(), 2);
        let pdf = String::from_utf8_lossy(&write(&d, &pages, "test")).into_owned();
        assert_eq!(pdf.matches("FIRSTPIECE").count(), 1);
        assert_eq!(pdf.matches("LASTPIECE").count(), 1);
        // A line crossing a page edge is kept on both pages.
        let mut long = Drawing::new(600.0, 50.0);
        long.line(DVec2::new(0.0, 10.0), DVec2::new(600.0, 10.0), Style::CUT);
        let pages = tile(&long, Paper::A4, 10.0, 10.0, "test");
        assert!(pages.len() >= 2);
        let pdf = String::from_utf8_lossy(&write(&long, &pages, "test")).into_owned();
        // The line starts at (0, 10 mm) = (0, 28.346 pt).
        assert_eq!(pdf.matches("0 28.346 m").count(), pages.len());
    }
}
