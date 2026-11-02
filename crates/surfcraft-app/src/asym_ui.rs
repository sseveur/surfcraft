//! Board > Asymmetric outline: a separate outline for each rail (see `surfcraft_core::asym`), and the window asking
//! which rail to keep when the board is made symmetric again.

use eframe::egui;
use surfcraft_core::Rail;

use crate::document::Document;
use crate::i18n::tr;
use crate::views::EditorState;

/// The menu entry: a tick showing whether the board is asymmetric. Ticking it copies the outline to the left rail
/// (one undo step); unticking it opens the window that asks which rail to keep.
pub fn menu_item(
    ui: &mut egui::Ui,
    doc: &mut Document,
    ed: &mut EditorState,
    keep_open: &mut bool,
) {
    let mut on = doc.board.is_asymmetric();
    let resp = ui
        .checkbox(&mut on, tr("Asymmetric outline"))
        .on_hover_text(tr(
            "A separate outline for each rail (toe side and heel side). In the Outline view the left rail is above the stringer and the right rail below it, standing on the deck facing the nose.",
        ));
    if resp.clicked() {
        if on {
            set(doc, ed, true, Rail::Right);
        } else {
            *keep_open = true;
        }
        ui.close();
    }
}

/// Make the board asymmetric or symmetric as one undo step; the selection (which may name a curve that goes away
/// or changes side) is cleared.
pub fn set(doc: &mut Document, ed: &mut EditorState, on: bool, keep: Rail) {
    if on != doc.board.is_asymmetric() {
        doc.edit(|b| {
            b.set_asymmetric(on, keep);
        });
        ed.selection = None;
    }
}

/// "Make the outline symmetric": keep the left or the right rail's outline for both sides.
pub fn keep_window(ctx: &egui::Context, open: &mut bool, doc: &mut Document, ed: &mut EditorState) {
    if !doc.board.is_asymmetric() {
        *open = false;
        return;
    }
    let mut close = false;
    egui::Window::new(tr("Make the outline symmetric"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(tr("Which rail's outline should both sides take?"));
            ui.horizontal(|ui| {
                if ui.button(tr("Keep the left rail (upper half)")).clicked() {
                    set(doc, ed, false, Rail::Left);
                    close = true;
                }
                if ui.button(tr("Keep the right rail (lower half)")).clicked() {
                    set(doc, ed, false, Rail::Right);
                    close = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    if close {
        *open = false;
    }
}
