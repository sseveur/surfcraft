//! Fins window (setup presets, box systems, per-fin table) and the fin drawing in the outline view.

use eframe::egui::{self, Color32, Stroke};
use surfcraft_core::Board;
use surfcraft_core::fin_catalog::{self, Product, Role};
use surfcraft_core::fins::{BoxSystem, Fin, FinSetup, FinShape, Preset, Side};

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
        Preset::TwinTrailer => tr("Twin + 1 (twin + stabiliser)"),
        Preset::Thruster => tr("Thruster"),
        Preset::Quad => tr("Quad"),
        Preset::TwoPlusOne => tr("2+1 (side bites + US box single)"),
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
    /// The fin whose shape is being edited.
    shape_fin: usize,
    /// The catalogue product picked in the fin models list (index into `fin_catalog::products()`).
    product: Option<usize>,
    /// List only products for the setup's box system.
    same_system: bool,
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
        shape_fin: 0,
        product: None,
        same_system: true,
    });
    let search_id = id.with("search");
    let mut search: String = ctx.data(|d| d.get_temp(search_id)).unwrap_or_default();
    let mut use_product = None;
    let mut setup = current.clone();
    let (mut apply, mut clear) = (false, false);
    let board = &doc.board;
    egui::Window::new(tr("Fins"))
        .id(egui::Id::unique("fins-window"))
        .open(open)
        .resizable(false)
        .min_width(760.0)
        .show(ctx, |ui| {
            // Grows with its content up to the screen height, then scrolls.
            let max = ui.ctx().content_rect().height() - 90.0;
            egui::ScrollArea::vertical()
                .id_salt("fins-scroll")
                .max_height(max)
                .min_scrolled_height(max)
                .show(ui, |ui| {
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
                        ui.separator();
                        shape_editor(ui, &mut setup, &mut choice.shape_fin, choice.mirror, set);
                    }
                    ui.separator();
                    if catalog_picker(ui, &mut choice, &mut search, &current, set) {
                        use_product = choice.product;
                    }
                    ui.label(
                        egui::RichText::new(tr(
                            "From tail and from rail: to the fin's trailing edge (rear of the base), square to the stringer. Toe: leading edge turned towards the stringer. Cant: tip tilted towards the rail. Box sizes are approximate: check them against your boxes before routing.",
                        ))
                        .small(),
                    );
                });
        });
    ctx.data_mut(|d| {
        d.insert_temp(id, choice);
        d.insert_temp(search_id, search);
    });
    if let Some(product) = use_product.and_then(|i| fin_catalog::products().get(i)) {
        doc.edit(|b| {
            let mut s = b.fin_setup();
            if s.is_empty() {
                s = FinSetup::preset(product.setup, product.system(), b);
            }
            product.apply(&mut s);
            b.set_fin_setup(s);
        });
    } else if apply {
        doc.edit(|b| {
            let mut s = FinSetup::preset(choice.preset, choice.system, b);
            // Fins in the same role as before keep their shape.
            let old = b.fin_setup();
            for i in 0..s.fins.len() {
                let role = s.role_label(i);
                if let Some(j) = (0..old.fins.len()).find(|&j| old.role_label(j) == role) {
                    s.fins[i].shape = old.fins[j].shape;
                }
            }
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

/// Translated name of a [`FinShape::TEMPLATES`] entry.
fn template_label(english: &str) -> &'static str {
    match english {
        "Raked (swept, small round tip)" => tr("Raked (swept, small round tip)"),
        "Upright (wide round tip)" => tr("Upright (wide round tip)"),
        "Square tip" => tr("Square tip"),
        "Pointed" => tr("Pointed"),
        "Keel (long, low, swept)" => tr("Keel (long, low, swept)"),
        _ => tr("Custom"),
    }
}

/// Foil choices: share of the thickness on the inside face.
const FOILS: [(f64, &str); 3] = [
    (0.5, "50/50 (symmetric)"),
    (0.2, "80/20"),
    (0.0, "Flat inside"),
];

fn foil_label(inside: f64) -> String {
    match FOILS.iter().find(|(v, _)| (v - inside).abs() < 1e-9) {
        Some((_, "50/50 (symmetric)")) => tr("50/50 (symmetric)").into(),
        Some((_, "Flat inside")) => tr("Flat inside").into(),
        Some((_, l)) => (*l).into(),
        None => format!("{:.0}/{:.0}", 100.0 * (1.0 - inside), 100.0 * inside),
    }
}

/// Planform area in the user's unit system (square inches or square centimetres).
fn area_text(cm2: f64, set: &Settings) -> String {
    if set.length_unit().is_imperial() {
        format!("{:.2} sq in", cm2 / (2.54 * 2.54))
    } else {
        format!("{:.1} cm²", cm2)
    }
}

/// The shape of one fin (mirrored to its partner with `mirror`): template, planform, foil and a side view.
fn shape_editor(
    ui: &mut egui::Ui,
    setup: &mut FinSetup,
    sel: &mut usize,
    mirror: bool,
    set: &Settings,
) {
    if setup.fins.is_empty() {
        return;
    }
    *sel = (*sel).min(setup.fins.len() - 1);
    let before = setup.clone();
    ui.strong(tr("Fin shape (3D model and export)"));
    ui.horizontal(|ui| {
        egui::ComboBox::new("fin-shape-fin", tr("Fin"))
            .selected_text(role_label(before.role_label(*sel)))
            .show_ui(ui, |ui| {
                for j in 0..before.fins.len() {
                    ui.selectable_value(sel, j, role_label(before.role_label(j)));
                }
            });
    });
    let i = *sel;
    let mut f = setup.fins[i];
    let centre = f.side() == Side::Centre;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            egui::ComboBox::new("fin-shape-template", tr("Template"))
                .selected_text(f.shape.template().map_or(tr("Custom"), template_label))
                .show_ui(ui, |ui| {
                    for (name, t) in FinShape::TEMPLATES {
                        if ui
                            .selectable_label(f.shape.template() == Some(name), template_label(name))
                            .clicked()
                        {
                            f.shape = FinShape {
                                inside: f.shape.inside,
                                ..t
                            };
                        }
                    }
                });
            egui::Grid::new("fin-shape-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    let pct = |ui: &mut egui::Ui, v: &mut f64, range: std::ops::RangeInclusive<f64>| {
                        let mut p = *v * 100.0;
                        let r = (*range.start() * 100.0)..=(*range.end() * 100.0);
                        if ui
                            .add(
                                egui::Slider::new(&mut p, r)
                                    .suffix(" %")
                                    .custom_formatter(|v, _| format!("{v:.0}")),
                            )
                            .changed()
                        {
                            *v = p / 100.0;
                        }
                    };
                    ui.label(tr("Rake")).on_hover_text(tr(
                        "How far the tip reaches behind the rear of the base, in % of the base",
                    ));
                    pct(ui, &mut f.shape.rake, FinShape::RAKE);
                    ui.end_row();
                    ui.label(tr("Tip width"))
                        .on_hover_text(tr("Width of the tip in % of the base (0 for a pointed tip)"));
                    pct(ui, &mut f.shape.tip, FinShape::TIP);
                    ui.end_row();
                    ui.label(tr("Tip rounding"))
                        .on_hover_text(tr("0 %: square-cut tip, 100 %: fully round"));
                    pct(ui, &mut f.shape.tip_round, 0.0..=1.0);
                    ui.end_row();
                    ui.label(tr("Fullness")).on_hover_text(tr(
                        "How long the fin stays wide going down: low for a slender fin, high for a full one (more area)",
                    ));
                    let r = FinShape::FULLNESS;
                    ui.add(
                        egui::Slider::new(&mut f.shape.fullness, r)
                            .logarithmic(true)
                            .custom_formatter(|v, _| format!("{v:.2}")),
                    );
                    ui.end_row();
                    ui.label(tr("Thickness"))
                        .on_hover_text(tr("Foil thickness at the root, in % of the base"));
                    ui.horizontal(|ui| {
                        pct(ui, &mut f.shape.thickness, FinShape::THICKNESS);
                        ui.weak(set.fmt(f.shape.clamped().thickness * f.base));
                    });
                    ui.end_row();
                    ui.label(tr("Foil"));
                    if centre {
                        ui.label(tr("50/50 (symmetric)"))
                            .on_hover_text(tr("A fin on the stringer is always symmetric"));
                    } else {
                        egui::ComboBox::from_id_salt("fin-shape-foil")
                            .selected_text(foil_label(f.shape.inside))
                            .show_ui(ui, |ui| {
                                for (v, _) in FOILS {
                                    ui.selectable_value(&mut f.shape.inside, v, foil_label(v));
                                }
                            });
                    }
                    ui.end_row();
                    ui.label(tr("Area")).on_hover_text(tr(
                        "Side-view area of the fin. Type a fin maker's published area to match it (changes the fullness).",
                    ));
                    let imperial = set.length_unit().is_imperial();
                    let k = if imperial { 2.54 * 2.54 } else { 1.0 };
                    let area = f.shape.area(f.base, f.depth);
                    let mut shown = area / k;
                    let resp = ui.add(
                        egui::DragValue::new(&mut shown)
                            .speed(0.05)
                            .range(0.1..=2000.0)
                            .max_decimals(2)
                            .suffix(if imperial { " sq in" } else { " cm²" })
                            .update_while_editing(false),
                    );
                    if resp.changed() && (shown * k - area).abs() > 1e-6 {
                        f.shape = f.shape.fitted(f.base, f.depth, Some(shown * k), None);
                    }
                    ui.end_row();
                });
            if ui
                .button(tr("Use this shape on every fin"))
                .on_hover_text(tr(
                    "Same template, tip and foil on all fins; each keeps its own base and depth",
                ))
                .clicked()
            {
                for g in &mut setup.fins {
                    g.shape = f.shape;
                }
            }
        });
        ui.add_space(12.0);
        planform_preview(ui, &f, set);
    });
    if f != before.fins[i] {
        f.shape = f.shape.clamped();
        setup.fins[i] = f;
        if mirror && let Some(j) = before.partner(i) {
            setup.fins[j].shape = f.shape;
        }
    }
}

/// Side view of a fin to scale: the base along the bottom, the planform hanging below it.
fn planform_preview(ui: &mut egui::Ui, f: &Fin, set: &Settings) {
    let size = egui::vec2(240.0, 190.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let pts = f.shape.outline(f.base, f.depth);
    if pts.len() < 3 {
        return;
    }
    let (mut min, mut max) = (pts[0], pts[0]);
    for p in &pts {
        min = min.min(*p);
        max = max.max(*p);
    }
    min.x = min.x.min(0.0);
    max.x = max.x.max(f.base);
    let pad = 16.0;
    let k = ((size.x - 2.0 * pad) as f64 / (max.x - min.x))
        .min((size.y - 2.0 * pad - 14.0) as f64 / (max.y - min.y).max(1e-9));
    // Nose to the right, the board's bottom at the top.
    let to = |u: f64, h: f64| {
        egui::pos2(
            rect.left() + pad + ((u - min.x) * k) as f32,
            rect.top() + pad + (h * k) as f32,
        )
    };
    let colour = Color32::from_rgb(82, 148, 224);
    let outline: Vec<egui::Pos2> = pts.iter().map(|p| to(p.x, p.y)).collect();
    painter.add(egui::Shape::closed_line(outline, Stroke::new(2.0, colour)));
    painter.line_segment(
        [to(min.x - 0.5, 0.0), to(max.x + 0.5, 0.0)],
        Stroke::new(1.0, visuals.weak_text_color()),
    );
    painter.line_segment([to(0.0, 0.0), to(f.base, 0.0)], Stroke::new(3.0, colour));
    painter.text(
        rect.left_bottom() + egui::vec2(6.0, -4.0),
        egui::Align2::LEFT_BOTTOM,
        format!(
            "{} {} · {} {} · {}",
            tr("Base"),
            set.fmt(f.base),
            tr("Depth"),
            set.fmt(f.depth),
            area_text(f.shape.area(f.base, f.depth), set)
        ),
        egui::FontId::proportional(11.0),
        visuals.text_color(),
    );
    painter.text(
        rect.right_top() + egui::vec2(-6.0, 4.0),
        egui::Align2::RIGHT_TOP,
        format!("{} >", tr("Nose")),
        egui::FontId::proportional(11.0),
        visuals.weak_text_color(),
    );
}

/// Translated name of a catalogue role.
fn catalog_role(r: Role) -> &'static str {
    match r {
        Role::Side => tr("Side"),
        Role::Centre => tr("Centre"),
        Role::Rear => tr("Rear"),
        Role::Front => tr("Front"),
        Role::All => tr("All fins"),
    }
}

/// Search the fin makers' models; returns true when the user asks to use the picked one.
fn catalog_picker(
    ui: &mut egui::Ui,
    choice: &mut Choice,
    search: &mut String,
    setup: &FinSetup,
    set: &Settings,
) -> bool {
    let mut use_it = false;
    egui::CollapsingHeader::new(tr("Fin models (FCS, Futures, Firewire)"))
        .id_salt("fin-models")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Search"));
                ui.add(
                    egui::TextEdit::singleline(search)
                        .hint_text(tr("e.g. performer M, F6, keel"))
                        .desired_width(220.0),
                );
                let system = if setup.is_empty() {
                    choice.system
                } else {
                    setup.system
                };
                ui.checkbox(
                    &mut choice.same_system,
                    trf("Only {0}", &[&box_label(system)]),
                );
            });
            let system = if setup.is_empty() {
                choice.system
            } else {
                setup.system
            };
            let found = fin_catalog::search(search, choice.same_system.then_some(system));
            let all = fin_catalog::products();
            ui.weak(trf("{0} of {1} models", &[&found.len(), &all.len()]));
            egui::ScrollArea::vertical()
                .id_salt("fin-models-list")
                .max_height(170.0)
                .show(ui, |ui| {
                    for p in found.iter().take(300) {
                        let i = all
                            .iter()
                            .position(|q| std::ptr::eq(q, *p))
                            .expect("product from the catalogue");
                        let text = format!(
                            "{}  ·  {}  ·  {}",
                            p.name(),
                            preset_label(p.setup),
                            box_label(p.system())
                        );
                        if ui.selectable_label(choice.product == Some(i), text).clicked() {
                            choice.product = Some(i);
                        }
                    }
                });
            let Some(p) = choice.product.and_then(|i| all.get(i)) else {
                return;
            };
            ui.separator();
            product_specs(ui, p, set);
            // Fins of the board this set has nothing for (or nothing that fits their box) stay as they are.
            let unchanged: Vec<&str> = p
                .rows_for(setup)
                .iter()
                .enumerate()
                .filter(|(_, r)| r.is_none())
                .map(|(i, _)| role_label(setup.role_label(i)))
                .collect();
            if !unchanged.is_empty() {
                ui.label(
                    egui::RichText::new(trf(
                        "Not in this set for your boxes, left as they are: {0}",
                        &[&unchanged.join(", ")],
                    ))
                    .color(ui.visuals().warn_fg_color),
                );
            }
            ui.horizontal(|ui| {
                use_it = ui
                    .button(trf("Use {0}", &[&p.name()]))
                    .on_hover_text(tr(
                        "Each fin takes the base, depth, area, sweep, foil and cant (where published) of its role in this set; positions stay. Without fins, the set's setup is placed first. One undo step.",
                    ))
                    .clicked();
                ui.hyperlink_to(tr("Maker's page"), &p.source_url);
            });
            ui.label(
                egui::RichText::new(tr(
                    "Specs as the maker publishes them. The fin outline is fitted to them, not the maker's exact template.",
                ))
                .small(),
            );
        });
    use_it
}

/// Published specs of a product, one row per role.
fn product_specs(ui: &mut egui::Ui, p: &Product, set: &Settings) {
    egui::Grid::new("fin-model-specs")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                tr("Fin"),
                tr("Base"),
                tr("Depth"),
                tr("Area"),
                tr("Sweep (°)"),
                tr("Foil"),
                tr("Cant (°)"),
            ] {
                ui.strong(h);
            }
            ui.end_row();
            for f in &p.fins {
                let opt = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{v:.1}"));
                ui.label(catalog_role(f.role));
                ui.label(set.fmt(f.base));
                ui.label(set.fmt(f.depth));
                ui.label(f.area.map_or("-".to_string(), |a| area_text(a, set)));
                ui.label(opt(f.sweep));
                ui.label(if f.foil.is_empty() { "-" } else { &f.foil });
                ui.label(opt(f.cant));
                ui.end_row();
            }
        });
}
