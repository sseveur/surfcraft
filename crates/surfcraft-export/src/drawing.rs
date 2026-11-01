//! Device-independent 2D vector drawing in millimetres (origin bottom-left, y up).

use surfcraft_core::DVec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub width_mm: f64,
    pub rgb: [u8; 3],
    /// Dash and gap lengths in mm.
    pub dash: Option<(f64, f64)>,
}

impl Style {
    pub const fn solid(width_mm: f64, rgb: [u8; 3]) -> Self {
        Self {
            width_mm,
            rgb,
            dash: None,
        }
    }
    pub const CUT: Style = Style::solid(0.35, [0, 0, 0]);
    pub const THIN: Style = Style::solid(0.15, [0, 0, 0]);
    pub const GUIDE: Style = Style {
        width_mm: 0.15,
        rgb: [120, 120, 120],
        dash: Some((2.0, 1.5)),
    };
    pub const GRID: Style = Style::solid(0.1, [190, 190, 190]);
    pub const ACCENT: Style = Style::solid(0.25, [200, 60, 30]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Left,
    Centre,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Path {
        points: Vec<DVec2>,
        closed: bool,
        style: Style,
    },
    Text {
        at: DVec2,
        size_mm: f64,
        text: String,
        anchor: Anchor,
        /// Degrees counter-clockwise.
        rotation: f64,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
    /// Separate pieces (min, max in mm) that [`tile`] prints each on their own pages, centred. Empty: the whole
    /// drawing is one piece.
    pub parts: Vec<(DVec2, DVec2)>,
}

impl Drawing {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            items: Vec::new(),
            parts: Vec::new(),
        }
    }

    /// Mark a region as a separate piece for printing.
    pub fn part(&mut self, min: DVec2, max: DVec2) {
        self.parts.push((min, max));
    }

    pub fn path(&mut self, points: Vec<DVec2>, style: Style) {
        if points.len() >= 2 {
            self.items.push(Item::Path {
                points,
                closed: false,
                style,
            });
        }
    }

    pub fn closed(&mut self, points: Vec<DVec2>, style: Style) {
        if points.len() >= 3 {
            self.items.push(Item::Path {
                points,
                closed: true,
                style,
            });
        }
    }

    pub fn line(&mut self, a: DVec2, b: DVec2, style: Style) {
        self.path(vec![a, b], style);
    }

    pub fn rect(&mut self, min: DVec2, max: DVec2, style: Style) {
        self.closed(
            vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)],
            style,
        );
    }

    pub fn text(&mut self, at: DVec2, size_mm: f64, text: impl Into<String>, anchor: Anchor) {
        self.items.push(Item::Text {
            at,
            size_mm,
            text: text.into(),
            anchor,
            rotation: 0.0,
        });
    }

    pub fn text_rotated(
        &mut self,
        at: DVec2,
        size_mm: f64,
        text: impl Into<String>,
        anchor: Anchor,
        rotation: f64,
    ) {
        self.items.push(Item::Text {
            at,
            size_mm,
            text: text.into(),
            anchor,
            rotation,
        });
    }

    /// Registration cross centred on `at`.
    pub fn cross(&mut self, at: DVec2, size: f64, style: Style) {
        self.line(
            at - DVec2::new(size, 0.0),
            at + DVec2::new(size, 0.0),
            style,
        );
        self.line(
            at - DVec2::new(0.0, size),
            at + DVec2::new(0.0, size),
            style,
        );
    }

    /// Copy another drawing's items, transformed by `offset` (mm).
    pub fn place(&mut self, other: &Drawing, offset: DVec2) {
        for item in &other.items {
            self.items.push(match item {
                Item::Path {
                    points,
                    closed,
                    style,
                } => Item::Path {
                    points: points.iter().map(|p| *p + offset).collect(),
                    closed: *closed,
                    style: *style,
                },
                Item::Text {
                    at,
                    size_mm,
                    text,
                    anchor,
                    rotation,
                } => Item::Text {
                    at: *at + offset,
                    size_mm: *size_mm,
                    text: text.clone(),
                    anchor: *anchor,
                    rotation: *rotation,
                },
            });
        }
    }

    /// Bounding box of all paths (text ignored).
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut it = self.items.iter().filter_map(|i| match i {
            Item::Path { points, .. } => Some(points),
            Item::Text { .. } => None,
        });
        let first = it.next()?;
        let (mut lo, mut hi) = (first[0], first[0]);
        for pts in std::iter::once(first).chain(it) {
            for p in pts {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        Some((lo, hi))
    }
}

/// Paper sizes in mm (portrait).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    A4,
    A3,
    Letter,
    Tabloid,
}

impl Paper {
    pub const ALL: [Paper; 4] = [Paper::A4, Paper::A3, Paper::Letter, Paper::Tabloid];

    pub fn size(self) -> DVec2 {
        match self {
            Paper::A4 => DVec2::new(210.0, 297.0),
            Paper::A3 => DVec2::new(297.0, 420.0),
            Paper::Letter => DVec2::new(215.9, 279.4),
            Paper::Tabloid => DVec2::new(279.4, 431.8),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::Letter => "US Letter",
            Paper::Tabloid => "US Tabloid",
        }
    }
}

/// One printed page: the drawing region it shows and where that region lands on the paper.
#[derive(Clone, Debug)]
pub struct Page {
    pub paper: DVec2,
    /// Printable area on paper (mm), content is clipped to it.
    pub clip_min: DVec2,
    pub clip_max: DVec2,
    /// Paper position of drawing point (0,0).
    pub offset: DVec2,
    /// Page furniture drawn unclipped (labels, marks), in paper coordinates.
    pub furniture: Drawing,
}

/// Split a 1:1 drawing over pages of `paper` with `margin` and `overlap` (mm). Each of the drawing's
/// [`parts`](Drawing::parts) (or the whole drawing) starts on a new page, is centred on the block of pages it
/// needs, and gets the orientation that needs the fewest pages. Each page gets a row/column label, corner
/// registration crosses and a 100 mm check bar on the first.
pub fn tile(drawing: &Drawing, paper: Paper, margin: f64, overlap: f64, title: &str) -> Vec<Page> {
    let portrait = paper.size();
    let landscape = DVec2::new(portrait.y, portrait.x);
    let regions = if drawing.parts.is_empty() {
        vec![(DVec2::ZERO, DVec2::new(drawing.width, drawing.height))]
    } else {
        drawing.parts.clone()
    };
    struct Layout {
        min: DVec2,
        max: DVec2,
        paper: DVec2,
        cols: usize,
        rows: usize,
        usable: DVec2,
    }
    let layouts: Vec<Layout> = regions
        .iter()
        .map(|&(min, max)| {
            let size = (max - min).max(DVec2::ZERO);
            let count = |p: DVec2| {
                let usable = p - DVec2::splat(2.0 * margin) - DVec2::splat(overlap);
                let cols = ((size.x - overlap) / usable.x).ceil().max(1.0) as usize;
                let rows = ((size.y - overlap) / usable.y).ceil().max(1.0) as usize;
                (cols, rows, usable)
            };
            let (pc, pr, pu) = count(portrait);
            let (lc, lr, lu) = count(landscape);
            let (paper, cols, rows, usable) = if lc * lr < pc * pr {
                (landscape, lc, lr, lu)
            } else {
                (portrait, pc, pr, pu)
            };
            Layout {
                min,
                max,
                paper,
                cols,
                rows,
                usable,
            }
        })
        .collect();
    let total: usize = layouts.iter().map(|l| l.cols * l.rows).sum();
    let pieces = layouts.len();
    let mut pages = Vec::new();
    for (k, l) in layouts.iter().enumerate() {
        let (paper_size, cols, rows, usable) = (l.paper, l.cols, l.rows, l.usable);
        // Centre the piece on the block of pages: equal blank space on both sides.
        let covered = DVec2::new(cols as f64, rows as f64) * usable + DVec2::splat(overlap);
        let shift = ((covered - (l.max - l.min)) / 2.0).max(DVec2::ZERO);
        for r in 0..rows {
            for c in 0..cols {
                let origin = DVec2::new(c as f64 * usable.x, r as f64 * usable.y);
                let offset = DVec2::splat(margin) - origin + shift - l.min;
                // Clip to the printable area and to this piece, so neighbouring pieces do not show.
                let pad = DVec2::splat(1.0);
                let clip_min = DVec2::splat(margin).max(l.min + offset - pad);
                let clip_max = (paper_size - DVec2::splat(margin)).min(l.max + offset + pad);
                let piece = if pieces > 1 {
                    format!(" piece {} of {pieces},", k + 1)
                } else {
                    String::new()
                };
                let label = format!(
                    "{title} —{piece} row {} col {} ({} of {total})",
                    rows - r,
                    c + 1,
                    pages.len() + 1,
                );
                pages.push(page_with_furniture(
                    paper_size,
                    margin,
                    clip_min,
                    clip_max,
                    offset,
                    label,
                    pages.is_empty(),
                ));
            }
        }
    }
    pages
}

fn page_with_furniture(
    paper_size: DVec2,
    margin: f64,
    clip_min: DVec2,
    clip_max: DVec2,
    offset: DVec2,
    label: String,
    first: bool,
) -> Page {
    let mut furniture = Drawing::new(paper_size.x, paper_size.y);
    furniture.text(DVec2::new(margin, margin * 0.4), 3.0, label, Anchor::Left);
    // Registration crosses at the corners of the printable area (same on every page, for lining sheets up).
    let (lo, hi) = (DVec2::splat(margin), paper_size - DVec2::splat(margin));
    for corner in [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)] {
        furniture.cross(corner, 4.0, Style::THIN);
    }
    if first {
        let a = DVec2::new(paper_size.x - margin - 100.0, margin * 0.5);
        furniture.line(a, a + DVec2::new(100.0, 0.0), Style::THIN);
        for i in 0..=10 {
            let x = a + DVec2::new(i as f64 * 10.0, 0.0);
            furniture.line(
                x,
                x + DVec2::new(0.0, if i % 5 == 0 { 2.5 } else { 1.5 }),
                Style::THIN,
            );
        }
        furniture.text(
            a + DVec2::new(50.0, 3.0),
            2.5,
            "100 mm — check print scale",
            Anchor::Centre,
        );
    }
    Page {
        paper: paper_size,
        clip_min,
        clip_max,
        offset,
        furniture,
    }
}

/// A single page holding the drawing scaled to fit (for spec sheets and previews).
pub fn fit_page(drawing: &Drawing, paper: DVec2) -> Page {
    Page {
        paper,
        clip_min: DVec2::ZERO,
        clip_max: paper,
        offset: (paper - DVec2::new(drawing.width, drawing.height)) / 2.0,
        furniture: Drawing::new(paper.x, paper.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_drawing() {
        let d = Drawing::new(1900.0, 250.0);
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "t");
        // Portrait A4 usable 180 x 267 mm -> 11 x 1; landscape would need 8 x 2.
        assert_eq!(pages.len(), 11);
        assert!(pages.iter().all(|p| p.paper.x < p.paper.y));
    }

    #[test]
    fn small_drawing_is_centred_on_its_page() {
        let d = Drawing::new(100.0, 50.0);
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "t");
        assert_eq!(pages.len(), 1);
        let p = &pages[0];
        let centre = p.offset + DVec2::new(50.0, 25.0);
        assert!((centre - p.paper / 2.0).length() < 1e-9, "{centre:?}");
    }

    #[test]
    fn each_part_gets_its_own_centred_pages() {
        let mut d = Drawing::new(400.0, 60.0);
        d.part(DVec2::new(0.0, 0.0), DVec2::new(150.0, 60.0));
        d.part(DVec2::new(150.0, 0.0), DVec2::new(400.0, 60.0));
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "t");
        // 150 mm fits one portrait page; 250 mm fits one landscape page (usable 267 mm).
        assert_eq!(pages.len(), 2);
        let c0 = pages[0].offset + DVec2::new(75.0, 30.0);
        assert!((c0 - pages[0].paper / 2.0).length() < 1e-9);
        assert!(
            pages[1].paper.x > pages[1].paper.y,
            "second piece goes landscape"
        );
        let c1 = pages[1].offset + DVec2::new(275.0, 30.0);
        assert!((c1 - pages[1].paper / 2.0).length() < 1e-9);
        // The first page is clipped to its piece, so the second piece does not show on it.
        assert!(pages[0].clip_max.x <= pages[0].offset.x + 151.0);
        assert!(
            pages[0].furniture.items.len() > pages[1].furniture.items.len(),
            "scale bar on page 1"
        );
    }

    #[test]
    fn long_drawing_is_centred_across_its_pages() {
        let d = Drawing::new(1900.0, 250.0);
        let pages = tile(&d, Paper::A4, 10.0, 10.0, "t");
        let (first, last) = (&pages[0], pages.last().unwrap());
        // Blank space before the tail on the first page equals blank space after the nose on the last.
        let before = first.offset.x - 10.0;
        let after = (last.paper.x - 10.0) - (last.offset.x + 1900.0);
        assert!((before - after).abs() < 1e-9, "{before} vs {after}");
        assert!(before > 0.0);
    }
}
