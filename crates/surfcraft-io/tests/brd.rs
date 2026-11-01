use surfcraft_io::brd;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

#[test]
fn round_trip_is_exact() {
    let out_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("brd-roundtrip");
    std::fs::create_dir_all(&out_dir).unwrap();
    for name in ["shortboard", "funboard", "longboard", "scan_template"] {
        let board = brd::read(&fixture(&format!("{name}.brd"))).unwrap();
        let text = brd::write(&board);
        // Kept so the files can be cross-checked with other programs.
        std::fs::write(out_dir.join(format!("{name}.brd")), &text).unwrap();
        let again = brd::read(text.as_bytes()).unwrap();
        assert_eq!(board, again, "{name}");
        assert_eq!(text, brd::write(&again), "{name}: writer not stable");
    }
}

#[test]
fn keeps_every_field() {
    let mut board = brd::read(&fixture("funboard.brd")).unwrap();
    board.info.author = "A. Shaper".into();
    board.info.description = "two\nlines".into();
    board.info.blank_file = "blank.brd".into();
    board.machine.nose_rocker_one_foot = 7.5;
    let again = brd::read(brd::write(&board).as_bytes()).unwrap();
    assert_eq!(again.info.author, "A. Shaper");
    assert_eq!(again.info.description, "two\nlines");
    assert_eq!(again.info.blank_file, "blank.brd");
    assert_eq!(again.machine.nose_rocker_one_foot, 7.5);
}

#[test]
fn reads_truncated_file() {
    // A file cut short (here: missing its final ")") still reads.
    let text = String::from_utf8(fixture("shortboard.brd")).unwrap();
    let truncated = text.trim_end().strip_suffix(')').unwrap();
    let a = brd::read(text.as_bytes()).unwrap();
    let b = brd::read(truncated.as_bytes()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn rejects_garbage() {
    assert!(brd::read(b"hello").is_err());
    assert!(brd::read(b"p32 : (\n(cp [1,2,3] true false)\n)\n").is_err());
}

#[cfg(feature = "legacy-encryption")]
#[test]
fn decrypts_file_encrypted_by_java() {
    let plain = brd::read(&fixture("longboard.brd")).unwrap();
    let decrypted = brd::read(&fixture("longboard_encrypted.brd")).unwrap();
    assert_eq!(plain, decrypted);
}

/// Geometry only: editing locks are applied when a partial file is merged into a board.
fn shape(s: &surfcraft_core::Spline) -> Vec<([surfcraft_core::DVec2; 3], bool)> {
    s.knots().iter().map(|k| (k.points, k.continuous)).collect()
}

#[test]
fn partial_curve_files_round_trip() {
    let board = brd::read(&fixture("shortboard.brd")).unwrap();
    let otl = brd::read_partial(&brd::write_outline(&board)).unwrap();
    assert_eq!(shape(&otl.outline.unwrap().0), shape(&board.outline));
    let pfl = brd::read_partial(&brd::write_profile(&board)).unwrap();
    assert_eq!(shape(&pfl.bottom.unwrap().0), shape(&board.bottom));
    assert_eq!(shape(&pfl.deck.unwrap().0), shape(&board.deck));
    let crs = brd::read_partial(&brd::write_section(&board.sections[2]))
        .unwrap()
        .section
        .unwrap();
    assert_eq!(crs.position, board.sections[2].position);
    assert_eq!(shape(&crs.spline), shape(&board.sections[2].spline));
}
