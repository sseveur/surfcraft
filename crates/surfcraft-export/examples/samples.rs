//! Write sample exports of a .brd file: `cargo run -p surfcraft-export --example samples -- board.brd out_dir`

use surfcraft_core::{DVec2, Mesh, MeshDetail};
use surfcraft_export::drawing::{Paper, fit_page, tile};
use surfcraft_export::{dxf, mesh_formats, pdf, svg, templates, woodwork};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: samples <board.brd> <out_dir>");
        std::process::exit(2);
    };
    let board = surfcraft_io::brd::read(&std::fs::read(&input).unwrap()).unwrap();
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let fmt = |cm: f64| surfcraft_core::LengthUnit::FeetInchesFraction.format(cm, 16);

    let a4 = Paper::A4.size();
    let sheet = templates::spec_sheet(&board, board.volume(), DVec2::new(a4.y, a4.x), &fmt);
    let page = fit_page(&sheet, DVec2::new(sheet.width, sheet.height));
    std::fs::write(
        out.join("spec_sheet.pdf"),
        pdf::write(&sheet, &[page], "Spec sheet"),
    )
    .unwrap();
    std::fs::write(out.join("spec_sheet.svg"), svg::write(&sheet)).unwrap();

    let outline = templates::outline(&board, templates::OutlineOptions::default(), &fmt);
    let pages = tile(&outline, Paper::A4, 10.0, 10.0, "Outline");
    std::fs::write(
        out.join("outline_template.pdf"),
        pdf::write(&outline, &pages, "Outline"),
    )
    .unwrap();
    std::fs::write(out.join("outline.dxf"), dxf::write(&outline)).unwrap();

    let sections = templates::sections(&board, &fmt);
    let pages = tile(&sections, Paper::A4, 10.0, 10.0, "Sections");
    std::fs::write(
        out.join("sections.pdf"),
        pdf::write(&sections, &pages, "Sections"),
    )
    .unwrap();

    let tiled = |name: &str, title: &str, d: &surfcraft_export::Drawing| {
        let pages = tile(d, Paper::A4, 10.0, 10.0, title);
        std::fs::write(out.join(name), pdf::write(d, &pages, title)).unwrap();
    };
    let sandwich = woodwork::SandwichOptions::default();
    tiled(
        "sandwich_rail.pdf",
        "Sandwich rail",
        &woodwork::sandwich_rail(&board, &sandwich, &fmt),
    );
    let hws = woodwork::HwsOptions::default();
    tiled(
        "hws_stringer.pdf",
        "HWS stringer",
        &woodwork::hws_stringer(&board, &hws, &fmt),
    );
    tiled(
        "hws_ribs.pdf",
        "HWS ribs",
        &woodwork::hws_ribs(&board, &hws, &fmt),
    );
    let planks = woodwork::chambered_planks(&board, &woodwork::ChamberedOptions::default(), &fmt);
    tiled(
        "chambered.pdf",
        "Chambered planks",
        &woodwork::stack(&planks, 10.0),
    );

    let mesh = Mesh::of_board(&board, MeshDetail::default());
    std::fs::write(out.join("board.stl"), mesh_formats::stl(&mesh, "board")).unwrap();
    println!("wrote samples to {}", out.display());
}
