//! Shared helpers for the legacy-format tests.
#![allow(dead_code)]

use surfcraft_core::Board;

pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");
pub const BOARDS: [&str; 4] = ["shortboard", "funboard", "longboard", "scan_template"];

pub fn fixture_board(name: &str) -> Board {
    let bytes = std::fs::read(format!("{FIXTURES}/{name}.brd")).unwrap();
    surfcraft_io::brd::read(&bytes).unwrap()
}

/// Generated fixtures are kept for cross-checking with the original Java readers.
pub fn keep(dir: &str, file: &str, bytes: &[u8]) {
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir);
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join(file), bytes).unwrap();
}

#[track_caller]
fn close(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got}, want {want} (diff {:.3e}, tol {tol:.1e})",
        (got - want).abs()
    );
}

/// Same shape: length, width/rocker/thickness at stations, and volume within `volume_rel`.
#[track_caller]
pub fn assert_same_shape(name: &str, want: &Board, got: &Board, tol: f64, volume_rel: f64) {
    let l = want.length();
    close(&format!("{name} length"), got.length(), l, tol);
    for i in 1..20 {
        let x = l * i as f64 / 20.0;
        close(
            &format!("{name} width@{x:.1}"),
            got.width_at(x),
            want.width_at(x),
            tol,
        );
        close(
            &format!("{name} rocker@{x:.1}"),
            got.rocker_at(x),
            want.rocker_at(x),
            tol,
        );
        close(
            &format!("{name} thickness@{x:.1}"),
            got.thickness_at(x),
            want.thickness_at(x),
            tol,
        );
    }
    let (v0, v1) = (want.volume(), got.volume());
    assert!(
        ((v1 - v0) / v0).abs() <= volume_rel,
        "{name} volume: got {v1:.1}, want {v0:.1} ({:+.2}%)",
        (v1 - v0) / v0 * 100.0
    );
    assert_eq!(got.sections.first().unwrap().position, 0.0);
    assert_eq!(got.sections.first().unwrap().spline.len(), 1);
    assert_eq!(got.sections.last().unwrap().spline.len(), 1);
}

/// Every prefix (sampled) of a valid file must load or fail cleanly.
pub fn assert_truncations_dont_panic(
    bytes: &[u8],
    read: fn(&[u8]) -> Result<Board, surfcraft_io::Error>,
) {
    let step = (bytes.len() / 400).max(1);
    for n in (0..bytes.len()).step_by(step) {
        let _ = read(&bytes[..n]);
    }
}
