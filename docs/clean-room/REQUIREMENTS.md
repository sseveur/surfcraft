# SurfCraft requirements beyond the specifications

`docs/spec/` describes how the reference program behaves (and how its known defects must be corrected). This file
lists what SurfCraft additionally requires from the libraries. The test suite (`crates/*/tests`, the GUI tests in
`crates/surfcraft-app/src/gui_tests*.rs`, and `tests/fixtures/*.golden.json`) is the executable form of both; where
this file and a test disagree, the test wins.

## General

- Rust 2024 edition, stable toolchain (`rust-version` in the workspace `Cargo.toml`), no `unsafe` unless unavoidable
  and documented. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` must pass.
- Licence header not needed per file; the workspace licence is `MIT OR Apache-2.0`.
- The public API of each library is whatever the application, session, MCP server and tests use: read their code
  (`crates/surfcraft-app`, `crates/surfcraft-session`, `crates/surfcraft-mcp`, `crates/*/tests`) to find the types,
  functions and fields they need, and provide them with the same names and signatures. You may add API; do not
  rename what callers use (if a caller needs a change to compile, keep it minimal and mention it).
- Internal length unit is centimetres; angles in the board model follow `docs/spec/geometry.md`.
- Types that the application sends to its background worker (the board and everything it contains) must be
  `Clone + Send + Sync`. Expensive derived values (e.g. curve length) may be cached but caches must be invalidated by
  every mutation.

## Geometry (`surfcraft-core`)

- Reference values: every number in `tests/fixtures/*.golden.json` must be reproduced within the tolerances used by
  `crates/surfcraft-io/tests/golden.rs`. Where the reference program is defective (keys ending in `_java`), the tests
  expect the corrected value.
- Spline extremes (max/min) include every segment.
- Boards always have the zero-size end sections at the tail (x = 0) and nose (x = length) after loading or creating;
  provide an operation that adds them when missing.
- Scaling by a non-positive or non-finite factor is refused (the board is left unchanged).
- Metric searches are bounded: e.g. finding the thickest point uses at most 10 000 samples, so huge or degenerate
  boards cannot stall the interface.
- If the default surface model cannot find a cross-section pair for some x (degenerate boards), it falls back to a
  sensible shape scaled to the local thickness instead of panicking. No geometry function may panic on any board that
  the file readers accept, including boards with zero-length curves or a single section.
- `units.rs` and `mesh.rs` are existing SurfCraft code (not derived from the reference program); keep them and adapt
  them to the new API if needed.
- Weight calculator: per `docs/spec/geometry.md`, with the original's branch defect corrected.
- Both surface models are required. SurfCraft's default (for new and loaded boards, unless the user picks the other
  one) is the S-blend model.
- Flip nose and tail (`Board::flip`, Board menu) mirrors the geometry along the length (outline, rocker, deck,
  sections and their positions, guide points, fins); flipping twice gives back exactly the same board. (The
  reference program only mirrored the views.)
- Scaling to target dimensions (`Board::scale(length, width, thickness)`) is used by "New board" (a built-in board
  scaled to the requested length, width and thickness) and by the Scale dialog; the volume target is solved by the
  callers on top of it.
- Every defect listed in the specs' "known defects" sections is corrected unless a test or this file says otherwise.
  Behaviours the specs label "compat" are kept, because the reference values depend on them.

## Editing (`surfcraft-session/src/edit.rs`)

Rewrite this module from the knot-editing behaviour in `docs/spec/geometry.md`; keep the API used by
`crates/surfcraft-session/src/commands.rs` and the application (selection of several knots on one curve, dragging
points or handles with the movement constraints, fine dragging, adding/removing knots, adding/moving/removing
cross-sections — refusing a section position already used by another section — numeric entry of a handle, toggling
continuity, and the tidy-up after an edit of a curve).

## File formats (`surfcraft-io`)

- `read_any(extension, bytes)` dispatches on the extension: `brd` (plain and legacy encrypted), `s3d`, `srf`,
  `stp`/`step`.
- `.brd` reading is tolerant: unknown fields are ignored, missing fields take defaults, a truncated file yields the
  part that was read; non-UTF-8 files are read as Latin-1. Partial curve files `.otl`, `.pfl`, `.crs` can be imported
  into an existing board and exported from it, and exported files can be imported again.
- `.brd` writing writes every field the reader understands, so a read → write → read round trip is lossless (see
  `crates/surfcraft-io/tests/roundtrip_full.rs`); the reference program must be able to read the result.
- STEP reading rebuilds a board that is continuous where the original intended it (smooth end knots get mirrored
  outer handles), with end sections ensured.
- `xml.rs` (a small XML reader) is existing SurfCraft code; keep it.
- Built-in boards: `templates::TEMPLATES` lists the starting boards offered by "New board"; use the SurfCraft designs in
  `tests/fixtures/` (shortboard first: it is the board the application opens with), each with a name and a one-line
  description, exactly: `shortboard.brd` "Shortboard" / "6'0\" performance shortboard"; `funboard.brd` "Mid-length"
  / "7'6\" mid-length"; `longboard.brd` "Longboard" / "9'2\" longboard" (these strings are translation keys).
- Errors carry a message suitable for showing to the user (which file, which line or field when known).

## Templates (`surfcraft-export/src/templates.rs`, `woodwork.rs`)

- Templates produce a `Drawing` (`crates/surfcraft-export/src/drawing.rs`, existing SurfCraft code) in millimetres
  at 1:1. Multi-piece templates (one cross-section per piece, ribs, planks) mark each piece with `Drawing::part` so
  the tiler prints each piece centred on its own pages.
- Label text is formatted with the caller-supplied length formatter (the user's units).
- `woodwork::TEMPLATES` lists `(name, label)` pairs and `woodwork::by_name` builds any of them with default options.
- Correct the reference program's template defects listed in `docs/spec/templates.md`.

## CNC (`surfcraft-cam`)

Implement `docs/spec/cam.md` with its defects corrected, plus:

- Settings are validated before generating (e.g. a cutter corner radius larger than the cutter radius is refused with
  a clear message).
- On 3-axis machines passes stop at the rail apex; the cutter never cuts into the board beyond a small tolerance and
  never enters a support or clamp (see `crates/surfcraft-cam/tests/programs.rs` and `qa_cam.rs` for the limits).
- On a rotating (4th-axis) board the tool retracts before large A-axis turns.
- Flat bottoms are fully covered by the bottom passes.
- A straight move between two consecutive pass points must not cut into the board either: where a pass crosses a
  hard edge (a cross-section corner) the move is subdivided or lifted (`qa_bug_bottom_pass_crossing_hard_edge_gouges`).
- On a rotating board, while the tool waits at a holding position and the board turns, the tool height must clear the
  board's swept rail as well as the clamps (`qa_bug_rotating_hold_height_below_wide_tail`).
- Both tests above are currently marked `#[ignore]` because the earlier implementation failed them; the new
  implementation must pass them, and the ignore attributes are to be removed.
- Programs report statistics (point count, cutting length, estimated machining time) and can be previewed.
- Generation of the finest allowed resolution stays usable (see the performance GUI test).
