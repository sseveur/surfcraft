//! Volume advisor: a suggested volume range for a rider (Guild factor method, see `docs/volume-advisor.md`), in the
//! New board dialog ("Suggest volume") and as Board > Volume advisor…. The rider is remembered for the session.

use eframe::egui;
use surfcraft_core::rider::{self, BoardType, Fitness, Rider, Skill, VolumeAdvice, Waves};
use surfcraft_core::units::POUNDS_PER_KG;

use crate::document::Document;
use crate::i18n::{tr, trf};
use crate::settings::Settings;

fn skill_label(s: Skill) -> &'static str {
    match s {
        Skill::Beginner => tr("Beginner"),
        Skill::Intermediate => tr("Intermediate"),
        Skill::Advanced => tr("Advanced"),
        Skill::Pro => tr("Pro"),
    }
}

fn fitness_label(f: Fitness) -> &'static str {
    match f {
        Fitness::Low => tr("Low"),
        Fitness::Average => tr("Average"),
        Fitness::High => tr("High"),
    }
}

fn waves_label(w: Waves) -> &'static str {
    match w {
        Waves::Small => tr("Small / weak"),
        Waves::Average => tr("Average"),
        Waves::Powerful => tr("Powerful / hollow"),
    }
}

fn board_label(b: BoardType) -> &'static str {
    match b {
        BoardType::Shortboard => tr("Shortboard"),
        BoardType::Fish => tr("Fish / hybrid"),
        BoardType::MidLength => tr("Mid-length / funboard"),
        BoardType::Longboard => tr("Longboard"),
    }
}

fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut T,
    all: &[T],
    label: fn(T) -> &'static str,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*value))
        .show_ui(ui, |ui| {
            for &v in all {
                ui.selectable_value(value, v, label(v));
            }
        });
}

fn rider_id() -> egui::Id {
    egui::Id::unique("volume-advisor-rider")
}

/// The rider form; returns the suggestion for the entered rider.
pub fn rider_form(ui: &mut egui::Ui, set: &Settings) -> Option<VolumeAdvice> {
    let mut r: Rider = ui.data(|d| d.get_temp(rider_id())).unwrap_or_default();
    let before = r;
    let imperial = set.length_unit().is_imperial();
    egui::Grid::new(ui.scope_id().with("rider-grid"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr("Rider weight"));
            let (mut w, unit) = if imperial {
                (r.weight_kg * POUNDS_PER_KG, " lb")
            } else {
                (r.weight_kg, " kg")
            };
            let shown = w;
            ui.add(
                egui::DragValue::new(&mut w)
                    .range(if imperial { 22.0..=550.0 } else { 10.0..=250.0 })
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(unit)
                    // Apply a typed weight on Enter only: applied per keystroke, the first digit is clamped to
                    // the minimum and (in pounds, via kg) the text resets around the caret.
                    .update_while_editing(false),
            );
            if w != shown {
                r.weight_kg = if imperial { w / POUNDS_PER_KG } else { w };
            }
            ui.end_row();
            ui.label(tr("Age"));
            ui.add(
                egui::DragValue::new(&mut r.age)
                    .range(0..=99)
                    .custom_formatter(|v, _| {
                        if v < 1.0 {
                            "-".into()
                        } else {
                            format!("{v:.0}")
                        }
                    }),
            )
            .on_hover_text(tr("Optional: older surfers paddle better with more volume"));
            ui.end_row();
            ui.label(tr("Skill"));
            combo(ui, "rider-skill", &mut r.skill, &Skill::ALL, skill_label);
            ui.end_row();
            ui.label(tr("Fitness"));
            combo(
                ui,
                "rider-fitness",
                &mut r.fitness,
                &Fitness::ALL,
                fitness_label,
            );
            ui.end_row();
            ui.label(tr("Typical waves"));
            combo(ui, "rider-waves", &mut r.waves, &Waves::ALL, waves_label);
            ui.end_row();
            ui.label(tr("Board type"));
            combo(
                ui,
                "rider-board",
                &mut r.board,
                &BoardType::ALL,
                board_label,
            );
            ui.end_row();
        });
    if r != before {
        ui.data_mut(|d| d.insert_temp(rider_id(), r));
    }
    let advice = rider::suggest_volume(&r);
    match advice {
        Some(a) => {
            ui.label(
                egui::RichText::new(trf(
                    "Suggested volume: {0} to {1} L",
                    &[&format!("{:.1}", a.min), &format!("{:.1}", a.max)],
                ))
                .strong(),
            );
        }
        None => {
            ui.weak(tr("Enter the rider's weight."));
        }
    }
    ui.weak(tr(
        "A starting point (Guild factor method): try boards and adjust to taste.",
    ));
    advice
}

/// "Suggest volume" section of the New board dialog: fills `litres` with the middle of the range.
pub fn suggest_in_new_board(ui: &mut egui::Ui, litres: &mut String, set: &Settings) {
    egui::CollapsingHeader::new(tr("Suggest volume"))
        .id_salt("nb-suggest-volume")
        .show(ui, |ui| {
            if let Some(a) = rider_form(ui, set) {
                let mid = format!("{:.1}", a.middle());
                if ui
                    .button(trf("Use {0} L", &[&mid]))
                    .on_hover_text(tr("Put the middle of the range into the target volume"))
                    .clicked()
                {
                    *litres = mid;
                }
            }
        });
}

/// Board > Volume advisor…: the suggestion against the open board, with a button to reach it.
pub fn advisor_window(
    ctx: &egui::Context,
    open: &mut bool,
    doc: &mut Document,
    volume_cm3: Option<f64>,
    set: &Settings,
) {
    egui::Window::new(tr("Volume advisor"))
        .id(egui::Id::unique("volume-advisor-window"))
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            let advice = rider_form(ui, set);
            ui.separator();
            let Some(v) = volume_cm3.map(|v| v / 1000.0) else {
                ui.weak(tr("computing…"));
                return;
            };
            let Some(a) = advice else {
                return;
            };
            let place = if a.contains(v) {
                tr("inside the range")
            } else if v < a.min {
                tr("below the range")
            } else {
                tr("above the range")
            };
            ui.label(trf(
                "This board: {0} L ({1})",
                &[&format!("{v:.1}"), &place],
            ));
            let mid = a.middle();
            if ui
                .button(trf("Set this board to {0} L", &[&format!("{mid:.1}")]))
                .on_hover_text(tr(
                    "Changes the thickness to reach this volume (one undo step)",
                ))
                .clicked()
            {
                doc.edit(|b| {
                    crate::dialogs::solve_volume(b, mid);
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use eframe::egui::accesskit::Role;
    use egui_kittest::kittest::Queryable as _;

    use super::*;
    use crate::gui_tests::{H, harness, settle, snap};

    fn click(h: &mut H, label: &str) {
        h.get_all_by_label(label)
            .last()
            .unwrap_or_else(|| panic!("no {label:?}"))
            .click();
        h.run_steps(3);
    }

    /// Open the combo box showing `current` and pick `choice`.
    fn choose(h: &mut H, current: &str, choice: &str) {
        h.get_all_by(|n| {
            n.label().as_deref() == Some(current) || n.value().as_deref() == Some(current)
        })
        .last()
        .unwrap_or_else(|| panic!("no combo showing {current:?}"))
        .click();
        h.run_steps(3);
        h.get_all_by(|n| {
            n.label().as_deref() == Some(choice) || n.value().as_deref() == Some(choice)
        })
        .last()
        .unwrap_or_else(|| panic!("no option {choice:?}"))
        .click();
        h.run_steps(3);
    }

    fn click_prefix(h: &mut H, prefix: &str) {
        h.get_all_by(|n| n.label().is_some_and(|l| l.starts_with(prefix)))
            .last()
            .unwrap_or_else(|| panic!("no {prefix:?}"))
            .click();
        h.run_steps(3);
    }

    /// Set the drag value on the row of `label`.
    fn set_spin(h: &mut H, label: &str, v: f64) {
        let y = h.get_all_by_label(label).last().unwrap().rect().center().y;
        h.get_all_by_role(Role::SpinButton)
            .find(|n| (n.rect().center().y - y).abs() < 8.0)
            .unwrap_or_else(|| panic!("no drag value next to {label:?}"))
            .set_value_accesskit(v);
        h.run_steps(3);
    }

    fn rider(h: &H) -> Rider {
        h.ctx.data(|d| d.get_temp(rider_id())).unwrap_or_default()
    }

    #[test]
    fn new_board_suggests_a_volume() {
        let mut h = harness();
        h.state_mut().set.unit = crate::settings::Unit::Centimetres;
        click(&mut h, "File");
        click_prefix(&mut h, "New…");
        assert!(h.state().dialogs.new_board.is_some());
        click(&mut h, "Suggest volume");
        set_spin(&mut h, "Rider weight", 80.0);
        assert!((rider(&h).weight_kg - 80.0).abs() < 1e-9);
        choose(&mut h, "Intermediate", "Advanced");
        assert_eq!(rider(&h).skill, Skill::Advanced);
        let a = rider::suggest_volume(&rider(&h)).unwrap();
        let mid = format!("{:.1}", a.middle());
        assert!(h.query_by_label_contains("Suggested volume:").is_some());
        snap(&mut h, "volume-advisor-new-board");
        click(&mut h, &format!("Use {mid} L"));
        assert!(
            h.query_all_by_role(Role::TextInput)
                .any(|n| n.value().as_deref() == Some(mid.as_str())),
            "target volume field holds {mid}"
        );
        click(&mut h, "Create");
        settle(&mut h);
        let v = h.state().doc.board.volume() / 1000.0;
        assert!((v - a.middle()).abs() < 0.1, "{v} vs {}", a.middle());
    }

    #[test]
    fn advisor_window_in_pounds_sets_board_volume() {
        let mut h = harness();
        click(&mut h, "Board");
        click_prefix(&mut h, "Volume advisor");
        assert!(h.state().dialogs.volume_advisor);
        // Imperial by default: the weight is in pounds.
        set_spin(&mut h, "Rider weight", 176.0);
        assert!((rider(&h).weight_kg - 176.0 / POUNDS_PER_KG).abs() < 1e-6);
        assert!(h.query_by_label_contains("This board:").is_some());
        let a = rider::suggest_volume(&rider(&h)).unwrap();
        click_prefix(&mut h, "Set this board to");
        settle(&mut h);
        let v = h.state().doc.board.volume() / 1000.0;
        assert!((v - a.middle()).abs() < 0.1, "{v} vs {}", a.middle());
        assert!(h.state().doc.is_dirty());
        h.state_mut().doc.undo();
        assert!(!h.state().doc.is_dirty(), "one undo step");
        settle(&mut h);
        snap(&mut h, "volume-advisor-window");
    }

    /// Click the weight with the mouse and type a new one, a key per frame like a person: the typed number
    /// replaces the old one. (Regression: in pounds, the first digit was clamped to the 22 lb minimum while
    /// typing and the field text reset around the caret, so "176" became 550 lb.)
    #[test]
    fn typing_a_weight_replaces_it() {
        for (unit, typed, expect_kg) in [
            (
                crate::settings::Unit::FeetInchesFraction,
                "176",
                176.0 / POUNDS_PER_KG,
            ),
            (crate::settings::Unit::Centimetres, "80", 80.0),
        ] {
            let mut h = harness();
            h.state_mut().set.unit = unit;
            click(&mut h, "Board");
            click_prefix(&mut h, "Volume advisor");
            let y = h
                .get_all_by_label("Rider weight")
                .last()
                .unwrap()
                .rect()
                .center()
                .y;
            let pos = h
                .get_all_by_role(Role::SpinButton)
                .find(|n| (n.rect().center().y - y).abs() < 8.0)
                .expect("weight field")
                .rect()
                .center();
            h.event(egui::Event::PointerMoved(pos));
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
            h.run_steps(2);
            for c in typed.chars() {
                h.event(egui::Event::Text(c.to_string()));
                h.step();
            }
            let shown: Vec<String> = h
                .get_all_by_role(Role::SpinButton)
                .filter_map(|n| n.value())
                .collect();
            assert!(
                shown.iter().any(|s| s == typed),
                "{unit:?}: field shows {shown:?}"
            );
            h.key_press(egui::Key::Enter);
            h.run_steps(2);
            let kg = rider(&h).weight_kg;
            assert!((kg - expect_kg).abs() < 1e-6, "{unit:?}: {kg} kg");
        }
    }
}
