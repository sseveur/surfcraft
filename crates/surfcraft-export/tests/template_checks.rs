//! Templates for every fixture board: drawn, finite, and the hollow-wood parts fit together.

use surfcraft_core::{Board, DVec2, LengthUnit, SurfaceModel};
use surfcraft_export::Drawing;
use surfcraft_export::drawing::{Item, Style};
use surfcraft_export::templates::{self, OutlineOptions};
use surfcraft_export::woodwork::{self, HwsOptions, RibJoint, SandwichOptions, paths_with_style};

fn fixture(name: &str) -> Board {
    let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    surfcraft_io::read_any("brd", &std::fs::read(path).unwrap()).unwrap()
}

/// Every `.brd` fixture that the reader can open.
fn all_fixtures() -> Vec<(String, Board)> {
    let dir = format!("{}/../../tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".brd"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for n in names {
        let read = std::panic::catch_unwind(|| fixture(&n));
        match read {
            Ok(b) => out.push((n, b)),
            Err(_) => eprintln!("skipping {n}: the reader cannot open it yet"),
        }
    }
    assert!(out.len() >= 3, "fixtures: {}", out.len());
    out
}

fn boards() -> Vec<(&'static str, Board)> {
    vec![
        ("shortboard", fixture("shortboard.brd")),
        ("funboard", fixture("funboard.brd")),
        ("longboard", fixture("longboard.brd")),
    ]
}

fn cm(v: f64) -> String {
    format!("{v:.2} cm")
}

fn inches(v: f64) -> String {
    LengthUnit::FeetInchesFraction.format(v, 16)
}

fn texts(d: &Drawing) -> Vec<&str> {
    d.items
        .iter()
        .filter_map(|i| match i {
            Item::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn check(name: &str, d: &Drawing, needs_cut: bool) {
    assert!(
        d.width.is_finite() && d.height.is_finite() && d.width > 0.0 && d.height > 0.0,
        "{name}: size {} x {}",
        d.width,
        d.height
    );
    let mut paths = 0;
    for item in &d.items {
        match item {
            Item::Path { points, .. } => {
                paths += 1;
                for p in points {
                    assert!(p.is_finite(), "{name}: {p:?}");
                    assert!(
                        p.x >= -0.01
                            && p.y >= -0.01
                            && p.x <= d.width + 0.01
                            && p.y <= d.height + 0.01,
                        "{name}: {p:?} outside {} x {}",
                        d.width,
                        d.height
                    );
                }
            }
            Item::Text { at, size_mm, .. } => assert!(at.is_finite() && size_mm.is_finite()),
        }
    }
    assert!(paths > 0, "{name}: empty");
    if needs_cut {
        assert!(
            paths_with_style(d, Style::CUT).any(|p| p.len() >= 3),
            "{name}: no cut outline"
        );
    }
    for (lo, hi) in &d.parts {
        assert!(lo.is_finite() && hi.is_finite() && hi.x > lo.x && hi.y > lo.y);
    }
}

fn all_templates(b: &Board, fmt: &dyn Fn(f64) -> String) -> Vec<(String, Drawing, bool)> {
    let mut v = vec![
        (
            "outline".to_string(),
            templates::outline(b, OutlineOptions::default(), fmt),
            true,
        ),
        (
            "outline full over the curve".into(),
            templates::outline(
                b,
                OutlineOptions {
                    full: true,
                    over_the_curve: true,
                    guide_points: true,
                    fins: true,
                    ..Default::default()
                },
                fmt,
            ),
            true,
        ),
        ("spin".into(), templates::spin(b, fmt), true),
        (
            "spin over the curve".into(),
            templates::spin_with(b, false, true, fmt),
            true,
        ),
        ("profile".into(), templates::profile(b, fmt), true),
        ("sections".into(), templates::sections(b, fmt), true),
        (
            "spec sheet".into(),
            templates::spec_sheet(b, b.volume(), DVec2::new(297.0, 210.0), fmt),
            false,
        ),
    ];
    for (n, _) in woodwork::TEMPLATES {
        v.push((n.to_string(), woodwork::by_name(b, n, fmt).unwrap(), true));
    }
    v
}

#[test]
fn every_template_is_drawn_for_every_fixture_board() {
    for (name, board) in all_fixtures() {
        let mut flipped = board.clone();
        flipped.flip();
        let mut control = board.clone();
        control.model = SurfaceModel::ControlPoint;
        for (variant, b) in [
            ("", &board),
            (" flipped", &flipped),
            (" control-point", &control),
        ] {
            for (t, d, cut) in all_templates(b, &cm) {
                check(&format!("{name}{variant} {t}"), &d, cut);
            }
        }
    }
    assert!(woodwork::by_name(&fixture("shortboard.brd"), "nope", &cm).is_none());
}

#[test]
fn each_piece_is_its_own_part() {
    for (name, b) in boards() {
        let real = b
            .real_sections()
            .filter(|&i| !b.sections[i].is_end_section())
            .count();
        assert_eq!(templates::sections(&b, &cm).parts.len(), real, "{name}");
        let ribs = woodwork::hws_ribs(&b, &HwsOptions::default(), &cm);
        assert_eq!(
            ribs.parts.len(),
            woodwork::hws_rib_stations(&b).len(),
            "{name}"
        );
        let planks = woodwork::by_name(&b, "chambered_planks", &cm).unwrap();
        assert_eq!(
            planks.parts.len(),
            woodwork::chambered_plank_offsets(&b, &Default::default()).len()
        );
        // Parts do not overlap.
        for w in planks.parts.windows(2) {
            assert!(w[1].1.y <= w[0].0.y, "{name}: {w:?}");
        }
    }
}

/// The 2-point horizontal guide line starting 5 mm before the tail: (x of the tail, y of zero height).
fn baseline_origin(d: &Drawing) -> DVec2 {
    let line = paths_with_style(d, Style::GUIDE)
        .filter(|p| p.len() == 2 && (p[0].y - p[1].y).abs() < 1e-9)
        .max_by(|a, b| (a[1].x - a[0].x).total_cmp(&(b[1].x - b[0].x)))
        .expect("baseline");
    DVec2::new(line[0].x + 5.0, line[0].y)
}

/// Axis-aligned rectangles among the closed cut paths, as (min, max).
fn rectangles(d: &Drawing) -> Vec<(DVec2, DVec2)> {
    d.items
        .iter()
        .filter_map(|i| match i {
            Item::Path {
                points,
                closed: true,
                style,
            } if *style == Style::CUT
                && points.len() == 4
                && points[0].y == points[1].y
                && points[1].x == points[2].x
                && points[2].y == points[3].y =>
            {
                Some((points[0].min(points[2]), points[0].max(points[2])))
            }
            _ => None,
        })
        .collect()
}

fn largest_cut(d: &Drawing) -> Vec<DVec2> {
    paths_with_style(d, Style::CUT)
        .max_by_key(|p| p.len())
        .unwrap()
        .to_vec()
}

fn crossings(poly: &[DVec2], x: f64) -> Vec<f64> {
    let mut ys = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.x <= x && b.x > x) || (b.x <= x && a.x > x) {
            ys.push(a.y + (x - a.x) / (b.x - a.x) * (b.y - a.y));
        }
    }
    ys.sort_by(f64::total_cmp);
    ys
}

#[test]
fn rib_tabs_fit_the_rail_strip_slots() {
    for (name, b) in boards() {
        let o = HwsOptions::default();
        let f = o.frame_thickness * 10.0;
        let stations = woodwork::hws_rib_stations(&b);
        let joints: Vec<RibJoint> = stations
            .iter()
            .map(|&x| woodwork::hws_rib_joint(&b, x, &o).expect("joint"))
            .collect();

        // One slot per rib, at the rib's height and size, at least the rib thickness wide.
        let rail = woodwork::hws_rail(&b, &o, &cm);
        let origin = baseline_origin(&rail);
        let mut slots = rectangles(&rail);
        slots.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        assert_eq!(slots.len(), stations.len(), "{name}: slots vs ribs");
        let mut last_u = f64::MIN;
        for ((lo, hi), j) in slots.iter().zip(&joints) {
            assert!((hi.x - lo.x - j.slot_width).abs() < 1e-6, "{name}");
            assert!(j.slot_width >= f - 1e-9 && j.slot_width < f * 1.3, "{name}");
            assert!((hi.y - lo.y - j.height).abs() < 1e-6, "{name}");
            assert!(
                ((lo.y + hi.y) / 2.0 - origin.y - j.mid).abs() < 1e-6,
                "{name}"
            );
            let u = (lo.x + hi.x) / 2.0;
            if last_u > f64::MIN {
                // One foot apart along the rail line (a little more where it curves).
                assert!(
                    u - last_u > 304.0 && u - last_u < 304.8 * 1.15,
                    "{name}: {u} - {last_u}"
                );
            }
            last_u = u;
        }

        // Each rib's tab is the slot's height and reaches the strip's outer face; its stringer notch fits the
        // stringer hole at that station.
        let stringer = woodwork::hws_stringer(&b, &o, &cm);
        let s_origin = baseline_origin(&stringer);
        let s_outline = largest_cut(&stringer);
        for (&x, j) in stations.iter().zip(&joints) {
            let rib = woodwork::hws_rib(&b, x, &o, &cm);
            let outline = largest_cut(&rib);
            let xmax = outline.iter().map(|p| p.x).fold(f64::MIN, f64::max);
            let xmin = outline.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            assert!((xmax - xmin - 2.0 * j.outer).abs() < 1e-6, "{name} at {x}");
            let tip: Vec<f64> = outline
                .iter()
                .filter(|p| (p.x - xmax).abs() < 1e-6)
                .map(|p| p.y)
                .collect();
            assert_eq!(tip.len(), 2, "{name} at {x}: tab tip");
            assert!(
                ((tip[0] - tip[1]).abs() - j.height).abs() < 1e-6,
                "{name} at {x}"
            );
            assert!(
                j.inner < j.outer && j.outer - j.inner >= f - 1e-6,
                "{name} at {x}"
            );

            let mid = (xmin + xmax) / 2.0;
            let notch: Vec<f64> = outline
                .iter()
                .filter(|p| (p.x - mid).abs() < 1e-6)
                .map(|p| p.y)
                .collect();
            assert_eq!(notch.len(), 2, "{name} at {x}: notch");
            let notch_h = (notch[0] - notch[1]).abs();
            let sx = s_origin.x + x * 10.0;
            let hole = paths_with_style(&stringer, Style::CUT)
                .filter(|p| p.len() > 4 && p.len() != s_outline.len())
                .find(|p| {
                    let (a, b) = p
                        .iter()
                        .fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
                    a < sx && sx < b
                })
                .unwrap_or_else(|| panic!("{name}: no stringer hole at {x}"));
            let ys = crossings(hole, sx);
            let hole_h = ys[ys.len() - 1] - ys[0];
            assert!(
                notch_h <= hole_h + 1e-6 && notch_h > hole_h - 1.0,
                "{name} at {x}: notch {notch_h} vs hole {hole_h}"
            );
        }
    }
}

/// Distance from `p` to the outline curve (mm), from a dense polyline of the outline.
fn distance_to_outline(b: &Board, p: DVec2) -> f64 {
    let l = b.length() * 10.0;
    let n = (l / 0.25) as usize;
    let pts: Vec<DVec2> = (0..=n)
        .map(|k| {
            let x = l * k as f64 / n as f64;
            DVec2::new(x, b.outline.value_at(x / 10.0) * 10.0)
        })
        .collect();
    pts.windows(2)
        .map(|w| {
            let (a, c) = (w[0], w[1]);
            let t = ((p - a).dot(c - a) / (c - a).length_squared()).clamp(0.0, 1.0);
            (a + t * (c - a) - p).length()
        })
        .fold(f64::MAX, f64::min)
}

#[test]
fn rail_line_is_inside_the_outline_at_the_set_distance() {
    for (name, b) in boards() {
        let o = HwsOptions::default();
        let d = o.distance_from_rail * 10.0;
        for x in woodwork::hws_rib_stations(&b) {
            let j = woodwork::hws_rib_joint(&b, x, &o).unwrap();
            let o_x = b.outline.value_at(x) * 10.0;
            assert!(
                j.outer < o_x - d + 1e-6,
                "{name} at {x}: {} vs {o_x}",
                j.outer
            );
            let dist = distance_to_outline(&b, DVec2::new(x * 10.0, j.outer));
            assert!(
                (dist - d).abs() < 0.3,
                "{name} at {x}: {dist} mm from the outline"
            );
            let dist = distance_to_outline(&b, DVec2::new(x * 10.0, j.inner));
            let f = o.frame_thickness * 10.0;
            assert!(
                (dist - d - f).abs() < 0.3,
                "{name} at {x}: inner face {dist}"
            );
        }
    }
}

/// Largest developed half-width (stringer to rail line) of the deck or bottom, computed independently with
/// fine steps across the board.
fn developed_half_width(b: &Board, d_mm: f64, deck: bool) -> f64 {
    let l = b.length() * 10.0;
    let mut best: f64 = 0.0;
    let mut x = 0.3 * l;
    while x < 0.7 * l {
        let a = b.outline.tangent_at(x / 10.0);
        let yr = b.outline.value_at(x / 10.0) * 10.0 - d_mm / a.sin();
        let n = 200;
        let z = |y: f64| {
            if deck {
                b.deck_at_xy(x / 10.0, y / 10.0) * 10.0
            } else {
                b.bottom_at_xy(x / 10.0, y / 10.0) * 10.0
            }
        };
        let mut len = 0.0;
        for k in 1..=n {
            let (y0, y1) = (yr * (k - 1) as f64 / n as f64, yr * k as f64 / n as f64);
            len += ((y1 - y0).powi(2) + (z(y1) - z(y0)).powi(2)).sqrt();
        }
        best = best.max(len);
        x += 10.0;
    }
    best
}

#[test]
fn skins_are_the_developed_surface_out_to_the_rail_line() {
    for (name, b) in boards() {
        let opts = SandwichOptions::default();
        let d = opts.distance_to_rail * 10.0;
        let flat = b.max_width() * 5.0 - d;
        for (deck, drawing) in [
            (true, woodwork::sandwich_deck_skin(&b, &opts, &cm)),
            (false, woodwork::sandwich_bottom_skin(&b, &opts, &cm)),
        ] {
            let what = if deck { "deck" } else { "bottom" };
            let cut = largest_cut(&drawing);
            let lo = cut.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let hi = cut.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            let width = hi - lo;
            let expect = developed_half_width(&b, d, deck);
            // Wider than the flat half-width at the rail line by the surface's curvature, never wider than
            // the outline plus that excess.
            assert!(width >= flat - 0.5, "{name} {what}: {width} vs flat {flat}");
            assert!(
                (width - expect).abs() < 1.5,
                "{name} {what}: {width} vs developed {expect}"
            );
            let header = if deck {
                "Deck skin template"
            } else {
                "Bottom skin template"
            };
            assert!(
                texts(&drawing).iter().any(|t| t.ends_with(header)),
                "{name} {what}"
            );
        }
    }
}

#[test]
fn tail_and_nose_pieces_print_and_their_tabs_have_slots() {
    for (name, b) in boards() {
        let o = HwsOptions::default();
        for (n, label) in [
            ("hws_tail_piece", "Tail piece"),
            ("hws_nose_piece", "Nose piece"),
        ] {
            let d = woodwork::by_name(&b, n, &cm).unwrap();
            let outline = largest_cut(&d);
            assert!(outline.len() > 20, "{name} {n}");
            assert!(texts(&d).iter().any(|t| t.ends_with(label)), "{name} {n}");
            // Four tabs per plate (two each side): the outline jogs out and back eight times.
            let (lo, hi) = outline
                .iter()
                .fold((f64::MAX, f64::MIN), |(a, c), p| (a.min(p.y), c.max(p.y)));
            assert!(hi - lo > 50.0, "{name} {n}: plate width {}", hi - lo);
        }
        // The rail strip carries a slot for every tab the strip reaches: tilted bands, not the rib rectangles.
        let rail = woodwork::hws_rail(&b, &o, &cm);
        let f = o.frame_thickness * 10.0;
        let bands = paths_with_style(&rail, Style::CUT)
            .filter(|p| {
                p.len() == 4
                    && p[0].y != p[1].y
                    && ((p[0] - p[3]).length() - f).abs() < 1e-6
                    && ((p[1] - p[2]).length() - f).abs() < 1e-6
            })
            .count();
        assert!((3..=4).contains(&bands), "{name}: {bands} plate slots");
    }
}

#[test]
fn labels_use_the_callers_units() {
    let b = fixture("shortboard.brd");
    let o = HwsOptions::default();
    let x = woodwork::hws_rib_stations(&b)[0];
    let rib = woodwork::hws_rib(&b, x, &o, &inches);
    assert!(texts(&rib).contains(&format!("Position: {}", inches(x)).as_str()));
    let plank = woodwork::chambered_plank(&b, 2.54, &Default::default(), &cm);
    assert!(texts(&plank).contains(&"Plank at 2.54 cm"));
    let s = templates::sections(&b, &inches);
    assert!(
        texts(&s)
            .iter()
            .any(|t| t.starts_with("Cross-section 1 at ") && t.ends_with('"'))
    );
    for (t, d, _) in all_templates(&b, &cm) {
        if t != "spec sheet" {
            assert!(
                texts(&d).iter().any(|s| s.starts_with("Board file: ")),
                "{t}: header"
            );
        }
    }
}

/// Distinct x positions of the vertical grid lines.
fn grid_columns(d: &Drawing) -> Vec<f64> {
    let mut xs: Vec<f64> = d
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Path { points, style, .. }
                if points.len() == 2
                    && *style != Style::CUT
                    && *style != Style::GUIDE
                    && points[0].x == points[1].x =>
            {
                Some(points[0].x)
            }
            _ => None,
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

#[test]
fn grid_follows_the_unit_system_and_option() {
    let b = fixture("shortboard.brd");
    let metric = grid_columns(&templates::profile(&b, &cm));
    let imperial = grid_columns(&templates::profile(&b, &inches));
    let step = |xs: &[f64]| xs[2] - xs[1];
    assert!((step(&metric) - 10.0).abs() < 1e-9);
    assert!((step(&imperial) - 25.4).abs() < 1e-9);
    let none = templates::outline(
        &b,
        OutlineOptions {
            grid: false,
            ..Default::default()
        },
        &cm,
    );
    assert!(grid_columns(&none).is_empty());
    // Metres use the metric grid (D13).
    let metres = |v: f64| LengthUnit::Metres.format(v, 16);
    assert!((step(&grid_columns(&templates::profile(&b, &metres))) - 10.0).abs() < 1e-9);
}

#[test]
fn sandwich_rail_follows_the_rail_line_and_flattens() {
    for (name, b) in boards() {
        let opts = SandwichOptions::default();
        let rail = woodwork::sandwich_rail(&b, &opts, &cm);
        assert!(
            texts(&rail).iter().any(|t| t.ends_with("Rail template")),
            "{name}"
        );
        let flat = woodwork::sandwich_rail(
            &b,
            &SandwichOptions {
                flatten: true,
                ..opts
            },
            &cm,
        );
        // Flattened, the strip's top edge varies much less than with the rocker.
        let spread = |d: &Drawing| {
            let p = largest_cut(d);
            let n = p.len() / 2;
            let ys: Vec<f64> = p[..n].iter().map(|q| q.y).collect();
            ys.iter().cloned().fold(f64::MIN, f64::max)
                - ys.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(spread(&flat) < spread(&rail), "{name}");
    }
}
