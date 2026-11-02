use surfcraft_core::{Mesh, MeshDetail};
use surfcraft_export::drawing::{Paper, fit_page, tile};
use surfcraft_export::{dxf, mesh_formats, pdf, svg, templates};

fn board() -> surfcraft_core::Board {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/shortboard.brd"
    ))
    .unwrap();
    surfcraft_io::brd::read(&bytes).unwrap()
}

fn cm(v: f64) -> String {
    format!("{v:.1} cm")
}

#[test]
fn stl_is_closed_and_full_size() {
    let b = board();
    let mesh = Mesh::of_board(&b, MeshDetail::default());
    assert!(mesh.is_watertight());
    let rel = (mesh.volume() - b.volume()).abs() / b.volume();
    assert!(rel < 0.02, "mesh volume off by {:.2}%", rel * 100.0);
    let bytes = mesh_formats::stl(&mesh, "shortboard");
    assert_eq!(bytes.len(), 84 + 50 * mesh.triangles.len());
    let tris = mesh_formats::stl_triangles(&bytes).unwrap();
    let max_x = tris.iter().flatten().map(|v| v[0]).fold(f32::MIN, f32::max);
    assert!(
        (max_x as f64 - b.length() * 10.0).abs() < 2.0,
        "length in mm: {max_x}"
    );
    let obj = mesh_formats::obj(&mesh, "shortboard");
    assert_eq!(
        obj.lines().filter(|l| l.starts_with("f ")).count(),
        mesh.triangles.len()
    );
}

#[test]
fn templates_are_true_scale() {
    let b = board();
    let d = templates::outline(&b, templates::OutlineOptions::default(), &cm);
    let (lo, hi) = d.bounds().unwrap();
    // Stringer guide extends 5 mm past each end.
    assert!(((hi.x - lo.x) - (b.length() * 10.0 + 10.0)).abs() < 0.5);
    let full = templates::outline(
        &b,
        templates::OutlineOptions {
            full: true,
            ..Default::default()
        },
        &cm,
    );
    let (lo, hi) = full.bounds().unwrap();
    assert!(((hi.y - lo.y) - b.max_width() * 10.0).abs() < 1.0);
    let s = templates::sections(&b, &cm);
    assert!(s.width > 0.0 && !s.items.is_empty());
}

#[test]
fn writers_produce_output() {
    let b = board();
    let d = templates::profile(&b, &cm);
    let pages = tile(&d, Paper::A4, 10.0, 10.0, "profile");
    assert!(pages.len() > 1);
    let bytes = pdf::write(&d, &pages, "profile");
    assert!(bytes.starts_with(b"%PDF"));
    let s = svg::write(&d);
    assert!(s.contains("<polyline") && s.contains("mm\""));
    let x = dxf::write(&d);
    assert!(x.contains("POLYLINE") && x.trim_end().ends_with("EOF"));

    let a4 = Paper::A4.size();
    let sheet = templates::spec_sheet(&b, b.volume(), surfcraft_core::DVec2::new(a4.y, a4.x), &cm);
    let page = fit_page(&sheet, sheet_size(&sheet));
    let one = pdf::write(&sheet, &[page], "spec");
    assert_eq!(
        String::from_utf8_lossy(&one)
            .matches("/Type /Page ")
            .count(),
        1
    );
}

fn sheet_size(d: &surfcraft_export::Drawing) -> surfcraft_core::DVec2 {
    surfcraft_core::DVec2::new(d.width, d.height)
}

/// The shortboard with its left rail 2 cm wider at the middle knot.
fn asym_board() -> surfcraft_core::Board {
    use surfcraft_core::{DVec2, Rail};
    let mut b = board();
    b.set_asymmetric(true, Rail::Right);
    let left = b.outline_left.as_mut().unwrap();
    let i = left.len() / 2;
    let p = left.knots()[i].point();
    left.knots_mut()[i].move_point_to(p + DVec2::new(0.0, 2.0));
    b.on_outline_changed();
    b
}

#[test]
fn asymmetric_stl_reaches_each_rail() {
    use surfcraft_core::Rail;
    let b = asym_board();
    let mesh = Mesh::of_board(&b, MeshDetail::default());
    assert!(mesh.is_watertight());
    let rel = (mesh.volume() - b.volume()).abs() / b.volume();
    assert!(rel < 0.02, "mesh volume off by {:.2}%", rel * 100.0);
    let tris = mesh_formats::stl_triangles(&mesh_formats::stl(&mesh, "asym")).unwrap();
    let ys = || tris.iter().flatten().map(|v| v[1] as f64);
    let (ymax, ymin) = (ys().fold(f64::MIN, f64::max), ys().fold(f64::MAX, f64::min));
    let (left, right) = (
        b.max_half_width(Rail::Left).0,
        b.max_half_width(Rail::Right).0,
    );
    assert!(left > right + 1.5);
    // mm; the mesh samples the rail apex, a little inside the outline.
    assert!((ymax - left * 10.0).abs() < 3.0, "{ymax} vs {left}");
    assert!((-ymin - right * 10.0).abs() < 3.0, "{ymin} vs {right}");
    // Per station: the extent on each side follows that rail's outline.
    for &target in &[60.0, 90.0, 130.0] {
        // The mesh station nearest the target.
        let x = tris
            .iter()
            .flatten()
            .map(|v| v[0] as f64 / 10.0)
            .min_by(|a, c| (a - target).abs().total_cmp(&(c - target).abs()))
            .unwrap();
        let near: Vec<&[f32; 3]> = tris
            .iter()
            .flatten()
            .filter(|v| (v[0] as f64 / 10.0 - x).abs() < 1e-3)
            .collect();
        let hi = near
            .iter()
            .map(|v| v[1] as f64 / 10.0)
            .fold(f64::MIN, f64::max);
        let lo = near
            .iter()
            .map(|v| v[1] as f64 / 10.0)
            .fold(f64::MAX, f64::min);
        assert!(
            (hi - b.half_width_at(x, Rail::Left)).abs() < 0.5,
            "{x}: {hi}"
        );
        assert!(
            (-lo - b.half_width_at(x, Rail::Right)).abs() < 0.5,
            "{x}: {lo}"
        );
    }
}

#[test]
fn asymmetric_templates_draw_both_rails() {
    use surfcraft_core::Rail;
    use surfcraft_export::drawing::Style;
    let b = asym_board();
    let (left, right) = (
        b.max_half_width(Rail::Left).0,
        b.max_half_width(Rail::Right).0,
    );
    // Even the half template is drawn full: one half cannot describe the board.
    for full in [false, true] {
        let opts = templates::OutlineOptions {
            full,
            ..Default::default()
        };
        let d = templates::outline(&b, opts, &cm);
        let (lo, hi) = d.bounds().unwrap();
        let stringer = surfcraft_export::woodwork::paths_with_style(&d, Style::GUIDE)
            .find(|p| p.len() == 2 && (p[0].y - p[1].y).abs() < 1e-9)
            .map(|p| p[0].y)
            .unwrap();
        assert!(((hi.y - stringer) - left * 10.0).abs() < 1.0, "{full}");
        assert!(((stringer - lo.y) - right * 10.0).abs() < 1.0, "{full}");
    }
    let spin = templates::spin(&b, &cm);
    let (lo, hi) = spin.bounds().unwrap();
    let mid = b.length() / 2.0;
    assert!(hi.y - lo.y > b.half_width_at(mid, Rail::Right) * 10.0 * 1.5);
    let a4 = Paper::A4.size();
    let page = surfcraft_core::DVec2::new(a4.y, a4.x);
    let sheet = templates::spec_sheet(&b, b.volume(), page, &cm);
    assert!(!sheet.items.is_empty());
    let ribs = surfcraft_export::woodwork::by_name(&b, "hws_ribs", &cm).unwrap();
    let sym = surfcraft_export::woodwork::by_name(&board(), "hws_ribs", &cm).unwrap();
    assert!(ribs.height > 1.9 * sym.height);
}
