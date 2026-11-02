//! Fin boxes on the printed templates.

use surfcraft_core::fins::{BoxSystem, FCS1, FCS2, FinSetup, Preset, Side, US_BOX};
use surfcraft_core::{Board, DVec2};
use surfcraft_export::drawing::{Drawing, Item};
use surfcraft_export::templates::{self, OutlineOptions};

fn board(preset: Preset, system: BoxSystem) -> Board {
    let mut b = surfcraft_io::templates::TEMPLATES[0].board();
    b.set_fin_setup(FinSetup::preset(preset, system, &b));
    b
}

fn cm(v: f64) -> String {
    format!("{v:.2} cm")
}

/// Largest distance between two points of a polygon (mm).
fn diameter(pts: &[DVec2]) -> f64 {
    let mut d: f64 = 0.0;
    for a in pts {
        for b in pts {
            d = d.max(a.distance(*b));
        }
    }
    d
}

fn closed_paths(d: &Drawing) -> Vec<&Vec<DVec2>> {
    d.items
        .iter()
        .filter_map(|i| match i {
            Item::Path {
                points,
                closed: true,
                style,
            } if *style == surfcraft_export::fins::BOX => Some(points),
            _ => None,
        })
        .collect()
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

fn outline(b: &Board, full: bool) -> Drawing {
    templates::outline(
        b,
        OutlineOptions {
            full,
            fins: true,
            ..Default::default()
        },
        &cm,
    )
}

#[test]
fn outline_template_has_the_box_footprints_at_full_size() {
    let b = board(Preset::Five, BoxSystem::FcsII);
    let full = outline(&b, true);
    let boxes = closed_paths(&full);
    assert_eq!(boxes.len(), 5);
    for p in &boxes {
        // A slot with round ends: its longest chord is the plug length, in mm at 1:1.
        assert!(
            (diameter(p) - FCS2.length * 10.0).abs() < 0.01,
            "{}",
            diameter(p)
        );
    }
    // The half template only shows the fins on its side and on the stringer.
    assert_eq!(closed_paths(&outline(&b, false)).len(), 3);
    let t = texts(&full);
    assert!(t.contains(&"Fins: 5 fins, FCS II"), "{t:?}");
    for want in [
        "From tail",
        "From rail",
        "Toe",
        "Cant",
        "Left front",
        "Right rear",
        "Centre",
    ] {
        assert!(t.contains(&want), "{want} missing from {t:?}");
    }
    assert_eq!(t.iter().filter(|s| **s == "FCS II").count(), 5);
}

#[test]
fn fcs1_plugs_are_circles() {
    let b = board(Preset::Thruster, BoxSystem::FcsI);
    let d = outline(&b, true);
    let plugs = closed_paths(&d);
    assert_eq!(plugs.len(), 6);
    for p in &plugs {
        assert!((diameter(p) - FCS1.plug_diameter * 10.0).abs() < 0.01);
    }
    // Plug pairs are the documented distance apart.
    let centre = |p: &Vec<DVec2>| p.iter().copied().sum::<DVec2>() / p.len() as f64;
    let gap = centre(plugs[0]).distance(centre(plugs[1]));
    assert!((gap - (FCS1.plug_offsets[1] - FCS1.plug_offsets[0]) * 10.0).abs() < 1e-6);
}

#[test]
fn two_plus_one_mixes_a_us_box_with_side_boxes() {
    let b = board(Preset::TwoPlusOne, BoxSystem::Futures);
    let d = outline(&b, true);
    // Lengths along the board (the centre box is not toed in).
    let span = |p: &Vec<DVec2>| {
        let xs = p.iter().map(|q| q.x);
        xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min)
    };
    let lens: Vec<f64> = closed_paths(&d).iter().map(|p| span(p)).collect();
    assert_eq!(lens.len(), 3);
    assert_eq!(
        lens.iter()
            .filter(|l| (**l - US_BOX.length * 10.0).abs() < 0.01)
            .count(),
        1
    );
    let s = b.fin_setup.as_ref().unwrap();
    assert!(
        s.fins
            .iter()
            .any(|f| f.side() == Side::Centre && f.system == BoxSystem::UsBox)
    );
}

#[test]
fn spec_sheet_lists_the_setup() {
    let b = board(Preset::Quad, BoxSystem::Futures);
    let d = templates::spec_sheet(&b, b.volume(), DVec2::new(297.0, 210.0), &cm);
    let t = texts(&d);
    assert!(t.contains(&"Fins: Quad, Futures"), "{t:?}");
    assert_eq!(t.iter().filter(|s| **s == "Futures").count(), 4);
    // The four boxes in the plan view, plus the three cross-sections.
    let closed = d
        .items
        .iter()
        .filter(|i| matches!(i, Item::Path { closed: true, .. }))
        .count();
    assert_eq!(closed, 4 + 3);
}

#[test]
fn boards_without_a_setup_print_as_before() {
    let mut b = surfcraft_io::templates::TEMPLATES[0].board();
    b.fins = [27.94, 21.0, 39.0, 20.4, 7.6, 18.0, 11.0, 11.5, 6.0];
    let d = outline(&b, true);
    assert!(!texts(&d).iter().any(|t| t.starts_with("Fins:")));
    let sheet = templates::spec_sheet(&b, b.volume(), DVec2::new(297.0, 210.0), &cm);
    assert!(
        texts(&sheet)
            .iter()
            .any(|t| t.starts_with("Side fins: Front:"))
    );
}
