#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod automation;
mod cam_ui;
mod compare_ui;
mod compute;
mod dialogs;
mod export_ui;
mod fins_ui;
#[cfg(test)]
mod gui_tests;
#[cfg(test)]
mod gui_tests_cam;
#[cfg(test)]
mod gui_tests_full;
mod i18n;
mod panels;
mod preview3d;
mod screenshot;
mod settings;
mod tools;
mod tour;
mod view2d;
mod viewer3d;
mod views;
mod volume_ui;

use std::path::PathBuf;

pub(crate) use surfcraft_session::{document, edit};

/// Command line:
/// `surfcraft [board.brd] [--tab overview|outline|rocker|section|3d] [--screenshot out.png] [--automation]`.
#[derive(Default)]
pub struct Args {
    pub open: Option<PathBuf>,
    pub tab: Option<String>,
    pub screenshot: Option<PathBuf>,
    /// Allow the local MCP server to drive this window (for this run).
    pub automation: bool,
    /// Open the file as an untitled board (Save asks for a location). Used for the MCP hand-off file.
    pub untitled: bool,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--screenshot") => args.screenshot = it.next().map(PathBuf::from),
            Some("--tab") => args.tab = it.next().and_then(|s| s.into_string().ok()),
            Some("--automation") => args.automation = true,
            Some("--untitled") => args.untitled = true,
            // macOS adds -psn_* when launched from Finder on old systems.
            Some(s) if s.starts_with("-psn_") => {}
            _ => args.open = Some(PathBuf::from(a)),
        }
    }
    args
}

const APP_ID: &str = "org.surfcraft.SurfCraft";
/// App id of the releases named BoardCAD, whose settings are carried over once.
const OLD_APP_ID: &str = "org.boardcad.BoardCAD";

/// First run after the rename: copy the old settings file (never overwrites, leaves the old one in place).
fn migrate_old_settings() {
    let (Some(new), Some(old)) = (eframe::storage_dir(APP_ID), eframe::storage_dir(OLD_APP_ID))
    else {
        return;
    };
    let (from, to) = (old.join("app.ron"), new.join("app.ron"));
    if from.is_file() && !to.exists() && std::fs::create_dir_all(&new).is_ok() {
        let _ = std::fs::copy(&from, &to);
    }
}

fn main() -> eframe::Result {
    let args = parse_args();
    migrate_old_settings();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("SurfCraft")
            .with_app_id(APP_ID)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "SurfCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::SurfCraftApp::new(cc, args)))),
    )
}
