# Demo and tutorial videos

SurfCraft's videos are not screen recordings. Each one is a script that drives the real app headlessly, the same way
the GUI tests do (egui_kittest + wgpu), renders every frame of the real window, paints a simulated cursor, click
ripples, key badges and captions on top, speaks the narration with the macOS `say` command and assembles an MP4 with
ffmpeg. Re-recording after a UI change is one command, and the result is the same every time: fixed window size
(1440 x 810 points rendered at 1920 x 1080 pixels), 30 fps, dark theme, English, no tour, no autosave or crash
recovery, files written only to temporary folders.

| Video | Length | Command |
|---|---|---|
| SurfCraft in 3 minutes (overview) | about 2.5 min | `cargo test -p surfcraft-app --release demo_overview -- --ignored --nocapture` |
| Tutorial 1: Your first board | about 2 min | `cargo test -p surfcraft-app --release demo_first_board -- --ignored --nocapture` |

Each run writes to `target/videos/<name>/` (override with `SURFCRAFT_VIDEO_DIR`):

- `<name>.mp4`: H.264 (yuv420p, CRF 20, faststart) with AAC narration
- `<name>.srt`: the captions as subtitles, for players and video sites
- `narration.wav`: the narration track alone
- `preview.gif` (overview only): 10 seconds at 960 pixels for the README, copied by hand to
  `docs/videos/overview-preview.gif`
- `work/`: intermediate files (video without sound, one WAV per sentence)

The tutorial script also rewrites the screenshots of [tutorials/01-first-board.md](tutorials/01-first-board.md) in
`docs/tutorials/img/` (1280 pixels wide, reduced to 256 colours with ffmpeg, about 100 KB each).

A recording takes well under a minute on an Apple Silicon Mac (a frame renders in about 10 ms). The tests are
`#[ignore]`d, so `cargo test --workspace` does not record anything.

**MP4 files are not committed.** They are published as assets of the GitHub releases
([Releases page](https://github.com/sseveur/surfcraft/releases)), named `SurfCraft-overview.mp4` and
`SurfCraft-tutorial-01-first-board.mp4`, with their `.srt` files.

## Requirements

- **ffmpeg** with libx264 on the `PATH`, in `/opt/homebrew/bin` or `/usr/local/bin`, or named by `$FFMPEG`.
- **Narration**: macOS `say`. The recorder picks an installed "Premium" or "Enhanced" English voice if there is one
  (System Settings > Accessibility > Spoken Content > System voice > Manage voices), else Samantha. Choose another
  with `SURFCRAFT_VOICE=Daniel`, or record silently with `SURFCRAFT_VOICE=none`. Without `say` (Linux, Windows)
  the videos are recorded without sound and every caption stays up long enough to read.
- A GPU usable by wgpu (any Mac; on Linux, Mesa or lavapipe as for the GUI tests).

## How it works

The code is in `crates/surfcraft-app/src/demo/` and compiled only for tests:

- `recorder.rs`: the `Recorder`. It builds the harness, keeps the cursor position, turns every action into frames
  and writes them to ffmpeg's standard input. While the app has nothing new to draw, frames are repeated instead of
  rendered. Before each frame it waits (in real time, without emitting frames) for the background worker, so the
  specs, volume and 3D mesh in the video are always up to date. Long jobs such as toolpath generation are skipped
  over with `wait_for`, so a video's length does not depend on the computer.
- `overview.rs`, `first_board.rs`: one script per video.

Narration drives the timing: `narrate` starts a segment, the actions that follow happen while it is spoken, and the
next `narrate` first waits until the sentence is finished. Captions show the narration text.

## Writing a new script

Add `crates/surfcraft-app/src/demo/<name>.rs`, declare it in `demo/mod.rs`, and write an ignored test:

```rust
use super::recorder::Recorder;

#[test]
#[ignore = "records a video (minutes); run with --ignored"]
fn demo_my_video() {
    let mut rec = Recorder::new("my-video");
    rec.narrate("What this video shows, in one sentence.");
    rec.title_card("My video", "SurfCraft tutorial", 4.0);

    rec.narrate("Board, Fins: choose a thruster.");
    rec.click("Board");          // a widget by its accessible label (menu items, buttons, tabs)
    rec.click("Fins…");
    rec.click("Setup");          // combo boxes too
    rec.click("Thruster");
    rec.click("Apply preset");
    assert!(rec.app().doc.board.fin_setup.is_some()); // check the app really did it

    rec.end_card("My video", "github.com/sseveur/surfcraft");
    rec.finish();
}
```

Actions (all in points; the cursor glides there first):

| Call | Does |
|---|---|
| `narrate(text)`, `narrate_as(caption, spoken)`, `caption(text)` | Start a segment: speech + caption, or a silent caption |
| `click(label or pos)`, `double_click()`, `move_to(..)`, `move_to_in(pos, s)` | Pointer |
| `drag(from, to, s)`, `drag_with(button, modifiers, from, to, s)` | Real drag events, one per frame (Alt for fine drags, right button to pan) |
| `wheel(dy, s)` | Scroll / zoom at the cursor |
| `fill(field, text)`, `type_text(text)`, `key(mods, key, badge)` | Keyboard; `badge` shows the shortcut on screen |
| `hold(s)`, `wait_for(cond)` | Let the picture rest; skip over a long job |
| `title_card`, `end_card`, `badge` | Overlays |
| `still(path, width)` | Save a screenshot without cursor and caption (for written tutorials) |
| `rect(label)`, `field_in(window, label)`, `fields_on_row(label)`, `combo(selected)`, `window(title)`, `close_window(title)`, `knot(tab, curve, i, handle)` | Find where things are |

Tips: give each sentence one idea, keep actions inside the sentence that describes them, assert what each step should
have done (the script fails instead of recording a wrong video), and look at a few frames afterwards
(`ffmpeg -ss 30 -i target/videos/my-video/my-video.mp4 -frames:v 1 frame.png`). When a script fails half-way, the
frames up to the failure are in `work/video.mp4`.
