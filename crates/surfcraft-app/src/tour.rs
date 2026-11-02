//! Guided tour: a small window that walks through the main parts of SurfCraft, outlining each one on screen.
//! It is not modal, so the user can try every step in the app before clicking Next. It opens by itself on first
//! launch and from Help > Tour of SurfCraft.

use eframe::egui::{self, Align2, Color32, Id, LayerId, Order, Rect, Stroke, StrokeKind};

use crate::app::Tab;
use crate::i18n::{tr, trf};

/// Part of the window a step points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Nothing,
    Tabs,
    Tools,
    Canvas,
    SidePanel,
    FileMenu,
    BoardMenu,
    MachineMenu,
    HelpMenu,
}

/// Screen rects of the parts the tour points at, recorded each frame while the window is laid out.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Targets {
    pub tabs: Option<Rect>,
    pub tools: Option<Rect>,
    pub canvas: Option<Rect>,
    pub side: Option<Rect>,
    pub file: Option<Rect>,
    pub board: Option<Rect>,
    pub machine: Option<Rect>,
    pub help: Option<Rect>,
}

impl Targets {
    pub fn get(&self, t: Target) -> Option<Rect> {
        match t {
            Target::Nothing => None,
            Target::Tabs => self.tabs,
            Target::Tools => self.tools,
            Target::Canvas => self.canvas,
            Target::SidePanel => self.side,
            Target::FileMenu => self.file,
            Target::BoardMenu => self.board,
            Target::MachineMenu => self.machine,
            Target::HelpMenu => self.help,
        }
    }
}

pub(crate) struct Step {
    pub title: &'static str,
    pub body: &'static str,
    pub target: Target,
    /// View shown when the step opens.
    pub tab: Option<Tab>,
}

/// The tour, in the current language.
pub(crate) fn steps() -> Vec<Step> {
    let step = |title, body, target, tab| Step {
        title,
        body,
        target,
        tab,
    };
    vec![
        step(
            tr("Welcome to SurfCraft"),
            tr(
                "This short tour shows the main parts of the window. Everything stays usable while it is open: try each part, then click Next when you are ready. You can close the tour at any time and reopen it from Help > Tour of SurfCraft.",
            ),
            Target::Nothing,
            None,
        ),
        step(
            tr("Views"),
            tr(
                "Overview shows the outline, the rocker, a cross-section and the 3D board together. Click a tab to work on one view at full size.",
            ),
            Target::Tabs,
            Some(Tab::Overview),
        ),
        step(
            tr("Outline"),
            tr(
                "The board seen from above. Red dots are control points, blue dots are their tangent handles: drag them to reshape the board, hold Alt for fine moves. Scroll or pinch to zoom, right-drag to pan, press F to fit the board.",
            ),
            Target::Canvas,
            Some(Tab::Outline),
        ),
        step(
            tr("Tools"),
            tr(
                "Select moves points (drag on empty space to select several). + Point adds a control point where you click on a curve, + Guide adds reference points. Measure (M) shows distances and angles. Undo and Redo cover every change. Units switches between feet and inches, centimetres and millimetres.",
            ),
            Target::Tools,
            None,
        ),
        step(
            tr("Rocker"),
            tr(
                "The board from the side: deck on top, bottom below. The buttons above the view choose whether you edit the deck, the bottom or both.",
            ),
            Target::Canvas,
            Some(Tab::Profile),
        ),
        step(
            tr("Cross-sections"),
            tr(
                "Cross-sections shape the rails. Step through them with < and > (or the + and - keys), add one at any distance from the tail, move or remove it, and drag its points like in the outline.",
            ),
            Target::Canvas,
            Some(Tab::Section),
        ),
        step(
            tr("Specs"),
            tr(
                "Length, widths, thickness, rocker, area and volume update as you edit. Below them, the selected control point can be typed in exactly, in any unit (6'2\", 47cm).",
            ),
            Target::SidePanel,
            None,
        ),
        step(
            tr("3D"),
            tr(
                "Drag to turn the board, scroll to zoom. View > 3D zebra stripes shows bumps in the surface.",
            ),
            Target::Canvas,
            Some(Tab::ThreeD),
        ),
        step(
            tr("Board menu"),
            tr(
                "Scale the board to new dimensions or a target volume, edit fins and board info, estimate the finished weight, add guide points, flip nose and tail.",
            ),
            Target::BoardMenu,
            Some(Tab::Overview),
        ),
        step(
            tr("Print & export"),
            tr(
                "In the File menu: new boards from templates, open and save, compare with a ghost board, trace a photo, and Print & export full-size templates (outline, rocker, cross-sections, wooden board parts) as PDF, SVG or DXF, or the board as STL, OBJ or STEP.",
            ),
            Target::FileMenu,
            None,
        ),
        step(
            tr("Machine"),
            tr(
                "CNC toolpaths and G-code for a shaping machine or a hot-wire cutter, with a preview and machining time.",
            ),
            Target::MachineMenu,
            None,
        ),
        step(
            tr("That's the tour"),
            tr(
                "The Help menu lists every shortcut, lets an AI assistant work on the board with you, and opens this tour again. Enjoy shaping!",
            ),
            Target::HelpMenu,
            None,
        ),
    ]
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Tour {
    pub open: bool,
    pub step: usize,
}

/// What the app should do after the tour window was shown.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Outcome {
    /// Switch to this view (the step changed).
    pub tab: Option<Tab>,
    /// The tour was finished or closed this frame.
    pub closed: bool,
}

const HIGHLIGHT: Color32 = Color32::from_rgb(255, 170, 40);

impl Tour {
    pub fn start(&mut self) {
        self.open = true;
        self.step = 0;
    }

    pub fn show(&mut self, ctx: &egui::Context, targets: &Targets) -> Outcome {
        let mut out = Outcome::default();
        if !self.open {
            return out;
        }
        let steps = steps();
        self.step = self.step.min(steps.len() - 1);
        let step = &steps[self.step];
        let target = targets.get(step.target);
        // Outline above the panels but below the tour window (Foreground) and open menus.
        if let Some(r) = target {
            let painter =
                ctx.layer_painter(LayerId::new(Order::Middle, Id::unique("tour-highlight")));
            painter.rect_stroke(
                r.expand(3.0),
                6.0,
                Stroke::new(3.0, HIGHLIGHT),
                StrokeKind::Outside,
            );
        }
        // Keep the window off what it points at: bottom right, or bottom left for the side panel.
        let (anchor, offset) = if step.target == Target::SidePanel {
            (Align2::LEFT_BOTTOM, egui::vec2(16.0, -40.0))
        } else {
            (Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0))
        };
        let mut open = true;
        let last = steps.len() - 1;
        let mut go = None;
        egui::Window::new(tr("Tour of SurfCraft"))
            .id(Id::unique("tour-window"))
            // Always on top of the views, the outline and other windows.
            .order(Order::Foreground)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(anchor, offset)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.weak(trf("Step {0} of {1}", &[&(self.step + 1), &steps.len()]));
                ui.heading(step.title);
                ui.label(step.body);
                if step.target == Target::SidePanel && target.is_none() {
                    ui.weak(tr("(Hidden right now: View > Specs panel shows it.)"));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.step > 0, egui::Button::new(tr("Back")))
                        .clicked()
                    {
                        go = Some(self.step - 1);
                    }
                    let next = if self.step == last {
                        tr("Finish")
                    } else {
                        tr("Next")
                    };
                    if ui.button(next).clicked() {
                        go = Some(self.step + 1);
                    }
                    if self.step < last {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr("Skip tour")).clicked() {
                                go = Some(steps.len());
                            }
                        });
                    }
                });
            });
        if let Some(s) = go {
            if s >= steps.len() {
                open = false;
            } else {
                self.step = s;
                out.tab = steps[s].tab;
            }
        }
        if !open {
            self.open = false;
            out.closed = true;
        }
        out
    }
}
