# SurfCraft

Surfboard design for macOS, Windows and Linux: shape the outline, rocker and cross-sections, watch the volume
update live, print full-size templates, and generate CNC toolpaths.

SurfCraft opens and saves [BoardCAD](http://www.boardcad.org) `.brd` files, so boards designed in BoardCAD (last
released in 2015) carry straight over, and its volumes and measurements agree with BoardCAD's: the test suite checks
them against values measured with BoardCAD 3.0.1.

## Download

Get the latest version from the [Releases page](https://github.com/sseveur/surfcraft/releases/latest).

| System | File | Install |
|---|---|---|
| macOS (Apple Silicon and Intel) | `SurfCraft-<version>-macos.dmg` | Open the dmg and drag SurfCraft to Applications. |
| Windows 10/11 | `SurfCraft-<version>-windows-setup.exe` | Run the installer. A portable `.zip` is also available. |
| Ubuntu, Debian, Mint, Pop!_OS | `surfcraft_<version>_amd64.deb` | `sudo apt install ./surfcraft_*_amd64.deb` |
| Fedora, RHEL, openSUSE | `surfcraft-<version>.x86_64.rpm` | `sudo dnf install ./surfcraft-*.rpm` (or `zypper install`) |
| Any other Linux | `SurfCraft-x86_64.AppImage` | `chmod +x SurfCraft-*.AppImage`, then run it. |

The builds are not signed with paid certificates yet, so the first launch needs one extra click:

- **macOS**: if macOS says the app cannot be opened, open **System Settings → Privacy & Security** and click
  **Open Anyway** next to SurfCraft (or run `xattr -dr com.apple.quarantine /Applications/SurfCraft.app`).
- **Windows**: if SmartScreen appears, click **More info → Run anyway**.

## Features

- **Design**: outline, rocker (deck + bottom) and cross-sections as Bezier curves. Drag points and tangents
  (Alt = fine), box select, arrow-key nudge, add/delete points, exact numeric entry in any unit
  (`6'2"`, `18 1/2"`, `47cm`), guide points, curvature combs, rail apex and tuck-under markers. Edit → Smooth
  removes kinks (selected points, or every small kink of a curve while keeping deliberate corners).
- **Live specs**: length, widths, thickness, rocker at standard stations, planform area, volume, centre of mass.
- **Compare**: ghost board overlay, "original" snapshot, background photo tracing (hold T / N and click the photo's
  tail / nose).
- **3D**: GPU view with zebra stripes for checking fairness.
- **New board from a template** at a target length, width, thickness and volume (litres).
- **Weight calculator**: foam, stringer, glass, laps, resin, hot coat.
- **Print**: outline (half or full), spin, profile and cross-section templates at true 1:1 scale, tiled and centred
  over A4/A3/Letter/Tabloid with registration marks and a 100 mm check bar; one-page spec sheet. PDF, SVG or DXF.
- **Wooden boards**: sandwich (core profile, rail, skins), hollow wood (stringer, ribs, rail strip, skins, nose and
  tail pieces) and chambered plank templates.
- **CNC (Machine menu)**: surface-splits and width-splits toolpaths for deck and bottom, bullnose/flat/STL cutters,
  4th-axis rotating board, supports or end clamps, hot-wire profile and outline; LinuxCNC G-code with a preview and
  machining-time estimate. Passes follow the modelled surface and stop at the rail apex on 3-axis machines. Check a
  new program in your machine's simulator before cutting.
- **3D model export**: STL, OBJ (mm), STEP.
- **Files**: `.brd` (including encrypted legacy files), Shape3D `.s3d`, `.srf`, BoardCAD STEP; `.otl/.pfl/.crs` curve
  pieces. Saving keeps a `.bak` of the previous version.
- **Languages**: English, French, Spanish, Portuguese, Dutch, Norwegian (View → Language).
- **Guided tour** on first launch, any time from Help → Tour of SurfCraft.
- **Report a bug** (Help menu): saves a zip with the recent log, system information and optionally the board, then
  opens a prefilled GitHub issue or e-mail for you to send. A daily log is kept for 14 days (Help → Open log folder;
  Help → Detailed logging records every edit and click).
- **AI assistant (MCP)**: Claude or another MCP client can read and edit the board while you watch (see below).

## Using SurfCraft with an AI assistant (MCP)

`surfcraft-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server installed next to the app.
With the SurfCraft window open and **Help → Allow AI assistant control** ticked (or after the assistant calls
`launch_app`), every tool runs in the window as a normal, undoable edit. Without a window the tools work headlessly
on `.brd` files.

Where the server is:

| System | Path |
|---|---|
| macOS | `/Applications/SurfCraft.app/Contents/MacOS/surfcraft-mcp` |
| Windows | `C:\Program Files\SurfCraft\surfcraft-mcp.exe` |
| Linux (.deb / .rpm) | `/usr/bin/surfcraft-mcp` |

Claude Code:

```sh
claude mcp add -s user surfcraft -- /Applications/SurfCraft.app/Contents/MacOS/surfcraft-mcp
```

Claude Desktop (`claude_desktop_config.json`):

```json
{ "mcpServers": { "surfcraft": { "command": "/Applications/SurfCraft.app/Contents/MacOS/surfcraft-mcp" } } }
```

Tools include `board_summary`, `sample_board`, `get_curve`, `set_knot`, `move_knot`, `add_knot`, `section_info`,
`add_section`, `scale_board` (with target volume), `new_board`, `open_board`, `save_board`, `set_fins`,
`estimate_weight`, `export`, `generate_toolpath`, `undo`/`redo`, `launch_app`, `screenshot`, `set_view`.

Security: the app only listens on 127.0.0.1, and only while control is enabled; every request needs a random token
stored in a file readable only by your user account. The server never connects to the internet.

## Reporting a bug

Help → **Report a bug…** writes `SurfCraft-bug-report-<date>-<time>.zip` to your Downloads folder and shows it,
then opens a new [GitHub issue](https://github.com/sseveur/surfcraft/issues) or an e-mail with your description and
system information filled in: attach the zip and send. Choose how much of the log to include (1 hour to all) and
preview it first; the board and a screenshot are optional. Your home folder is written as `~`, and SurfCraft sends
nothing by itself.

For a problem that is hard to describe, tick Help → **Detailed logging**, reproduce the problem, then report it.
The log files are in the `logs` folder of SurfCraft's data folder (Help → Open log folder).

## Build from source

Needs [Rust](https://rustup.rs) 1.95 or newer.

```sh
cargo run -p surfcraft-app --release          # the app
cargo test --workspace                        # all tests, including headless GUI tests
packaging/macos/build.sh --no-install         # macOS .app and .dmg
```

Linux needs the usual winit development packages (see `.github/workflows/ci.yml`). Windows and Linux installers are
built by `.github/workflows/release.yml` when a `v*` tag is pushed.

Command line: `surfcraft [board.brd] [--tab overview|outline|rocker|section|3d] [--screenshot out.png] [--automation]`.

## Layout

| Path | What |
|---|---|
| `crates/surfcraft-core` | Geometry: Bezier curves, cross-sections, surface models, metrics, mesh, units, weight |
| `crates/surfcraft-io` | File formats and built-in templates |
| `crates/surfcraft-export` | STL/OBJ, PDF/SVG/DXF writers, printable templates |
| `crates/surfcraft-session` | Document with undo, edit operations, JSON commands, automation bridge |
| `crates/surfcraft-cam` | Toolpaths, cutters, holding systems, G-code |
| `crates/surfcraft-app` | Desktop app (egui/eframe + wgpu) |
| `crates/surfcraft-mcp` | MCP server |
| `docs/spec` | Functional specifications the geometry, file-format, CNC and template code is written from |
| `docs/clean-room` | How that code was written, and SurfCraft's own requirements |
| `tests/fixtures` | Test boards (SurfCraft designs) with reference values, and a test cutter |
| `tools` | Generators for the test boards and cutter; QA drivers for the MCP server |
| `packaging` | macOS, Windows and Linux packaging |

## Licence

SurfCraft is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Contributions are accepted under the same terms.

SurfCraft's geometry, file-format, CNC and template code was written in a clean-room process from functional
specifications (see `docs/clean-room/PROCESS.md`); it contains no code from BoardCAD, which is GPL-licensed.

## Acknowledgements

Thanks to the authors of BoardCAD — Jonas Hörnstein, Ola Helenius, Håvard Nygård Jakobsen and Sven Wesley — whose
program and `.brd` format SurfCraft is compatible with.
