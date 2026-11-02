//! QA: every JSON command with valid and invalid input, unit strings, undo invariants, randomised edits and
//! structural checks of every export.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use surfcraft_core::{Board, LengthUnit};
use surfcraft_io::templates::TEMPLATES;
use surfcraft_session::commands::{Context, execute, tool_specs};
use surfcraft_session::document::Document;

fn session(t: usize) -> (Document, Context) {
    (
        Document::new(TEMPLATES[t].board(), None),
        Context::default(),
    )
}

#[track_caller]
fn ok(doc: &mut Document, ctx: &mut Context, name: &str, a: Value) -> Value {
    execute(doc, ctx, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}"))
}

#[track_caller]
fn err(doc: &mut Document, ctx: &mut Context, name: &str, a: Value) -> String {
    match execute(doc, ctx, name, &a) {
        Ok(v) => panic!("{name} {a} should fail, got {v}"),
        Err(e) => e,
    }
}

fn tmp(sub: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("qa-commands")
        .join(sub);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Tiny deterministic generator (no new dependencies).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn f(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn finite_board(b: &Board) -> bool {
    let splines = [&b.outline, &b.deck, &b.bottom]
        .into_iter()
        .chain(b.sections.iter().map(|s| &s.spline));
    for s in splines {
        for k in s.knots() {
            for p in k.points {
                if !p.x.is_finite() || !p.y.is_finite() {
                    return false;
                }
            }
        }
    }
    b.sections.iter().all(|s| s.position.is_finite())
}

// ------------------------------------------------------------------------------------------------ units

#[test]
fn unit_strings_parse() {
    let cases: &[(&str, f64)] = &[
        ("6'2\"", 187.96),
        ("6' 2\"", 187.96),
        ("6'2", 187.96),
        ("6ft 2in", 187.96),
        ("6 ft", 182.88),
        ("18 1/2\"", 18.5 * 2.54),
        ("18 1/2 in", 18.5 * 2.54),
        ("2 3/8\"", 2.375 * 2.54),
        ("47cm", 47.0),
        ("47 cm", 47.0),
        ("1.2 m", 120.0),
        ("1,2m", 120.0),
        ("25,5mm", 2.55),
        ("25.5 mm", 2.55),
        ("-3cm", -3.0),
        ("  47  ", 47.0), // bare number = cm in the command interface
        ("6'2 1/2\"", 74.5 * 2.54),
    ];
    let (mut doc, mut ctx) = session(0);
    for &(s, cm) in cases {
        let got = LengthUnit::Centimetres
            .parse(s)
            .unwrap_or_else(|| panic!("{s:?} did not parse"));
        assert!((got - cm).abs() < 1e-9, "{s:?}: {got} != {cm}");
        // Same string through a command (stations must lie on the board, so negatives are refused there).
        let r = execute(
            &mut doc,
            &mut ctx,
            "sample_board",
            &json!({"stations": [s]}),
        );
        if cm < 0.0 || cm > doc.board.length() {
            assert!(r.is_err(), "{s}: off-board station accepted");
            continue;
        }
        let r = r.unwrap_or_else(|e| panic!("{s}: {e}"));
        assert!(
            (r[0]["from_tail"]["cm"].as_f64().unwrap() - cm).abs() < 1e-3,
            "{s}"
        );
    }
    for bad in ["", "abc", "1/0", "6'x", "12 parsecs", "1..2cm"] {
        assert!(
            LengthUnit::Centimetres.parse(bad).is_none(),
            "{bad:?} should not parse"
        );
    }
}

#[test]
fn unit_parser_rejects_non_finite() {
    for bad in ["nan", "NaN", "inf", "infinity", "-inf", "nan cm", "inf\""] {
        assert!(
            LengthUnit::Centimetres.parse(bad).is_none(),
            "{bad:?} parsed to {:?}",
            LengthUnit::Centimetres.parse(bad)
        );
    }
}

#[test]
fn set_knot_rejects_nan_string() {
    let (mut doc, mut ctx) = session(0);
    let r = execute(
        &mut doc,
        &mut ctx,
        "set_knot",
        &json!({"curve": "outline", "index": 2, "x": "nan", "y": 20}),
    );
    assert!(r.is_err(), "accepted NaN: {r:?}");
    assert!(finite_board(&doc.board));
}

#[test]
fn unit_format_round_trips_every_unit() {
    for u in LengthUnit::ALL {
        for cm in [0.3, 2.54, 5.0, 18.5 * 2.54, 47.0, 91.44, 187.96, 274.32] {
            let text = u.format(cm, 16);
            let back = LengthUnit::Centimetres
                .parse(&text)
                .unwrap_or_else(|| panic!("{u:?} output {text:?} does not parse back"));
            let tol = match u {
                LengthUnit::FeetInchesFraction => 2.54 / 32.0,
                LengthUnit::FeetInchesDecimal => 2.54 * 0.005,
                LengthUnit::Millimetres => 0.005,
                LengthUnit::Centimetres => 0.005,
                LengthUnit::Metres => 0.005,
            } + 1e-9;
            assert!((back - cm).abs() <= tol, "{u:?} {cm} -> {text:?} -> {back}");
        }
    }
}

#[test]
fn no_negative_zero_in_formatted_lengths() {
    for u in LengthUnit::ALL {
        let t = u.format(-1e-4, 16);
        assert!(!t.starts_with('-'), "{u:?}: {t:?}");
    }
}

// --------------------------------------------------------------------------------------------- commands

#[test]
fn tool_specs_are_complete_and_unique() {
    let specs = tool_specs();
    assert_eq!(
        specs.len(),
        29,
        "command count (25 + generate_toolpath + suggest_volume + apply_bottom_contour + set_rails)"
    );
    let mut names: Vec<_> = specs.iter().map(|s| s.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), specs.len(), "duplicate names");
    for s in &specs {
        assert_eq!(s.input_schema["type"], "object", "{}", s.name);
        assert!(!s.description.is_empty());
        for req in s.input_schema["required"].as_array().unwrap() {
            let k = req.as_str().unwrap();
            assert!(
                s.input_schema["properties"].get(k).is_some(),
                "{}: required {k} not in properties",
                s.name
            );
        }
    }
}

#[test]
fn every_command_rejects_bad_input_without_panicking() {
    let bad_values = [
        json!(null),
        json!({}),
        json!({"curve": 5}),
        json!({"curve": "outline", "index": -1}),
        json!({"curve": "outline", "index": 1.5}),
        json!({"curve": "section", "section_index": 999, "index": 0, "x": 1, "y": 1}),
        json!({"curve": "deck", "index": 0, "handle": "middle", "x": 1, "y": 1}),
        json!({"curve": "deck", "index": 1, "x": [1], "y": {}}),
        json!({"position": "far"}),
        json!({"section_index": "1", "position": 50}),
        json!({"template": "fish"}),
        json!({"path": "/nonexistent/dir/x.brd"}),
        json!({"what": "nothing", "format": "pdf", "path": "/tmp/x"}),
        json!({"what": "outline", "format": "stl", "path": "/tmp/x"}),
        json!({"model": "nurbs"}),
        json!({"length": 5}),
        json!({"volume_litres": 9999}),
        json!({"stations": ["banana"]}),
        json!({"stations": [true]}),
    ];
    for spec in tool_specs() {
        for a in &bad_values {
            let (mut doc, mut ctx) = session(0);
            let before = doc.board.clone();
            let r = execute(&mut doc, &mut ctx, spec.name, a);
            // Whatever happens, no panic, and a failed command leaves the board alone.
            if r.is_err() {
                assert_eq!(
                    doc.board, before,
                    "{} {a} failed but changed the board",
                    spec.name
                );
                assert!(!doc.can_undo(), "{} {a} failed but pushed undo", spec.name);
            }
            assert!(finite_board(&doc.board), "{} {a}", spec.name);
        }
    }
}

#[test]
fn invalid_inputs_are_errors() {
    let (mut d, mut c) = session(0);
    let cases = [
        ("get_curve", json!({})),
        ("get_curve", json!({"curve": "rail"})),
        ("get_curve", json!({"curve": "section", "section_index": 0})),
        ("set_knot", json!({"curve": "outline", "index": 2, "x": 1})),
        (
            "set_knot",
            json!({"curve": "outline", "index": 2, "x": "6 parsecs", "y": 1}),
        ),
        (
            "set_knot",
            json!({"curve": "outline", "index": 2, "handle": "mid", "x": 1, "y": 1}),
        ),
        ("move_knot", json!({"curve": "outline"})),
        ("move_knot", json!({"curve": "outline", "index": 50})),
        ("set_smooth", json!({"curve": "outline", "index": 2})),
        ("smooth_curve", json!({"curve": "outline", "indices": [99]})),
        (
            "smooth_curve",
            json!({"curve": "outline", "indices": "all"}),
        ),
        ("smooth_curve", json!({"curve": "outline", "indices": [-1]})),
        ("smooth_curve", json!({"curve": "nose"})),
        ("add_knot", json!({"curve": "outline", "x": 10})),
        ("delete_knot", json!({"curve": "outline", "index": 0})),
        ("delete_knot", json!({"curve": "outline", "index": 99})),
        ("section_info", json!({"section_index": 999})),
        ("add_section", json!({})),
        ("add_section", json!({"position": 0.1})),
        ("add_section", json!({"position": 500})),
        ("move_section", json!({"section_index": 0, "position": 50})),
        ("move_section", json!({"section_index": 1})),
        ("remove_section", json!({"section_index": 0})),
        ("remove_section", json!({})),
        ("scale_board", json!({"length": 3})),
        ("scale_board", json!({"width": "0cm"})),
        ("scale_board", json!({"thickness": -5})),
        ("scale_board", json!({"volume_litres": 0.5})),
        ("new_board", json!({})),
        ("new_board", json!({"template": "gun"})),
        ("new_board", json!({"template": "shortboard", "length": 2})),
        ("open_board", json!({})),
        ("open_board", json!({"path": "/definitely/not/here.brd"})),
        ("save_board", json!({})), // untitled
        ("save_board", json!({"path": "/definitely/not/here/x.brd"})),
        ("export", json!({"what": "outline", "format": "pdf"})),
        (
            "export",
            json!({"what": "outline", "format": "step", "path": "/tmp/x.stp"}),
        ),
        (
            "export",
            json!({"what": "model", "format": "dxf", "path": "/tmp/x.dxf"}),
        ),
        (
            "export",
            json!({"what": "rail", "format": "pdf", "path": "/tmp/x.pdf"}),
        ),
        (
            "export",
            json!({"what": "model", "format": "3mf", "path": "/tmp/x.3mf", "scale": 0}),
        ),
        (
            "export",
            json!({"what": "model", "format": "glb", "path": "/tmp/x.glb", "scale": "big"}),
        ),
        ("set_surface_model", json!({})),
        ("set_surface_model", json!({"model": "loft"})),
        ("apply_bottom_contour", json!({})),
        ("apply_bottom_contour", json!({"type": "spiral"})),
        ("apply_bottom_contour", json!({"type": "vee", "depth": -1})),
        (
            "apply_bottom_contour",
            json!({"type": "vee", "start": 100, "end": 50}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "vee", "run_out_fraction": 0.5, "run_out_from_rail": 2}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "vee", "run_out_fraction": 3}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "channels", "channel_count": 0}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "channels", "channel_count": -2}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "single_concave", "section_spacing": 0.1}),
        ),
        (
            "apply_bottom_contour",
            json!({"type": "vee", "keep": "deck"}),
        ),
        ("apply_bottom_contour", json!({"type": "vee", "keep": 3})),
        ("set_rails", json!({"preset": "90/10"})),
        ("set_rails", json!({"apex": 1.5})),
        ("set_rails", json!({"softness": "soft"})),
        ("set_rails", json!({"tuck": "a lot"})),
        ("set_rails", json!({"tail": 3})),
        ("set_rails", json!({"start": 100, "end": 20})),
        ("does_not_exist", json!({})),
    ];
    for (name, a) in cases {
        let before = d.board.clone();
        err(&mut d, &mut c, name, a);
        assert_eq!(d.board, before, "{name}");
    }
}

/// Every mutating command, applied then undone, restores the exact board; redo reapplies it exactly.
#[test]
fn undo_after_every_command_restores_exact_board() {
    let edits = |b: &Board| -> Vec<(&'static str, Value)> {
        // A smooth outline knot, so that making it a corner changes the board.
        let smooth = (1..b.outline.len() - 1)
            .find(|&i| b.outline.knots()[i].continuous)
            .expect("a smooth outline knot");
        // A corner outline knot (the tail corner), so that smoothing it changes the board.
        let corner = (1..b.outline.len() - 1)
            .find(|&i| !b.outline.knots()[i].continuous)
            .expect("a corner outline knot");
        // A free station: the middle of the widest gap between two sections.
        let free = b
            .sections
            .windows(2)
            .map(|w| (w[0].position, w[1].position))
            .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)))
            .map(|(lo, hi)| (lo + hi) / 2.0)
            .unwrap();
        vec![
            (
                "set_knot",
                json!({"curve": "outline", "index": 2, "x": 100, "y": 24}),
            ),
            (
                "set_knot",
                json!({"curve": "deck", "index": 1, "handle": "next", "x": "3'", "y": "2 1/2\""}),
            ),
            (
                "move_knot",
                json!({"curve": "bottom", "index": 1, "dx": 1, "dy": "5mm"}),
            ),
            (
                "move_knot",
                json!({"curve": "section", "index": 1, "dy": 0.3}),
            ),
            (
                "set_smooth",
                json!({"curve": "outline", "index": smooth, "smooth": false}),
            ),
            (
                "smooth_curve",
                json!({"curve": "outline", "indices": [corner]}),
            ),
            ("add_knot", json!({"curve": "outline", "x": 60, "y": 20})),
            ("add_knot", json!({"curve": "section", "x": 10, "y": 3})),
            ("delete_knot", json!({"curve": "outline", "index": 1})),
            ("add_section", json!({"position": free})),
            ("move_section", json!({"section_index": 2, "position": 70})),
            ("remove_section", json!({"section_index": 2})),
            (
                "scale_board",
                json!({"length": "6'6\"", "width": "20\"", "thickness": "2 5/8\""}),
            ),
            ("scale_board", json!({"volume_litres": 30})),
            ("set_info", json!({"name": "QA", "comments": "x"})),
            (
                "set_fins",
                json!({"side_rear_x": "11\"", "splay_degrees": 3}),
            ),
            ("set_surface_model", json!({"model": "control_point"})),
            ("flip_board", json!({})),
            (
                "apply_bottom_contour",
                json!({"type": "single_concave", "depth": "1/8\"", "end": "4'"}),
            ),
            ("set_rails", json!({"preset": "70/30"})),
        ]
    };
    for (t, template) in TEMPLATES.iter().enumerate() {
        for (name, a) in &edits(&template.board()) {
            let (mut doc, mut ctx) = session(t);
            let before = doc.board.clone();
            ok(&mut doc, &mut ctx, name, a.clone());
            let after = doc.board.clone();
            assert_ne!(after, before, "{name} {a} changed nothing on template {t}");
            assert!(doc.is_dirty());
            assert!(doc.can_undo());
            ok(&mut doc, &mut ctx, "undo", json!({}));
            assert_eq!(doc.board, before, "undo of {name} on template {t}");
            assert!(!doc.is_dirty(), "undo of {name}: still dirty");
            ok(&mut doc, &mut ctx, "redo", json!({}));
            assert_eq!(doc.board, after, "redo of {name}");
        }
    }
}

#[test]
fn undo_redo_on_empty_history_report_false() {
    let (mut d, mut c) = session(0);
    assert_eq!(ok(&mut d, &mut c, "undo", json!({}))["undone"], false);
    assert_eq!(ok(&mut d, &mut c, "redo", json!({}))["redone"], false);
}

#[test]
fn read_only_commands_do_not_touch_history() {
    let (mut d, mut c) = session(1);
    let before = d.board.clone();
    for (n, a) in [
        ("board_summary", json!({})),
        ("sample_board", json!({})),
        ("get_curve", json!({"curve": "deck"})),
        ("get_curve", json!({"curve": "section"})),
        ("section_info", json!({})),
        ("estimate_weight", json!({"foam_density": 0.03})),
        (
            "suggest_volume",
            json!({"weight_kg": 75, "skill": "advanced", "board_type": "fish"}),
        ),
    ] {
        ok(&mut d, &mut c, n, a);
    }
    assert_eq!(d.board, before);
    assert!(!d.can_undo() && !d.is_dirty());
}

#[test]
fn scale_board_hits_requested_dimensions() {
    for t in 0..TEMPLATES.len() {
        let (mut d, mut c) = session(t);
        let s = ok(
            &mut d,
            &mut c,
            "scale_board",
            json!({"length": "7'0\"", "width": "21 1/4\"", "thickness": "2 3/4\""}),
        );
        assert!((s["length"]["cm"].as_f64().unwrap() - 213.36).abs() < 0.01);
        assert!((s["max_width"]["cm"].as_f64().unwrap() - 21.25 * 2.54).abs() < 0.02);
        assert!((s["max_thickness"]["cm"].as_f64().unwrap() - 2.75 * 2.54).abs() < 0.02);
        let v = ok(&mut d, &mut c, "scale_board", json!({"volume_litres": 40}));
        assert!(
            (v["volume_litres"].as_f64().unwrap() - 40.0).abs() < 0.1,
            "t{t}: {}",
            v["volume_litres"]
        );
        // Length and width are kept by the volume solve.
        assert!((v["length"]["cm"].as_f64().unwrap() - 213.36).abs() < 0.01);
    }
}

#[test]
fn new_board_with_target_volume_for_every_template() {
    for (name, l) in [
        ("shortboard", "5'10\""),
        ("funboard", "7'6\""),
        ("longboard", "9'6\""),
    ] {
        let (mut d, mut c) = session(0);
        let s = ok(
            &mut d,
            &mut c,
            "new_board",
            json!({"template": name, "length": l, "volume_litres": 35}),
        );
        assert!(
            (s["volume_litres"].as_f64().unwrap() - 35.0).abs() < 0.1,
            "{name}"
        );
        assert_eq!(s["name"], "");
        assert!(!d.is_dirty() && !d.can_undo());
        assert!(d.board.real_sections().contains(&c.current_section));
    }
}

#[test]
fn scale_board_volume_as_string_is_not_silently_ignored() {
    let (mut d, mut c) = session(0);
    let r = execute(
        &mut d,
        &mut c,
        "scale_board",
        &json!({"volume_litres": "30"}),
    );
    match r {
        Err(_) => {}
        Ok(s) => assert!(
            (s["volume_litres"].as_f64().unwrap() - 30.0).abs() < 0.1,
            "ignored: {s}"
        ),
    }
}

#[test]
fn export_rejects_unknown_paper() {
    let (mut d, mut c) = session(0);
    let p = tmp("paper").join("o.pdf");
    let r = execute(
        &mut d,
        &mut c,
        "export",
        &json!({"what": "outline", "format": "pdf", "path": p, "paper": "A5"}),
    );
    assert!(r.is_err(), "A5 accepted: {r:?}");
}

#[test]
fn move_section_onto_another_section_is_refused() {
    let (mut d, mut c) = session(0);
    let r = d.board.real_sections();
    let (a, b) = (r.start, r.start + 1);
    let target = d.board.sections[b].position;
    let _ = execute(
        &mut d,
        &mut c,
        "move_section",
        &json!({"section_index": a, "position": target}),
    );
    let mut pos: Vec<f64> = d.board.sections.iter().map(|s| s.position).collect();
    let n = pos.len();
    pos.dedup_by(|x, y| (*x - *y).abs() < 0.1);
    assert_eq!(pos.len(), n, "duplicate section positions: {pos:?}");
}

#[test]
fn move_section_onto_another_keeps_volume_finite() {
    let (mut d, mut c) = session(0);
    let r = d.board.real_sections();
    let target = d.board.sections[r.start + 1].position;
    // Refused (two sections at one station), and the board stays valid.
    let r = execute(
        &mut d,
        &mut c,
        "move_section",
        &json!({"section_index": r.start, "position": target}),
    );
    assert!(r.is_err());
    let v = d.board.volume();
    assert!(v.is_finite() && v > 0.0, "volume {v}");
}

#[test]
fn remove_sections_down_to_one() {
    let (mut d, mut c) = session(0);
    while d.board.real_sections().len() > 1 {
        let i = d.board.real_sections().start;
        ok(
            &mut d,
            &mut c,
            "remove_section",
            json!({"section_index": i}),
        );
        let v = d.board.volume();
        assert!(
            v.is_finite() && v > 0.0,
            "volume {v} with {} sections",
            d.board.sections.len()
        );
    }
    let i = d.board.real_sections().start;
    err(
        &mut d,
        &mut c,
        "remove_section",
        json!({"section_index": i}),
    );
}

#[test]
fn add_section_too_close_is_refused() {
    let (mut d, mut c) = session(0);
    let p = d.board.sections[d.board.real_sections().start].position;
    err(&mut d, &mut c, "add_section", json!({"position": p + 0.05}));
    let s = ok(&mut d, &mut c, "add_section", json!({"position": p + 3.0}));
    assert_eq!(
        c.current_section as u64,
        s["section_index"].as_u64().unwrap()
    );
    let i = s["section_index"].as_u64().unwrap() as usize;
    assert!((d.board.sections[i].width() - d.board.width_at(p + 3.0)).abs() < 0.05);
}

#[test]
fn delete_knots_until_refused() {
    let (mut d, mut c) = session(0);
    let mut deleted = 0;
    while execute(
        &mut d,
        &mut c,
        "delete_knot",
        &json!({"curve": "outline", "index": 1}),
    )
    .is_ok()
    {
        deleted += 1;
        assert!(deleted < 50);
    }
    assert!(d.board.outline.len() >= 2);
    assert!(d.board.volume().is_finite());
}

#[test]
fn set_knot_end_points_are_constrained() {
    let (mut d, mut c) = session(0);
    let r = ok(
        &mut d,
        &mut c,
        "set_knot",
        json!({"curve": "outline", "index": 0, "x": 5, "y": 5}),
    );
    assert_eq!(r["point"], json!([0.0, 0.0]));
    assert!(r["note"].as_str().unwrap().contains("constrained"));
}

#[test]
fn sample_board_defaults() {
    let (mut d, mut c) = session(0);
    let r = ok(&mut d, &mut c, "sample_board", json!({}));
    let xs: Vec<f64> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["from_tail"]["cm"].as_f64().unwrap())
        .collect();
    assert!(xs.windows(2).all(|w| w[0] <= w[1]));
    assert!(xs.iter().any(|&x| (x - 30.48).abs() < 1e-6));
}

#[test]
fn sample_board_default_stations_are_unique() {
    let (mut d, mut c) = session(0);
    ok(&mut d, &mut c, "scale_board", json!({"length": 180}));
    let r = ok(&mut d, &mut c, "sample_board", json!({}));
    let xs: Vec<f64> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["from_tail"]["cm"].as_f64().unwrap())
        .collect();
    let mut u = xs.clone();
    u.dedup();
    assert_eq!(u.len(), xs.len(), "{xs:?}");
}

#[test]
fn units_in_context_format_results() {
    // The longboard template is 110" long.
    for (u, needle) in [
        (LengthUnit::FeetInchesFraction, "9' 2\""),
        (LengthUnit::FeetInchesDecimal, "9' 2.00\""),
        (LengthUnit::Millimetres, "2794.0 mm"),
        (LengthUnit::Centimetres, "279.40 cm"),
        (LengthUnit::Metres, "2.7940 m"),
    ] {
        let (mut d, _) = session(2);
        let mut c = Context {
            unit: u,
            ..Default::default()
        };
        let s = ok(&mut d, &mut c, "board_summary", json!({}));
        assert_eq!(s["length"]["text"], needle, "{u:?}");
    }
}

#[test]
fn estimate_weight_is_plausible_and_reacts_to_overrides() {
    let (mut d, mut c) = session(0);
    let base = ok(&mut d, &mut c, "estimate_weight", json!({}))["total_kg"]
        .as_f64()
        .unwrap();
    assert!((1.5..5.0).contains(&base), "{base}");
    let heavy = ok(
        &mut d,
        &mut c,
        "estimate_weight",
        json!({"foam_density": 0.05}),
    )["total_kg"]
        .as_f64()
        .unwrap();
    assert!(heavy > base);
}

#[test]
fn suggest_volume_follows_the_guild_factor() {
    let (mut d, mut c) = session(0);
    let s = ok(
        &mut d,
        &mut c,
        "suggest_volume",
        json!({"weight_kg": 75, "skill": "intermediate"}),
    );
    assert_eq!(s["min_litres"], 30.0);
    assert_eq!(s["max_litres"], 33.8);
    let lb = ok(
        &mut d,
        &mut c,
        "suggest_volume",
        json!({"weight_lb": 165.35, "skill": "intermediate"}),
    );
    assert_eq!(lb["min_litres"], 30.0);
    let long = ok(
        &mut d,
        &mut c,
        "suggest_volume",
        json!({"weight_kg": 75, "skill": "beginner", "board_type": "longboard", "waves": "small", "age": 60, "fitness": "low"}),
    );
    assert!(long["min_litres"].as_f64().unwrap() > 60.0);
    for bad in [
        json!({}),
        json!({"weight_kg": 75, "weight_lb": 165}),
        json!({"weight_kg": 2}),
        json!({"weight_kg": 75, "skill": "god"}),
    ] {
        assert!(
            execute(&mut d, &mut c, "suggest_volume", &bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn estimate_weight_rejects_negative_density() {
    let (mut d, mut c) = session(0);
    let r = execute(
        &mut d,
        &mut c,
        "estimate_weight",
        &json!({"foam_density": -1.0}),
    );
    if let Ok(v) = r {
        assert!(v["total_kg"].as_f64().unwrap() > 0.0, "{v}");
    }
}

#[test]
fn flip_twice_is_identity() {
    for t in 0..TEMPLATES.len() {
        let (mut d, mut c) = session(t);
        let before = d.board.clone();
        ok(&mut d, &mut c, "flip_board", json!({}));
        ok(&mut d, &mut c, "flip_board", json!({}));
        let b = &d.board;
        for (s0, s1) in [
            (&before.outline, &b.outline),
            (&before.deck, &b.deck),
            (&before.bottom, &b.bottom),
        ] {
            assert_eq!(s0.len(), s1.len());
            for (k0, k1) in s0.knots().iter().zip(s1.knots()) {
                for (p0, p1) in k0.points.iter().zip(k1.points.iter()) {
                    assert!((*p0 - *p1).length() < 1e-9, "t{t}: {p0} vs {p1}");
                }
            }
        }
        assert!((b.volume() - before.volume()).abs() / before.volume() < 1e-6);
    }
}

#[test]
fn flip_keeps_volume_and_mirrors_rocker() {
    let (mut d, mut c) = session(0);
    let b0 = d.board.clone();
    ok(&mut d, &mut c, "flip_board", json!({}));
    let l = b0.length();
    assert!((d.board.rocker_at(5.0) - b0.rocker_at(l - 5.0)).abs() < 1e-6);
    assert!((d.board.volume() - b0.volume()).abs() / b0.volume() < 0.005);
}

#[test]
fn surface_models_give_close_volumes() {
    for t in 0..TEMPLATES.len() {
        let (mut d, mut c) = session(t);
        let a = ok(
            &mut d,
            &mut c,
            "set_surface_model",
            json!({"model": "sblend"}),
        )["volume_litres"]
            .as_f64()
            .unwrap();
        let b = ok(
            &mut d,
            &mut c,
            "set_surface_model",
            json!({"model": "control_point"}),
        )["volume_litres"]
            .as_f64()
            .unwrap();
        assert!(a > 0.0 && b > 0.0);
        assert!(
            (a - b).abs() / a < 0.05,
            "t{t}: sblend {a} vs control point {b}"
        );
    }
}

// ------------------------------------------------------------------------------------ random edit fuzz

#[test]
fn random_edits_keep_volume_positive_and_undo_all_restores() {
    for seed in 1..=6u64 {
        let mut rng = Lcg(seed * 7919);
        let t = (seed as usize) % TEMPLATES.len();
        let (mut d, mut c) = session(t);
        let original = d.board.clone();
        let mut steps = 0;
        for _ in 0..40 {
            let b = &d.board;
            let l = b.length();
            let curve = ["outline", "deck", "bottom", "section"][rng.below(4)];
            let n = match curve {
                "outline" => b.outline.len(),
                "deck" => b.deck.len(),
                "bottom" => b.bottom.len(),
                _ => b.sections[c.current_section].spline.len(),
            };
            let (cmd, a) = match rng.below(9) {
                0 | 1 => (
                    "move_knot",
                    json!({"curve": curve, "index": rng.below(n), "dx": rng.range(-1.0, 1.0), "dy": rng.range(-0.3, 0.3),
                        "handle": (["point", "prev", "next"][rng.below(3)])}),
                ),
                2 => {
                    let x = rng.range(0.1, 0.9) * l;
                    let y = if curve == "outline" {
                        b.width_at(x) / 2.0
                    } else {
                        0.0
                    };
                    if curve == "section" {
                        (
                            "add_knot",
                            json!({"curve": curve, "x": rng.range(1.0, 20.0), "y": rng.range(0.5, 4.0)}),
                        )
                    } else {
                        ("add_knot", json!({"curve": curve, "x": x, "y": y}))
                    }
                }
                3 => (
                    "delete_knot",
                    json!({"curve": curve, "index": 1 + rng.below(n.saturating_sub(2).max(1))}),
                ),
                4 => ("add_section", json!({"position": rng.range(5.0, l - 5.0)})),
                5 => {
                    let r = b.real_sections();
                    (
                        "move_section",
                        json!({"section_index": r.start + rng.below(r.len()), "position": rng.range(5.0, l - 5.0)}),
                    )
                }
                6 => {
                    let r = b.real_sections();
                    (
                        "remove_section",
                        json!({"section_index": r.start + rng.below(r.len())}),
                    )
                }
                7 => (
                    "scale_board",
                    json!({"length": l * rng.range(0.95, 1.05), "width": b.max_width() * rng.range(0.95, 1.05)}),
                ),
                _ => ("flip_board", json!({})),
            };
            let before = d.board.clone();
            let r = execute(&mut d, &mut c, cmd, &a);
            if r.is_ok() && d.board != before {
                steps += 1;
            }
            assert!(
                finite_board(&d.board),
                "seed {seed}: {cmd} {a} made the board non-finite"
            );
            let v = d.board.volume();
            assert!(
                v.is_finite() && v > 0.0,
                "seed {seed}: volume {v} after {cmd} {a}"
            );
            // Keep the context in range like the GUI does (see remove_section_keeps_current_section_valid).
            let rs = d.board.real_sections();
            c.current_section = c.current_section.clamp(rs.start, rs.end - 1);
        }
        for _ in 0..steps {
            ok(&mut d, &mut c, "undo", json!({}));
        }
        assert_eq!(d.board, original, "seed {seed}: undoing all {steps} steps");
    }
}

// ------------------------------------------------------------------------------------ save/open round trip

#[test]
fn save_open_round_trip_for_every_template() {
    let dir = tmp("roundtrip");
    for (t, name) in ["shortboard", "funboard", "longboard"].iter().enumerate() {
        let (mut d, mut c) = session(t);
        ok(
            &mut d,
            &mut c,
            "set_info",
            json!({"name": format!("QA {name}"), "description": "line1\nline2", "comments": "a & b <c> \"d\""}),
        );
        ok(
            &mut d,
            &mut c,
            "set_fins",
            json!({"side_rear_x": 11, "side_rear_y": 13, "splay_degrees": 4.5}),
        );
        let p = dir.join(format!("{name}.brd"));
        ok(&mut d, &mut c, "save_board", json!({"path": p}));
        assert!(!d.is_dirty());
        let saved = d.board.clone();
        // Second save keeps a backup.
        ok(&mut d, &mut c, "set_info", json!({"name": "changed"}));
        ok(&mut d, &mut c, "save_board", json!({}));
        assert!(dir.join(format!("{name}.brd.bak")).exists());
        let bak =
            surfcraft_io::brd::read(&std::fs::read(dir.join(format!("{name}.brd.bak"))).unwrap())
                .unwrap();
        assert_eq!(bak.info.name, format!("QA {name}"));
        let (mut d2, mut c2) = session(0);
        ok(&mut d2, &mut c2, "open_board", json!({"path": p}));
        assert!(!d2.is_dirty());
        let mut got = d2.board.clone();
        got.info.name = saved.info.name.clone();
        assert_eq!(got.outline, saved.outline, "{name} outline");
        assert_eq!(got.deck, saved.deck, "{name} deck");
        assert_eq!(got.bottom, saved.bottom, "{name} bottom");
        assert_eq!(got.sections, saved.sections, "{name} sections");
        assert_eq!(got.fins, saved.fins, "{name} fins");
        assert_eq!(got.info.comments, saved.info.comments);
    }
}

#[test]
fn multiline_info_round_trips() {
    let dir = tmp("multiline");
    let (mut d, mut c) = session(0);
    ok(
        &mut d,
        &mut c,
        "set_info",
        json!({"description": "line1\nline2", "comments": "a\nb"}),
    );
    let p = dir.join("m.brd");
    ok(&mut d, &mut c, "save_board", json!({"path": p}));
    ok(&mut d, &mut c, "open_board", json!({"path": p}));
    assert_eq!(d.board.info.description, "line1\nline2");
    assert_eq!(d.board.info.comments, "a\nb");
}

#[test]
fn open_board_imports_untitled_and_save_refuses_without_path() {
    let dir = tmp("import");
    let (mut d, mut c) = session(0);
    let stp = dir.join("board.stp");
    ok(
        &mut d,
        &mut c,
        "export",
        json!({"what": "model", "format": "step", "path": stp}),
    );
    ok(&mut d, &mut c, "open_board", json!({"path": stp}));
    assert!(d.path.is_none());
    assert_eq!(d.board.info.name, "Shortboard"); // name carried in the STEP file, or the file stem
    err(&mut d, &mut c, "save_board", json!({}));
}

// ---------------------------------------------------------------------------------------- exports

fn check_pdf(bytes: &[u8], what: &str) -> usize {
    assert!(bytes.starts_with(b"%PDF-1."), "{what}: header");
    assert!(bytes.ends_with(b"%%EOF\n"), "{what}: trailer");
    let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(64)..]).into_owned();
    let xref_at: usize = tail
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(bytes[xref_at..].starts_with(b"xref\n"), "{what}: startxref");
    let table = String::from_utf8_lossy(&bytes[xref_at..]).into_owned();
    let mut lines = table.lines();
    lines.next();
    let header = lines.next().unwrap();
    let count: usize = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    lines.next(); // free entry
    for i in 1..count {
        let line = lines.next().unwrap();
        let off: usize = line[..10].parse().unwrap();
        assert!(
            bytes[off..].starts_with(format!("{i} 0 obj").as_bytes()),
            "{what}: xref entry {i} -> {off}"
        );
    }
    // Stream /Length values match the data (byte offsets: the header has non-UTF-8 bytes).
    let find = |from: usize, needle: &[u8]| {
        bytes[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    };
    let mut pos = 0;
    while let Some(i) = find(pos, b"/Length ") {
        let at = i + 8;
        let end = find(at, b" ").unwrap();
        let len: usize = std::str::from_utf8(&bytes[at..end])
            .unwrap()
            .parse()
            .unwrap();
        let start = find(at, b"stream\n").unwrap() + 7;
        assert!(
            bytes[start + len..].starts_with(b"\nendstream"),
            "{what}: stream length at {at}"
        );
        pos = start + len;
    }
    let text = String::from_utf8_lossy(bytes);
    // Page count matches.
    let pages = text.matches("/Type /Page ").count();
    assert!(text.contains(&format!("/Count {pages} ")), "{what}: /Count");
    pages
}

fn check_svg(path: &Path, what: &str) {
    let out = std::process::Command::new("xmllint")
        .arg("--noout")
        .arg(path)
        .output();
    match out {
        Ok(o) => assert!(
            o.status.success(),
            "{what}: xmllint: {}",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) => eprintln!("xmllint not available; skipped"),
    }
    let s = std::fs::read_to_string(path).unwrap();
    assert!(
        s.contains("width=\"") && s.contains("mm\""),
        "{what}: mm size"
    );
    assert!(
        !s.contains("NaN") && !s.contains("inf"),
        "{what}: non-finite numbers"
    );
}

fn check_dxf(s: &str, what: &str) {
    let lines: Vec<&str> = s.lines().collect();
    assert!(lines.len().is_multiple_of(2), "{what}: odd number of lines");
    for pair in lines.chunks(2) {
        assert!(
            pair[0].trim().parse::<i32>().is_ok(),
            "{what}: bad group code {:?}",
            pair[0]
        );
    }
    assert_eq!(lines[lines.len() - 2].trim(), "0");
    assert_eq!(lines[lines.len() - 1], "EOF");
    assert_eq!(
        s.matches("POLYLINE").count(),
        s.matches("SEQEND").count(),
        "{what}"
    );
    assert!(!s.contains("NaN"), "{what}: NaN");
}

#[test]
fn every_export_is_structurally_valid() {
    let dir = tmp("exports");
    let whats = [
        "outline",
        "outline_full",
        "spin",
        "profile",
        "sections",
        "spec_sheet",
    ];
    for t in 0..TEMPLATES.len() {
        let (mut d, mut c) = session(t);
        ok(
            &mut d,
            &mut c,
            "set_info",
            json!({"name": "Q&A <test> \"board\" — ñ 波"}),
        );
        for what in whats {
            for paper in ["A4", "A3", "Letter", "Tabloid"] {
                let p = dir.join(format!("t{t}-{what}-{paper}.pdf"));
                ok(
                    &mut d,
                    &mut c,
                    "export",
                    json!({"what": what, "format": "pdf", "path": p, "paper": paper}),
                );
                let pages = check_pdf(&std::fs::read(&p).unwrap(), &format!("t{t} {what} {paper}"));
                if what == "spec_sheet" {
                    assert_eq!(pages, 1);
                } else {
                    assert!(pages >= 1);
                }
            }
            let p = dir.join(format!("t{t}-{what}.svg"));
            ok(
                &mut d,
                &mut c,
                "export",
                json!({"what": what, "format": "svg", "path": p}),
            );
            check_svg(&p, &format!("t{t} {what} svg"));
            let p = dir.join(format!("t{t}-{what}.dxf"));
            ok(
                &mut d,
                &mut c,
                "export",
                json!({"what": what, "format": "dxf", "path": p}),
            );
            check_dxf(
                &std::fs::read_to_string(&p).unwrap(),
                &format!("t{t} {what} dxf"),
            );
        }
        // 3D.
        let p = dir.join(format!("t{t}.stl"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": "model", "format": "stl", "path": p}),
        );
        let bytes = std::fs::read(&p).unwrap();
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 84 + 50 * n);
        let tris = surfcraft_export::mesh_formats::stl_triangles(&bytes).unwrap();
        assert_eq!(tris.len(), n);
        let max_x = tris.iter().flatten().map(|v| v[0]).fold(f32::MIN, f32::max) as f64;
        assert!((max_x - d.board.length() * 10.0).abs() < 2.0, "STL in mm");
        assert!(tris.iter().flatten().flatten().all(|v| v.is_finite()));
        let p = dir.join(format!("t{t}.obj"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": "model", "format": "obj", "path": p}),
        );
        let obj = std::fs::read_to_string(&p).unwrap();
        let nv = obj.lines().filter(|l| l.starts_with("v ")).count();
        for l in obj.lines().filter(|l| l.starts_with("f ")) {
            for idx in l[2..].split_whitespace() {
                let i: usize = idx.split('/').next().unwrap().parse().unwrap();
                assert!(i >= 1 && i <= nv, "OBJ index {i} of {nv}");
            }
        }
        // 3MF and GLB: same mesh as the STL, at 1:10.
        let p = dir.join(format!("t{t}.3mf"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": "model", "format": "3mf", "path": p, "scale": 0.1}),
        );
        let entries = surfcraft_export::zip::read(&std::fs::read(&p).unwrap()).unwrap();
        let xml = String::from_utf8(entries[2].1.clone()).unwrap();
        assert_eq!(xml.matches("<triangle ").count(), n, "3MF triangles");
        assert!(xml.contains("Scale 1:10<"));
        let p = dir.join(format!("t{t}.glb"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": "model", "format": "glb", "path": p}),
        );
        let (gl, _) = surfcraft_export::gltf::read(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(
            gl["accessors"][2]["count"].as_u64().unwrap() as usize,
            3 * n
        );
        let z = gl["accessors"][0]["max"][2].as_f64().unwrap()
            - gl["accessors"][0]["min"][2].as_f64().unwrap();
        assert!((z * 100.0 - d.board.length()).abs() < 0.3, "GLB in metres");
        // STEP re-imports to the same knots.
        let p = dir.join(format!("t{t}.stp"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": "model", "format": "step", "path": p}),
        );
        let back = surfcraft_io::step::read(&std::fs::read(&p).unwrap()).unwrap();
        for (name, a, b) in [
            ("outline", &d.board.outline, &back.outline),
            ("deck", &d.board.deck, &back.deck),
            ("bottom", &d.board.bottom, &back.bottom),
        ] {
            assert_eq!(a.len(), b.len(), "t{t} STEP {name} knot count");
            let n = a.len();
            for (i, (k0, k1)) in a.knots().iter().zip(b.knots()).enumerate() {
                // The outer handles of the end knots do not shape the curve and are not stored in STEP.
                for h in surfcraft_core::Handle::ALL {
                    if (i == 0 && h == surfcraft_core::Handle::Prev)
                        || (i + 1 == n && h == surfcraft_core::Handle::Next)
                    {
                        continue;
                    }
                    let (p0, p1) = (k0.get(h), k1.get(h));
                    assert!(
                        (p0 - p1).length() < 1e-6,
                        "t{t} STEP {name} knot {i} {h:?}: {p0} vs {p1}"
                    );
                }
            }
        }
        let rs = d.board.real_sections();
        assert_eq!(
            back.real_sections().len(),
            rs.len(),
            "t{t} STEP section count"
        );
        for (s0, s1) in d.board.sections[rs.clone()]
            .iter()
            .zip(&back.sections[back.real_sections()])
        {
            assert!((s0.position - s1.position).abs() < 1e-6);
            assert_eq!(s0.spline.len(), s1.spline.len());
        }
        assert!((back.volume() - d.board.volume()).abs() / d.board.volume() < 1e-4);
    }
}

#[test]
fn dxf_is_ascii() {
    let dir = tmp("dxf-ascii");
    let (mut d, mut c) = session(0);
    for what in ["outline", "spin", "profile", "spec_sheet"] {
        let p = dir.join(format!("{what}.dxf"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": what, "format": "dxf", "path": p}),
        );
        let s = std::fs::read(&p).unwrap();
        assert!(s.is_ascii(), "{what}: non-ASCII bytes in DXF");
    }
}

#[test]
fn export_overwrites_with_backup() {
    let dir = tmp("export-bak");
    let (mut d, mut c) = session(0);
    let p = dir.join("o.svg");
    ok(
        &mut d,
        &mut c,
        "export",
        json!({"what": "outline", "format": "svg", "path": p}),
    );
    ok(
        &mut d,
        &mut c,
        "export",
        json!({"what": "profile", "format": "svg", "path": p}),
    );
    assert!(dir.join("o.svg.bak").exists());
    // No temp file left behind.
    assert!(!dir.join(".o.svg.tmp").exists());
}

#[test]
fn exports_of_a_heavily_edited_board_stay_finite() {
    let dir = tmp("edited-exports");
    let (mut d, mut c) = session(2);
    ok(
        &mut d,
        &mut c,
        "scale_board",
        json!({"length": "10'6\"", "width": "24\"", "volume_litres": 90}),
    );
    ok(&mut d, &mut c, "add_section", json!({"position": "2'"}));
    ok(&mut d, &mut c, "flip_board", json!({}));
    for (what, format) in [
        ("outline", "svg"),
        ("sections", "svg"),
        ("spec_sheet", "svg"),
        ("profile", "dxf"),
    ] {
        let p = dir.join(format!("{what}.{format}"));
        ok(
            &mut d,
            &mut c,
            "export",
            json!({"what": what, "format": format, "path": p}),
        );
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(!s.contains("NaN") && !s.contains("inf"), "{what}.{format}");
    }
}

#[test]
fn remove_section_keeps_current_section_valid() {
    let (mut d, mut c) = session(0);
    let last = d.board.real_sections().end - 1;
    c.current_section = last;
    ok(
        &mut d,
        &mut c,
        "remove_section",
        json!({"section_index": last}),
    );
    assert!(
        d.board.real_sections().contains(&c.current_section),
        "current_section {} not in {:?}",
        c.current_section,
        d.board.real_sections()
    );
    ok(&mut d, &mut c, "section_info", json!({}));
}

#[test]
fn double_minus_is_rejected() {
    assert_eq!(LengthUnit::Centimetres.parse("--3"), None);
}

/// Shaping tools on every template: every contour type and rail preset as one undo step, volumes reported,
/// widths and stringer thickness kept, mesh and CAM still work.
#[test]
fn shaping_commands_on_every_template() {
    for t in 0..TEMPLATES.len() {
        let kinds = [
            json!({"type": "flat"}),
            json!({"type": "single_concave", "depth": "3/16\"", "fade_out": "2'"}),
            json!({"type": "double_concave", "depth": "1/8\"", "inner_depth": "1/16\"", "end": "3'"}),
            json!({"type": "vee", "depth": "1/4\"", "end": "2'", "run_out_fraction": 0.85}),
            json!({"type": "channels", "channel_count": 3, "channel_width": "3/4\"", "end": "18\""}),
            json!({"type": "remove"}),
        ];
        let rails = ["50/50", "60/40", "70/30", "80/20"]
            .map(|p| json!({"preset": p}))
            .into_iter()
            .chain([json!({"apex": 0.45, "softness": 0.5, "tuck": "1/4\"",
                "tail": {"preset": "80/20"}, "nose": {"softness": 1}, "start": "1'", "end": "5'"})]);
        let cases = kinds
            .into_iter()
            .map(|mut a| {
                a["keep"] = json!("stringer");
                ("apply_bottom_contour", a)
            })
            .chain(rails.map(|a| ("set_rails", a)));
        for (name, a) in cases {
            let (mut d, mut c) = session(t);
            let before = d.board.clone();
            let r = ok(&mut d, &mut c, name, a.clone());
            let (v0, v1) = (
                r["volume_before_litres"].as_f64().unwrap(),
                r["volume_after_litres"].as_f64().unwrap(),
            );
            assert!((v1 - v0).abs() < 0.06 * v0, "{t} {name} {a}: {v0} -> {v1}");
            assert!(r["sections_changed"].as_u64().unwrap() > 0, "{t} {a}");
            assert!(finite_board(&d.board));
            let l = before.length();
            for i in 1..20 {
                let x = l * i as f64 / 20.0;
                assert!((d.board.width_at(x) - before.width_at(x)).abs() < 1e-9);
                assert!((d.board.thickness_at(x) - before.thickness_at(x)).abs() < 1e-9);
            }
            assert_eq!(d.board.outline, before.outline);
            assert_eq!(d.board.bottom, before.bottom, "rocker kept");
            assert_eq!(d.board.deck, before.deck);
            let mesh =
                surfcraft_core::Mesh::of_board(&d.board, surfcraft_core::MeshDetail::PREVIEW);
            assert!(mesh.is_watertight(), "{t} {a}");
            // One undo step restores the board exactly.
            ok(&mut d, &mut c, "undo", json!({}));
            assert_eq!(d.board, before, "{t} {a}");
            assert!(!d.can_undo());
        }
    }
}

#[test]
fn shaped_board_still_machines() {
    for t in 0..TEMPLATES.len() {
        let (mut d, mut c) = session(t);
        ok(
            &mut d,
            &mut c,
            "apply_bottom_contour",
            json!({"type": "double_concave", "end": "3'"}),
        );
        ok(&mut d, &mut c, "set_rails", json!({"preset": "70/30"}));
        let r = ok(
            &mut d,
            &mut c,
            "generate_toolpath",
            json!({"job": "bottom"}),
        );
        assert!(r.to_string().contains("passes"), "{r}");
    }
}

/// Cutting foam (the default): the rail edge stays, the stringer rocker deepens, the deck stays, the volume
/// drops, one undo step.
#[test]
fn contours_cut_foam_by_default() {
    for t in 0..TEMPLATES.len() {
        for kind in [
            "single_concave",
            "double_concave",
            "vee",
            "channels",
            "flat",
        ] {
            let (mut d, mut c) = session(t);
            let before = d.board.clone();
            let r = ok(
                &mut d,
                &mut c,
                "apply_bottom_contour",
                json!({"type": kind, "depth": "3/16\""}),
            );
            assert_eq!(r["keep"], "rails");
            let (v0, v1) = (
                r["volume_before_litres"].as_f64().unwrap(),
                r["volume_after_litres"].as_f64().unwrap(),
            );
            assert!(v1 < v0 && v0 - v1 < 0.06 * v0, "{t} {kind}: {v0} -> {v1}");
            let l = before.length();
            let dz = d.board.deck_at(l / 2.0) - before.deck_at(l / 2.0);
            for i in 1..20 {
                let x = l * i as f64 / 20.0;
                assert!((d.board.width_at(x) - before.width_at(x)).abs() < 1e-9);
                assert!((d.board.deck_at(x) - before.deck_at(x) - dz).abs() < 1e-6);
                assert!(d.board.thickness_at(x) <= before.thickness_at(x) + 1e-3);
            }
            if kind.ends_with("concave") {
                // Thinner on the stringer inside the zone.
                let x = 0.3 * l;
                assert!(
                    before.thickness_at(x) - d.board.thickness_at(x) > 0.4,
                    "{t} {kind}"
                );
            }
            assert!(finite_board(&d.board));
            let mesh =
                surfcraft_core::Mesh::of_board(&d.board, surfcraft_core::MeshDetail::PREVIEW);
            assert!(mesh.is_watertight(), "{t} {kind}");
            ok(&mut d, &mut c, "undo", json!({}));
            assert_eq!(d.board, before, "{t} {kind}");
            assert!(!d.can_undo());
        }
    }
}

#[test]
fn set_fins_places_presets_with_boxes() {
    let (mut d, mut c) = session(0);
    let r = ok(
        &mut d,
        &mut c,
        "set_fins",
        json!({"preset": "five", "box_system": "fcs2"}),
    );
    let fins = r["setup"]["fins"].as_array().unwrap();
    assert_eq!(fins.len(), 5, "{r}");
    assert!(fins.iter().all(|f| f["box_system"] == "fcs2"));
    assert_eq!(r["setup"]["preset"], "five");
    assert_eq!(r["setup"]["stored"], true);
    // The legacy marks follow the setup.
    assert_eq!(
        r["fins"].as_array().unwrap()[0].as_f64().unwrap(),
        d.board.fins[0]
    );
    assert!(d.board.fins[0] > 0.0);
    // One undo step.
    ok(&mut d, &mut c, "undo", json!({}));
    assert_eq!(d.board.fin_setup, None);
    ok(&mut d, &mut c, "redo", json!({}));
    // Changing only the box system keeps the positions.
    let before = d.board.fin_setup.clone().unwrap();
    let r = ok(&mut d, &mut c, "set_fins", json!({"box_system": "Futures"}));
    let after = d.board.fin_setup.clone().unwrap();
    assert_eq!(r["setup"]["box_system"], "futures");
    for (a, b) in before.fins.iter().zip(&after.fins) {
        assert_eq!((a.x, a.y), (b.x, b.y));
    }
    // 2+1: US box in the middle.
    let r = ok(
        &mut d,
        &mut c,
        "set_fins",
        json!({"preset": "2+1", "box_system": "fcs1"}),
    );
    let systems: Vec<&str> = r["setup"]["fins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["box_system"].as_str().unwrap())
        .collect();
    assert_eq!(systems, ["fcs1", "fcs1", "usbox"]);
    // The old arguments still work, and replace the setup by plain marks.
    let r = ok(&mut d, &mut c, "set_fins", json!({"side_rear_x": 25}));
    assert_eq!(r["fins"][0], 25.0);
    assert_eq!(d.board.fin_setup, None);
    assert_eq!(r["setup"]["stored"], false);
    // A call without arguments changes nothing.
    ok(&mut d, &mut c, "set_fins", json!({"preset": "thruster"}));
    let s = d.board.fin_setup.clone();
    ok(&mut d, &mut c, "set_fins", json!({}));
    assert_eq!(d.board.fin_setup, s);
    for bad in [
        json!({"preset": "hexa"}),
        json!({"box_system": "glue"}),
        json!({"preset": 3}),
        json!({"preset": "custom"}),
        json!({"preset": "quad", "side_rear_x": 20}),
    ] {
        err(&mut d, &mut c, "set_fins", bad);
    }
}
