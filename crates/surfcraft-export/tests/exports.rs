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
