//! Outline, profile and cross-section editors.

use std::collections::BTreeSet;

use eframe::egui::{self, Align2, Color32, Key, Rect, Stroke, StrokeKind};
use surfcraft_core::section::{APEX_ANGLE, TUCK_UNDER_ANGLE};
use surfcraft_core::{Board, DVec2, Handle, Spline, units::FOOT};

use crate::document::Document;
use crate::edit::{self, CurveId, Selection};
use crate::i18n::{tr, trf};
use crate::settings::Settings;
use crate::view2d::{Canvas, ViewState, ident, mirror_x, mirror_y, sample};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    AddKnot,
    AddGuide,
    Measure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileTarget {
    Deck,
    Bottom,
    Both,
}

struct Drag {
    before: Board,
    curve: CurveId,
    grabbed: (usize, Handle),
    /// Accumulated sub-pixel motion in fine mode.
    moved: bool,
    /// Grabbed point or handle where the drag started, and the pointer motion since (board coordinates).
    start: DVec2,
    accum: DVec2,
    /// Snapping took part in this drag: the grabbed element follows `start + accum` (snapped) from now on.
    absolute: bool,
}

struct BoxSelect {
    start: egui::Pos2,
    curves: Vec<CurveId>,
}

pub struct EditorState {
    pub tool: Tool,
    pub selection: Option<Selection>,
    pub current_section: usize,
    pub profile_target: ProfileTarget,
    pub outline_view: ViewState,
    pub profile_view: ViewState,
    pub section_view: ViewState,
    pub ghost: Option<Board>,
    /// Result previewed by a Shape menu window, drawn dashed in the Section view.
    pub shaping_preview: Option<Board>,
    /// Photo/drawing under the outline view for tracing.
    pub background: Option<crate::tools::Background>,
    /// Latest computed centre of mass (cm from tail), for the marker.
    pub center_of_mass: Option<f64>,
    /// World position under the cursor in the hovered view, for the status bar.
    pub cursor: Option<DVec2>,
    /// What the point under the pointer snapped to this frame, for the status bar.
    pub snap_hint: Option<crate::snap::SnapKind>,
    /// Measure tool dimensions and read-out.
    pub measure: crate::measure::MeasureState,
    drag: Option<Drag>,
    boxsel: Option<BoxSelect>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            selection: None,
            current_section: 1,
            profile_target: ProfileTarget::Both,
            outline_view: ViewState::default(),
            profile_view: ViewState::default(),
            section_view: ViewState::default(),
            ghost: None,
            shaping_preview: None,
            background: None,
            center_of_mass: None,
            cursor: None,
            snap_hint: None,
            measure: Default::default(),
            drag: None,
            boxsel: None,
        }
    }
}

impl EditorState {
    pub fn fit_all(&mut self) {
        self.outline_view.needs_fit = true;
        self.profile_view.needs_fit = true;
        self.section_view.needs_fit = true;
    }

    pub fn reset_for(&mut self, board: &Board) {
        self.selection = None;
        self.drag = None;
        self.boxsel = None;
        // Start on the section nearest the middle of the board.
        self.current_section = board
            .nearest_section_index(board.length() / 2.0)
            .unwrap_or(0);
        self.fit_all();
    }

    pub fn clamp_section(&mut self, board: &Board) {
        let r = board.real_sections();
        if r.is_empty() {
            self.current_section = 0;
        } else {
            self.current_section = self.current_section.clamp(r.start, r.end - 1);
        }
    }
}

pub struct Palette {
    pub bg: Color32,
    pub grid: Color32,
    pub curve: Color32,
    pub curve_alt: Color32,
    pub inactive: Color32,
    pub ghost: Color32,
    pub original: Color32,
    pub point: Color32,
    pub handle: Color32,
    pub selected: Color32,
    pub guide: Color32,
    pub section_line: Color32,
    pub text: Color32,
    pub accent: Color32,
    pub fin: Color32,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                bg: Color32::from_rgb(24, 26, 30),
                grid: Color32::from_rgb(52, 56, 64),
                curve: Color32::from_rgb(235, 238, 242),
                curve_alt: Color32::from_rgb(120, 180, 255),
                inactive: Color32::from_rgb(110, 116, 128),
                ghost: Color32::from_rgb(230, 160, 60),
                original: Color32::from_rgb(120, 200, 140),
                point: Color32::from_rgb(255, 90, 80),
                handle: Color32::from_rgb(90, 170, 255),
                selected: Color32::from_rgb(255, 220, 60),
                guide: Color32::from_rgb(255, 80, 200),
                section_line: Color32::from_rgb(90, 96, 110),
                text: Color32::from_rgb(200, 205, 212),
                accent: Color32::from_rgb(255, 170, 60),
                fin: Color32::from_rgb(170, 140, 255),
            }
        } else {
            Self {
                bg: Color32::from_rgb(250, 250, 248),
                grid: Color32::from_rgb(220, 222, 226),
                curve: Color32::from_rgb(20, 24, 30),
                curve_alt: Color32::from_rgb(30, 100, 210),
                inactive: Color32::from_rgb(150, 155, 165),
                ghost: Color32::from_rgb(210, 120, 20),
                original: Color32::from_rgb(40, 150, 70),
                point: Color32::from_rgb(220, 40, 30),
                handle: Color32::from_rgb(30, 110, 220),
                selected: Color32::from_rgb(230, 160, 0),
                guide: Color32::from_rgb(200, 30, 150),
                section_line: Color32::from_rgb(170, 175, 185),
                text: Color32::from_rgb(50, 55, 62),
                accent: Color32::from_rgb(220, 120, 20),
                fin: Color32::from_rgb(110, 80, 210),
            }
        }
    }
}

/// An editable curve shown in a view, with a vertical display scale (profile exaggeration).
#[derive(Clone, Copy)]
struct Editable {
    id: CurveId,
    y_scale: f64,
}

impl Editable {
    fn display(&self, p: DVec2) -> DVec2 {
        DVec2::new(p.x, p.y * self.y_scale)
    }
}

const HIT_RADIUS: f32 = 8.0;

fn hit_test(
    canvas: &Canvas,
    board: &Board,
    curves: &[Editable],
    pos: egui::Pos2,
) -> Option<(CurveId, usize, Handle)> {
    let mut best: Option<(f32, CurveId, usize, Handle)> = None;
    for e in curves {
        let Some(spline) = e.id.spline(board) else {
            continue;
        };
        for (i, k) in spline.knots().iter().enumerate() {
            for which in [Handle::Point, Handle::Prev, Handle::Next] {
                let p = canvas.xf.to_screen(e.display(k.get(which)));
                let d = p.distance(pos);
                // Prefer points over handles when they overlap.
                let d = if which == Handle::Point { d - 1.5 } else { d };
                if d < HIT_RADIUS && best.is_none_or(|b| d < b.0) {
                    best = Some((d, e.id, i, which));
                }
            }
        }
    }
    best.map(|(_, c, i, w)| (c, i, w))
}

fn nearest_curve(
    canvas: &Canvas,
    board: &Board,
    curves: &[Editable],
    world: DVec2,
) -> Option<CurveId> {
    curves
        .iter()
        .filter_map(|e| {
            let s = e.id.spline(board)?;
            let p = DVec2::new(world.x, world.y / e.y_scale);
            Some((s.distance_to(p), e.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|(d, _)| canvas.xf.scale as f64 * d < 40.0)
        .map(|(_, id)| id)
}

/// Mouse and keyboard editing of `curves` on `canvas`.
fn interact(
    ui: &egui::Ui,
    canvas: &Canvas,
    doc: &mut Document,
    ed: &mut EditorState,
    curves: &[Editable],
    set: &Settings,
) {
    let resp = &canvas.response;
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    if let Some(p) = resp.hover_pos() {
        let w = canvas.xf.to_world(p);
        let ys = curves.first().map_or(1.0, |e| e.y_scale);
        ed.cursor = Some(DVec2::new(w.x, w.y / ys));
    }
    let mods = ui.input(|i| i.modifiers);
    let space = ui.input(|i| i.key_down(Key::Space));
    let snap = curves.first().map(|e| {
        let pane = crate::snap::Pane::of(e.id);
        crate::snap::ViewSnap::new(ui, canvas, &doc.board, pane, e.y_scale, set.snap)
    });
    // Where a click with an add tool lands, snapped; shown with a ring while hovering.
    let placed = match (&snap, ed.tool) {
        (Some(snap), Tool::AddKnot | Tool::AddGuide) if ed.drag.is_none() => {
            resp.hover_pos().map(|p| {
                let s = snap.snap(
                    snap.board(canvas.xf.to_world(p)),
                    &|_, _| false,
                    None,
                    false,
                    false,
                );
                snap.draw(canvas, &s, Palette::new(ui.visuals().dark_mode).selected);
                ed.snap_hint = s.kind.or(ed.snap_hint);
                s
            })
        }
        _ => None,
    };
    if let Some(snap) = &snap {
        let active = ed.tool == Tool::Measure;
        crate::measure::view(
            ui,
            canvas,
            &doc.board,
            &mut ed.measure,
            &mut ed.snap_hint,
            active,
            snap,
            set,
        );
    }

    // Hover feedback.
    if ed.drag.is_none()
        && let Some(p) = resp.hover_pos()
        && hit_test(canvas, &doc.board, curves, p).is_some()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if resp.clicked_by(egui::PointerButton::Primary)
        && let Some(p) = pointer
    {
        let world = canvas.xf.to_world(p);
        match ed.tool {
            // The second click of a double-click must not add a second point.
            Tool::AddKnot | Tool::AddGuide if resp.double_clicked() => {}
            Tool::AddKnot => {
                if let Some(id) = nearest_curve(canvas, &doc.board, curves, world) {
                    let ys = curves
                        .iter()
                        .find(|e| e.id == id)
                        .map_or(1.0, |e| e.y_scale);
                    let target = placed.map_or(DVec2::new(world.x, world.y / ys), |s| s.pos);
                    let mut new_index = None;
                    doc.edit(|b| new_index = edit::add_knot(b, id, target));
                    if let Some(i) = new_index {
                        ed.selection = Some(Selection::single(id, i, Handle::Point));
                    }
                }
            }
            Tool::AddGuide => {
                if let Some(e) = curves.first() {
                    let id = e.id;
                    let target = placed.map_or(DVec2::new(world.x, world.y / e.y_scale), |s| s.pos);
                    doc.edit(|b| {
                        if let Some(g) = id.guides_mut(b) {
                            g.push(target);
                        }
                    });
                }
            }
            Tool::Select => match hit_test(canvas, &doc.board, curves, p) {
                Some((id, i, which)) => select_knot(ed, id, i, which, mods),
                None if !mods.shift && !mods.command => ed.selection = None,
                None => {}
            },
            Tool::Measure => {}
        }
    }

    if resp.drag_started_by(egui::PointerButton::Primary) && !space && ed.tool == Tool::Select {
        let start = resp.interact_pointer_pos().map(|p| p - resp.drag_delta());
        if let Some(p) = start {
            match hit_test(canvas, &doc.board, curves, p) {
                Some((id, i, which)) => {
                    let keep = ed.selection.as_ref().is_some_and(|s| {
                        s.curve == id && s.knots.contains(&i) && which == Handle::Point
                    });
                    if !keep {
                        select_knot(ed, id, i, which, mods);
                    } else if let Some(s) = ed.selection.as_mut() {
                        s.handle = which;
                    }
                    let start = id
                        .spline(&doc.board)
                        .and_then(|s| s.knots().get(i))
                        .map_or(DVec2::ZERO, |k| k.get(which));
                    ed.drag = Some(Drag {
                        before: doc.board.clone(),
                        curve: id,
                        grabbed: (i, which),
                        moved: false,
                        start,
                        accum: DVec2::ZERO,
                        absolute: false,
                    });
                }
                None => {
                    ed.boxsel = Some(BoxSelect {
                        start: p,
                        curves: curves.iter().map(|e| e.id).collect(),
                    });
                }
            }
        }
    }

    if let Some(drag) = ed.drag.as_mut()
        && resp.dragged_by(egui::PointerButton::Primary)
    {
        let d = resp.drag_delta();
        if d != egui::Vec2::ZERO {
            let ys = curves
                .iter()
                .find(|e| e.id == drag.curve)
                .map_or(1.0, |e| e.y_scale);
            let fine = if mods.alt { 0.1 } else { 1.0 };
            let delta = DVec2::new(
                canvas.xf.len_to_world(d.x) * fine,
                -canvas.xf.len_to_world(d.y) * fine / ys,
            );
            drag.accum += delta;
            if !drag.absolute
                && let Some(sel) = ed.selection.clone()
            {
                edit::drag(&mut doc.board, &sel, drag.grabbed, delta);
                drag.moved = true;
                doc.touch();
            }
        }
        // Snapping (or Shift) moves the grabbed element to its snapped target instead.
        if let Some(snap) = &snap {
            drag.absolute |= snap.drag_snaps() || mods.shift;
            if drag.absolute
                && let Some(sel) = ed.selection.clone()
            {
                let dragged = |c: CurveId, i: usize| {
                    c == sel.curve && (i == drag.grabbed.0 || sel.knots.contains(&i))
                };
                let s = snap.snap(
                    drag.start + drag.accum,
                    &dragged,
                    Some(drag.start),
                    mods.shift,
                    false,
                );
                snap.draw(canvas, &s, Palette::new(ui.visuals().dark_mode).selected);
                ed.snap_hint = s.kind.or(ed.snap_hint);
                let current = sel
                    .curve
                    .spline(&doc.board)
                    .and_then(|sp| sp.knots().get(drag.grabbed.0))
                    .map(|k| k.get(drag.grabbed.1));
                if let Some(current) = current
                    && s.pos != current
                {
                    edit::drag(&mut doc.board, &sel, drag.grabbed, s.pos - current);
                    drag.moved = true;
                    doc.touch();
                }
            }
        }
    }

    if resp.drag_stopped() {
        if let Some(drag) = ed.drag.take()
            && drag.moved
        {
            edit::after_edit(&mut doc.board, drag.curve);
            doc.commit(drag.before);
        }
        if let Some(bs) = ed.boxsel.take()
            && let Some(end) = resp.interact_pointer_pos().or(resp.hover_pos())
        {
            let rect = Rect::from_two_pos(bs.start, end);
            for id in bs.curves {
                let ys = curves
                    .iter()
                    .find(|e| e.id == id)
                    .map_or(1.0, |e| e.y_scale);
                let Some(spline) = id.spline(&doc.board) else {
                    continue;
                };
                let inside: BTreeSet<usize> = spline
                    .knots()
                    .iter()
                    .enumerate()
                    .filter(|(_, k)| {
                        rect.contains(
                            canvas
                                .xf
                                .to_screen(DVec2::new(k.point().x, k.point().y * ys)),
                        )
                    })
                    .map(|(i, _)| i)
                    .collect();
                if !inside.is_empty() {
                    ed.selection = Some(Selection {
                        curve: id,
                        knots: inside,
                        handle: Handle::Point,
                    });
                    break;
                }
            }
        }
    }

    if let Some(bs) = &ed.boxsel
        && let Some(p) = resp.hover_pos().or(resp.interact_pointer_pos())
    {
        let r = Rect::from_two_pos(bs.start, p);
        canvas
            .painter
            .rect_stroke(r, 0.0, Stroke::new(1.0, Color32::GRAY), StrokeKind::Inside);
    }

    // Keyboard nudge of the selection while this view is hovered (never while typing in a text field).
    if resp.hovered()
        && ed.drag.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && let Some(sel) = ed.selection.clone()
        && let Some(e) = curves.iter().find(|e| e.id == sel.curve)
    {
        let step_px = if mods.alt { 0.1 } else { 1.0 };
        let step = canvas.xf.len_to_world(step_px);
        // One screen pixel vertically is less board height when the profile is exaggerated.
        let step_y = step / e.y_scale;
        let mut delta = DVec2::ZERO;
        ui.input(|i| {
            if i.key_pressed(Key::ArrowLeft) {
                delta.x -= step;
            }
            if i.key_pressed(Key::ArrowRight) {
                delta.x += step;
            }
            if i.key_pressed(Key::ArrowUp) {
                delta.y += step_y;
            }
            if i.key_pressed(Key::ArrowDown) {
                delta.y -= step_y;
            }
        });
        if delta != DVec2::ZERO
            && let Some(k) = sel.primary()
        {
            let curve = sel.curve;
            doc.edit(|b| {
                edit::drag(b, &sel, (k, sel.handle), delta);
                edit::after_edit(b, curve);
            });
        }
        let delete = ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace));
        if delete && sel.handle == Handle::Point {
            let mut deleted = false;
            doc.edit(|b| deleted = edit::delete_knots(b, sel.curve, &sel.knots));
            if deleted {
                ed.selection = None;
            }
        }
    }
}

fn select_knot(ed: &mut EditorState, id: CurveId, i: usize, which: Handle, mods: egui::Modifiers) {
    match ed.selection.as_mut() {
        Some(s) if s.curve == id && (mods.shift || mods.command) && which == Handle::Point => {
            if !s.knots.insert(i) && mods.command {
                s.knots.remove(&i);
            }
            s.handle = which;
        }
        _ => ed.selection = Some(Selection::single(id, i, which)),
    }
}

/// Draw a spline's control points and handles.
fn draw_knots(
    canvas: &Canvas,
    spline: &Spline,
    e: Editable,
    sel: Option<&Selection>,
    pal: &Palette,
) {
    for (i, k) in spline.knots().iter().enumerate() {
        let selected = sel.is_some_and(|s| s.curve == e.id && s.knots.contains(&i));
        let p = canvas.xf.to_screen(e.display(k.point()));
        for which in [Handle::Prev, Handle::Next] {
            let h = k.get(which);
            if (h - k.point()).length() < 1e-9 {
                continue;
            }
            let hp = canvas.xf.to_screen(e.display(h));
            canvas
                .painter
                .line_segment([p, hp], Stroke::new(1.0, pal.handle.gamma_multiply(0.7)));
            let active = selected && sel.is_some_and(|s| s.handle == which);
            canvas.painter.circle_filled(
                hp,
                if active { 4.5 } else { 3.5 },
                if active { pal.selected } else { pal.handle },
            );
        }
        let size = 4.5;
        let r = Rect::from_center_size(p, egui::vec2(size * 2.0, size * 2.0));
        let fill = if selected { pal.selected } else { pal.point };
        if k.continuous {
            canvas.painter.circle_filled(p, size, fill);
        } else {
            canvas.painter.rect_filled(r, 0.0, fill);
        }
    }
}

fn draw_guides(canvas: &Canvas, guides: &[DVec2], y_scale: f64, pal: &Palette) {
    for g in guides {
        let p = canvas.xf.to_screen(DVec2::new(g.x, g.y * y_scale));
        let s = Stroke::new(1.5, pal.guide);
        canvas
            .painter
            .line_segment([p - egui::vec2(5.0, 5.0), p + egui::vec2(5.0, 5.0)], s);
        canvas
            .painter
            .line_segment([p - egui::vec2(5.0, -5.0), p + egui::vec2(5.0, -5.0)], s);
    }
}

/// Curvature comb: normals scaled by curvature, drawn outward from the curve.
fn draw_curvature(
    canvas: &Canvas,
    spline: &Spline,
    map: impl Fn(DVec2) -> DVec2,
    scale: f64,
    color: Color32,
) {
    let mut tips = Vec::new();
    for c in spline.segments() {
        for j in 0..=12 {
            let t = j as f64 / 12.0;
            let p = c.point(t);
            let d = c.derivative(t);
            if d.length() < 1e-9 {
                continue;
            }
            let n = DVec2::new(-d.y, d.x).normalize();
            let tip = p - n * c.curvature(t) * scale;
            canvas.segment(
                map(p),
                map(tip),
                Stroke::new(1.0, color.gamma_multiply(0.5)),
            );
            tips.push(map(tip));
        }
    }
    canvas.polyline(tips, Stroke::new(1.0, color));
}

fn draw_overlay_board(canvas: &Canvas, b: &Board, which: View, color: Color32, ys: f64) {
    let s = Stroke::new(1.2, color);
    match which {
        View::Outline => {
            canvas.dashed(sample(&b.outline, 24, ident), s);
            canvas.dashed(sample(&b.outline, 24, mirror_y), s);
        }
        View::Profile => {
            canvas.dashed(sample(&b.deck, 24, |p| DVec2::new(p.x, p.y * ys)), s);
            canvas.dashed(sample(&b.bottom, 24, |p| DVec2::new(p.x, p.y * ys)), s);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Outline,
    Profile,
}

// ------------------------------------------------------------------------------------------- outline

pub fn outline(
    ui: &mut egui::Ui,
    doc: &mut Document,
    ed: &mut EditorState,
    set: &Settings,
    pal: &Palette,
) {
    let length = doc.board.length().max(1.0);
    let half = (doc.board.max_width() / 2.0).max(1.0);
    let bounds = (DVec2::new(0.0, -half), DVec2::new(length, half));
    let canvas = Canvas::show(ui, &mut ed.outline_view, bounds, pal.bg);
    let v = &set.view;
    if v.background_image
        && let Some(bg) = &ed.background
    {
        bg.draw(&canvas.painter, |p| canvas.xf.to_screen(p), length);
    }
    if v.grid {
        canvas.grid(
            if set.length_unit().is_imperial() {
                2.54
            } else {
                1.0
            },
            if set.length_unit().is_imperial() {
                12
            } else {
                10
            },
            pal.grid,
        );
    }
    // Hold T or N and click to pin the background image's tail or nose.
    if let Some(bg) = ed.background.as_mut()
        && canvas.response.clicked()
        && let Some(p) = canvas.response.interact_pointer_pos()
    {
        let (t, n) = ui.input(|i| (i.key_down(egui::Key::T), i.key_down(egui::Key::N)));
        if t || n {
            let px = bg.to_pixel(canvas.xf.to_world(p), length);
            if t {
                bg.tail_px = px;
            } else {
                bg.nose_px = px;
            }
            return;
        }
    }
    let board = &doc.board;
    if v.center_of_mass
        && let Some(com) = ed.center_of_mass
    {
        canvas.painter.circle_stroke(
            canvas.xf.to_screen(DVec2::new(com, 0.0)),
            7.0,
            Stroke::new(2.0, pal.accent),
        );
        // Beside the marker, clear of the centre line.
        canvas.text(
            DVec2::new(com, 0.0) + DVec2::new(9.0, 4.0) / canvas.xf.scale as f64,
            Align2::LEFT_BOTTOM,
            tr("CoM"),
            pal.accent,
        );
    }
    if v.center_line {
        canvas.segment(
            DVec2::new(-5.0, 0.0),
            DVec2::new(length + 5.0, 0.0),
            Stroke::new(1.0, pal.section_line),
        );
    }
    if v.ghost
        && let Some(g) = &ed.ghost
    {
        draw_overlay_board(&canvas, g, View::Outline, pal.ghost, 1.0);
    }
    if v.original {
        draw_overlay_board(&canvas, &doc.original, View::Outline, pal.original, 1.0);
    }
    if v.section_positions {
        for i in board.real_sections() {
            let x = board.sections[i].position;
            let w = board.width_at(x) / 2.0;
            let color = if i == ed.current_section {
                pal.accent
            } else {
                pal.section_line
            };
            canvas.segment(DVec2::new(x, -w), DVec2::new(x, w), Stroke::new(1.0, color));
        }
    }
    let stroke = Stroke::new(2.0, pal.curve);
    canvas.polyline(sample(&board.outline, 32, ident), stroke);
    canvas.polyline(sample(&board.outline, 32, mirror_y), stroke);
    if v.curvature {
        draw_curvature(&canvas, &board.outline, ident, 300.0, pal.accent);
    }
    if v.fins {
        crate::fins_ui::draw_outline(&canvas, board, pal.fin);
    }
    if v.measurements {
        for x in [FOOT, length / 2.0, length - FOOT] {
            let w = board.width_at(x);
            canvas.segment(
                DVec2::new(x, -w / 2.0),
                DVec2::new(x, w / 2.0),
                Stroke::new(1.0, pal.inactive),
            );
            canvas.text(
                DVec2::new(x, -w / 2.0),
                Align2::CENTER_TOP,
                set.fmt(w),
                pal.text,
            );
        }
        let mx = board.max_width_pos();
        // A little above the rail so the label never sits on the curve.
        canvas.text(
            DVec2::new(mx, half + 6.0 / canvas.xf.scale as f64),
            Align2::CENTER_BOTTOM,
            trf("wide point {0}", &[&set.fmt(board.max_width())]),
            pal.text,
        );
    }
    let editable = [Editable {
        id: CurveId::Outline,
        y_scale: 1.0,
    }];
    if v.guide_points {
        draw_guides(&canvas, &board.outline_guides, 1.0, pal);
    }
    if v.control_points {
        draw_knots(
            &canvas,
            &board.outline,
            editable[0],
            ed.selection.as_ref(),
            pal,
        );
    }
    interact(ui, &canvas, doc, ed, &editable, set);
    section_click(ui, &canvas, doc, ed);
}

/// Double-click near a cross-section marker in the outline/profile to make it current.
fn section_click(_ui: &egui::Ui, canvas: &Canvas, doc: &Document, ed: &mut EditorState) {
    if canvas.response.double_clicked()
        && let Some(p) = canvas.response.interact_pointer_pos()
    {
        let x = canvas.xf.to_world(p).x;
        if let Some(i) = doc.board.nearest_section_index(x) {
            ed.current_section = i;
        }
    }
}

// ------------------------------------------------------------------------------------------- profile

pub fn profile(
    ui: &mut egui::Ui,
    doc: &mut Document,
    ed: &mut EditorState,
    set: &Settings,
    pal: &Palette,
) {
    ui.horizontal(|ui| {
        ui.label(tr("Edit:"));
        ui.selectable_value(
            &mut ed.profile_target,
            ProfileTarget::Both,
            tr("Deck + bottom"),
        );
        ui.selectable_value(&mut ed.profile_target, ProfileTarget::Deck, tr("Deck"));
        ui.selectable_value(&mut ed.profile_target, ProfileTarget::Bottom, tr("Bottom"));
    });
    let ys = set.profile_exaggeration.max(1.0) as f64;
    let length = doc.board.length().max(1.0);
    let top = doc
        .board
        .deck
        .max_y()
        .max(doc.board.bottom.max_y())
        .max(1.0)
        * ys;
    let bounds = (DVec2::new(0.0, -1.0), DVec2::new(length, top));
    let canvas = Canvas::show(ui, &mut ed.profile_view, bounds, pal.bg);
    let v = &set.view;
    if v.grid {
        canvas.grid(
            if set.length_unit().is_imperial() {
                2.54
            } else {
                1.0
            },
            if set.length_unit().is_imperial() {
                12
            } else {
                10
            },
            pal.grid,
        );
    }
    let board = &doc.board;
    let disp = |p: DVec2| DVec2::new(p.x, p.y * ys);
    canvas.segment(
        DVec2::new(-5.0, 0.0),
        DVec2::new(length + 5.0, 0.0),
        Stroke::new(1.0, pal.section_line),
    );
    if v.ghost
        && let Some(g) = &ed.ghost
    {
        draw_overlay_board(&canvas, g, View::Profile, pal.ghost, ys);
    }
    if v.original {
        draw_overlay_board(&canvas, &doc.original, View::Profile, pal.original, ys);
    }
    if v.section_positions {
        for i in board.real_sections() {
            let x = board.sections[i].position;
            let color = if i == ed.current_section {
                pal.accent
            } else {
                pal.section_line
            };
            canvas.segment(
                disp(DVec2::new(x, board.rocker_at(x))),
                disp(DVec2::new(x, board.deck_at(x))),
                Stroke::new(1.0, color),
            );
        }
    }
    let deck_active = ed.profile_target != ProfileTarget::Bottom;
    let bottom_active = ed.profile_target != ProfileTarget::Deck;
    let st = |active: bool| Stroke::new(2.0, if active { pal.curve } else { pal.inactive });
    canvas.polyline(sample(&board.deck, 32, disp), st(deck_active));
    canvas.polyline(sample(&board.bottom, 32, disp), st(bottom_active));
    if v.curvature {
        if deck_active {
            draw_curvature(&canvas, &board.deck, disp, 2000.0, pal.accent);
        }
        if bottom_active {
            draw_curvature(&canvas, &board.bottom, disp, 2000.0, pal.accent);
        }
    }
    if v.measurements {
        for x in [FOOT, length / 2.0, length - FOOT] {
            canvas.text(
                disp(DVec2::new(x, board.deck_at(x))),
                Align2::CENTER_BOTTOM,
                set.fmt(board.thickness_at(x)),
                pal.text,
            );
        }
        // Rocker labels go below the bottom curve so they never collide with the thickness labels above the deck.
        canvas.text(
            disp(DVec2::new(0.0, board.rocker_at(0.0)))
                - DVec2::new(0.0, 6.0 / canvas.xf.scale as f64),
            Align2::LEFT_TOP,
            format!(" {}", trf("tail {0}", &[&set.fmt(board.rocker_at(0.0))])),
            pal.text,
        );
        canvas.text(
            disp(DVec2::new(length, board.rocker_at(length)))
                - DVec2::new(0.0, 6.0 / canvas.xf.scale as f64),
            Align2::RIGHT_TOP,
            format!("{} ", trf("nose {0}", &[&set.fmt(board.rocker_at(length))])),
            pal.text,
        );
    }
    let mut editable = Vec::new();
    if deck_active {
        editable.push(Editable {
            id: CurveId::Deck,
            y_scale: ys,
        });
    }
    if bottom_active {
        editable.push(Editable {
            id: CurveId::Bottom,
            y_scale: ys,
        });
    }
    if v.guide_points {
        if deck_active {
            draw_guides(&canvas, &board.deck_guides, ys, pal);
        }
        if bottom_active {
            draw_guides(&canvas, &board.bottom_guides, ys, pal);
        }
    }
    if v.control_points {
        for e in &editable {
            if let Some(s) = e.id.spline(board) {
                draw_knots(&canvas, s, *e, ed.selection.as_ref(), pal);
            }
        }
    }
    interact(ui, &canvas, doc, ed, &editable, set);
    section_click(ui, &canvas, doc, ed);
}

// ------------------------------------------------------------------------------------------- section

pub fn section(
    ui: &mut egui::Ui,
    doc: &mut Document,
    ed: &mut EditorState,
    set: &Settings,
    pal: &Palette,
) {
    ed.clamp_section(&doc.board);
    section_toolbar(ui, doc, ed, set);
    let idx = ed.current_section;
    let board = &doc.board;
    if !board.real_sections().contains(&idx) {
        ui.label(tr("This board has no cross-sections."));
        return;
    }
    let half = (board.max_width() / 2.0).max(1.0);
    let thick = board.max_thickness().max(1.0);
    let bounds = (DVec2::new(-half, -1.0), DVec2::new(half, thick + 1.0));
    let canvas = Canvas::show(ui, &mut ed.section_view, bounds, pal.bg);
    let v = &set.view;
    if v.grid {
        canvas.grid(
            if set.length_unit().is_imperial() {
                2.54 / 4.0
            } else {
                0.5
            },
            if set.length_unit().is_imperial() {
                4
            } else {
                2
            },
            pal.grid,
        );
    }
    let board = &doc.board;
    canvas.segment(
        DVec2::new(0.0, -1.0),
        DVec2::new(0.0, thick + 1.0),
        Stroke::new(1.0, pal.section_line),
    );
    if v.other_sections {
        for i in board.real_sections().filter(|&i| i != idx) {
            let s = &board.sections[i].spline;
            let st = Stroke::new(1.0, pal.inactive.gamma_multiply(0.6));
            canvas.polyline(sample(s, 24, ident), st);
            canvas.polyline(sample(s, 24, mirror_x), st);
        }
    }
    if v.ghost
        && let Some(g) = &ed.ghost
        && let Some(gs) = g.interpolated_section(board.sections[idx].position)
    {
        let st = Stroke::new(1.2, pal.ghost);
        canvas.dashed(sample(&gs.spline, 24, ident), st);
        canvas.dashed(sample(&gs.spline, 24, mirror_x), st);
    }
    if let Some(p) = &ed.shaping_preview
        && let Some(ps) = crate::shaping_ui::section_at(p, board.sections[idx].position)
    {
        let st = Stroke::new(2.0, pal.accent);
        canvas.dashed(sample(&ps.spline, 32, ident), st);
        canvas.dashed(sample(&ps.spline, 32, mirror_x), st);
    }
    let cs = &board.sections[idx];
    let stroke = Stroke::new(2.0, pal.curve);
    canvas.polyline(sample(&cs.spline, 32, ident), stroke);
    canvas.polyline(sample(&cs.spline, 32, mirror_x), stroke);
    if v.curvature {
        draw_curvature(&canvas, &cs.spline, ident, 3.0, pal.accent);
    }
    let apex = cs.point_at_s(cs.spline.s_by_normal_reverse(APEX_ANGLE.to_radians(), true));
    let tuck = cs.point_at_s(
        cs.spline
            .s_by_normal_reverse(TUCK_UNDER_ANGLE.to_radians(), true),
    );
    if v.apex_line {
        canvas
            .painter
            .circle_stroke(canvas.xf.to_screen(apex), 6.0, Stroke::new(1.5, pal.accent));
        // The apex is at the rail edge of the view, so label it on the inside.
        canvas.text(
            apex,
            Align2::RIGHT_CENTER,
            format!("{}  ", tr("apex")),
            pal.accent,
        );
    }
    if v.tuck_line {
        canvas.painter.circle_stroke(
            canvas.xf.to_screen(tuck),
            6.0,
            Stroke::new(1.5, pal.curve_alt),
        );
        canvas.text(
            tuck,
            Align2::LEFT_TOP,
            format!("  {}", tr("tuck")),
            pal.curve_alt,
        );
    }
    if v.measurements {
        let lines = [
            trf("at {0} from tail", &[&set.fmt(cs.position)]),
            trf("width {0}", &[&set.fmt(cs.width())]),
            trf("thickness {0}", &[&set.fmt(cs.center_thickness())]),
            // Near 180° the tangents are collinear: a soft rail without a release edge.
            if cs.release_angle().to_degrees() > 170.0 {
                tr("soft rail (no release edge)").to_owned()
            } else {
                trf(
                    "release angle {0}°",
                    &[&format!("{:.0}", cs.release_angle().to_degrees())],
                )
            },
            trf("tuck radius {0}", &[&set.fmt(cs.tuck_radius())]),
        ];
        let mut y = canvas.rect.top() + 8.0;
        for l in lines {
            canvas.painter.text(
                egui::pos2(canvas.rect.left() + 10.0, y),
                Align2::LEFT_TOP,
                l,
                egui::FontId::proportional(13.0),
                pal.text,
            );
            y += 17.0;
        }
    }
    let editable = [Editable {
        id: CurveId::Section(idx),
        y_scale: 1.0,
    }];
    if v.guide_points {
        draw_guides(&canvas, &cs.guide_points, 1.0, pal);
    }
    if v.control_points {
        draw_knots(&canvas, &cs.spline, editable[0], ed.selection.as_ref(), pal);
    }
    interact(ui, &canvas, doc, ed, &editable, set);
}

fn section_toolbar(ui: &mut egui::Ui, doc: &mut Document, ed: &mut EditorState, set: &Settings) {
    let real = doc.board.real_sections();
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("<")
            .on_hover_text(tr("Previous section (-)"))
            .clicked()
            || (!ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Minus)))
        {
            ed.current_section = if ed.current_section <= real.start {
                real.end.saturating_sub(1)
            } else {
                ed.current_section - 1
            };
            ed.selection = None;
        }
        let label = if real.contains(&ed.current_section) {
            trf(
                "Section {0} of {1} at {2}",
                &[
                    &(ed.current_section - real.start + 1),
                    &real.len(),
                    &set.fmt(doc.board.sections[ed.current_section].position),
                ],
            )
        } else {
            tr("No section").into()
        };
        ui.label(label);
        if ui
            .button(">")
            .on_hover_text(tr("Next section (+)"))
            .clicked()
            || (!ui.ctx().egui_wants_keyboard_input()
                && ui.input(|i| i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)))
        {
            ed.current_section = if ed.current_section + 1 >= real.end {
                real.start
            } else {
                ed.current_section + 1
            };
            ed.selection = None;
        }
        ui.separator();
        section_position_editor(ui, doc, ed, set);
    });
}

fn section_position_editor(
    ui: &mut egui::Ui,
    doc: &mut Document,
    ed: &mut EditorState,
    set: &Settings,
) {
    let id = ui.scope_id().with("new-section-pos");
    let mut text: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
    ui.add(
        egui::TextEdit::singleline(&mut text)
            .desired_width(90.0)
            .hint_text(tr("position")),
    );
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    let pos = set.length_unit().parse(&text);
    if ui
        .add_enabled(pos.is_some(), egui::Button::new(tr("Add")))
        .on_hover_text(tr("Add a section at this distance from the tail"))
        .clicked()
        && let Some(x) = pos
    {
        let mut new = None;
        doc.edit(|b| new = edit::add_section(b, x));
        if let Some(i) = new {
            ed.current_section = i;
            ed.selection = None;
        }
    }
    if ui
        .add_enabled(pos.is_some(), egui::Button::new(tr("Move here")))
        .clicked()
        && let Some(x) = pos
    {
        let i = ed.current_section;
        let mut new = i;
        doc.edit(|b| new = edit::move_section(b, i, x));
        ed.current_section = new;
    }
    if ui.button(tr("Remove")).clicked() {
        let i = ed.current_section;
        doc.edit(|b| {
            edit::remove_section(b, i);
        });
        ed.selection = None;
        ed.clamp_section(&doc.board);
    }
}
