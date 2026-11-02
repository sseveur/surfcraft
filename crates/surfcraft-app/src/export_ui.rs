//! "Print & export" window: templates and spec sheet as PDF/SVG/DXF, board mesh as STL/OBJ/3MF/glTF.

use eframe::egui;
use surfcraft_core::{Board, DVec2, Mesh, MeshDetail};
use surfcraft_export::drawing::{Drawing, Paper, fit_page, tile};
use surfcraft_export::{dxf, gltf, mesh_formats, pdf, svg, templates, threemf, woodwork};

use crate::i18n::{tr, trf};
use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Outline,
    OutlineFull,
    Spin,
    Profile,
    Sections,
    SpecSheet,
    Model,
    /// A woodworking template from `surfcraft_export::woodwork::TEMPLATES` (by name).
    Wood(&'static str),
}

impl What {
    pub const SHAPER: [What; 7] = [
        What::Outline,
        What::OutlineFull,
        What::Spin,
        What::Profile,
        What::Sections,
        What::SpecSheet,
        What::Model,
    ];

    pub fn label(self) -> &'static str {
        match self {
            What::Outline => tr("Outline template (half, 1:1)"),
            What::OutlineFull => tr("Outline template (full, 1:1)"),
            What::Spin => tr("Spin template (1:1)"),
            What::Profile => tr("Profile template (1:1)"),
            What::Sections => tr("Cross-section templates (1:1)"),
            What::SpecSheet => tr("Spec sheet (one page)"),
            What::Model => tr("3D model"),
            What::Wood(name) => woodwork::TEMPLATES
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(name, |(_, l)| tr(l)),
        }
    }

    fn slug(self) -> &'static str {
        match self {
            What::Outline => "outline",
            What::OutlineFull => "outline-full",
            What::Spin => "spin",
            What::Profile => "profile",
            What::Sections => "sections",
            What::SpecSheet => "spec-sheet",
            What::Model => "model",
            What::Wood(name) => name,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Svg,
    Dxf,
    Stl,
    Obj,
    ThreeMf,
    Glb,
}

impl Format {
    fn ext(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Svg => "svg",
            Format::Dxf => "dxf",
            Format::Stl => "stl",
            Format::Obj => "obj",
            Format::ThreeMf => "3mf",
            Format::Glb => "glb",
        }
    }

    pub fn is_model(self) -> bool {
        matches!(
            self,
            Format::Stl | Format::Obj | Format::ThreeMf | Format::Glb
        )
    }

    fn label(self) -> &'static str {
        match self {
            Format::Pdf => tr("PDF (print, tiled pages)"),
            Format::Svg => tr("SVG (1:1, one sheet)"),
            Format::Dxf => tr("DXF (1:1, for CNC/laser)"),
            Format::Stl => tr("STL (binary, mm)"),
            Format::Obj => "OBJ (mm)",
            Format::ThreeMf => tr("3MF (3D printing, mm)"),
            Format::Glb => tr("glTF binary .glb (web, AR, metres)"),
        }
    }
}

pub struct ExportState {
    pub what: What,
    pub format: Format,
    pub paper: Paper,
    pub overlap_mm: f64,
    pub fine_mesh: bool,
    /// Model scale denominator: 1 = full size, 10 = a 1:10 scale model.
    pub scale_ratio: f64,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            what: What::Outline,
            format: Format::Pdf,
            paper: Paper::A4,
            overlap_mm: 10.0,
            fine_mesh: true,
            scale_ratio: 1.0,
        }
    }
}

/// Build the drawing for a 2D export.
pub fn drawing(board: &Board, what: What, paper: Paper, set: &Settings) -> Drawing {
    let fmt = |cm: f64| set.fmt(cm);
    match what {
        // Boards with a fin setup get the fin boxes and their measurements on the outline template.
        What::Outline => templates::outline(
            board,
            templates::OutlineOptions {
                fins: board.fin_setup.is_some(),
                ..Default::default()
            },
            &fmt,
        ),
        What::OutlineFull => templates::outline(
            board,
            templates::OutlineOptions {
                full: true,
                fins: board.fin_setup.is_some(),
                ..Default::default()
            },
            &fmt,
        ),
        What::Spin => templates::spin(board, &fmt),
        What::Profile => templates::profile(board, &fmt),
        What::Sections => templates::sections(board, &fmt),
        What::SpecSheet | What::Model => {
            let p = paper.size();
            templates::spec_sheet(board, board.volume(), DVec2::new(p.y, p.x), &fmt)
        }
        What::Wood(name) => woodwork::by_name(board, name, &fmt).unwrap_or_default(),
    }
}

/// Encode an export to bytes.
pub fn render(board: &Board, st: &ExportState, set: &Settings, title: &str) -> Vec<u8> {
    match st.format {
        Format::Stl | Format::Obj | Format::ThreeMf | Format::Glb => {
            let detail = if st.fine_mesh {
                MeshDetail {
                    stations: 160,
                    half_section: 40,
                }
            } else {
                MeshDetail::default()
            };
            let mesh = Mesh::of_board(board, detail);
            let scale = 1.0 / st.scale_ratio.max(1e-3);
            match st.format {
                Format::Stl => mesh_formats::stl(&mesh_formats::scaled(&mesh, scale), title),
                Format::Obj => {
                    mesh_formats::obj(&mesh_formats::scaled(&mesh, scale), title).into_bytes()
                }
                Format::ThreeMf => threemf::write(&mesh, title, scale),
                _ => gltf::write(&mesh, title, scale),
            }
        }
        Format::Pdf => {
            let d = drawing(board, st.what, st.paper, set);
            let pages = if st.what == What::SpecSheet {
                vec![fit_page(&d, DVec2::new(d.width, d.height))]
            } else {
                tile(
                    &d,
                    st.paper,
                    10.0,
                    st.overlap_mm,
                    &format!("{title} {}", st.what.slug()),
                )
            };
            pdf::write(&d, &pages, title)
        }
        Format::Svg => svg::write(&drawing(board, st.what, st.paper, set)).into_bytes(),
        Format::Dxf => dxf::write(&drawing(board, st.what, st.paper, set)).into_bytes(),
    }
}

pub fn page_count(board: &Board, st: &ExportState, set: &Settings) -> usize {
    if st.what == What::SpecSheet {
        return 1;
    }
    let d = drawing(board, st.what, st.paper, set);
    tile(&d, st.paper, 10.0, st.overlap_mm, "").len()
}

/// Returns `Some((suggested file name, bytes))` when the user clicks Export.
pub fn window(
    ctx: &egui::Context,
    open: &mut bool,
    st: &mut ExportState,
    board: &Board,
    set: &Settings,
    title: &str,
) -> Option<(String, Vec<u8>)> {
    let mut result = None;
    egui::Window::new(tr("Print & export"))
        .id(egui::Id::unique("export-window"))
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(tr("What"));
            for w in What::SHAPER {
                ui.radio_value(&mut st.what, w, w.label());
            }
            egui::CollapsingHeader::new(tr("Wooden board templates"))
                .id_salt("wood-templates")
                .show(ui, |ui| {
                    for (name, label) in woodwork::TEMPLATES {
                        ui.radio_value(&mut st.what, What::Wood(name), tr(label));
                    }
                });
            ui.separator();
            let formats: &[Format] = if st.what == What::Model {
                &[Format::Stl, Format::Obj, Format::ThreeMf, Format::Glb]
            } else {
                &[Format::Pdf, Format::Svg, Format::Dxf]
            };
            if !formats.contains(&st.format) {
                st.format = formats[0];
            }
            ui.label(tr("Format"));
            for &f in formats {
                ui.radio_value(&mut st.format, f, f.label());
            }
            if st.format == Format::Pdf {
                ui.horizontal(|ui| {
                    egui::ComboBox::new("paper", tr("Paper"))
                        .selected_text(tr(st.paper.label()))
                        .show_ui(ui, |ui| {
                            for p in Paper::ALL {
                                ui.selectable_value(&mut st.paper, p, tr(p.label()));
                            }
                        });
                    if st.what != What::SpecSheet {
                        ui.label(tr("Overlap"));
                        ui.add(
                            egui::DragValue::new(&mut st.overlap_mm)
                                .range(0.0..=40.0)
                                .suffix(" mm"),
                        );
                    }
                });
                ui.weak(trf(
                    "{0} page(s). Print at 100% / actual size, then check the 100 mm bar.",
                    &[&page_count(board, st, set)],
                ));
            }
            if st.what == What::Model {
                ui.checkbox(&mut st.fine_mesh, tr("High resolution mesh"));
                scale_row(ui, &mut st.scale_ratio);
            }
            ui.separator();
            if ui.button(tr("Export…")).clicked() {
                let scale = if st.format.is_model() && st.scale_ratio != 1.0 {
                    format!("-1to{}", st.scale_ratio)
                } else {
                    String::new()
                };
                let name = format!("{title}-{}{scale}.{}", st.what.slug(), st.format.ext());
                result = Some((name, render(board, st, set, title)));
            }
        });
    result
}

/// Scale presets for 3D-printed scale models, plus a custom ratio.
fn scale_row(ui: &mut egui::Ui, ratio: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(tr("Scale")).on_hover_text(tr(
            "Full size, or a scale model for 3D printing (1:10 is a tenth of the size)",
        ));
        for (label, factor) in mesh_formats::SCALES {
            let r = 1.0 / factor;
            if ui
                .selectable_label((*ratio - r).abs() < 1e-9, label)
                .clicked()
            {
                *ratio = r;
            }
        }
        ui.add(
            egui::DragValue::new(ratio)
                .range(1.0..=100.0)
                .speed(0.1)
                .prefix("1:"),
        )
        .on_hover_text(tr("Custom scale"));
    });
}

pub fn extension_of(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or("")
}
