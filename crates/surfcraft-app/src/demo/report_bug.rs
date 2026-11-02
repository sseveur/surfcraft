//! Tutorial 7: "Reporting a bug": Help > Detailed logging, reproducing the problem, Help > Report a bug (title,
//! description, log period and preview, what to include, copy, GitHub issue / e-mail), Help > Open log folder.
//!
//! Nothing leaves the computer and nothing is written to the real Downloads or data folders: the recording logs to
//! a temporary data folder, the zip goes to a temporary folder, and in tests the browser, mail client and file
//! manager are not started (`crate::platform` records what would have been opened).
//!
//! Record: `cargo test -p surfcraft-app --release demo_report_bug -- --ignored --nocapture`
//! Output: `target/videos/reporting-a-bug/reporting-a-bug.mp4` (+ `.srt`) and the screenshots of
//! `docs/tutorials/07-reporting-a-bug.md` in `docs/tutorials/img/`.

use std::path::PathBuf;

use eframe::egui::{self, Key, Modifiers, accesskit::Role};
use egui_kittest::kittest::Queryable as _;

use super::overview::GITHUB;
use super::recorder::{Recorder, repo_root};

/// Screenshot width for the written tutorial.
const STILL_WIDTH: u32 = 1280;

fn still(rec: &mut Recorder, name: &str) {
    let p = repo_root().join("docs/tutorials/img").join(name);
    rec.still(&p, STILL_WIDTH);
}

/// A neutral temporary folder (its path shows in the window): `data` for the log, `Downloads` for the zip.
fn demo_dir(name: &str) -> PathBuf {
    let base = if cfg!(unix) {
        PathBuf::from("/tmp/surfcraft-demo")
    } else {
        std::env::temp_dir().join("surfcraft-demo")
    };
    let d = base.join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The multi-line field under "What happened / steps to reproduce" (the first multi-line field of the window).
fn description_field(rec: &Recorder) -> egui::Pos2 {
    let w = rec.window("Report a bug");
    rec.h
        .get_all_by_role(Role::MultilineTextInput)
        .map(|n| n.rect())
        .filter(|r| w.contains(r.center()))
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .expect("description field")
        .center()
}

/// The period picker on the row of "Include logs from the last:".
fn since_picker(rec: &Recorder) -> egui::Pos2 {
    rec.right_of("Include logs from the last:", Role::ComboBox)
        .center()
}

fn report(rec: &Recorder) -> &crate::bugreport_ui::BugReport {
    rec.app().bug_report.as_ref().expect("bug report window")
}

#[test]
#[ignore = "records a video (minutes); run with --ignored"]
fn demo_report_bug() {
    let mut rec = Recorder::new("reporting-a-bug");
    // The app logs to a temporary data folder, as it logs to its own folder when it runs for real.
    let data = demo_dir("data");
    let downloads = demo_dir("Downloads");
    crate::logging::init(&data).expect("log folder");
    log::info!("{} starting", crate::logging::app_line());
    rec.app_mut().data_dir = Some(data.clone());
    crate::platform::take_opened();

    rec.narrate(
        "Found a bug? A good report gets it fixed fast. \
         Here is how to send one, with the log that shows what happened.",
    );
    rec.title_card("Reporting a bug", "SurfCraft tutorial 7", 5.0);

    // ---------------------------------------------------------------------------------------- log
    rec.narrate(
        "SurfCraft keeps a log on your computer, one file per day, for two weeks. \
         Everything is in the Help menu.",
    );
    rec.click("Help");
    rec.move_to("Report a bug…");
    rec.hold(0.5);
    rec.move_to("Open log folder");
    rec.hold(0.6);

    rec.narrate(
        "First, tick Detailed logging. Now the log also records every edit, click and view change.",
    );
    rec.click("Detailed logging");
    rec.hold(0.6);
    assert!(rec.app().set.detailed_logging);
    assert!(crate::logging::detailed());
    // The menu closes on the click: open it again to show the tick.
    if rec.try_rect("Open log folder").is_none() {
        rec.click("Help");
    }
    rec.move_to("Detailed logging");
    rec.hold(0.6);
    still(&mut rec, "bug-help-menu.png");
    rec.key(Modifiers::NONE, Key::Escape, None);
    rec.hold(0.3);

    rec.narrate("Then do again what went wrong. Here: Board, Flip nose and tail, then Undo.");
    let board = rec.app().doc.board.clone();
    rec.click("Board");
    rec.click("Flip nose and tail");
    rec.hold(1.0);
    assert_ne!(rec.app().doc.board, board, "flipped");
    rec.click("Undo");
    rec.hold(0.8);
    assert_eq!(rec.app().doc.board, board, "undone");

    // ---------------------------------------------------------------------------------------- report
    rec.narrate(
        "Now Help, Report a bug. Give it a short title, then say what you did, \
         what you expected, and what happened instead.",
    );
    rec.click("Help");
    rec.click("Report a bug…");
    rec.hold(0.5);
    assert!(report(&rec).open);
    // The zip goes to a temporary folder instead of Downloads.
    rec.app_mut().bug_report.as_mut().unwrap().out_dir = Some(downloads.clone());
    rec.click(rec.field_in("Report a bug", "Title"));
    rec.type_text("Flip leaves the rocker upside down");
    rec.click(description_field(&rec));
    for (i, line) in [
        "1. Board > Flip nose and tail",
        "2. Undo",
        "Expected: the board as before.",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            rec.key(Modifiers::NONE, Key::Enter, None);
        }
        rec.type_text(line);
    }
    rec.hold(0.4);
    assert!(report(&rec).title.starts_with("Flip"));
    assert!(report(&rec).description.contains("Expected"));
    still(&mut rec, "bug-report.png");

    rec.narrate(
        "Include logs from the last: choose how far back to go. \
         Twenty-four hours is the default; one hour is enough for a bug you just made happen.",
    );
    rec.click(since_picker(&rec));
    rec.click("1 hour");
    rec.hold(0.6);
    assert_eq!(report(&rec).since, crate::bugreport::Since::Hour);
    rec.move_to(rec.rect("Include logs from the last:").right_center() + egui::vec2(150.0, 0.0));
    rec.hold(0.4);

    rec.narrate(
        "Open Preview to read exactly what goes into the report. \
         Clicks are logged by their label, never what you type, and your home folder shows as a tilde.",
    );
    rec.click("Preview");
    rec.hold(0.4);
    {
        let logs = &report(&rec).logs.as_ref().expect("log text").1;
        assert!(logs.contains("detailed logging on"), "{logs}");
        assert!(
            logs.contains("Flip nose and tail"),
            "the click is in the log"
        );
        assert!(!logs.contains("upside down"), "typed text is never logged");
    }
    let preview = rec.window("Report a bug");
    rec.move_to(egui::pos2(preview.center().x, preview.center().y + 40.0));
    rec.wheel(-120.0, 1.2);
    rec.hold(1.0);
    still(&mut rec, "bug-preview.png");
    rec.click("Preview");
    rec.hold(0.3);

    rec.narrate(
        "Tick what else helps: system information, a copy of the board you have open, \
         and a screenshot of the window.",
    );
    rec.move_to("System information");
    rec.hold(0.5);
    rec.click("The current board");
    rec.hold(0.4);
    rec.move_to("A screenshot of the window");
    rec.hold(0.6);
    assert!(report(&rec).include_system && report(&rec).include_board);

    rec.narrate(
        "Copy report text puts the title, description and system information on the clipboard, \
         to paste anywhere.",
    );
    rec.click("Copy report text");
    rec.hold(0.8);
    assert!(
        rec.app()
            .status
            .as_ref()
            .is_some_and(|s| s.0.contains("copied"))
    );

    rec.narrate(
        "GitHub issue and E-mail first save everything as a zip in your Downloads folder, \
         then open a new issue in your browser, or a new message in your mail program.",
    );
    rec.click("GitHub issue");
    rec.hold(0.4);
    let zip = report(&rec).saved.clone().expect("zip saved");
    assert_eq!(zip.parent(), Some(downloads.as_path()));
    {
        let bytes = std::fs::read(&zip).unwrap();
        let has = |s: &str| bytes.windows(s.len()).any(|w| w == s.as_bytes());
        assert!(has("report.txt") && has("system-info.txt") && has("board.brd"));
    }
    let opened = crate::platform::take_opened();
    assert_eq!(opened[0], format!("reveal:{}", zip.display()));
    assert!(
        opened[1].starts_with("url:https://github.com/sseveur/surfcraft/issues/new?title=Flip"),
        "{opened:?}"
    );
    let saved = rec
        .h
        .query_all_by(|n| n.value().is_some_and(|v| v.starts_with("Report saved to")))
        .next()
        .expect("saved message")
        .rect();
    rec.move_to(saved.center());
    rec.hold(1.0);
    still(&mut rec, "bug-saved.png");

    rec.narrate(
        "SurfCraft sends nothing by itself. Check the message, attach the zip, and send it.",
    );
    rec.hold(0.6);

    rec.narrate(
        "Help, Open log folder shows the log files themselves. \
         When you are done, turn Detailed logging off again.",
    );
    rec.close_window("Report a bug");
    rec.click("Help");
    rec.click("Open log folder");
    rec.hold(0.4);
    assert_eq!(
        crate::platform::take_opened(),
        [format!("open:{}", crate::logging::log_dir(&data).display())]
    );
    rec.click("Help");
    rec.click("Detailed logging");
    rec.hold(0.4);
    assert!(!rec.app().set.detailed_logging);
    rec.key(Modifiers::NONE, Key::Escape, None);
    rec.hold(0.4);

    rec.narrate_as(
        &format!("Thank you for reporting! SurfCraft: {GITHUB}"),
        "That's it. Thank you for helping make SurfCraft better!",
    );
    rec.end_card("Reporting a bug", GITHUB);
    let out = rec.finish();
    eprintln!(
        "{} ({:.1} s), {}",
        out.mp4.display(),
        out.seconds,
        out.srt.display()
    );
}
