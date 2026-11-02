//! Board → Design by numbers: the shaper's key dimensions of the open board, each editable; typing a new value
//! reshapes the curves smoothly around that station as one undo step (see `surfcraft_session::by_numbers`).

use eframe::egui;
use surfcraft_session::by_numbers::{self, Dimension, Group, Options, Refusal};

use crate::document::Document;
use crate::i18n::{tr, trf};
use crate::panels::length_field;
use crate::settings::Settings;

/// Window state: the change options and the last result message.
#[derive(Clone, Debug, Default)]
pub struct NumbersState {
    /// Spread of width and thickness changes each side of the station; `None` = a third of the board.
    pub span: Option<f64>,
    /// Rocker changes leave the flat part of the bottom alone.
    pub loose_flat: bool,
    /// Last message, and whether it is a warning.
    pub message: Option<(String, bool)>,
}

fn label(d: Dimension) -> &'static str {
    match d {
        Dimension::MaxWidth => tr("Max width"),
        Dimension::WidePoint => tr("Wide point from centre"),
        Dimension::NoseWidth12 => tr("Nose width at 12\""),
        Dimension::TailWidth12 => tr("Tail width at 12\""),
        Dimension::NoseWidth30cm => tr("Nose width at 30 cm"),
        Dimension::TailWidth30cm => tr("Tail width at 30 cm"),
        Dimension::TailBlock => tr("Tail block width"),
        Dimension::CenterWidth => tr("Centre width"),
        Dimension::NoseRocker => tr("Nose rocker"),
        Dimension::TailRocker => tr("Tail rocker"),
        Dimension::NoseRocker12 => tr("Rocker 12\" from nose"),
        Dimension::NoseRocker24 => tr("Rocker 24\" from nose"),
        Dimension::TailRocker12 => tr("Rocker 12\" from tail"),
        Dimension::TailRocker24 => tr("Rocker 24\" from tail"),
        Dimension::MaxThickness => tr("Max thickness"),
        Dimension::NoseThickness12 => tr("Thickness 12\" from nose"),
        Dimension::TailThickness12 => tr("Thickness 12\" from tail"),
        Dimension::Foil => tr("Thickest point from centre"),
    }
}

fn refusal(r: &Refusal) -> &'static str {
    match r {
        Refusal::NotAvailable => tr("This board does not have that dimension."),
        Refusal::OutOfRange => tr("That value is out of range for this board."),
        Refusal::TooFar => tr(
            "Cannot reshape the curves that far smoothly; try a smaller change or a longer span.",
        ),
    }
}

/// Show the window while `state` is set.
pub fn window(
    ctx: &egui::Context,
    state: &mut Option<NumbersState>,
    doc: &mut Document,
    set: &Settings,
) {
    let mut open = state.is_some();
    let Some(s) = state.as_mut() else {
        return;
    };
    egui::Window::new(tr("Design by numbers"))
        .id(egui::Id::unique("design-by-numbers"))
        .open(&mut open)
        .resizable(false)
        .default_pos(ctx.content_rect().right_top() + egui::vec2(-420.0, 80.0))
        .show(ctx, |ui| {
            ui.weak(tr(
                "Type a new value: the curves are reshaped smoothly around that station. Each change is one undo step.",
            ));
            let metric = !set.length_unit().is_imperial();
            let mut change: Option<(Dimension, f64)> = None;
            for (group, heading) in [
                (Group::Outline, tr("Outline")),
                (Group::Rocker, tr("Rocker")),
                (Group::Thickness, tr("Thickness")),
            ] {
                ui.add_space(4.0);
                ui.strong(heading);
                egui::Grid::new(("numbers", heading))
                    .num_columns(2)
                    .min_col_width(190.0)
                    .show(ui, |ui| {
                        for d in Dimension::ALL.into_iter().filter(|d| d.group() == group) {
                            let metric_only =
                                matches!(d, Dimension::NoseWidth30cm | Dimension::TailWidth30cm);
                            if metric_only && !metric {
                                continue;
                            }
                            let Some(v) = d.measure(&doc.board) else {
                                continue;
                            };
                            let l = ui.label(label(d));
                            if d.is_position() {
                                l.on_hover_text(tr("Positive toward the nose"));
                            }
                            if let Some(new) = length_field(ui, ("numbers", d.code()), v, set) {
                                change = Some((d, new));
                            }
                            ui.end_row();
                        }
                    });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("Span"))
                    .on_hover_text(tr(
                        "How far a width or thickness change spreads each side of the station (default: a third of the board)",
                    ));
                let span = s.span.unwrap_or(doc.board.length() / 3.0);
                if let Some(v) = length_field(ui, "numbers-span", span, set)
                    && v.is_finite()
                    && v >= 5.0
                {
                    s.span = Some(v);
                }
            });
            let mut keep = !s.loose_flat;
            ui.checkbox(&mut keep, tr("Keep the bottom flat in the middle"))
                .on_hover_text(tr("Rocker changes leave the flat part of the bottom untouched"));
            s.loose_flat = !keep;
            if let Some((d, v)) = change {
                let opts = Options {
                    span: s.span,
                    keep_flat: keep,
                };
                let mut b = doc.board.clone();
                s.message = Some(match by_numbers::set_dimension(&mut b, d, v, opts) {
                    Ok(out) => {
                        doc.edit(|board| *board = b);
                        if let Some(c) = out.new_inflections.first() {
                            (
                                trf("The {0} now bends both ways somewhere; check it for fairness.", &[&tr(c)]),
                                true,
                            )
                        } else if out.inserted_knots == 1 {
                            (
                                tr("Done. Added a control point to keep the change local.").to_string(),
                                false,
                            )
                        } else if out.inserted_knots > 1 {
                            (
                                trf("Done. Added {0} control points to keep the change local.", &[&out.inserted_knots]),
                                false,
                            )
                        } else {
                            (tr("Done.").to_string(), false)
                        }
                    }
                    Err(e) => (refusal(&e).to_string(), true),
                });
            }
            if let Some((m, warn)) = &s.message {
                if *warn {
                    ui.colored_label(egui::Color32::from_rgb(230, 120, 40), m);
                } else {
                    ui.weak(m);
                }
            }
        });
    if !open {
        *state = None;
    }
}
