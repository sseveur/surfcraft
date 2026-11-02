//! Scripted demo and tutorial videos, recorded from the real app UI (no screen recording).
//!
//! [`recorder::Recorder`] drives the app headlessly with egui_kittest + wgpu like the GUI tests, paints a
//! simulated cursor and captions on top, speaks the narration with macOS `say` and assembles an MP4 with
//! ffmpeg. Each video is an `#[ignore]`d test, so `cargo test` stays fast; record one with e.g.
//! `cargo test -p surfcraft-app --release demo_overview -- --ignored --nocapture`.
//! See `docs/videos.md`.

#[allow(dead_code)] // used by the video scripts
pub(crate) mod recorder;

mod overview;
