//! Fins window (setup presets, box systems, per-fin table) and the fin drawing in the outline view.

use eframe::egui::{self, Color32, Stroke};
use surfcraft_core::Board;
use surfcraft_core::fins::{BoxSystem, Fin, FinSetup, Preset, Side};

use crate::document::Document;
use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;
use crate::view2d::Canvas;

pub fn preset_label(p: Preset) -> &'static str {
    match p {
        Preset::Custom => tr("Custom"),
        Preset::Single => tr("Single"),
        Preset::Twin => tr("Twin"),
        Preset::TwinTrailer => tr("Twin + trailer"),
        Preset::Thruster => tr("Thruster"),
        Preset::Quad => tr("Quad"),
        Preset::TwoPlusOne => tr("2+1"),
        Preset::Five => tr("5 fins"),
    }
}

pub fn box_label(b: BoxSystem) -> &'static str {
    match b {
        BoxSystem::None => tr("None (position only)"),
        BoxSystem::FcsI => tr("FCS I"),
        BoxSystem::FcsII => tr("FCS II"),
        BoxSystem::Futures => tr("Futures"),
        BoxSystem::UsBox => tr("US box (single)"),
    }
}

/// Translated name of a fin from [`FinSetup::role_label`].
fn role_label(english: &str) -> &'static str {
    match english {
        "Centre" => tr("Centre"),
        "Left" => tr("Left"),
        "Right" => tr("Right"),
        "Left front" => tr("Left front"),
        "Left rear" => tr("Left rear"),
        "Left middle" => tr("Left middle"),
        "Right front" => tr("Right front"),
        "Right rear" => tr("Right rear"),
        _ => tr("Right middle"),
    }
}

/// The choices of the window that are not part of the board yet.
#[derive(Clone, Copy)]
struct Choice {
    preset: Preset,
    system: BoxSystem,
    mirror: bool,
}

pub fn window(ctx: &egui::Context, open: &mut bool, doc: &mut Document, set: &Settings) {
    let id = egui::Id::unique("fins-ui-choice");
    let current = doc.board.fin_setup();
    let mut choice = ctx.data(|d| d.get_temp::<Choice>(id)).unwrap_or(Choice {
        preset: match current.preset {
            Preset::Custom => Preset::Thruster,
            p => p,
        },
        system: match current.system {
            BoxSystem::None => BoxSystem::FcsII,
            s => s,
        },
        mirror: true,
    });
    let mut setup = current.clone();
    let (mut apply, mut clear) = (false, false);
    let board = &doc.board;
    egui::Window::new(tr("Fins"))
        .id(egui::Id::unique("fins-window"))
        .open(open)
        .resizable(false)
        .min_width(760.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::new("fins-preset", tr("Setup"))
                    .selected_text(preset_label(choice.preset))
                    .show_ui(ui, |ui| {
                        for p in Preset::ALL {
                            ui.selectable_value(&mut choice.preset, p, preset_label(p));
                        }
                    });
                ui.add_space(12.0);
                egui::ComboBox::new("fins-box", tr("Box system"))
                    .selected_text(box_label(choice.system))
                    .show_ui(ui, |ui| {
                        for b in BoxSystem::ALL {
                            ui.selectable_value(&mut choice.system, b, box_label(b));
                        }
                    });
            });
            ui.horizontal(|ui| {
                apply = ui
                    .button(tr("Apply preset"))
                    .on_hover_text(tr(
                        "Place the fins of this setup at standard positions for this board's length and outline (one undo step).",
                    ))
                    .clicked();
                clear = ui
                    .add_enabled(!setup.is_empty(), egui::Button::new(tr("Remove fins")))
                    .clicked();
            });
            ui.separator();
            if setup.is_empty() {
                ui.label(tr("No fins. Choose a setup and press Apply preset."));
            } else {
                ui.label(trf(
                    "On the board: {0}, {1}",
                    &[&preset_label(setup.preset), &box_label(setup.system)],
                ));
                fin_table(ui, board, &mut setup, choice.mirror, set);
                ui.checkbox(&mut choice.mirror, tr("Mirror edits to the other side"));
            }
            ui.label(
                egui::RichText::new(tr(
                    "From tail and from rail: to the fin's trailing edge (rear of the base), square to the stringer. Toe: leading edge turned towards the stringer. Cant: tip tilted towards the rail. Box sizes are approximate: check them against your boxes before routing.",
                ))
                .small(),
            );
        });
    ctx.data_mut(|d| d.insert_temp(id, choice));
    if apply {
        doc.edit(|b| {
            let s = FinSetup::preset(choice.preset, choice.system, b);
            b.set_fin_setup(s);
        });
    } else if clear {
        doc.edit(|b| b.set_fin_setup(FinSetup::default()));
    } else if setup != current {
        // One undo step per field edit (closed by the app when focus leaves the field).
        doc.edit_grouped(|b| b.set_fin_setup(setup));
    }
}

/// One row per fin; edits go into `setup` (and the mirrored fin with `mirror`).
fn fin_table(ui: &mut egui::Ui, board: &Board, setup: &mut FinSetup, mirror: bool, set: &Settings) {
    egui::Grid::new("fins-table")
        .num_columns(8)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                tr("Fin"),
                tr("From tail"),
                tr("From rail"),
                tr("Toe (°)"),
                tr("Cant (°)"),
                tr("Base"),
                tr("Depth"),
                tr("Box"),
            ] {
                ui.strong(h);
            }
            ui.end_row();
            let before = setup.clone();
            for i in 0..setup.fins.len() {
                let f0 = setup.fins[i];
                let mut f = f0;
                let centre = f.side() == Side::Centre;
                ui.label(role_label(before.role_label(i)));
                if let Some(v) = cell(ui, ("fin-x", i), f.x, set) {
                    // A side fin keeps its distance from the rail as it moves along.
                    let rail = f.from_rail(board);
                    f.x = v.clamp(0.0, board.length());
                    if !centre {
                        f.set_from_rail(board, rail);
                    }
                }
                if centre {
                    ui.label("-");
                } else if let Some(v) = cell(ui, ("fin-rail", i), f.from_rail(board), set) {
                    f.set_from_rail(board, v);
                }
                if centre {
                    ui.label("-");
                    ui.label("-");
                } else {
                    ui.add(
                        egui::DragValue::new(&mut f.toe)
                            .speed(0.05)
                            .range(-10.0..=10.0)
                            .max_decimals(2),
                    );
                    ui.add(
                        egui::DragValue::new(&mut f.cant)
                            .speed(0.1)
                            .range(-15.0..=20.0)
                            .max_decimals(1),
                    );
                }
                if let Some(v) = cell(ui, ("fin-base", i), f.base, set) {
                    f.base = v.clamp(0.5, 60.0);
                }
                if let Some(v) = cell(ui, ("fin-depth", i), f.depth, set) {
                    f.depth = v.clamp(0.0, 60.0);
                }
                egui::ComboBox::from_id_salt(("fin-box", i))
                    .selected_text(box_label(f.system))
                    .show_ui(ui, |ui| {
                        for b in BoxSystem::ALL {
                            ui.selectable_value(&mut f.system, b, box_label(b));
                        }
                    });
                ui.end_row();
                if f != f0 {
                    setup.fins[i] = f;
                    if mirror && let Some(j) = before.partner(i) {
                        setup.fins[j] = f.mirrored();
                    }
                }
            }
        });
}

/// A length field sized for the fin table (grid cells otherwise start too narrow).
fn cell(ui: &mut egui::Ui, id: (&str, usize), value: f64, set: &Settings) -> Option<f64> {
    ui.allocate_ui(egui::vec2(78.0, ui.spacing().interact_size.y), |ui| {
        length_field(ui, id, value, set)
    })
    .inner
}

/// Fins in the outline view: each fin's base and its box footprint, to scale and turned by the toe.
pub fn draw_outline(canvas: &Canvas, board: &Board, color: Color32) {
    let setup = board.fin_setup();
    for f in &setup.fins {
        draw_fin(canvas, f, color);
    }
}

fn draw_fin(canvas: &Canvas, f: &Fin, color: Color32) {
    canvas.segment(f.rear(), f.front(), Stroke::new(3.0, color));
    for mut poly in f.footprint() {
        if let Some(&first) = poly.first() {
            poly.push(first);
        }
        canvas.polyline(poly, Stroke::new(1.0, color));
    }
}
