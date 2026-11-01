# Clean-room process

SurfCraft's geometry, file-format, CNC and template code was written in a clean-room process so that SurfCraft can be
licensed under MIT OR Apache-2.0, independently of BoardCAD 3.0.1 (GPL-3.0), whose `.brd` files it reads and writes.

## Roles

- **Specification writers** read the original BoardCAD 3.0.1 Java sources and wrote the functional specifications in
  `docs/spec/`: behaviour, mathematics, numerical parameters and file-format facts, in their own words, with no source
  code, no line-by-line pseudo-code, no copied comments and no identifiers from the original.
- **Reviewer** checked each specification for copied expression before it was handed over.
- **Implementers** wrote the Rust code from the specifications, `docs/clean-room/REQUIREMENTS.md` and the test suite
  only. They never had access to the original sources, to the earlier GPL translation of them, or to its history;
  that material was made unreadable on the machine for the duration of the implementation.

## What the implementers had

- `docs/spec/*.md` (this repository).
- `docs/clean-room/REQUIREMENTS.md`: SurfCraft's own behaviour beyond the original.
- The parts of SurfCraft that were never derived from BoardCAD: the desktop application (`crates/surfcraft-app`),
  the session and command layer, the MCP server, the 2D drawing and PDF/SVG/DXF/STL/OBJ writers, units and meshing.
- The test suite, including reference values in `tests/fixtures/*.golden.json`. Those numbers were produced by running
  the original program, offline, on boards designed for SurfCraft (`tools/fixtures/make_boards.py`). Measured outputs
  of a program are facts; no BoardCAD data (built-in boards, translations) is included.

## What was replaced

| Area | Before | Now |
|---|---|---|
| Geometry core, surface models, metrics | translated from the Java | written from `docs/spec/geometry.md` |
| `.brd`, legacy encryption, STEP, `.s3d`, `.srf`, curve files | translated from the Java | written from `docs/spec/file-formats.md` |
| CNC toolpaths, cutters, holding systems, G-code | translated from the Java | written from `docs/spec/cam.md` |
| Print and wooden-board templates | translated from the Java | written from `docs/spec/templates.md` |
| Built-in boards | BoardCAD's templates | SurfCraft designs |
| Translations | partly based on BoardCAD's language files | translated anew from the English interface |
| Original Java sources in the repository | included for reference | removed |
