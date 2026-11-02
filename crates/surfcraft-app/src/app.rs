//! Main window: menus, toolbar, tabs, file handling.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use eframe::egui_wgpu;
use surfcraft_core::Board;

use crate::Args;
use crate::compute::{Results, Worker};
use crate::dialogs::{self, Dialogs, NewBoard, Scale};
use crate::document::{Document, write_with_backup};
use crate::i18n::{self, Language, tr, trf};
use crate::panels;
use crate::preview3d::{self, Orbit};
use crate::screenshot::Screenshot;
use crate::settings::{self, Model, Settings, Unit};
use crate::tour::{self, Tour};
use crate::viewer3d::Viewer3d;
use crate::views::{self, EditorState, Palette, Tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    /// Outline, rocker, cross-section and 3D together.
    Overview,
    Outline,
    Profile,
    Section,
    ThreeD,
}

/// Something waiting for "discard unsaved changes?" confirmation.
pub(crate) enum Pending {
    New(Box<Board>),
    Open(PathBuf),
    Quit,
    /// Open an autosave left by a crash.
    Recover(Box<crate::autosave::Entry>),
}

// Fields are crate-visible so GUI tests (and the automation bridge) can inspect state.
pub struct SurfCraftApp {
    pub(crate) doc: Document,
    pub(crate) ed: EditorState,
    pub(crate) set: Settings,
    pub(crate) worker: Worker,
    pub(crate) results: Option<Results>,
    pub(crate) tab: Tab,
    pub(crate) orbit: Orbit,
    pub(crate) dialogs: Dialogs,
    pub(crate) status: Option<(String, Instant)>,
    pub(crate) pending: Option<Pending>,
    allow_close: bool,
    last_title: String,
    screenshot: Option<Screenshot>,
    viewer: Viewer3d,
    /// When set, file dialogs take paths from this queue instead of showing native dialogs (tests, automation).
    pub(crate) scripted_dialogs: Option<std::collections::VecDeque<PathBuf>>,
    /// Local MCP bridge, when enabled.
    pub(crate) automation: Option<crate::automation::Automation>,
    /// `--automation` on the command line enables the bridge for this run only.
    automation_forced: bool,
    /// Interface language when the settings do not choose one.
    system_language: Language,
    /// Guided tour (Help → Tour of SurfCraft; opens by itself on first launch).
    pub(crate) tour: Tour,
    pub(crate) tour_targets: tour::Targets,
    /// Folder for logs (and autosaves); `None` in tests unless a test sets a temporary one.
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) trace: crate::app_log::Trace,
    /// Help > Report a bug.
    pub(crate) bug_report: Option<crate::bugreport_ui::BugReport>,
    pub(crate) autosave: Option<crate::autosave::Autosave>,
    pub(crate) recovery: crate::autosave_ui::Recovery,
}

/// Board formats the app can open.
pub(crate) const BOARD_EXTS: &[&str] = &["brd", "s3d", "srf", "stp", "step"];

/// Read any supported board file. The flag is true for native `.brd` files (safe to save back in place).
pub(crate) fn read_board_file(path: &Path) -> Result<(Board, bool), surfcraft_io::Error> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let bytes = std::fs::read(path)?;
    let board = surfcraft_io::read_any(if ext.is_empty() { "brd" } else { &ext }, &bytes)?;
    Ok((board, ext.is_empty() || ext == "brd"))
}

const SC_NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const SC_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SC_SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SC_SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const SC_UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const SC_REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
const SC_REDO2: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const SC_FIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F);
const SC_QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
const SC_EXPORT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::P);

impl SurfCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        let set: Settings = cc
            .storage
            .and_then(|s| {
                eframe::get_value(s, settings::STORAGE_KEY)
                    .or_else(|| eframe::get_value(s, settings::OLD_STORAGE_KEY))
            })
            .unwrap_or_default();
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            let i = rs.adapter.get_info();
            let gpu = format!("{} ({:?}, {:?})", i.name, i.backend, i.device_type);
            log::info!("graphics: {gpu}");
            crate::logging::set_gpu(gpu);
        }
        crate::logging::set_detailed(set.detailed_logging);
        let system_language = i18n::system_language();
        i18n::set_language(set.language.unwrap_or(system_language));
        let mut board = surfcraft_io::templates::TEMPLATES[0].board();
        board.model = set.model.into();
        // First launch: start the tour, except for scripted runs (screenshots, AI assistant, tests).
        let tour = Tour {
            open: !set.tour_done && args.screenshot.is_none() && !args.automation && !cfg!(test),
            step: 0,
        };
        // Autosave except for screenshots and tests; crash recovery also not for the AI assistant.
        let autosave = args.screenshot.is_none() && !cfg!(test);
        let recover = autosave && !args.automation;
        let mut app = Self {
            ed: EditorState::default(),
            doc: Document::new(board, None),
            set,
            worker: Worker::default(),
            results: None,
            tab: Tab::Overview,
            orbit: Orbit::default(),
            dialogs: Dialogs::default(),
            status: None,
            pending: None,
            allow_close: false,
            last_title: String::new(),
            screenshot: args.screenshot.map(Screenshot::new),
            viewer: Viewer3d::default(),
            scripted_dialogs: None,
            automation: None,
            automation_forced: args.automation,
            system_language,
            tour,
            tour_targets: Default::default(),
            data_dir: if cfg!(test) {
                None
            } else {
                crate::logging::data_dir()
            },
            trace: Default::default(),
            bug_report: None,
            autosave: None,
            recovery: Default::default(),
        };
        app.ed.reset_for(&app.doc.board);
        if autosave {
            app.init_autosave(recover);
        }
        if let Some(p) = args.open {
            app.open_path(&p);
            if args.untitled {
                app.doc.path = None;
                app.set.recent.retain(|r| r != &p);
            }
        }
        app.tab = match args.tab.as_deref() {
            Some("outline") => Tab::Outline,
            Some("rocker" | "profile") => Tab::Profile,
            Some("section") => Tab::Section,
            Some("3d") => Tab::ThreeD,
            _ => Tab::Overview,
        };
        app
    }

    /// Curves the user is working on: the selection's curve, else those shown by the current view.
    pub(crate) fn active_curves(&self) -> Vec<crate::edit::CurveId> {
        use crate::edit::CurveId;
        if let Some(sel) = &self.ed.selection {
            return vec![sel.curve];
        }
        match self.tab {
            Tab::Profile => match self.ed.profile_target {
                views::ProfileTarget::Deck => vec![CurveId::Deck],
                views::ProfileTarget::Bottom => vec![CurveId::Bottom],
                views::ProfileTarget::Both => vec![CurveId::Deck, CurveId::Bottom],
            },
            Tab::Section
                if self
                    .doc
                    .board
                    .real_sections()
                    .contains(&self.ed.current_section) =>
            {
                vec![CurveId::Section(self.ed.current_section)]
            }
            _ => vec![CurveId::Outline],
        }
    }

    /// Smooth knots of `curves` (see [`crate::edit::smooth_knots`]) as one undo step, and say what happened.
    pub(crate) fn smooth(
        &mut self,
        curves: &[crate::edit::CurveId],
        knots: Option<&std::collections::BTreeSet<usize>>,
    ) {
        let mut probe = self.doc.board.clone();
        let n: usize = curves
            .iter()
            .map(|&c| crate::edit::smooth_knots(&mut probe, c, knots))
            .sum();
        if n == 0 {
            self.notify(tr("No kinks to smooth."));
            return;
        }
        self.doc.edit(|b| {
            for &c in curves {
                crate::edit::smooth_knots(b, c, knots);
            }
        });
        if n == 1 {
            self.notify(tr("Smoothed 1 point."));
        } else {
            self.notify(trf("Smoothed {0} points.", &[&n]));
        }
    }

    pub(crate) fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("message shown: {msg}");
        self.status = Some((msg, Instant::now()));
    }

    pub(crate) fn replace_document(&mut self, mut board: Board, path: Option<PathBuf>) {
        self.autosave_document_closed();
        board.model = self.set.model.into();
        self.doc = Document::new(board, path);
        self.ed.reset_for(&self.doc.board);
        self.results = None;
    }

    pub(crate) fn guarded(&mut self, action: Pending) {
        if self.doc.is_dirty() {
            self.pending = Some(action);
        } else {
            self.run_pending(action);
        }
    }

    fn run_pending(&mut self, action: Pending) {
        match action {
            Pending::New(b) => self.replace_document(*b, None),
            Pending::Open(p) => self.open_path(&p),
            Pending::Recover(e) => self.recover(*e),
            Pending::Quit => {
                self.allow_close = true;
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        match read_board_file(path) {
            Ok((mut board, native)) => {
                // Imported formats open as untitled boards so Save never overwrites the original file with .brd text.
                let doc_path = if native {
                    Some(path.to_path_buf())
                } else {
                    if board.info.name.is_empty() {
                        board.info.name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                    }
                    None
                };
                log::info!("opened {}", crate::logging::path(path));
                self.replace_document(board, doc_path);
                self.set.add_recent(path.to_path_buf());
                self.notify(trf("Opened {0}", &[&path.display()]));
            }
            Err(e) => {
                log::warn!("could not open {}: {e}", crate::logging::path(path));
                self.notify(trf("Could not open {0}: {1}", &[&path.display(), &e]));
            }
        }
    }

    fn file_dialog(&self) -> rfd::FileDialog {
        let mut d = rfd::FileDialog::new();
        if let Some(dir) = self.set.last_dir.as_ref().or_else(|| {
            self.doc
                .path
                .as_ref()
                .and_then(|p| p.parent().map(Path::new).map(|_| p))
        }) {
            let dir = if dir.is_dir() {
                dir.clone()
            } else {
                dir.parent().map(Path::to_path_buf).unwrap_or_default()
            };
            d = d.set_directory(dir);
        }
        d
    }

    /// Native open dialog, or the next scripted path in tests and automation.
    pub(crate) fn pick_file(&mut self, what: &str, exts: &[&str]) -> Option<PathBuf> {
        if let Some(q) = self.scripted_dialogs.as_mut() {
            return q.pop_front();
        }
        self.file_dialog().add_filter(what, exts).pick_file()
    }

    /// Native save dialog, or the next scripted path in tests and automation.
    pub(crate) fn save_file(
        &mut self,
        what: &str,
        ext: &str,
        name: Option<&str>,
    ) -> Option<PathBuf> {
        let mut p = match self.scripted_dialogs.as_mut() {
            Some(q) => q.pop_front()?,
            None => {
                let mut d = self.file_dialog().add_filter(what, &[ext]);
                if let Some(n) = name {
                    d = d.set_file_name(n);
                }
                d.save_file()?
            }
        };
        if p.extension().is_none() {
            p.set_extension(ext);
        }
        Some(p)
    }

    fn open_dialog(&mut self) {
        if let Some(p) = self.pick_file(tr("Boards (.brd, .s3d, .srf, .stp)"), BOARD_EXTS) {
            self.guarded(Pending::Open(p));
        }
    }

    fn save(&mut self) {
        match self.doc.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        let name = if self.doc.board.info.name.is_empty() {
            "board".to_string()
        } else {
            self.doc.board.info.name.clone()
        };
        if let Some(p) = self.save_file(tr("BoardCAD board"), "brd", Some(&format!("{name}.brd"))) {
            self.save_to(&p);
        }
    }

    fn save_to(&mut self, path: &Path) {
        match self.doc.save_to(path) {
            Ok(()) => {
                log::info!("saved {}", crate::logging::path(path));
                self.set.add_recent(path.to_path_buf());
                self.notify(trf("Saved {0}", &[&path.display()]));
            }
            Err(e) => {
                log::error!("could not save {}: {e}", crate::logging::path(path));
                self.notify(trf("Could not save {0}: {1}", &[&path.display(), &e]));
            }
        }
    }

    fn open_ghost(&mut self) {
        if let Some(p) = self.pick_file(tr("Boards (.brd, .s3d, .srf, .stp)"), BOARD_EXTS) {
            match read_board_file(&p) {
                Ok((mut b, _)) => {
                    b.model = self.set.model.into();
                    self.ed.ghost = Some(b);
                    self.set.view.ghost = true;
                }
                Err(e) => self.notify(trf("Could not open ghost: {0}", &[&e])),
            }
        }
    }

    fn import_partial(&mut self) {
        let Some(p) = self.pick_file(tr("BoardCAD curves"), &["otl", "pfl", "crs"]) else {
            return;
        };
        let parsed = std::fs::read_to_string(&p)
            .map_err(surfcraft_io::Error::from)
            .and_then(|t| surfcraft_io::brd::read_partial(&t));
        let part = match parsed {
            Ok(part) => part,
            Err(e) => return self.notify(trf("Could not import: {0}", &[&e])),
        };
        let current = self.ed.current_section;
        self.doc.edit(|b| {
            if let Some((s, g)) = part.outline {
                b.outline = s;
                b.outline_guides = g;
            }
            if let Some((s, g)) = part.bottom {
                b.bottom = s;
                b.bottom_guides = g;
            }
            if let Some((s, g)) = part.deck {
                b.deck = s;
                b.deck_guides = g;
            }
            if let Some(mut s) = part.section
                && b.real_sections().contains(&current)
            {
                s.position = b.sections[current].position;
                b.sections[current] = s;
            }
            b.apply_locks();
            b.fit_sections();
        });
        self.notify(trf("Imported {0}", &[&p.display()]));
    }

    pub(crate) fn load_background(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pick_file(tr("Image"), &["png", "jpg", "jpeg"]) else {
            return;
        };
        match crate::tools::Background::load(ctx, &p) {
            Ok(bg) => {
                self.ed.background = Some(bg);
                self.set.view.background_image = true;
                self.tab = Tab::Outline;
                self.notify(tr(
                    "Hold T and click the photo's tail, hold N and click its nose",
                ));
            }
            Err(e) => self.notify(trf("Could not load image: {0}", &[&e])),
        }
    }

    fn export_text(&mut self, what: &str, ext: &str, text: String) {
        self.save_bytes(what, ext, None, text.as_bytes());
    }

    /// Ask where to save an export from the Print & export window, then write it.
    fn save_export(&mut self, name: &str, bytes: &[u8]) {
        let ext = crate::export_ui::extension_of(name).to_owned();
        self.save_bytes(&ext.to_uppercase(), &ext, Some(name), bytes);
    }

    fn save_bytes(&mut self, what: &str, ext: &str, name: Option<&str>, bytes: &[u8]) {
        if let Some(p) = self.save_file(what, ext, name) {
            self.write_export(&p, bytes);
        }
    }

    pub fn write_export(&mut self, path: &Path, bytes: &[u8]) {
        let r = write_with_backup(path, bytes);
        let shown = crate::logging::path(path);
        match r {
            Ok(()) => {
                log::info!("exported {shown} ({} bytes)", bytes.len());
                self.notify(trf("Exported {0}", &[&path.display()]));
            }
            Err(e) => {
                log::error!("export to {shown} failed: {e}");
                self.notify(trf("Export failed: {0}", &[&e]));
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |sc: &KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(sc));
        if pressed(&SC_NEW) {
            self.dialogs.new_board = Some(NewBoard::new());
        }
        if pressed(&SC_OPEN) {
            self.open_dialog();
        }
        if pressed(&SC_SAVE_AS) {
            self.save_as();
        }
        if pressed(&SC_SAVE) {
            self.save();
        }
        if pressed(&SC_QUIT) {
            self.guarded(Pending::Quit);
        }
        if pressed(&SC_EXPORT) {
            self.dialogs.export = true;
        }
        if !typing {
            if pressed(&SC_REDO) || pressed(&SC_REDO2) {
                self.doc.redo();
            }
            if pressed(&SC_UNDO) {
                self.doc.undo();
            }
            if pressed(&SC_FIT) {
                self.ed.fit_all();
                self.orbit.reset();
            }
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.ed.tool = Tool::Select;
                self.ed.selection = None;
            }
            for (k, tab) in [
                (Key::Num1, Tab::Overview),
                (Key::Num2, Tab::Outline),
                (Key::Num3, Tab::Profile),
                (Key::Num4, Tab::Section),
                (Key::Num5, Tab::ThreeD),
            ] {
                if ctx.input_mut(|i| {
                    i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k))
                }) {
                    self.tab = tab;
                }
            }
        }
    }

    /// Document title for the window and dialogs, with "Untitled" in the interface language.
    fn display_title(&self) -> String {
        let title = self.doc.title();
        if self.doc.path.is_none() && self.doc.board.info.name.is_empty() {
            title.replacen("Untitled", tr("Untitled"), 1)
        } else {
            title
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let file = ui.menu_button(tr("File"), |ui| {
                if ui
                    .add(
                        egui::Button::new(tr("New…"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_NEW)),
                    )
                    .clicked()
                {
                    self.dialogs.new_board = Some(NewBoard::new());
                }
                if ui
                    .add(
                        egui::Button::new(tr("Open…"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_OPEN)),
                    )
                    .clicked()
                {
                    self.open_dialog();
                }
                ui.menu_button(tr("Open recent"), |ui| {
                    if self.set.recent.is_empty() {
                        ui.weak(tr("No recent boards"));
                    }
                    for p in self.set.recent.clone() {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if ui
                            .button(name)
                            .on_hover_text(p.display().to_string())
                            .clicked()
                        {
                            self.guarded(Pending::Open(p));
                        }
                    }
                });
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(tr("Save"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_SAVE)),
                    )
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add(
                        egui::Button::new(tr("Save as…"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_SAVE_AS)),
                    )
                    .clicked()
                {
                    self.save_as();
                }
                if ui
                    .button(tr("Save and set as original"))
                    .on_hover_text(tr(
                        "Save, then use this version as the green 'original' overlay",
                    ))
                    .clicked()
                {
                    self.save();
                    // Only when the save actually happened (not cancelled or failed).
                    if self.doc.path.is_some() && !self.doc.is_dirty() {
                        self.doc.refresh_original();
                    }
                }
                self.autosave_menu(ui);
                ui.separator();
                if ui
                    .button(tr("Open ghost board…"))
                    .on_hover_text(tr("Overlay another board for comparison"))
                    .clicked()
                {
                    self.open_ghost();
                }
                if ui
                    .add_enabled(
                        self.ed.ghost.is_some(),
                        egui::Button::new(tr("Close ghost board")),
                    )
                    .clicked()
                {
                    self.ed.ghost = None;
                }
                if ui
                    .button(tr("Compare with…"))
                    .on_hover_text(tr(
                        "Table of the differences with another board (or the ghost board)",
                    ))
                    .clicked()
                {
                    self.compare_with();
                }
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(tr("Print & export…"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_EXPORT)),
                    )
                    .clicked()
                {
                    self.dialogs.export = true;
                }
                ui.separator();
                if ui.button(tr("Import curves (.otl/.pfl/.crs)…")).clicked() {
                    self.import_partial();
                }
                if ui
                    .button(tr("Load background image…"))
                    .on_hover_text(tr(
                        "Trace a photo: hold T or N and click its tail or nose in the Outline view",
                    ))
                    .clicked()
                {
                    self.load_background(ui.ctx());
                }
                if ui
                    .add_enabled(
                        self.ed.background.is_some(),
                        egui::Button::new(tr("Remove background image")),
                    )
                    .clicked()
                {
                    self.ed.background = None;
                }
                ui.menu_button(tr("Export curves"), |ui| {
                    if ui.button(tr("Outline (.otl)…")).clicked() {
                        let t = surfcraft_io::brd::write_outline(&self.doc.board);
                        self.export_text(tr("Outline"), "otl", t);
                    }
                    if ui.button(tr("Profile (.pfl)…")).clicked() {
                        let t = surfcraft_io::brd::write_profile(&self.doc.board);
                        self.export_text(tr("Profile"), "pfl", t);
                    }
                    let cur = self.ed.current_section;
                    if ui
                        .add_enabled(
                            self.doc.board.real_sections().contains(&cur),
                            egui::Button::new(tr("Current cross-section (.crs)…")),
                        )
                        .clicked()
                    {
                        let t = surfcraft_io::brd::write_section(&self.doc.board.sections[cur]);
                        self.export_text(tr("Cross-section"), "crs", t);
                    }
                    ui.separator();
                    if ui.button(tr("Whole board as STEP (.stp)…")).clicked() {
                        let t = surfcraft_io::step::write(&self.doc.board);
                        self.export_text("STEP", "stp", t);
                    }
                });
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(tr("Quit"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_QUIT)),
                    )
                    .clicked()
                {
                    self.guarded(Pending::Quit);
                }
            });
            self.tour_targets.file = Some(file.response.rect);
            ui.menu_button(tr("Edit"), |ui| {
                if ui
                    .add_enabled(
                        self.doc.can_undo(),
                        egui::Button::new(tr("Undo"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_UNDO)),
                    )
                    .clicked()
                {
                    self.doc.undo();
                }
                if ui
                    .add_enabled(
                        self.doc.can_redo(),
                        egui::Button::new(tr("Redo"))
                            .shortcut_text(ui.ctx().format_shortcut(&SC_REDO)),
                    )
                    .clicked()
                {
                    self.doc.redo();
                }
                ui.separator();
                ui.radio_value(&mut self.ed.tool, Tool::Select, tr("Select and move"));
                ui.radio_value(&mut self.ed.tool, Tool::AddKnot, tr("Add control point"));
                ui.radio_value(&mut self.ed.tool, Tool::AddGuide, tr("Add guide point"));
                ui.separator();
                if ui
                    .add_enabled(
                        self.ed.selection.is_some(),
                        egui::Button::new(tr("Delete selected control points"))
                            .shortcut_text("Del"),
                    )
                    .clicked()
                    && let Some(sel) = self.ed.selection.take()
                {
                    self.doc.edit(|b| {
                        crate::edit::delete_knots(b, sel.curve, &sel.knots);
                    });
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.ed.selection.is_some(),
                        egui::Button::new(tr("Smooth selected points")),
                    )
                    .on_hover_text(tr("Removes the corner at each selected point: both handles turn to one tangent and keep their lengths."))
                    .clicked()
                    && let Some(sel) = self.ed.selection.clone()
                {
                    self.smooth(&[sel.curve], Some(&sel.knots));
                }
                if ui
                    .button(tr("Smooth kinks in this curve"))
                    .on_hover_text(tr("Smooths every corner that turns by less than 30°. Bigger corners, such as a squash tail, stay; on a cross-section this also rounds a hard rail edge."))
                    .clicked()
                {
                    let curves = self.active_curves();
                    self.smooth(&curves, None);
                }
                ui.separator();
                if ui.button(tr("Clear guide points")).clicked() {
                    self.doc.edit(|b| {
                        b.outline_guides.clear();
                        b.deck_guides.clear();
                        b.bottom_guides.clear();
                        for s in &mut b.sections {
                            s.guide_points.clear();
                        }
                    });
                }
            });
            ui.menu_button(tr("View"), |ui| {
                let v = &mut self.set.view;
                ui.checkbox(&mut v.grid, tr("Grid"));
                ui.checkbox(&mut v.control_points, tr("Control points"));
                ui.checkbox(&mut v.guide_points, tr("Guide points"));
                ui.checkbox(&mut v.measurements, tr("Measurements"));
                ui.checkbox(&mut v.curvature, tr("Curvature comb"));
                ui.checkbox(&mut v.section_positions, tr("Cross-section positions"));
                ui.checkbox(&mut v.other_sections, tr("Other cross-sections"));
                ui.checkbox(&mut v.apex_line, tr("Rail apex"));
                ui.checkbox(&mut v.tuck_line, tr("Tuck-under point"));
                ui.checkbox(&mut v.fins, tr("Fins"));
                ui.checkbox(&mut v.center_line, tr("Centre line"));
                ui.checkbox(&mut v.ghost, tr("Ghost board"));
                ui.checkbox(&mut v.original, tr("Original board"));
                ui.checkbox(&mut v.center_of_mass, tr("Centre of mass"));
                ui.checkbox(&mut v.background_image, tr("Background image"));
                ui.checkbox(&mut self.set.blanks.show, tr("Foam blank"));
                if let Some(bg) = self.ed.background.as_mut() {
                    ui.add(egui::Slider::new(&mut bg.opacity, 0.05..=1.0).text(tr("Image opacity")));
                }
                ui.checkbox(
                    &mut self.viewer.zebra,
                    tr("3D zebra stripes (surface fairness)"),
                );
                ui.checkbox(&mut self.orbit.wireframe, tr("3D wireframe (CPU preview)"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Profile vertical scale"));
                    ui.add(
                        egui::DragValue::new(&mut self.set.profile_exaggeration)
                            .range(1.0..=10.0)
                            .speed(0.1)
                            .suffix("×"),
                    );
                });
                ui.separator();
                if ui
                    .add(egui::Button::new(tr("Fit all")).shortcut_text("F"))
                    .clicked()
                {
                    self.ed.fit_all();
                    self.orbit.reset();
                }
                ui.checkbox(&mut self.set.show_side_panel, tr("Specs panel"));
                ui.separator();
                self.language_menu(ui);
                theme_buttons(ui);
            });
            let board = ui.menu_button(tr("Board"), |ui| {
                if ui.button(tr("Scale…")).clicked() {
                    self.dialogs.scale = Some(Scale::of(&self.doc.board));
                }
                if ui.button(tr("Board info…")).clicked() {
                    self.dialogs.info = true;
                }
                if ui
                    .button(tr("Fins…"))
                    .on_hover_text(tr(
                        "Fin setups (thruster, quad, 5 fins…) with FCS I, FCS II, Futures or US box footprints",
                    ))
                    .clicked()
                {
                    self.dialogs.fins = true;
                }
                if ui.button(tr("Weight calculator…")).clicked() {
                    self.dialogs.weight = Some(crate::tools::WeightState::new(&self.doc.board));
                }
                if ui
                    .button(tr("Volume advisor…"))
                    .on_hover_text(tr("Suggested volume for a rider's weight, skill and waves"))
                    .clicked()
                {
                    self.dialogs.volume_advisor = true;
                }
                if ui
                    .button(tr("Fit in blank…"))
                    .on_hover_text(tr("Check that the board fits inside a foam blank before you buy it"))
                    .clicked()
                {
                    self.dialogs.blank.open = true;
                }
                if ui.button(tr("Guide points…")).clicked() {
                    self.dialogs.guides = true;
                }
                ui.separator();
                if ui.button(tr("Flip nose and tail")).clicked() {
                    self.doc.edit(Board::flip);
                }
                if ui.button(tr("Put lowest rocker point at zero")).clicked() {
                    self.doc.edit(Board::on_rocker_changed);
                }
                ui.separator();
                ui.label(tr("Cross-section blending"));
                // Shows the open board's model (so undo keeps menu and board in step); the choice also becomes
                // the default for new boards.
                let current = match self.doc.board.model {
                    surfcraft_core::SurfaceModel::SBlend => Model::SBlend,
                    surfcraft_core::SurfaceModel::ControlPoint => Model::ControlPoint,
                };
                let mut m = current;
                ui.radio_value(&mut m, Model::SBlend, tr("S-blend (smooth, default)"));
                ui.radio_value(&mut m, Model::ControlPoint, tr("Control-point matching"));
                if m != current {
                    self.set.model = m;
                    self.doc.edit(|b| b.model = m.into());
                }
            });
            self.tour_targets.board = Some(board.response.rect);
            ui.menu_button(tr("Shape"), |ui| {
                if ui
                    .button(tr("Bottom contours…"))
                    .on_hover_text(tr("Concaves, vee and channels along the board, applied to every cross-section in a zone."))
                    .clicked()
                {
                    self.dialogs.shaping.open_contours();
                }
                if ui
                    .button(tr("Rails…"))
                    .on_hover_text(tr("Rail shape for every cross-section: apex height (50/50, 60/40, 70/30), softness and tuck."))
                    .clicked()
                {
                    self.dialogs.shaping.open_rails();
                }
            });
            let machine = ui.menu_button(tr("Machine"), |ui| {
                if ui
                    .button(tr("Machine (CAM)…"))
                    .on_hover_text(tr("CNC toolpaths and G-code, hot-wire cutting"))
                    .clicked()
                {
                    self.dialogs.cam = true;
                }
            });
            self.tour_targets.machine = Some(machine.response.rect);
            let help = ui.menu_button(tr("Help"), |ui| {
                if ui.button(tr("Tour of SurfCraft")).clicked() {
                    self.tour.start();
                    ui.close();
                }
                ui.separator();
                self.log_menu_items(ui);
                ui.separator();
                ui.label(tr("Shortcuts"));
                ui.weak(tr(
                    "Drag: move · Alt+drag: fine · Shift/Cmd+click: multi-select",
                ));
                ui.weak(tr(
                    "Drag empty space: box select · Arrows: nudge · Del: delete point",
                ));
                ui.weak(tr(
                    "Wheel/pinch: zoom · Right/middle-drag or Space+drag: pan · F: fit",
                ));
                ui.weak(tr("+/-: next/previous cross-section · Cmd+1…5: views"));
                ui.separator();
                ui.checkbox(
                    &mut self.set.allow_automation,
                    tr("Allow AI assistant control (MCP)"),
                )
                .on_hover_text(tr("Lets the SurfCraft MCP server on this computer edit the open board. Local connections only, protected by a private token."));
                ui.separator();
                if ui.button(tr("About SurfCraft")).clicked() {
                    self.dialogs.about = true;
                }
            });
            self.tour_targets.help = Some(help.response.rect);
        });
    }

    /// Interface language picker; the choice applies from the next frame and is saved with the settings.
    fn language_menu(&mut self, ui: &mut egui::Ui) {
        // Keep the English word visible so a user lost in a foreign language can find the way back.
        let title = if i18n::language() == Language::En {
            "Language".to_string()
        } else {
            format!("{} (Language)", tr("Language"))
        };
        ui.menu_button(title, |ui| {
            let current = self.set.language.unwrap_or(self.system_language);
            for lang in Language::ALL {
                if ui.radio(current == lang, lang.native_name()).clicked() {
                    self.set.language = Some(lang);
                    i18n::set_language(lang);
                    ui.close();
                }
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        // Areas the tour points at.
        let (mut tabs, mut tools) = (None, None);
        ui.horizontal(|ui| {
            for (tab, name) in [
                (Tab::Overview, tr("Overview")),
                (Tab::Outline, tr("Outline")),
                (Tab::Profile, tr("Rocker")),
                (Tab::Section, tr("Cross-section")),
                (Tab::ThreeD, "3D"),
            ] {
                let r = ui.selectable_value(&mut self.tab, tab, name).rect;
                tabs = Some(tabs.map_or(r, |t: egui::Rect| t.union(r)));
            }
            ui.separator();
            let mut grow =
                |r: egui::Rect| tools = Some(tools.map_or(r, |t: egui::Rect| t.union(r)));
            grow(
                ui.selectable_value(&mut self.ed.tool, Tool::Select, tr("Select"))
                    .rect,
            );
            grow(
                ui.selectable_value(&mut self.ed.tool, Tool::AddKnot, tr("+ Point"))
                    .rect,
            );
            grow(
                ui.selectable_value(&mut self.ed.tool, Tool::AddGuide, tr("+ Guide"))
                    .rect,
            );
            ui.separator();
            let undo = ui.add_enabled(self.doc.can_undo(), egui::Button::new(tr("Undo")));
            grow(undo.rect);
            if undo.clicked() {
                self.doc.undo();
            }
            let redo = ui.add_enabled(self.doc.can_redo(), egui::Button::new(tr("Redo")));
            grow(redo.rect);
            if redo.clicked() {
                self.doc.redo();
            }
            ui.separator();
            let mut unit = self.set.unit;
            let units = egui::ComboBox::new("units", tr("Units"))
                .selected_text(tr(surfcraft_core::LengthUnit::from(unit).label()))
                .show_ui(ui, |ui| {
                    for u in surfcraft_core::LengthUnit::ALL {
                        ui.selectable_value(&mut unit, Unit::from(u), tr(u.label()));
                    }
                });
            grow(units.response.rect);
            self.set.unit = unit;
        });
        self.tour_targets.tabs = tabs;
        self.tour_targets.tools = tools;
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some((msg, at)) = &self.status {
                if at.elapsed() < Duration::from_secs(6) {
                    ui.label(msg);
                } else {
                    self.status = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(c) = self.ed.cursor {
                    ui.monospace(format!("{}  {}", self.set.fmt(c.x), self.set.fmt(c.y)));
                }
                if self.worker.is_busy() {
                    ui.spinner();
                }
            });
        });
    }

    fn confirm_discard(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut choice = None;
        let name = self.display_title();
        egui::Modal::new(egui::Id::unique("discard")).show(ctx, |ui| {
            ui.heading(tr("Unsaved changes"));
            ui.label(trf(
                "Save changes to {0} first?",
                &[&name.trim_end_matches(" *")],
            ));
            ui.horizontal(|ui| {
                if ui.button(tr("Save")).clicked() {
                    choice = Some(0);
                }
                if ui.button(tr("Don't save")).clicked() {
                    choice = Some(1);
                }
                if ui.button(tr("Cancel")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.save();
                if !self.doc.is_dirty()
                    && let Some(a) = self.pending.take()
                {
                    self.run_pending(a);
                }
            }
            Some(1) => {
                if let Some(a) = self.pending.take() {
                    self.run_pending(a);
                }
            }
            Some(2) => self.pending = None,
            _ => {}
        }
    }

    fn central(&mut self, ui: &mut egui::Ui, rs: Option<&egui_wgpu::RenderState>) {
        let pal = Palette::new(ui.visuals().dark_mode);
        let busy = self.worker.is_busy();
        let base = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(214, 220, 228)
        } else {
            egui::Color32::from_rgb(196, 204, 214)
        };
        let mesh = self.results.as_ref().map(|r| r.mesh.clone());
        // GPU renderer unless the user asked for wireframe (CPU preview) or wgpu is unavailable.
        let rs = rs.filter(|_| !self.orbit.wireframe);
        let viewer = &mut self.viewer;
        let mut view3d = |ui: &mut egui::Ui, orbit: &mut Orbit| {
            if !viewer.show(ui, rs, mesh.as_ref(), orbit, pal.bg) {
                preview3d::show(ui, mesh.as_deref(), orbit, pal.bg, base, busy);
            }
        };
        match self.tab {
            Tab::Outline => views::outline(ui, &mut self.doc, &mut self.ed, &self.set, &pal),
            Tab::Profile => views::profile(ui, &mut self.doc, &mut self.ed, &self.set, &pal),
            Tab::Section => views::section(ui, &mut self.doc, &mut self.ed, &self.set, &pal),
            Tab::ThreeD => view3d(ui, &mut self.orbit),
            Tab::Overview => {
                let size = ui.available_size();
                let half = egui::vec2((size.x - 6.0) / 2.0, (size.y - 6.0) / 2.0);
                let cell = |ui: &mut egui::Ui, f: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(
                        half,
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_size(half);
                            ui.push_id(ui.next_auto_id(), |ui| f(ui));
                        },
                    );
                };
                ui.horizontal(|ui| {
                    cell(ui, &mut |ui| {
                        views::outline(ui, &mut self.doc, &mut self.ed, &self.set, &pal)
                    });
                    cell(ui, &mut |ui| {
                        views::section(ui, &mut self.doc, &mut self.ed, &self.set, &pal)
                    });
                });
                ui.horizontal(|ui| {
                    cell(ui, &mut |ui| {
                        views::profile(ui, &mut self.doc, &mut self.ed, &self.set, &pal)
                    });
                    cell(ui, &mut |ui| view3d(ui, &mut self.orbit));
                });
            }
        }
    }
}

impl eframe::App for SurfCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let render_state = frame.wgpu_render_state().cloned();
        i18n::set_language(self.set.language.unwrap_or(self.system_language));

        if let Some(shot) = self.screenshot.as_mut()
            && shot.tick(&ctx)
        {
            self.screenshot = None;
            self.allow_close = true;
        }

        // Window close button.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Dropped files.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if let Some(p) = dropped.into_iter().find(|p| {
            p.extension()
                .is_some_and(|e| BOARD_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
        }) {
            self.guarded(Pending::Open(p));
        }

        self.shortcuts(&ctx);
        let want_automation = self.set.allow_automation || self.automation_forced;
        self.set_automation(&ctx, want_automation);
        self.handle_automation(&ctx);

        let title = format!("{} — SurfCraft", self.display_title());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        self.worker
            .request(&self.doc.board, self.doc.revision, &ctx);
        if let Some(r) = self.worker.poll(&ctx) {
            self.results = Some(r);
        }
        self.ed.center_of_mass = self.results.as_ref().map(|r| r.center_of_mass);
        let stale = self
            .results
            .as_ref()
            .is_none_or(|r| r.revision != self.doc.revision);

        self.tour_targets = Default::default();
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        // Fixed height: the cursor readout appearing must not resize (and so shift) the views.
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .show(ui, |ui| self.status_bar(ui));
        if self.set.show_side_panel {
            let side = egui::Panel::right("side")
                .default_size(300.0)
                .resizable(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        panels::specs(ui, &self.doc, self.results.as_ref(), stale, &self.set);
                        ui.separator();
                        ui.heading(tr("Control point"));
                        panels::knot_inspector(ui, &mut self.doc, &mut self.ed, &self.set);
                    });
                });
            self.tour_targets.side = Some(side.response.rect);
        }
        let central = egui::CentralPanel::default().show(ui, |ui| {
            self.ed.cursor = None;
            self.central(ui, render_state.as_ref());
        });
        self.tour_targets.canvas = Some(central.response.rect);

        if let Some(b) = dialogs::new_board_window(&ctx, &mut self.dialogs.new_board, &self.set) {
            self.guarded(Pending::New(Box::new(b)));
        }
        dialogs::scale_window(&ctx, &mut self.dialogs.scale, &mut self.doc, &self.set);
        if self.dialogs.info {
            dialogs::info_window(&ctx, &mut self.dialogs.info, &mut self.doc);
        }
        if self.dialogs.fins {
            dialogs::fins_window(&ctx, &mut self.dialogs.fins, &mut self.doc, &self.set);
        }
        if self.dialogs.about {
            dialogs::about_window(&ctx, &mut self.dialogs.about);
        }
        let volume = self
            .results
            .as_ref()
            .filter(|r| r.revision == self.doc.revision)
            .map(|r| r.specs.volume);
        crate::tools::weight_window(&ctx, &mut self.dialogs.weight, &self.doc, volume, &self.set);
        if self.dialogs.volume_advisor {
            crate::volume_ui::advisor_window(
                &ctx,
                &mut self.dialogs.volume_advisor,
                &mut self.doc,
                volume,
                &self.set,
            );
        }
        crate::blank_ui::window(&ctx, &mut self.dialogs.blank, &mut self.doc, &mut self.set);
        if self.dialogs.guides {
            let curve = self
                .ed
                .selection
                .as_ref()
                .map(|s| s.curve)
                .unwrap_or(match self.tab {
                    Tab::Profile => crate::edit::CurveId::Deck,
                    Tab::Section => crate::edit::CurveId::Section(self.ed.current_section),
                    _ => crate::edit::CurveId::Outline,
                });
            crate::tools::guide_window(
                &ctx,
                &mut self.dialogs.guides,
                curve,
                &mut self.doc,
                &self.set,
            );
        }
        if self.dialogs.cam {
            let title = self
                .doc
                .title()
                .trim_end_matches(" *")
                .trim_end_matches(".brd")
                .to_string();
            if let Some((name, bytes)) = crate::cam_ui::window(
                &ctx,
                &mut self.dialogs.cam,
                &mut self.dialogs.cam_state,
                &self.doc.board,
                self.doc.revision,
                &self.set,
                &title,
            ) && let Some(p) = self.save_file("G-code", "nc", Some(&name))
            {
                self.write_export(&p, &bytes);
            }
        }
        if self.dialogs.export {
            let title = self
                .doc
                .title()
                .trim_end_matches(" *")
                .trim_end_matches(".brd")
                .to_string();
            if let Some((name, bytes)) = crate::export_ui::window(
                &ctx,
                &mut self.dialogs.export,
                &mut self.dialogs.export_state,
                &self.doc.board,
                &self.set,
                &title,
            ) {
                self.save_export(&name, &bytes);
            }
        }
        if let Some(msg) = crate::shaping_ui::windows(
            &ctx,
            &mut self.dialogs.shaping,
            &mut self.doc,
            &mut self.ed,
            &mut self.set,
        ) {
            self.notify(msg);
        }
        self.compare_window(&ctx);
        self.bug_report_window(&ctx);
        self.autosave_frame(&ctx);
        let tour = self.tour.show(&ctx, &self.tour_targets);
        if let Some(tab) = tour.tab {
            self.tab = tab;
        }
        if tour.closed {
            self.set.tour_done = true;
        }
        self.confirm_discard(&ctx);
        // Close grouped edits (typing in Board info / Fins) once no field has focus and no button is held.
        if !ctx.egui_wants_keyboard_input() && !ctx.input(|i| i.pointer.any_down()) {
            self.doc.end_group();
        }
        self.log_frame(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, settings::STORAGE_KEY, &self.set);
    }

    fn on_exit(&mut self) {
        self.autosave_shutdown();
        log::info!("exit");
    }
}

/// System / dark / light theme buttons. egui's own use emoji icons, which the bundled fonts cannot draw, and
/// English tooltips.
fn theme_buttons(ui: &mut egui::Ui) {
    use egui::ThemePreference as T;
    let mut pref = ui.options(|o| o.theme_preference);
    ui.horizontal(|ui| {
        ui.label(tr("Theme"));
        ui.selectable_value(&mut pref, T::System, tr("System"))
            .on_hover_text(tr("Follow the system theme"));
        ui.selectable_value(&mut pref, T::Dark, tr("Dark"));
        ui.selectable_value(&mut pref, T::Light, tr("Light"));
    });
    ui.ctx().set_theme(pref);
}
