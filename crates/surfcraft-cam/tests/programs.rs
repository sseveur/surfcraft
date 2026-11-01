//! End-to-end checks of deck and bottom programs on the fixture boards.

use std::sync::{Arc, OnceLock};

use surfcraft_cam::*;
use surfcraft_core::{Board, DVec2, DVec3};

const NAMES: [&str; 3] = ["shortboard", "funboard", "longboard"];

fn load(name: &str) -> Board {
    let bytes = std::fs::read(format!(
        "{}/../../tests/fixtures/{name}.brd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    surfcraft_io::brd::read(&bytes).unwrap()
}

struct Case {
    name: &'static str,
    board: Board,
    deck: Program,
    bottom: Program,
}

fn cases() -> &'static [Case] {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| {
        let cfg = MachineConfig::default();
        NAMES
            .iter()
            .map(|&name| {
                let board = load(name);
                let deck = generate_deck(&board, &cfg).unwrap();
                let bottom = generate_bottom(&board, &cfg).unwrap();
                Case {
                    name,
                    board,
                    deck,
                    bottom,
                }
            })
            .collect()
    })
}

/// Default settings with a coarser lengthwise resolution, for the tests that generate many programs.
fn coarse() -> MachineConfig {
    MachineConfig {
        lengthwise_resolution: 3.0,
        ..Default::default()
    }
}

fn code_lines(g: &str) -> Vec<&str> {
    g.lines().filter(|l| !l.starts_with('(')).collect()
}

fn splits(board: &Board, cfg: &MachineConfig) -> usize {
    ((board.length() - 0.1) / cfg.lengthwise_resolution) as usize + 1
}

#[test]
fn programs_are_finite_and_within_machine_bounds() {
    for c in cases() {
        for p in [&c.deck, &c.bottom] {
            let half = c.board.max_width() / 2.0 * 10.0;
            assert!(p.moves().count() > 1000);
            for m in p.moves() {
                assert!(m.pos.is_finite(), "{}: {m:?}", c.name);
                assert!(m.feed > 0.0);
                assert!(
                    m.pos.x > 900.0 && m.pos.x < 1000.0 + c.board.length() * 10.0 + 100.0,
                    "{} {:?}: x {}",
                    c.name,
                    p.side,
                    m.pos.x
                );
                assert!(
                    m.pos.y.abs() < half + 12.7 + 50.0,
                    "{}: y {}",
                    c.name,
                    m.pos.y
                );
                assert!(
                    m.pos.z > 100.0 && m.pos.z < 700.0,
                    "{}: z {}",
                    c.name,
                    m.pos.z
                );
            }
            for p in p.toolpath.points() {
                assert!(p.pos.is_finite() && p.normal.is_finite());
                assert!((p.normal.length() - 1.0).abs() < 1e-6, "{p:?}");
            }
        }
    }
}

#[test]
fn programs_start_and_end_with_the_right_codes() {
    for c in cases() {
        for p in [&c.deck, &c.bottom] {
            let g = p.to_gcode();
            let lines = code_lines(&g);
            assert_eq!(lines[..5], ["G21", "G90", "G28", "M3", "G04 P2"]);
            assert!(lines[5].starts_with("G01 X"), "{}", lines[5]);
            assert!(
                lines[5].contains(" F"),
                "first move must carry a feed: {}",
                lines[5]
            );
            assert_eq!(lines[lines.len() - 3..], ["M5", "G28", "M2"]);
            assert!(g.starts_with("(Generated with SurfCraft)\n"));
            // Every move is written, nothing else uses G01.
            let move_lines = lines
                .iter()
                .filter(|l| l.starts_with('X') || l.starts_with("G01"))
                .count();
            assert_eq!(move_lines, p.moves().count());
            assert!(!g.contains("NaN") && !g.contains("inf"));
        }
    }
}

#[test]
fn pass_counts_match_settings() {
    let cfg = MachineConfig::default();
    for c in cases() {
        let n = splits(&c.board, &cfg);
        let d = &c.deck.toolpath;
        assert_eq!(d.count(PassKind::Approach, None), 1);
        assert_eq!(d.count(PassKind::Stringer, None), 1);
        assert_eq!(d.count(PassKind::Regular, Some(Side::Right)), cfg.deck_cuts);
        assert_eq!(d.count(PassKind::Regular, Some(Side::Left)), cfg.deck_cuts);
        assert_eq!(
            d.count(PassKind::Rail, Some(Side::Right)),
            cfg.deck_rail_cuts
        );
        assert_eq!(
            d.count(PassKind::Rail, Some(Side::Left)),
            cfg.deck_rail_cuts
        );
        assert_eq!(d.count(PassKind::Outline, None), 2);
        assert_eq!(d.count(PassKind::Cleanup, None), 2 * cfg.deck_cleanup_cuts);
        assert_eq!(d.count(PassKind::SideChange, None), 1);
        assert_eq!(d.count(PassKind::NoseCutOff, None), 0);
        let b = &c.bottom.toolpath;
        assert_eq!(b.count(PassKind::Stringer, None), 1);
        assert_eq!(b.count(PassKind::Regular, None), 2 * cfg.bottom_cuts);
        assert_eq!(b.count(PassKind::Rail, None), 0);
        assert_eq!(b.count(PassKind::Outline, None), 2);
        assert_eq!(
            b.count(PassKind::Cleanup, None),
            2 * cfg.bottom_cleanup_cuts
        );
        assert_eq!(b.count(PassKind::SideChange, None), 1);
        for p in d.passes.iter().chain(&b.passes) {
            let expect = match p.kind {
                PassKind::Approach => 1,
                PassKind::Regular | PassKind::Rail => cfg.move_between_steps + n + 1,
                PassKind::SideChange => cfg.side_change_steps + 1,
                _ => n + 2,
            };
            if matches!(
                p.kind,
                PassKind::Regular
                    | PassKind::Rail
                    | PassKind::Stringer
                    | PassKind::Outline
                    | PassKind::Cleanup
            ) {
                // Points are inserted where the surface turns fast along a pass (the board's ends) and where
                // a pass rolls over a hard edge.
                assert!(
                    p.points.len() >= expect && p.points.len() <= expect + expect / 4,
                    "{} {:?}: {} points for {expect} steps",
                    c.name,
                    p.kind,
                    p.points.len()
                );
            } else {
                assert_eq!(p.points.len(), expect, "{} {:?}", c.name, p.kind);
            }
        }
        let stats = c.deck.stats();
        assert_eq!(stats.passes, d.passes.len() - 1);
        assert_eq!(stats.points, c.deck.moves().count());
        assert!(stats.cut_length > 100_000.0);
        assert!(stats.estimated_time > 600.0 && stats.estimated_time < 10_000.0);
    }
}

/// Regular passes lie on the board surface, and the cutter (25.4 mm bullnose, 5 mm corner) touches each
/// contact point.
#[test]
fn regular_passes_are_on_the_surface_and_touch_the_cutter() {
    let cfg = MachineConfig::default();
    let (big, r) = (12.7, 5.0);
    let stay = big + cfg.stringer_width * 10.0;
    for c in cases() {
        let len = c.board.length();
        for p in [&c.deck, &c.bottom] {
            let deck = p.side == BoardSide::Deck;
            let mut checked = 0;
            for (k, pass) in p.toolpath.passes.iter().enumerate() {
                if pass.kind != PassKind::Regular {
                    continue;
                }
                // Moves equal at the written precision (1e-4 mm) to the previous move are merged, possibly
                // into the previous pass.
                let moves: Vec<&Move> = p
                    .moves()
                    .filter(|m| m.pass == Some(k) || m.pass == Some(k - 1))
                    .collect();
                for (i, pt) in pass.points.iter().enumerate().skip(pass.lead_in).step_by(7) {
                    let (x, y) = (pt.pos.x, pt.pos.y.abs());
                    if x > 2.0 && x < len - 2.0 {
                        // On the modelled cross-section at x: the whole section (-360, 360), as the 3D mesh and
                        // STL export, from the deck centre over the rail apex (widest point) to the bottom centre.
                        // (The S-blend surface depends on the parameter range, so `deck_at_xy` over (-90, 90) is
                        // not an exact reference.)
                        let at = |s: f64| c.board.surface_point(x, s, -360.0, 360.0);
                        let apex = (0..=100)
                            .map(|k| (k as f64 / 100.0, at(k as f64 / 100.0).y))
                            .max_by(|a, b| a.1.total_cmp(&b.1))
                            .unwrap()
                            .0;
                        let (mut a, mut b) = if deck { (0.0, apex) } else { (1.0, apex) };
                        for _ in 0..60 {
                            let m = (a + b) / 2.0;
                            if at(m).y < y { a = m } else { b = m }
                        }
                        // Distance to the section curve near there (z by y alone is ill-conditioned on the
                        // vertical part of the rail).
                        let dist = |s: f64| {
                            let q = at(s.clamp(0.0, 1.0));
                            (q.y - y).hypot(q.z - pt.pos.z)
                        };
                        let mut d = dist(a);
                        if d >= 0.01 {
                            let (mut lo, mut hi) = (a - 0.02, a + 0.02);
                            for _ in 0..40 {
                                let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                                if dist(m1) < dist(m2) {
                                    hi = m2
                                } else {
                                    lo = m1
                                }
                            }
                            d = dist(lo);
                        }
                        assert!(
                            d < 0.01,
                            "{} {:?} pass {k} point {i}: {:?} is {d} cm off the section",
                            c.name,
                            p.side,
                            pt.pos
                        );
                    }
                    let contact = p.placement.point(pt.pos);
                    let side = if pass.side == Side::Right { 1.0 } else { -1.0 };
                    let tip = cfg
                        .cutter
                        .tip(
                            contact,
                            p.placement.vector(pt.normal),
                            Some(cfg.stringer_width),
                            side,
                            None,
                        )
                        .pos;
                    assert!(
                        moves.iter().any(|m| m.pos.distance(tip) < 2e-4),
                        "{} {:?} pass {k} point {i}: tip {tip:?} not in program, nearest {:?}",
                        c.name,
                        p.side,
                        moves
                            .iter()
                            .map(|m| m.pos.distance(tip))
                            .fold(f64::MAX, f64::min)
                    );
                    if (tip.y.abs() - stay).abs() < 1e-9 {
                        continue; // pushed off the stringer
                    }
                    let rel = contact - tip;
                    let radial = DVec2::new(rel.x, rel.y).length();
                    let on_flat = rel.z.abs() < 1e-9 && radial <= big - r + 1e-9;
                    let torus = ((radial - (big - r)).powi(2) + (rel.z - r).powi(2)).sqrt();
                    assert!(
                        on_flat || (torus - r).abs() < 1e-6,
                        "{} pass {k} point {i}: {rel:?}",
                        c.name
                    );
                    checked += 1;
                }
            }
            assert!(checked > 500, "{checked}");
        }
    }
}

#[test]
fn deck_passes_are_above_bottom_passes() {
    for c in cases() {
        let b = &c.board;
        let len = b.length();
        let inside = |p: &ToolPoint| {
            p.pos.x > 2.0 && p.pos.x < len - 2.0 && p.pos.y.abs() < b.width_at(p.pos.x) / 2.0 - 0.5
        };
        let mut deck_mean = (0.0, 0);
        for p in c
            .deck
            .toolpath
            .passes
            .iter()
            .filter(|p| p.kind == PassKind::Regular)
        {
            for pt in p.points.iter().filter(|p| inside(p)) {
                assert!(
                    pt.pos.z > b.bottom_at_xy(pt.pos.x, pt.pos.y.abs()),
                    "{}",
                    c.name
                );
                assert!(pt.normal.z > 0.0);
                deck_mean = (deck_mean.0 + pt.pos.z, deck_mean.1 + 1);
            }
        }
        let mut bottom_mean = (0.0, 0);
        for p in c
            .bottom
            .toolpath
            .passes
            .iter()
            .filter(|p| p.kind == PassKind::Regular)
        {
            for pt in p.points.iter().filter(|p| inside(p)) {
                assert!(
                    pt.pos.z < b.deck_at_xy(pt.pos.x, pt.pos.y.abs()),
                    "{}",
                    c.name
                );
                assert!(pt.normal.z < 0.0);
                bottom_mean = (bottom_mean.0 + pt.pos.z, bottom_mean.1 + 1);
            }
        }
        assert!(deck_mean.0 / deck_mean.1 as f64 > bottom_mean.0 / bottom_mean.1 as f64 + 3.0);
    }
}

#[test]
fn stringer_and_outline_offsets_point_outward() {
    let cfg = MachineConfig {
        stay_away_from_stringer: false,
        ..Default::default()
    };
    let board = load("shortboard");
    let p = generate_deck(&board, &cfg).unwrap();
    // Stringer: the tool never goes below the deck centre line (an inward-facing normal would put it up to
    // two corner radii below on any slope).
    let k = p
        .toolpath
        .passes
        .iter()
        .position(|p| p.kind == PassKind::Stringer)
        .unwrap();
    for (m, pt) in p
        .moves()
        .filter(|m| m.pass == Some(k))
        .zip(&p.toolpath.passes[k].points)
    {
        let surface = p.placement.point(pt.pos);
        let n = p.placement.vector(pt.normal);
        assert!(m.pos.z >= surface.z - 5.0 * (1.0 - n.z) - 1e-6, "{m:?}");
    }
    // Outline: the tool axis is a cutter radius outside the outline.
    for (k, pass) in p
        .toolpath
        .passes
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == PassKind::Outline)
    {
        for (m, pt) in p
            .moves()
            .filter(|m| m.pass == Some(k))
            .zip(&pass.points)
            .skip(20)
            .step_by(10)
        {
            if pt.pos.x < 20.0 || pt.pos.x > board.length() - 20.0 {
                continue;
            }
            // The offset is a cutter radius along the outline normal; its y part shrinks where the outline
            // curves (0.94 of the radius at 20 cm from the nose of the shortboard).
            let half = board.width_at(pt.pos.x) / 2.0 * 10.0;
            let across = 12.7 * pt.normal.y.abs();
            assert!(
                m.pos.y.abs() > half + across - 0.3 && across > 11.0,
                "{} vs {half} + {across}",
                m.pos.y
            );
        }
    }
}

#[test]
fn parallel_and_sequential_output_are_identical() {
    let board = load("shortboard");
    for generator in [
        GeneratorKind::SurfaceSplits,
        GeneratorKind::WidthSplits { splits: 10 },
    ] {
        let par = MachineConfig {
            generator: generator.clone(),
            ..coarse()
        };
        let seq = MachineConfig {
            parallel: false,
            ..par.clone()
        };
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            let a = generate(&board, &par, side).unwrap();
            let b = generate(&board, &seq, side).unwrap();
            assert_eq!(a.toolpath, b.toolpath);
            assert_eq!(a.to_gcode(), b.to_gcode());
        }
    }
}

#[test]
fn cut_offs_terminate() {
    let board = load("funboard");
    let cfg = MachineConfig {
        cut_off_nose: true,
        cut_off_tail: true,
        ..coarse()
    };
    let deck = generate_deck(&board, &cfg).unwrap();
    assert_eq!(deck.toolpath.count(PassKind::NoseCutOff, None), 1);
    let bottom = generate_bottom(&board, &cfg).unwrap();
    assert_eq!(bottom.toolpath.count(PassKind::TailCutOff, None), 1);
    // The bottom cut-off lifts away from the bottom (up on the machine), not through the board.
    let k = bottom
        .toolpath
        .passes
        .iter()
        .position(|p| p.kind == PassKind::TailCutOff)
        .unwrap();
    let zs: Vec<f64> = bottom
        .moves()
        .filter(|m| m.pass == Some(k))
        .map(|m| m.pos.z)
        .collect();
    let low = zs.iter().cloned().fold(f64::MAX, f64::min);
    let high = zs.iter().cloned().fold(f64::MIN, f64::max);
    let surface = bottom
        .placement
        .point(DVec3::new(1.0, 0.0, board.rocker_at(1.0)))
        .z;
    // Lifted by the thickness plus 5 cm above the bottom, lowered by the corner radius into it.
    assert!(high > surface + 50.0, "{zs:?} vs {surface}");
    assert!(low < surface && low > surface - 15.0, "{zs:?} vs {surface}");
}

/// Regression: support 2 was measured from the machine origin while support 1 was measured from the tail
/// stop. With both relative to the tail stop the board rests on both supports, deck down and bottom down.
#[test]
fn board_rests_on_both_supports() {
    let supports = SupportsConfig::default();
    let holding = HoldingSystem::Supports(supports.clone());
    for name in NAMES {
        let board = load(name);
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            let p = holding.placement(&board, None, side);
            for (from_tailstop, height) in [
                (supports.support1_from_tailstop, supports.support1_height),
                (supports.support2_from_tailstop, supports.support2_height),
            ] {
                let target = supports.tailstop_pos + from_tailstop;
                // The surface lying on the supports: the bottom for the deck program, the deck for the bottom.
                let at = |x: f64| {
                    let z = if side == BoardSide::Deck {
                        board.rocker_at(x)
                    } else {
                        board.deck_at(x)
                    };
                    p.point(DVec3::new(x, 0.0, z))
                };
                let (mut a, mut b) = (0.0, board.length());
                for _ in 0..60 {
                    let m = (a + b) / 2.0;
                    if at(m).x < target { a = m } else { b = m }
                }
                let z = at(a).z;
                assert!(
                    (z - height).abs() < 0.5,
                    "{name} {side:?}: {z} on a {height} mm support"
                );
            }
        }
    }
}

#[test]
fn lift_clear_is_in_machine_millimetres() {
    let board = load("shortboard");
    let cfg = MachineConfig {
        before_deck_cut: MachineAction::LiftClear,
        after_deck_cut: MachineAction::LiftClear,
        clear_height: 10.0,
        ..coarse()
    };
    let p = generate_deck(&board, &cfg).unwrap();
    let lifts: Vec<(f64, f64)> = p
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::MoveZ { z, feed } => Some((*z, *feed)),
            _ => None,
        })
        .collect();
    assert_eq!(lifts.len(), 2);
    let first = p.moves().next().unwrap().pos.z;
    let last = p.moves().last().unwrap().pos.z;
    assert!(
        lifts[0].0 >= first + 100.0 - 1e-9 && lifts[0].0 < first + 200.0,
        "{lifts:?}"
    );
    assert!((lifts[1].0 - (last + 100.0)).abs() < 1e-9);
    let g = p.to_gcode();
    let lines = code_lines(&g);
    assert_eq!(lines[..3], ["G21", "G90", lines[2]]);
    assert!(
        lines[2].starts_with("G01 Z") && lines[2].contains(" F"),
        "{}",
        lines[2]
    );
}

#[test]
fn scripts_are_written_verbatim() {
    let board = load("shortboard");
    let cfg = MachineConfig {
        before_bottom_cut: MachineAction::Script("G0 Z300\nM8".into()),
        after_bottom_cut: MachineAction::Nothing,
        ..coarse()
    };
    let g = generate_bottom(&board, &cfg).unwrap().to_gcode();
    assert!(g.contains("G90\nG0 Z300\nM8\nM3\n"));
    assert!(g.ends_with("M5\nM2\n"));
}

#[test]
fn sandwich_compensation_moves_points_inward() {
    let board = load("shortboard");
    let base = coarse();
    let plain = toolpath(&board, &base, BoardSide::Deck).unwrap();
    let cfg = MachineConfig {
        sandwich: SandwichCompensation {
            deck: true,
            deck_thickness: 0.3,
            ..Default::default()
        },
        ..base.clone()
    };
    let comp = toolpath(&board, &cfg, BoardSide::Deck).unwrap();
    for (a, b) in plain.passes.iter().zip(&comp.passes) {
        for (p, q) in a.points.iter().zip(&b.points) {
            if a.kind == PassKind::SideChange || a.kind == PassKind::Approach {
                continue;
            }
            assert!(
                ((p.pos - q.pos).length() - 0.3).abs() < 1e-9,
                "{:?}",
                a.kind
            );
            assert!((p.pos - q.pos).dot(p.normal) > 0.0, "inward");
        }
    }
    // Regression: the deck rail limit was stored in the bottom field. A limit wider than the board
    // switches deck compensation off everywhere, and leaves the bottom alone.
    let limited = MachineConfig {
        sandwich: SandwichCompensation {
            deck: true,
            deck_rail_limit: 100.0,
            bottom: true,
            ..Default::default()
        },
        ..base.clone()
    };
    let deck = toolpath(&board, &limited, BoardSide::Deck).unwrap();
    assert_eq!(deck.passes[1..], plain.passes[1..]);
    let bottom_plain = toolpath(&board, &base, BoardSide::Bottom).unwrap();
    let bottom = toolpath(&board, &limited, BoardSide::Bottom).unwrap();
    assert_ne!(bottom.passes[3], bottom_plain.passes[3]);
    // The limit applies on both sides (measured with |y|).
    let left_limited = MachineConfig {
        sandwich: SandwichCompensation {
            deck: true,
            deck_rail_limit: 3.0,
            ..Default::default()
        },
        ..base
    };
    let tp = toolpath(&board, &left_limited, BoardSide::Deck).unwrap();
    let unchanged = |side: Side| {
        tp.passes
            .iter()
            .zip(&plain.passes)
            .filter(|(p, _)| p.kind == PassKind::Rail && p.side == side)
            .flat_map(|(p, q)| p.points.iter().zip(&q.points))
            .filter(|(a, b)| a.pos == b.pos)
            .count()
    };
    assert!(unchanged(Side::Right) > 100);
    assert_eq!(unchanged(Side::Right), unchanged(Side::Left));
}

#[test]
fn width_splits_have_one_centre_pass() {
    let board = load("funboard");
    let cfg = MachineConfig {
        generator: GeneratorKind::WidthSplits { splits: 30 },
        ..coarse()
    };
    for side in [BoardSide::Deck, BoardSide::Bottom] {
        let p = generate(&board, &cfg, side).unwrap();
        let tp = &p.toolpath;
        assert_eq!(tp.count(PassKind::WidthSplit, None), 59);
        assert_eq!(tp.count(PassKind::WidthSplit, Some(Side::Center)), 1);
        assert_eq!(tp.count(PassKind::WidthSplit, Some(Side::Left)), 29);
        let max_frac = tp
            .points()
            .filter(|p| p.pos.x > 10.0 && p.pos.x < board.length() - 10.0)
            .map(|p| p.pos.y.abs() / (board.width_at(p.pos.x) / 2.0))
            .fold(0.0, f64::max);
        assert!((max_frac - 29.0 / 30.0).abs() < 0.01, "{max_frac}");
        for pt in tp.points().skip(1) {
            let up = if side == BoardSide::Deck { 1.0 } else { -1.0 };
            if pt.pos.y.abs() < board.width_at(pt.pos.x) / 2.0 * 0.6 {
                assert!(pt.normal.z * up > 0.0, "{pt:?}");
            }
        }
        // Placed like the surface-splits programs, feeds in mm/min.
        let first = p.moves().nth(2).unwrap();
        assert!(first.pos.x > 900.0 && first.pos.z > 100.0);
        assert_eq!(first.feed, cfg.stringer_speed * 60.0);
    }
}

#[test]
fn hotwire_profile_and_outline() {
    let board = load("shortboard");
    let cfg = HotwireConfig {
        splits: 500,
        ..Default::default()
    };
    let p = generate_hotwire(&board, &cfg, HotwireKind::Profile).unwrap();
    let tp = &p.toolpath;
    assert_eq!(tp.passes.len(), 3);
    // Regression: `n-1.0/n` made the closing deck segment empty.
    let knots = board.deck.len() as f64;
    let closing = &tp.passes[2].points;
    assert!(closing.len() > 1);
    let expect = (500.0 / knots).floor() as usize + 1;
    assert!(
        closing.len().abs_diff(expect) <= 1,
        "{} vs {expect}",
        closing.len()
    );
    for pt in tp.points() {
        assert_eq!(pt.pos.y, 0.0);
        assert!(pt.pos.is_finite());
    }
    assert!(p.moves().all(|m| m.pos.y == 0.0 && m.feed == 180.0));
    let o = generate_hotwire(
        &board,
        &HotwireConfig {
            splits: 500,
            expand: 1.0,
            kerf: 2.0,
            ..Default::default()
        },
        HotwireKind::Outline,
    )
    .unwrap();
    let mid = board.length() / 2.0;
    let right = o.toolpath.passes[0]
        .points
        .iter()
        .min_by(|a, b| (a.pos.x - mid).abs().total_cmp(&(b.pos.x - mid).abs()))
        .unwrap();
    // Half width + expand/2 + kerf/2.
    assert!(
        (right.pos.z - (board.width_at(mid) / 2.0 + 0.5 + 0.1)).abs() < 0.01,
        "{right:?}"
    );
    let left = o.toolpath.passes[1]
        .points
        .iter()
        .min_by(|a, b| (a.pos.x - mid).abs().total_cmp(&(b.pos.x - mid).abs()))
        .unwrap();
    assert!((left.pos.z + right.pos.z).abs() < 0.01);
    let og = o.to_gcode();
    let lines = code_lines(&og);
    assert_eq!(lines[..3], ["G21", "G90", "M3"]);
    assert!(lines[3].starts_with("G01 ") && lines[3].contains(" F"));
}

#[test]
fn rotating_board_writes_continuous_a_axis_in_degrees() {
    let board = load("shortboard");
    let cfg = MachineConfig {
        cutter: Cutter::RotatingBoard(Box::new(Cutter::Bullnose(BullnoseCutter::default()))),
        holding: HoldingSystem::RotatingBoard(EndClampsConfig::default()),
        deck_cuts: 8,
        deck_rail_cuts: 4,
        ..coarse()
    };
    let p = generate_deck(&board, &cfg).unwrap();
    let a: Vec<f64> = p.moves().map(|m| m.a.unwrap()).collect();
    assert!(a.iter().all(|a| a.is_finite()));
    assert!(a.windows(2).all(|w| (w[0] - w[1]).abs() < 180.0));
    // Rail cuts reach past 90 degrees of rotation: degrees, not radians.
    assert!(a.iter().cloned().fold(0.0, f64::max) > 100.0);
    let g = p.to_gcode();
    assert_eq!(
        g.lines().filter(|l| l.contains(" A")).count(),
        p.moves().count()
    );
    // In the rotated frame the normal points up, so the tip stays above the axis-aligned contact point.
    for m in p.moves().take(200) {
        assert!(m.pos.is_finite());
    }
    // Inverse mapping for previews lands back near the board.
    let back = p.tool_path_in_board_space();
    assert!(
        back.iter()
            .all(|q| q.x > -20.0 && q.x < board.length() + 20.0)
    );
}

#[test]
fn end_clamps_keep_the_cutter_out_of_the_clamps() {
    let board = load("longboard");
    let clamps = EndClampsConfig::default();
    let cfg = MachineConfig {
        holding: HoldingSystem::EndClamps(clamps.clone()),
        deck_cuts: 6,
        deck_rail_cuts: 2,
        ..coarse()
    };
    let p = generate_deck(&board, &cfg).unwrap();
    let r = 12.7;
    let safe = clamps.clamp_safe_zone * 10.0;
    let top = clamps.clamp_height * 10.0 / 2.0 + safe;
    let side = clamps.clamp_width * 10.0 / 2.0 + safe;
    let (mut held, mut beside) = (0, 0);
    for m in p.moves() {
        assert!(m.pos.is_finite());
        // Over the tail clamp (x < safe zone edge + radius, |y| < half clamp width + safe zone + radius) the
        // cutter is above the clamp box. A tail wider than the clamp is cut beside it, at any height.
        if m.pos.x < safe + r {
            if m.pos.y.abs() < side + r {
                assert!(m.pos.z > top, "{m:?}");
                held += 1;
            } else {
                beside += 1;
            }
        }
    }
    assert!(held > 0, "the tail of the blank sits in the clamp");
    assert!(beside > 0, "the longboard tail is wider than the clamp");
    let bottom = generate_bottom(&board, &cfg).unwrap();
    assert!(bottom.moves().all(|m| m.pos.is_finite()));
}

#[test]
fn stl_cutter_bucketed_search_matches_brute_force() {
    let text = std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/cutters/bullnose.stl",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let c = StlCutter::from_ascii(&text, DVec3::ONE).unwrap();
    let (radius, z0, z1) = c.bounds();
    assert!(
        (radius - 15.5).abs() < 1e-6 && z0 == 0.0 && z1 == 10.0,
        "{radius} {z0} {z1}"
    );
    let mut seed = 12345u64;
    let mut rnd = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    for _ in 0..3000 {
        let n = DVec3::new(rnd(), rnd(), rnd() * 0.5 + 0.5).normalize();
        assert_eq!(
            c.contact_vertex(n),
            c.contact_vertex_brute_force(n),
            "{n:?}"
        );
    }
    // Flat area: contact on the bottom face, tip at the contact height.
    let tip = c.offset(DVec3::new(0.0, 0.0, 50.0), DVec3::Z);
    assert!((tip.z - 50.0).abs() < 1e-9);
    // Generate a short program with it.
    let board = load("shortboard");
    let cfg = MachineConfig {
        cutter: Cutter::Stl(Arc::new(c)),
        deck_cuts: 4,
        deck_rail_cuts: 2,
        lengthwise_resolution: 5.0,
        ..Default::default()
    };
    let p = generate_deck(&board, &cfg).unwrap();
    assert!(p.moves().all(|m| m.pos.is_finite()));
}

#[test]
fn axis_scaling_reaches_the_output() {
    let board = load("shortboard");
    let base = MachineConfig {
        deck_cuts: 2,
        deck_rail_cuts: 0,
        lengthwise_resolution: 10.0,
        ..Default::default()
    };
    let scaled = MachineConfig {
        scaling: AxisScaling {
            x: 2.0,
            ..Default::default()
        },
        ..base.clone()
    };
    let a = generate_deck(&board, &base).unwrap().to_gcode();
    let b = generate_deck(&board, &scaled).unwrap().to_gcode();
    let x = |g: &str| -> f64 {
        let l = g.lines().find(|l| l.starts_with("G01 X")).unwrap();
        l[5..].split(' ').next().unwrap().parse().unwrap()
    };
    assert!((x(&b) - 2.0 * x(&a)).abs() < 1e-3);
}

#[test]
fn invalid_input_is_rejected() {
    let cfg = MachineConfig::default();
    assert!(matches!(
        generate_deck(&Board::default(), &cfg),
        Err(Error::InvalidBoard)
    ));
    let board = load("shortboard");
    let bad = MachineConfig {
        lengthwise_resolution: 0.0,
        ..Default::default()
    };
    assert!(matches!(
        generate_deck(&board, &bad),
        Err(Error::InvalidConfig(_))
    ));
    let invalid = [
        MachineConfig {
            lengthwise_resolution: f64::NAN,
            ..coarse()
        },
        MachineConfig {
            cutting_speed: 0.0,
            ..coarse()
        },
        MachineConfig {
            rail_speed: f64::INFINITY,
            ..coarse()
        },
        MachineConfig {
            outline_speed: -1.0,
            ..coarse()
        },
        MachineConfig {
            plunge_speed_factor: 0.0,
            ..coarse()
        },
        MachineConfig {
            clear_height: f64::NAN,
            ..coarse()
        },
        MachineConfig {
            deck_angle: 150.0,
            deck_rail_angle: 130.0,
            ..coarse()
        },
        MachineConfig {
            bottom_rail_cuts: 3,
            bottom_angle: 90.0,
            bottom_rail_angle: 120.0,
            ..coarse()
        },
        MachineConfig {
            deck_angle: -5.0,
            ..coarse()
        },
        MachineConfig {
            scaling: AxisScaling {
                f: 0.0,
                ..Default::default()
            },
            ..coarse()
        },
        MachineConfig {
            cutter_offset: DVec3::new(0.0, 0.0, f64::NAN),
            ..coarse()
        },
        MachineConfig {
            generator: GeneratorKind::WidthSplits { splits: 0 },
            ..coarse()
        },
    ];
    for cfg in &invalid {
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            assert!(
                matches!(generate(&board, cfg, side), Err(Error::InvalidConfig(_))),
                "accepted {cfg:?}"
            );
        }
    }
    // Reversed rail angles do not matter without rail passes.
    let unused = MachineConfig {
        deck_rail_cuts: 0,
        deck_angle: 150.0,
        deck_rail_angle: 130.0,
        ..coarse()
    };
    assert!(generate_deck(&board, &unused).is_ok());
    let wire = HotwireConfig {
        speed: 0.0,
        ..Default::default()
    };
    assert!(matches!(
        generate_hotwire(&board, &wire, HotwireKind::Profile),
        Err(Error::InvalidConfig(_))
    ));
    let nothing = MachineConfig {
        deck_stringer_cut: false,
        deck_cuts: 0,
        deck_rail_cuts: 0,
        deck_cleanup_cuts: 0,
        deck_outline_cut: false,
        ..Default::default()
    };
    assert!(matches!(
        generate_deck(&board, &nothing),
        Err(Error::NothingToCut)
    ));
}
