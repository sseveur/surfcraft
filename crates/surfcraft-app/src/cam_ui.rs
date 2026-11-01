//! Machine (CAM) window: settings, toolpath generation in the background, preview, G-code export.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Stroke};
use surfcraft_cam::{
    BoardSide, Cutter, GeneratorKind, HoldingSystem, HotwireConfig, HotwireKind, MachineConfig,
    PassKind, Program,
};
use surfcraft_core::{Board, DVec2};

use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;
use crate::view2d::{Canvas, ViewState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    Deck,
    Bottom,
    HotwireProfile,
    HotwireOutline,
}

impl Job {
    fn label(self) -> &'static str {
        match self {
            Job::Deck => tr("Deck"),
            Job::Bottom => tr("Bottom"),
            Job::HotwireProfile => tr("Hot-wire profile"),
            Job::HotwireOutline => tr("Hot-wire outline"),
        }
    }

    fn generate_label(self) -> &'static str {
        match self {
            Job::Deck => tr("Generate deck"),
            Job::Bottom => tr("Generate bottom"),
            Job::HotwireProfile => tr("Generate hot-wire profile"),
            Job::HotwireOutline => tr("Generate hot-wire outline"),
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Job::Deck => "deck",
            Job::Bottom => "bottom",
            Job::HotwireProfile => "hotwire-profile",
            Job::HotwireOutline => "hotwire-outline",
        }
    }
}

type Slot = Arc<Mutex<Option<(Job, u64, Result<Program, String>)>>>;

#[derive(Default)]
pub struct CamState {
    pub cfg: MachineConfig,
    pub hotwire: HotwireConfig,
    pub program: Option<(Job, u64, Program)>,
    pub error: Option<String>,
    pub(crate) running: Option<Job>,
    slot: Slot,
    plan_view: ViewState,
    side_view: ViewState,
}

pub fn generate(
    board: &Board,
    cfg: &MachineConfig,
    hot: &HotwireConfig,
    job: Job,
) -> Result<Program, String> {
    let r = match job {
        Job::Deck => surfcraft_cam::generate(board, cfg, BoardSide::Deck),
        Job::Bottom => surfcraft_cam::generate(board, cfg, BoardSide::Bottom),
        Job::HotwireProfile => surfcraft_cam::generate_hotwire(board, hot, HotwireKind::Profile),
        Job::HotwireOutline => surfcraft_cam::generate_hotwire(board, hot, HotwireKind::Outline),
    };
    r.map_err(|e| e.to_string())
}

fn kind_color(k: PassKind) -> Color32 {
    match k {
        PassKind::Approach => Color32::GRAY,
        PassKind::Stringer => Color32::from_rgb(255, 200, 60),
        PassKind::NoseCutOff | PassKind::TailCutOff => Color32::from_rgb(255, 90, 80),
        PassKind::Regular | PassKind::WidthSplit => Color32::from_rgb(90, 170, 255),
        PassKind::Rail => Color32::from_rgb(120, 220, 140),
        PassKind::Outline => Color32::from_rgb(220, 120, 255),
        PassKind::Cleanup => Color32::from_rgb(255, 150, 60),
        PassKind::SideChange => Color32::from_rgb(150, 150, 150),
        PassKind::HotwireProfile | PassKind::HotwireOutline => Color32::from_rgb(255, 120, 60),
    }
}

fn preview(ui: &mut egui::Ui, view: &mut ViewState, program: &Program, top: bool, bg: Color32) {
    let pts = program.toolpath.passes.iter().flat_map(|p| p.points.iter());
    let proj = |p: &surfcraft_core::DVec3| {
        if top {
            DVec2::new(p.x, p.y)
        } else {
            DVec2::new(p.x, p.z)
        }
    };
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in pts {
        lo = lo.min(proj(&p.pos));
        hi = hi.max(proj(&p.pos));
    }
    if lo.x > hi.x {
        return;
    }
    let canvas = Canvas::show(ui, view, (lo, hi), bg);
    for pass in &program.toolpath.passes {
        let stroke = Stroke::new(1.0, kind_color(pass.kind));
        canvas.polyline(pass.points.iter().map(|p| proj(&p.pos)), stroke);
    }
}

/// Returns `Some((file name, gcode))` when the user asks to save.
pub fn window(
    ctx: &egui::Context,
    open: &mut bool,
    st: &mut CamState,
    board: &Board,
    revision: u64,
    set: &Settings,
    title: &str,
) -> Option<(String, Vec<u8>)> {
    // Collect finished background work.
    if let Some((job, rev, result)) = st.slot.lock().unwrap().take() {
        st.running = None;
        match result {
            Ok(p) => {
                st.program = Some((job, rev, p));
                st.error = None;
                st.plan_view.needs_fit = true;
                st.side_view.needs_fit = true;
            }
            Err(e) => st.error = Some(e),
        }
    }
    let mut save = None;
    let dark = ctx.global_style().visuals.dark_mode;
    let bg = if dark {
        Color32::from_rgb(24, 26, 30)
    } else {
        Color32::from_rgb(250, 250, 248)
    };
    egui::Window::new(tr("Machine (CAM)"))
        .id(egui::Id::unique("cam-window"))
        .open(open)
        .default_size([980.0, 640.0])
        .resizable(true)
        .show(ctx, |ui| {
            egui::Panel::left("cam-settings").resizable(true).default_size(300.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| settings_ui(ui, st, set));
            });
            ui.horizontal_wrapped(|ui| {
                for job in [Job::Deck, Job::Bottom, Job::HotwireProfile, Job::HotwireOutline] {
                    let busy = st.running.is_some();
                    if ui.add_enabled(!busy, egui::Button::new(job.generate_label())).clicked() {
                        st.running = Some(job);
                        st.error = None;
                        let (b, cfg, hot, slot) = (board.clone(), st.cfg.clone(), st.hotwire.clone(), st.slot.clone());
                        let repaint = ctx.clone();
                        std::thread::spawn(move || {
                            let r = generate(&b, &cfg, &hot, job);
                            *slot.lock().unwrap() = Some((job, revision, r));
                            repaint.request_repaint();
                        });
                    }
                }
                if st.running.is_some() {
                    ui.spinner();
                }
            });
            if let Some(e) = &st.error {
                ui.colored_label(Color32::from_rgb(230, 80, 60), e);
            }
            let Some((job, rev, program)) = &st.program else {
                ui.weak(tr("Generate a program to preview toolpaths. Check the machine settings first."));
                return;
            };
            let s = program.stats();
            ui.horizontal_wrapped(|ui| {
                ui.strong(job.label());
                ui.label(trf(
                    "{0} passes · {1} moves · {2} m cut · about {3} min",
                    &[
                        &s.passes,
                        &s.points,
                        &format!("{:.1}", s.cut_length / 1000.0),
                        &format!("{:.0}", s.estimated_time / 60.0),
                    ],
                ));
                if *rev != revision {
                    ui.colored_label(Color32::from_rgb(230, 160, 40), tr("board changed since: regenerate"));
                }
                if ui.button(tr("Save G-code…")).clicked() {
                    save = Some((format!("{title}-{}.nc", job.slug()), program.to_gcode().into_bytes()));
                }
            });
            // Hot-wire programs are 2D in the X/Z plane: one full-height view of the cut shape.
            let hotwire = matches!(job, Job::HotwireProfile | Job::HotwireOutline);
            if hotwire {
                let h = ui.available_height() - 20.0;
                ui.label(if *job == Job::HotwireOutline { tr("Plan (from above)") } else { tr("Side") });
                ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| preview(ui, &mut st.side_view, program, false, bg));
            } else {
                let h = (ui.available_height() - 8.0) / 2.0;
                ui.label(tr("Plan (from above)"));
                ui.allocate_ui(egui::vec2(ui.available_width(), h - 14.0), |ui| preview(ui, &mut st.plan_view, program, true, bg));
                ui.label(tr("Side"));
                ui.allocate_ui(egui::vec2(ui.available_width(), h - 14.0), |ui| preview(ui, &mut st.side_view, program, false, bg));
            }
        });
    save
}

fn settings_ui(ui: &mut egui::Ui, st: &mut CamState, set: &Settings) {
    let c = &mut st.cfg;
    ui.heading(tr("Toolpaths"));
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut c.generator,
            GeneratorKind::SurfaceSplits,
            tr("Surface splits"),
        );
        let width = matches!(c.generator, GeneratorKind::WidthSplits { .. });
        if ui.radio(width, tr("Width splits")).clicked() && !width {
            c.generator = GeneratorKind::WidthSplits { splits: 30 };
        }
    });
    if let GeneratorKind::WidthSplits { splits } = &mut c.generator {
        ui.horizontal(|ui| {
            ui.label(tr("Passes per side"));
            ui.add(egui::DragValue::new(splits).range(2..=200));
        });
    }
    egui::Grid::new("cam-cuts").num_columns(2).show(ui, |ui| {
        ui.label(tr("Lengthwise step"));
        if let Some(v) = length_field(ui, "cam-res", c.lengthwise_resolution, set) {
            c.lengthwise_resolution = v.clamp(0.05, 20.0);
        }
        ui.end_row();
        for (label, v) in [
            (tr("Deck cuts"), &mut c.deck_cuts),
            (tr("Deck rail cuts"), &mut c.deck_rail_cuts),
            (tr("Bottom cuts"), &mut c.bottom_cuts),
            (tr("Bottom rail cuts"), &mut c.bottom_rail_cuts),
            (tr("Deck clean-up cuts"), &mut c.deck_cleanup_cuts),
            (tr("Bottom clean-up cuts"), &mut c.bottom_cleanup_cuts),
        ] {
            ui.label(label);
            ui.add(egui::DragValue::new(v).range(0..=200));
            ui.end_row();
        }
        for (label, v) in [
            (tr("Deck angle (°)"), &mut c.deck_angle),
            (tr("Deck rail angle (°)"), &mut c.deck_rail_angle),
            (tr("Bottom angle (°)"), &mut c.bottom_angle),
            (tr("Bottom rail angle (°)"), &mut c.bottom_rail_angle),
        ] {
            ui.label(label);
            ui.add(egui::DragValue::new(v).range(0.0..=180.0).speed(0.5));
            ui.end_row();
        }
    });
    ui.checkbox(&mut c.deck_stringer_cut, tr("Deck stringer cut"));
    ui.checkbox(&mut c.bottom_stringer_cut, tr("Bottom stringer cut"));
    ui.checkbox(
        &mut c.stay_away_from_stringer,
        tr("Stay away from stringer"),
    );
    ui.checkbox(&mut c.deck_outline_cut, tr("Deck outline cut"));
    ui.checkbox(&mut c.bottom_outline_cut, tr("Bottom outline cut"));
    ui.checkbox(&mut c.cut_off_nose, tr("Cut off nose"));
    ui.checkbox(&mut c.cut_off_tail, tr("Cut off tail"));
    ui.checkbox(&mut c.parallel, tr("Use all CPU cores"));

    ui.separator();
    ui.heading(tr("Speeds (mm/s)"));
    egui::Grid::new("cam-speeds").num_columns(2).show(ui, |ui| {
        for (label, v) in [
            (tr("Cutting"), &mut c.cutting_speed),
            (tr("Stringer"), &mut c.stringer_speed),
            (tr("Rail"), &mut c.rail_speed),
            (tr("Outline"), &mut c.outline_speed),
        ] {
            ui.label(label);
            ui.add(egui::DragValue::new(v).range(1.0..=2000.0));
            ui.end_row();
        }
    });

    ui.separator();
    ui.heading(tr("Cutter"));
    if let Cutter::Bullnose(b) = &mut c.cutter {
        egui::Grid::new("cam-cutter").num_columns(2).show(ui, |ui| {
            ui.label(tr("Diameter"));
            if let Some(v) = length_field(ui, "cam-dia", b.diameter, set) {
                b.diameter = v.max(0.1);
                // Keep the corner valid when the cutter gets smaller.
                b.corner_radius = b.corner_radius.min(b.diameter / 2.0);
            }
            ui.end_row();
            ui.label(tr("Corner radius"));
            if let Some(v) = length_field(ui, "cam-rad", b.corner_radius, set) {
                b.corner_radius = v.clamp(0.0, b.diameter / 2.0);
            }
            ui.end_row();
        });
    } else {
        ui.weak(tr("Custom cutter (from settings)."));
    }

    ui.separator();
    ui.heading(tr("Blank holding"));
    let mut kind = match c.holding {
        HoldingSystem::Supports(_) => 0,
        HoldingSystem::EndClamps(_) => 1,
        HoldingSystem::RotatingBoard(_) => 2,
    };
    let before = kind;
    ui.radio_value(&mut kind, 0, tr("Two supports and tail stop"));
    ui.radio_value(&mut kind, 1, tr("Nose and tail clamps"));
    if kind != before {
        c.holding = match kind {
            0 => HoldingSystem::Supports(Default::default()),
            _ => HoldingSystem::EndClamps(Default::default()),
        };
    }
    ui.horizontal(|ui| {
        // Stored in cm like every other length; shown and typed in the user's units.
        ui.label(tr("Clear height"));
        if let Some(v) = length_field(ui, "cam-clear", c.clear_height, set) {
            c.clear_height = v.clamp(0.0, 50.0);
        }
    });
    if ui.button(tr("Reset to defaults")).clicked() {
        st.cfg = MachineConfig::default();
    }

    ui.separator();
    ui.heading(tr("Hot-wire"));
    egui::Grid::new("cam-hotwire")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr("Wire speed (mm/s)"));
            ui.add(
                egui::DragValue::new(&mut st.hotwire.speed)
                    .range(0.1..=100.0)
                    .speed(0.1),
            );
            ui.end_row();
            ui.label(tr("Extra material"));
            if let Some(v) = length_field(ui, "cam-expand", st.hotwire.expand, set) {
                st.hotwire.expand = v.max(0.0);
            }
            ui.end_row();
            ui.label(tr("Kerf (mm)"));
            ui.add(
                egui::DragValue::new(&mut st.hotwire.kerf)
                    .range(0.0..=10.0)
                    .speed(0.05),
            );
            ui.end_row();
        });
}
