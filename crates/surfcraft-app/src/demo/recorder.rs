//! Records a scripted session of the real app as a narrated MP4.
//!
//! Every video frame is one or more egui passes of the app followed by a wgpu render of the window, so what you see
//! is exactly what the app draws. Input is synthetic (pointer moves, button presses, keys, text) and goes through
//! egui like real input. A plugin paints the cursor, click ripples, captions, key badges and title cards on the top
//! layer. Narration comes from macOS `say` (skipped elsewhere); ffmpeg encodes the frames and muxes the audio.
//!
//! Output, in `target/videos/<name>/` (or `$SURFCRAFT_VIDEO_DIR/<name>/`): `<name>.mp4`, `<name>.srt`,
//! `narration.wav`, an optional `preview.gif`, and `work/` with the intermediate files.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, FontId, Id, Key, LayerId, Modifiers, Order, Pos2, Rect, Stroke, Vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

use surfcraft_core::Handle;

use crate::app::{SurfCraftApp, Tab};
use crate::edit::CurveId;
use crate::gui_tests::H;

/// Frames per second of the video.
pub const FPS: u32 = 30;
/// Window size in points. With [`PIXELS_PER_POINT`] the frames are 1920 x 1080 pixels.
pub const WINDOW: Vec2 = egui::vec2(1440.0, 810.0);
pub const PIXELS_PER_POINT: f32 = 4.0 / 3.0;
/// Narration starts this long after its segment begins, and the segment lasts this long after it ends.
const LEAD: f64 = 0.25;
const TAIL: f64 = 0.7;
/// Caption fade, in frames.
const FADE: u64 = 8;
/// Frames of an animated overlay (click ripple).
const RIPPLE: u64 = 14;
const SAMPLE_RATE: u32 = 22_050;

/// What the overlay paints on top of the app, shared with [`OverlayPlugin`].
#[derive(Default)]
struct Overlay {
    frame: u64,
    cursor: Option<Pos2>,
    pressed: bool,
    ripples: Vec<(Pos2, u64)>,
    caption: Option<(String, f32)>,
    badge: Option<(String, u64)>,
    card: Option<(String, String, f32)>,
}

impl Overlay {
    /// True while something in the overlay changes from frame to frame.
    fn animating(&self) -> bool {
        self.ripples.iter().any(|&(_, at)| self.frame < at + RIPPLE)
    }

    fn paint(&self, ctx: &egui::Context) {
        let p = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("demo-overlay")));
        let screen = ctx.content_rect();
        if let Some((title, sub, a)) = &self.card {
            paint_card(&p, screen, title, sub, *a);
        }
        if let Some((text, a)) = &self.caption {
            paint_caption(&p, screen, text, *a);
        }
        if let Some((text, until)) = &self.badge
            && self.frame < *until
        {
            let a = ((until - self.frame) as f32 / FADE as f32).min(1.0);
            paint_badge(&p, screen, text, a);
        }
        for &(at, start) in &self.ripples {
            if self.frame >= start && self.frame < start + RIPPLE {
                let t = (self.frame - start) as f32 / RIPPLE as f32;
                let a = (1.0 - t) * 0.9;
                p.circle_stroke(
                    at,
                    6.0 + 26.0 * t,
                    Stroke::new(3.0 - 1.5 * t, rgba(255, 196, 64, a)),
                );
            }
        }
        let card_up = self.card.as_ref().is_some_and(|c| c.2 > 0.3);
        if let Some(c) = self.cursor
            && !card_up
        {
            paint_cursor(&p, c, self.pressed);
        }
    }
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn paint_card(p: &egui::Painter, screen: Rect, title: &str, sub: &str, a: f32) {
    p.rect_filled(screen, 0.0, rgba(10, 22, 34, 0.97 * a));
    let c = screen.center();
    p.text(
        c - egui::vec2(0.0, 34.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(60.0),
        rgba(255, 255, 255, a),
    );
    p.line_segment(
        [c + egui::vec2(-120.0, 8.0), c + egui::vec2(120.0, 8.0)],
        Stroke::new(3.0, rgba(64, 170, 230, a)),
    );
    p.text(
        c + egui::vec2(0.0, 42.0),
        Align2::CENTER_CENTER,
        sub,
        FontId::proportional(26.0),
        rgba(200, 215, 230, a),
    );
}

fn paint_caption(p: &egui::Painter, screen: Rect, text: &str, a: f32) {
    if a <= 0.0 {
        return;
    }
    let galley = p.layout(
        text.to_owned(),
        FontId::proportional(21.0),
        rgba(255, 255, 255, a),
        screen.width() * 0.62,
    );
    let size = galley.size();
    let pos = egui::pos2(
        screen.center().x - size.x / 2.0,
        screen.bottom() - 58.0 - size.y,
    );
    let bg = Rect::from_min_size(pos, size).expand2(egui::vec2(18.0, 10.0));
    p.rect_filled(bg, 8.0, rgba(0, 0, 0, 0.78 * a));
    p.galley(pos, galley, Color32::WHITE);
}

fn paint_badge(p: &egui::Painter, screen: Rect, text: &str, a: f32) {
    let galley = p.layout_no_wrap(
        text.to_owned(),
        FontId::monospace(20.0),
        rgba(20, 20, 20, a),
    );
    let size = galley.size();
    // Top middle of the drawing area, clear of the menus and the specs panel.
    let pos = egui::pos2(
        screen.center().x - 120.0 - size.x / 2.0,
        screen.top() + 84.0,
    );
    let bg = Rect::from_min_size(pos, size).expand2(egui::vec2(14.0, 8.0));
    p.rect_filled(bg, 6.0, rgba(255, 196, 64, 0.95 * a));
    p.galley(pos, galley, Color32::BLACK);
}

/// An arrow pointer with its tip at `at`.
fn paint_cursor(p: &egui::Painter, at: Pos2, pressed: bool) {
    const PTS: [[f32; 2]; 7] = [
        [0.0, 0.0],
        [0.0, 17.0],
        [4.0, 13.0],
        [7.0, 20.0],
        [9.5, 19.0],
        [6.5, 12.0],
        [12.0, 12.0],
    ];
    // Convex pieces sharing the outline's vertices (a fan from the tip plus the tail).
    const TRIS: [[u32; 3]; 5] = [[0, 1, 2], [0, 2, 5], [0, 5, 6], [2, 3, 4], [2, 4, 5]];
    let s = 1.25;
    let pt = |o: Vec2, i: usize| at + o + egui::vec2(PTS[i][0], PTS[i][1]) * s;
    let mut shadow = egui::Mesh::default();
    let mut body = egui::Mesh::default();
    let fill = if pressed {
        Color32::from_rgb(255, 226, 150)
    } else {
        Color32::WHITE
    };
    for i in 0..PTS.len() {
        shadow.colored_vertex(pt(egui::vec2(1.5, 2.0), i), rgba(0, 0, 0, 0.35));
        body.colored_vertex(pt(Vec2::ZERO, i), fill);
    }
    for t in TRIS {
        shadow.add_triangle(t[0], t[1], t[2]);
        body.add_triangle(t[0], t[1], t[2]);
    }
    p.add(shadow);
    p.add(body);
    let outline: Vec<Pos2> = (0..PTS.len()).map(|i| pt(Vec2::ZERO, i)).collect();
    p.add(egui::Shape::closed_line(
        outline,
        Stroke::new(1.3, Color32::BLACK),
    ));
}

struct OverlayPlugin(Arc<Mutex<Overlay>>);

impl egui::Plugin for OverlayPlugin {
    fn debug_name(&self) -> &'static str {
        "demo overlay"
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        self.0.lock().unwrap().paint(ui.ctx());
    }
}

/// A stretch of the video with one caption (and its narration, when spoken).
struct Segment {
    text: String,
    start: u64,
    end: u64,
    /// Narration samples (mono, 16 bit, [`SAMPLE_RATE`]).
    audio: Option<Vec<i16>>,
    min_frames: u64,
}

/// Where a pointer action happens: a widget found by its accessible label, or a position in points.
pub enum At<'a> {
    Label(&'a str),
    Pos(Pos2),
}

impl<'a> From<&'a str> for At<'a> {
    fn from(l: &'a str) -> Self {
        At::Label(l)
    }
}

impl From<Pos2> for At<'_> {
    fn from(p: Pos2) -> Self {
        At::Pos(p)
    }
}

/// Finished video files.
pub struct Output {
    pub mp4: PathBuf,
    pub srt: PathBuf,
    pub seconds: f64,
}

pub struct Recorder {
    pub h: H,
    name: String,
    dir: PathBuf,
    overlay: Arc<Mutex<Overlay>>,
    ffmpeg: PathBuf,
    encoder: Option<(Child, ChildStdin)>,
    size: (u32, u32),
    frame: u64,
    cursor: Pos2,
    /// Modifiers held during pointer actions (e.g. Alt for a fine drag).
    held: Modifiers,
    last: Option<Vec<u8>>,
    /// Frames since the app last got input; once still, frames are repeated instead of rendered.
    idle: u32,
    segments: Vec<Segment>,
    open: Option<usize>,
    voice: Option<String>,
    started: Instant,
    render_time: Duration,
    rendered: u64,
}

impl Recorder {
    /// Start a recording named `name` with the app in its test configuration (English, dark theme, no tour,
    /// no autosave or crash recovery, scripted file dialogs).
    pub fn new(name: &str) -> Self {
        let overlay = Arc::new(Mutex::new(Overlay::default()));
        let plugin = OverlayPlugin(overlay.clone());
        let mut h = Harness::builder()
            .with_size(WINDOW)
            .with_pixels_per_point(PIXELS_PER_POINT)
            .with_theme(egui::Theme::Dark)
            .with_step_dt(1.0 / FPS as f32)
            .with_max_steps(400)
            .with_render_cursor(false)
            .wgpu()
            .with_context_setup(move |ctx| ctx.add_plugin(plugin))
            .build_eframe(|cc| SurfCraftApp::new(cc, crate::Args::default()));
        h.ctx
            .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
        h.state_mut().scripted_dialogs = Some(Default::default());
        crate::gui_tests::settle(&mut h);
        let dir = video_root().join(name);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let ffmpeg = find_ffmpeg().expect(
            "ffmpeg is needed to record videos (set $FFMPEG or put it on the PATH, e.g. brew install ffmpeg)",
        );
        let voice = pick_voice();
        match &voice {
            Some(v) => eprintln!("[{name}] narration voice: {v}"),
            None => eprintln!("[{name}] no `say` voice: recording without narration audio"),
        }
        Self {
            h,
            name: name.to_owned(),
            dir,
            overlay,
            ffmpeg,
            encoder: None,
            size: (0, 0),
            frame: 0,
            cursor: egui::pos2(WINDOW.x * 0.55, WINDOW.y * 0.6),
            held: Modifiers::NONE,
            last: None,
            idle: 0,
            segments: Vec::new(),
            open: None,
            voice,
            started: Instant::now(),
            render_time: Duration::ZERO,
            rendered: 0,
        }
    }

    pub fn app(&self) -> &SurfCraftApp {
        self.h.state()
    }

    pub fn app_mut(&mut self) -> &mut SurfCraftApp {
        self.h.state_mut()
    }

    /// Scratch directory of this recording (files the script saves go here, never into the user's folders).
    pub fn work_dir(&self) -> PathBuf {
        self.dir.join("work")
    }

    /// Current time in the video, in seconds.
    pub fn time(&self) -> f64 {
        self.frame as f64 / FPS as f64
    }

    // ------------------------------------------------------------------------------------------- frames

    fn overlay(&self) -> std::sync::MutexGuard<'_, Overlay> {
        self.overlay.lock().unwrap()
    }

    /// Emit one video frame. `input` says whether events were queued for the app.
    fn frame(&mut self, input: bool) {
        self.update_caption();
        let still = !input && self.idle >= 6 && !self.overlay().animating() && self.last.is_some();
        {
            let mut o = self.overlay();
            o.frame = self.frame;
            o.cursor = Some(self.cursor);
        }
        if still {
            let bytes = self.last.take().unwrap();
            self.write(&bytes);
            self.last = Some(bytes);
        } else {
            self.idle = if input { 0 } else { self.idle + 1 };
            self.h.step();
            self.sync_worker();
            let t = Instant::now();
            let img = self.h.render().expect("render");
            self.render_time += t.elapsed();
            self.rendered += 1;
            let bytes = img.into_raw();
            self.write(&bytes);
            self.last = Some(bytes);
        }
        self.frame += 1;
    }

    /// Wait (in real time, without emitting frames) until the background worker has caught up with the board, so
    /// specs, volume and the 3D mesh in the video are always current.
    fn sync_worker(&mut self) {
        let start = Instant::now();
        loop {
            let app = self.h.state();
            let current = app
                .results
                .as_ref()
                .is_some_and(|r| r.revision == app.doc.revision);
            if current && !app.worker.is_busy() {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(120),
                "background worker did not finish"
            );
            std::thread::sleep(Duration::from_millis(3));
            self.h.run_steps(1);
        }
    }

    fn write(&mut self, rgba: &[u8]) {
        if self.encoder.is_none() {
            let size = self.h.render().expect("render").dimensions();
            self.size = size;
            let out = self.dir.join("work/video.mp4");
            let mut child = Command::new(&self.ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
                .args(["-s", &format!("{}x{}", size.0, size.1)])
                .args(["-r", &FPS.to_string(), "-i", "-"])
                .args([
                    "-vf",
                    "crop=trunc(iw/2)*2:trunc(ih/2)*2,scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
                ])
                .args(["-c:v", "libx264", "-preset", "medium", "-tune", "animation"])
                .args(["-crf", "20", "-pix_fmt", "yuv420p"])
                .args(["-colorspace", "bt709", "-color_primaries", "bt709"])
                .args(["-color_trc", "bt709", "-color_range", "tv"])
                .arg(&out)
                .stdin(Stdio::piped())
                .spawn()
                .expect("start ffmpeg");
            let stdin = child.stdin.take().unwrap();
            self.encoder = Some((child, stdin));
        }
        let (_, stdin) = self.encoder.as_mut().unwrap();
        stdin.write_all(rgba).expect("write frame to ffmpeg");
    }

    fn update_caption(&mut self) {
        let Some(i) = self.open else {
            return;
        };
        let s = &self.segments[i];
        let a = ((self.frame - s.start) as f32 / FADE as f32).min(1.0);
        let text = s.text.clone();
        self.overlay().caption = Some((text, a));
    }

    // ------------------------------------------------------------------------------------------- timing

    /// Keep the picture still for `seconds` (the app keeps running).
    pub fn hold(&mut self, seconds: f64) {
        for _ in 0..frames(seconds) {
            self.frame(false);
        }
    }

    /// Start a narrated segment: `text` is spoken (if a voice is available) and shown as the caption. The previous
    /// segment first runs until its narration is over. Actions that follow happen while it is spoken.
    pub fn narrate(&mut self, text: &str) {
        self.segment(text, Some(text));
    }

    /// Narrate with a caption that differs from what is spoken (e.g. a URL shown but spoken in words).
    pub fn narrate_as(&mut self, caption: &str, spoken: &str) {
        self.segment(caption, Some(spoken));
    }

    /// Like [`Self::narrate`] but silent: a caption shown long enough to read.
    #[allow(dead_code)] // scripting API (docs/videos.md), for silent captions
    pub fn caption(&mut self, text: &str) {
        self.segment(text, None);
    }

    fn segment(&mut self, text: &str, spoken: Option<&str>) {
        self.end_segment();
        let audio = spoken.and_then(|spoken| {
            self.voice.clone().and_then(|v| {
                let wav = self
                    .work_dir()
                    .join(format!("say-{:03}.wav", self.segments.len()));
                say(&v, spoken, &wav)
            })
        });
        let seconds = match &audio {
            Some(a) => LEAD + a.len() as f64 / SAMPLE_RATE as f64 + TAIL,
            // Reading time: about 2.6 words a second, at least 2 seconds.
            None => (text.split_whitespace().count() as f64 / 2.6).max(2.0) + TAIL,
        };
        self.segments.push(Segment {
            text: text.to_owned(),
            start: self.frame,
            end: self.frame,
            audio,
            min_frames: frames(seconds),
        });
        self.open = Some(self.segments.len() - 1);
    }

    /// Let the current segment's narration finish, then fade its caption out.
    pub fn end_segment(&mut self) {
        let Some(i) = self.open else {
            return;
        };
        while self.frame < self.segments[i].start + self.segments[i].min_frames {
            self.frame(false);
        }
        self.open = None;
        let text = self.segments[i].text.clone();
        for f in 0..FADE {
            let a = 1.0 - (f + 1) as f32 / FADE as f32;
            self.overlay().caption = Some((text.clone(), a));
            self.frame(false);
        }
        self.overlay().caption = None;
        self.segments[i].end = self.frame;
    }

    /// A full-screen title card that fades in, stays for `seconds` and fades out (hiding the app).
    pub fn title_card(&mut self, title: &str, subtitle: &str, seconds: f64) {
        let fade = 12u64;
        let total = frames(seconds).max(2 * fade);
        for f in 0..total {
            let a = if f < fade {
                (f + 1) as f32 / fade as f32
            } else if f >= total - fade {
                (total - f) as f32 / fade as f32
            } else {
                1.0
            };
            self.overlay().card = Some((title.to_owned(), subtitle.to_owned(), a));
            self.idle = 0;
            self.frame(false);
        }
        self.overlay().card = None;
    }

    /// Show a title card and keep it up (for the end of a video); [`Self::finish`] ends with it.
    pub fn end_card(&mut self, title: &str, subtitle: &str) {
        for f in 0..12u64 {
            self.overlay().card =
                Some((title.to_owned(), subtitle.to_owned(), (f + 1) as f32 / 12.0));
            self.idle = 0;
            self.frame(false);
        }
    }

    /// Show a key badge (e.g. "Ctrl+Z") at the top of the drawing area for a moment.
    pub fn badge(&mut self, text: &str, seconds: f64) {
        let until = self.frame + frames(seconds);
        self.overlay().badge = Some((text.to_owned(), until));
    }

    // ------------------------------------------------------------------------------------------- finding

    /// Rectangle of the widget with this accessible label: an exact match (the last one, preferring buttons and
    /// fields over plain labels, so an open menu wins over the toolbar), else the first label starting with it.
    pub fn rect(&self, label: &str) -> Rect {
        self.try_rect(label)
            .unwrap_or_else(|| panic!("no widget labelled {label:?}"))
    }

    pub fn try_rect(&self, label: &str) -> Option<Rect> {
        use egui::accesskit::Role;
        let exact: Vec<(Role, Rect)> = self
            .h
            .query_all_by_label(label)
            .map(|n| (n.accesskit_node().role(), n.rect()))
            .collect();
        if let Some((_, r)) = exact
            .iter()
            .rev()
            .find(|(role, _)| !matches!(role, Role::Label | Role::TextRun | Role::GenericContainer))
            .or(exact.last())
        {
            return Some(*r);
        }
        self.h
            .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(label)))
            .map(|n| n.rect())
            .next()
            .or_else(|| self.h.query_all_by_value(label).map(|n| n.rect()).next())
    }

    pub fn has(&self, text: &str) -> bool {
        self.h
            .query_all_by(|n| {
                n.label().is_some_and(|l| l.contains(text))
                    || n.value().is_some_and(|v| v.contains(text))
            })
            .next()
            .is_some()
    }

    /// The widget of `role` on the same row as the text `label` and to its right (a field or combo box).
    pub fn right_of(&self, label: &str, role: egui::accesskit::Role) -> Rect {
        let l = self.rect(label);
        self.h
            .get_all_by_role(role)
            .map(|n| n.rect())
            .filter(|r| (r.center().y - l.center().y).abs() < 10.0 && r.min.x >= l.max.x - 1.0)
            .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
            .unwrap_or_else(|| panic!("no {role:?} right of {label:?}"))
    }

    /// The last widget of `role` (e.g. the combo box of the most recently opened window).
    pub fn last_of_role(&self, role: egui::accesskit::Role) -> Rect {
        self.h
            .get_all_by_role(role)
            .map(|n| n.rect())
            .next_back()
            .unwrap_or_else(|| panic!("no {role:?}"))
    }

    /// Centre of the combo box currently showing `selected`.
    pub fn combo(&self, selected: &str) -> Pos2 {
        let shows = |n: &egui_kittest::kittest::AccessKitNode<'_>| {
            n.label().as_deref() == Some(selected) || n.value().as_deref() == Some(selected)
        };
        self.h
            .query_all_by(|n| n.role() == egui::accesskit::Role::ComboBox && shows(n))
            .next()
            .or_else(|| self.h.query_all_by(|n| shows(n)).next_back())
            .unwrap_or_else(|| panic!("no combo box showing {selected:?}"))
            .rect()
            .center()
    }

    /// Text fields on the row of the text `label`, left to right.
    pub fn fields_on_row(&self, label: &str) -> Vec<Rect> {
        let l = self.rect(label);
        let mut v: Vec<Rect> = self
            .h
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .map(|n| n.rect())
            .filter(|r| (r.center().y - l.center().y).abs() < 10.0 && r.min.x >= l.max.x - 1.0)
            .collect();
        v.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        v
    }

    /// Text field on the row of the text `label` inside the window titled `title`.
    pub fn field_in(&self, title: &str, label: &str) -> Pos2 {
        let w = self.window(title);
        let l = self
            .h
            .query_all_by(|n| {
                n.label().as_deref() == Some(label) || n.value().as_deref() == Some(label)
            })
            .map(|n| n.rect())
            .find(|r| w.contains(r.center()))
            .unwrap_or_else(|| panic!("no text {label:?} in {title:?}"));
        self.h
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .map(|n| n.rect())
            .filter(|r| {
                w.contains(r.center())
                    && (r.center().y - l.center().y).abs() < 10.0
                    && r.min.x >= l.max.x - 1.0
            })
            .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
            .unwrap_or_else(|| panic!("no text field right of {label:?} in {title:?}"))
            .center()
    }

    /// Text field on the row of `label`.
    #[allow(dead_code)] // scripting API (docs/videos.md)
    pub fn field(&self, label: &str) -> Pos2 {
        self.right_of(label, egui::accesskit::Role::TextInput)
            .center()
    }

    fn resolve(&self, at: At<'_>) -> Pos2 {
        match at {
            At::Label(l) => self.rect(l).center(),
            At::Pos(p) => p,
        }
    }

    /// Screen position (points) of a knot or tangent handle of `curve` in the 2D view of `tab`.
    pub fn knot(&self, tab: Tab, curve: CurveId, knot: usize, which: Handle) -> Pos2 {
        let app = self.app();
        let s = curve.spline(&app.doc.board).expect("curve");
        let p = s.knots()[knot].get(which);
        match tab {
            Tab::Profile => {
                let ys = app.set.profile_exaggeration.max(1.0) as f64;
                app.ed
                    .profile_view
                    .xf
                    .to_screen(surfcraft_core::DVec2::new(p.x, p.y * ys))
            }
            Tab::Section => app.ed.section_view.xf.to_screen(p),
            _ => app.ed.outline_view.xf.to_screen(p),
        }
    }

    /// Rectangle of the window titled `title`.
    pub fn window(&self, title: &str) -> Rect {
        self.h
            .get_all_by_role(egui::accesskit::Role::Window)
            .find(|n| n.accesskit_node().label().as_deref() == Some(title))
            .unwrap_or_else(|| panic!("no window {title:?}"))
            .rect()
    }

    /// Close the window titled `title` with its close button.
    pub fn close_window(&mut self, title: &str) {
        let w = self.window(title);
        let close = self
            .h
            .query_all_by_label("Close window")
            .map(|n| n.rect())
            .rfind(|r| w.contains(r.center()) && r.center().y < w.min.y + 30.0)
            .unwrap_or_else(|| panic!("no close button on {title:?}"));
        self.click(close.center());
    }

    /// Step the app without emitting frames until `done` holds (e.g. a toolpath being generated). The video cuts
    /// straight to the result, so its length does not depend on the machine's speed.
    pub fn wait_for(&mut self, done: impl Fn(&SurfCraftApp) -> bool) {
        let start = Instant::now();
        while !done(self.app()) {
            assert!(
                start.elapsed() < Duration::from_secs(300),
                "timed out waiting"
            );
            std::thread::sleep(Duration::from_millis(5));
            self.h.run_steps(1);
        }
        self.sync_worker();
        self.idle = 0;
        self.last = None;
    }

    // ------------------------------------------------------------------------------------------- pointer

    /// Glide the cursor to a widget or point, eased, over a duration that grows with the distance.
    pub fn move_to<'a>(&mut self, at: impl Into<At<'a>>) {
        let to = self.resolve(at.into());
        // From a menu bar button, go straight down into its menu first: crossing the neighbouring menu buttons
        // would open their menus instead (as it would for a real mouse).
        if self.cursor.y < MENU_BAR && to.y > MENU_BAR && (to.x - self.cursor.x).abs() > 4.0 {
            let down = egui::pos2(self.cursor.x, to.y);
            self.move_to_in(down, glide(self.cursor, down) * 0.7);
        }
        self.move_to_in(to, glide(self.cursor, to));
    }

    /// Glide the cursor to `to` over `seconds`, one pointer event per frame.
    pub fn move_to_in(&mut self, to: Pos2, seconds: f64) {
        let from = self.cursor;
        let n = frames(seconds).max(1);
        for i in 1..=n {
            let t = ease(i as f32 / n as f32);
            self.cursor = from + (to - from) * t;
            self.h.event(egui::Event::PointerMoved(self.cursor));
            self.frame(true);
        }
    }

    fn button(&mut self, button: egui::PointerButton, pressed: bool) {
        self.overlay().pressed = pressed;
        if pressed {
            let at = (self.cursor, self.frame);
            self.overlay().ripples.push(at);
        }
        self.h.event(egui::Event::PointerButton {
            pos: self.cursor,
            button,
            pressed,
            modifiers: self.held,
        });
        self.frame(true);
    }

    /// Move to a widget (or point) and click it.
    pub fn click<'a>(&mut self, at: impl Into<At<'a>>) {
        self.move_to(at);
        self.hold(0.1);
        self.button(egui::PointerButton::Primary, true);
        self.button(egui::PointerButton::Primary, false);
        self.hold(0.25);
    }

    /// Double-click where the cursor is.
    #[allow(dead_code)] // scripting API (docs/videos.md)
    pub fn double_click(&mut self) {
        for _ in 0..2 {
            self.button(egui::PointerButton::Primary, true);
            self.button(egui::PointerButton::Primary, false);
        }
        self.hold(0.2);
    }

    /// Drag with the left button from `from` to `to` over `seconds`, an event and a frame per step so the change
    /// is visible as it happens.
    pub fn drag(&mut self, from: Pos2, to: Pos2, seconds: f64) {
        self.drag_with(
            egui::PointerButton::Primary,
            Modifiers::NONE,
            from,
            to,
            seconds,
        );
    }

    /// Drag while holding `modifiers` (e.g. Alt for a fine drag) with any button.
    pub fn drag_with(
        &mut self,
        button: egui::PointerButton,
        modifiers: Modifiers,
        from: Pos2,
        to: Pos2,
        seconds: f64,
    ) {
        self.move_to(from);
        self.hold(0.15);
        if modifiers != Modifiers::NONE {
            self.held = modifiers;
            self.h.event(egui::Event::ModifiersChanged(modifiers));
        }
        self.button(button, true);
        self.hold_input(0.1);
        let n = frames(seconds).max(2);
        for i in 1..=n {
            let t = ease(i as f32 / n as f32);
            self.cursor = from + (to - from) * t;
            self.h.event(egui::Event::PointerMoved(self.cursor));
            self.frame(true);
        }
        self.hold_input(0.1);
        self.button(button, false);
        if modifiers != Modifiers::NONE {
            self.held = Modifiers::NONE;
            self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        }
        self.hold(0.2);
    }

    /// Hold still but keep stepping the app every frame (e.g. while a button is down).
    fn hold_input(&mut self, seconds: f64) {
        for _ in 0..frames(seconds) {
            self.frame(true);
        }
    }

    /// Scroll the mouse wheel at the cursor (positive = zoom in / scroll up), spread over `seconds`.
    pub fn wheel(&mut self, delta_y: f32, seconds: f64) {
        let n = frames(seconds).max(1);
        for _ in 0..n {
            self.h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta_y / n as f32),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            });
            self.frame(true);
        }
    }

    // ------------------------------------------------------------------------------------------- keyboard

    /// Type text, a few characters per frame like a quick typist.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.h.event(egui::Event::Text(c.to_string()));
            self.frame(true);
            self.frame(true);
        }
        self.hold(0.15);
    }

    /// Press a key (with modifiers); `badge` shows the shortcut on screen.
    pub fn key(&mut self, modifiers: Modifiers, key: Key, badge: Option<&str>) {
        if let Some(b) = badge {
            self.badge(b, 1.4);
        }
        self.h.key_press_modifiers(modifiers, key);
        self.frame(true);
        self.hold(0.2);
    }

    /// Click into a text field, replace its content with `text` and press Enter.
    pub fn fill<'a>(&mut self, at: impl Into<At<'a>>, text: &str) {
        self.click(at);
        self.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        self.frame(true);
        self.type_text(text);
        self.h.key_press(Key::Enter);
        self.frame(true);
        self.hold(0.3);
    }

    // ------------------------------------------------------------------------------------------- stills

    /// Save the current window as a PNG `width` pixels wide, without cursor and caption (for written tutorials).
    /// Not part of the video.
    pub fn still(&mut self, path: &Path, width: u32) {
        let (cursor, caption, badge) = {
            let mut o = self.overlay();
            o.frame = self.frame;
            (o.cursor.take(), o.caption.take(), o.badge.take())
        };
        // Move egui's pointer away too, so nothing shows as hovered.
        self.h.event(egui::Event::PointerGone);
        self.h.step();
        self.sync_worker();
        self.overlay().cursor = None;
        self.h.step();
        let img = self.h.render().expect("render");
        let (w, h) = img.dimensions();
        let height = (h as f64 * width as f64 / w as f64).round() as u32;
        let small =
            image::imageops::resize(&img, width, height, image::imageops::FilterType::Lanczos3);
        let rgb = image::DynamicImage::ImageRgba8(small).to_rgb8();
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        // Full colour first, then a 256-colour palette version (UI screenshots stay sharp at a third of the size).
        let full = self.work_dir().join("still.png");
        rgb.save(&full).unwrap();
        let ok = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&full)
            .args([
                "-vf",
                "split[a][b];[a]palettegen=max_colors=256:stats_mode=single[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
            ])
            .arg(path)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            std::fs::copy(&full, path).unwrap();
        }
        {
            let mut o = self.overlay();
            o.cursor = cursor;
            o.caption = caption;
            o.badge = badge;
        }
        // Back to where the pointer was.
        self.h.event(egui::Event::PointerMoved(self.cursor));
        self.last = None;
        self.idle = 0;
    }

    // ------------------------------------------------------------------------------------------- output

    /// End the recording: encode, mix the narration, mux, write the subtitles. Returns the files written.
    pub fn finish(&mut self) -> Output {
        self.end_segment();
        self.hold(0.6);
        let frames_total = self.frame;
        let (mut child, stdin) = self.encoder.take().expect("no frames recorded");
        drop(stdin);
        assert!(child.wait().unwrap().success(), "ffmpeg (video) failed");
        let seconds = frames_total as f64 / FPS as f64;

        let srt = self.dir.join(format!("{}.srt", self.name));
        std::fs::write(&srt, srt_text(&self.segments)).unwrap();

        let video = self.dir.join("work/video.mp4");
        let mp4 = self.dir.join(format!("{}.mp4", self.name));
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&video);
        if self.segments.iter().any(|s| s.audio.is_some()) {
            let wav = self.dir.join("narration.wav");
            write_wav(&wav, &mix(&self.segments, seconds));
            cmd.arg("-i")
                .arg(&wav)
                .args(["-c:a", "aac", "-b:a", "128k", "-ar", "44100"]);
        }
        cmd.args(["-c:v", "copy", "-movflags", "+faststart"])
            .arg(&mp4);
        assert!(cmd.status().unwrap().success(), "ffmpeg (mux) failed");

        let wall = self.started.elapsed().as_secs_f64();
        eprintln!(
            "[{}] {} frames ({:.1} s of video, {}x{}), {} rendered in {:.1} s ({:.0} ms each), {:.0} s wall clock",
            self.name,
            frames_total,
            seconds,
            self.size.0,
            self.size.1,
            self.rendered,
            self.render_time.as_secs_f64(),
            1000.0 * self.render_time.as_secs_f64() / self.rendered.max(1) as f64,
            wall
        );
        eprintln!("[{}] wrote {}", self.name, mp4.display());
        Output { mp4, srt, seconds }
    }

    /// An animated GIF of `seconds` of the finished video from `start`, 960 pixels wide, for the README.
    pub fn preview_gif(&self, out: &Output, start: f64, seconds: f64, fps: u32) -> PathBuf {
        let gif = self.dir.join("preview.gif");
        let filter = format!(
            "fps={fps},scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];\
             [b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
        );
        let ok = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args([
                "-ss",
                &format!("{start}"),
                "-t",
                &format!("{seconds}"),
                "-i",
            ])
            .arg(&out.mp4)
            .args(["-vf", &filter, "-loop", "0"])
            .arg(&gif)
            .status()
            .unwrap()
            .success();
        assert!(ok, "ffmpeg (gif) failed");
        gif
    }
}

impl Drop for Recorder {
    /// A script that fails half-way still leaves a playable `work/video.mp4` up to the failure, to see why.
    fn drop(&mut self) {
        if let Some((mut child, stdin)) = self.encoder.take() {
            drop(stdin);
            let _ = child.wait();
            eprintln!(
                "[{}] unfinished recording: {}",
                self.name,
                self.dir.join("work/video.mp4").display()
            );
        }
    }
}

// ----------------------------------------------------------------------------------------------- helpers

/// Height of the menu bar, in points.
const MENU_BAR: f32 = 22.0;

/// Time for a natural cursor glide between two points.
fn glide(from: Pos2, to: Pos2) -> f64 {
    (0.3 + (to - from).length() as f64 / 1500.0).min(0.9)
}

fn frames(seconds: f64) -> u64 {
    (seconds * FPS as f64).round().max(0.0) as u64
}

fn ease(t: f32) -> f32 {
    // Smootherstep: gentle start and stop like a hand on a mouse.
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn video_root() -> PathBuf {
    std::env::var_os("SURFCRAFT_VIDEO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/videos"))
}

/// The repository root (for files the scripts write into the docs).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn find_ffmpeg() -> Option<PathBuf> {
    let candidates = std::env::var_os("FFMPEG")
        .map(PathBuf::from)
        .into_iter()
        .chain([
            PathBuf::from("ffmpeg"),
            PathBuf::from("/opt/homebrew/bin/ffmpeg"),
            PathBuf::from("/usr/local/bin/ffmpeg"),
        ]);
    for c in candidates {
        let ok = Command::new(&c)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Some(c);
        }
    }
    None
}

/// The narration voice: `$SURFCRAFT_VOICE`, else the first installed of a few natural English voices (an
/// "Enhanced" or "Premium" one when installed). `None` without `say` (not macOS) or with `SURFCRAFT_VOICE=none`.
fn pick_voice() -> Option<String> {
    let wanted = std::env::var("SURFCRAFT_VOICE").ok();
    if wanted.as_deref() == Some("none") {
        return None;
    }
    let list = Command::new("say").args(["-v", "?"]).output().ok()?;
    if !list.status.success() {
        return None;
    }
    let list = String::from_utf8_lossy(&list.stdout).into_owned();
    // Lines look like "Samantha            en_US    # Hello! My name is Samantha."
    let voices: Vec<(String, String)> = list
        .lines()
        .filter_map(|l| {
            let (left, _) = l.split_once('#')?;
            let left = left.trim_end();
            let lang = left.rsplit(' ').next()?.to_owned();
            let name = left[..left.len() - lang.len()].trim().to_owned();
            Some((name, lang))
        })
        .collect();
    if let Some(w) = wanted {
        return voices.iter().any(|(n, _)| *n == w).then_some(w);
    }
    let english = |(_, l): &&(String, String)| l.starts_with("en_");
    voices
        .iter()
        .filter(english)
        .find(|(n, _)| n.contains("Premium") || n.contains("Enhanced"))
        .or_else(|| {
            ["Samantha", "Daniel", "Karen", "Moira"]
                .iter()
                .find_map(|w| voices.iter().find(|(n, _)| n == w))
        })
        .map(|(n, _)| n.clone())
}

/// Speak `text` into a WAV file and return its samples.
fn say(voice: &str, text: &str, wav: &Path) -> Option<Vec<i16>> {
    let ok = Command::new("say")
        .args(["-v", voice, "-r", "172"])
        .arg("--file-format=WAVE")
        .arg(format!("--data-format=LEI16@{SAMPLE_RATE}"))
        .arg("-o")
        .arg(wav)
        .arg(text)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("say failed for {text:?}");
        return None;
    }
    read_wav(&std::fs::read(wav).ok()?)
}

/// Samples of a mono 16-bit PCM WAV at [`SAMPLE_RATE`] (what `say` was asked for).
fn read_wav(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let mut fmt_ok = false;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = bytes.get(i + 8..(i + 8 + len).min(bytes.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            let format = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            fmt_ok = format == 1 && channels == 1 && rate == SAMPLE_RATE && bits == 16;
        } else if id == b"data" {
            return fmt_ok.then(|| {
                body.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| i16::from_le_bytes(*c))
                    .collect()
            });
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// One narration track: each segment's speech placed at its start (plus a short lead-in).
fn mix(segments: &[Segment], seconds: f64) -> Vec<i16> {
    let mut out = vec![0i16; (seconds * SAMPLE_RATE as f64).ceil() as usize];
    for s in segments {
        let Some(a) = &s.audio else { continue };
        let at = ((s.start as f64 / FPS as f64 + LEAD) * SAMPLE_RATE as f64) as usize;
        for (i, &v) in a.iter().enumerate() {
            if let Some(o) = out.get_mut(at + i) {
                *o = o.saturating_add(v);
            }
        }
    }
    out
}

fn write_wav(path: &Path, samples: &[i16]) {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    b.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

fn srt_time(frame: u64) -> String {
    let ms = frame * 1000 / FPS as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn srt_text(segments: &[Segment]) -> String {
    let mut s = String::new();
    for (i, seg) in segments.iter().enumerate() {
        s.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            srt_time(seg.start),
            srt_time(seg.end),
            seg.text
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trip_and_mix() {
        let p = crate::gui_tests::temp_dir("demo").join("t.wav");
        write_wav(&p, &[0, 1000, -1000, i16::MAX]);
        assert_eq!(
            read_wav(&std::fs::read(&p).unwrap()).unwrap(),
            vec![0, 1000, -1000, i16::MAX]
        );
        let seg = |start, audio| Segment {
            text: String::new(),
            start,
            end: start,
            audio: Some(audio),
            min_frames: 0,
        };
        let m = mix(&[seg(0, vec![100; 10]), seg(0, vec![i16::MAX; 10])], 1.0);
        assert_eq!(m.len(), SAMPLE_RATE as usize);
        let at = (LEAD * SAMPLE_RATE as f64) as usize;
        assert_eq!(m[at], i16::MAX, "mixing saturates");
        assert_eq!(m[at - 1], 0);
    }

    #[test]
    fn srt_timestamps() {
        assert_eq!(srt_time(0), "00:00:00,000");
        assert_eq!(srt_time(FPS as u64 * 61 + 15), "00:01:01,500");
        let s = srt_text(&[Segment {
            text: "Hello".into(),
            start: 30,
            end: 90,
            audio: None,
            min_frames: 0,
        }]);
        assert_eq!(s, "1\n00:00:01,000 --> 00:00:03,000\nHello\n\n");
    }

    #[test]
    fn ease_is_monotonic_from_zero_to_one() {
        assert_eq!(ease(0.0), 0.0);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        let mut prev = 0.0;
        for i in 1..=20 {
            let v = ease(i as f32 / 20.0);
            assert!(v >= prev);
            prev = v;
        }
    }
}
