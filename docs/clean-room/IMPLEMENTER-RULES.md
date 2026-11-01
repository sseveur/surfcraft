# Rules for implementers

You are writing original code from specifications. To keep SurfCraft's code independent of the reference program
(BoardCAD 3.0.1, GPL-3.0), follow these rules for the whole task.

## Allowed sources

- Everything inside `/Users/Steven/Desktop/projects/surfcraft-next/` (this repository, including its git history,
  which contains no reference code).
- `docs/spec/*.md`, `docs/clean-room/REQUIREMENTS.md`, the existing SurfCraft code that calls your libraries
  (`crates/surfcraft-app`, `crates/surfcraft-session`, `crates/surfcraft-mcp`) and the tests.
- Rust documentation and the source of crates.io dependencies (`~/.cargo/registry`), general knowledge of mathematics,
  CAD, CNC and file formats.

## Forbidden sources

- Any BoardCAD source code (Java, Jython, `.properties`), on disk or online, and any translation of it.
- These locations, which hold such material: `/Users/Steven/Desktop/projects/surfcraft`,
  `/Users/Steven/Desktop/projects/board-cad`, the git history of `/Users/Steven/Desktop/projects` (do not run git
  commands outside `surfcraft-next`), anything under `/private/tmp/claude-501`, and anything under `~/.claude`.
- Web searches for BoardCAD's code or its repository.

If you come across reference code by accident, stop reading it, do not use it, and mention it in your report.

## Working

- Temporary files: `/Users/Steven/Desktop/projects/surfcraft-next/target/scratch/` (ignored by git).
- Keep the workspace compiling: other implementers build it at the same time. If something you depend on is not
  written yet, call it anyway against the API the callers use; if you must add a placeholder, make it
  `unimplemented!("<what> not written yet")` and say so in your report.
- Use your own `CARGO_TARGET_DIR` (given in your task) so builds don't wait on each other.
- Commit your work in small steps in `surfcraft-next` with short messages (subject plus at most three bullet lines,
  no attribution lines). Don't commit files outside your area unless your task says so.
- Don't install software. Don't use `rm` on paths built from shell variables.
- Finish with `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` (check the exit code, don't
  pipe it) and the tests of your area; report what passes, what fails and why.
