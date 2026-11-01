//! Independent QA of the CAM pipeline: the G-code text is parsed back with a small parser written here, checked for
//! modal sanity, feeds, finite numbers, machine envelope, blank bottom, clamp boxes and stats, and the cut is
//! simulated coarsely against the board surface (gouge = tool body below the surface, leftover = material left
//! above the surface in the cut region).
//!
//! Run with `cargo test -p surfcraft-cam --test qa_cam -- --nocapture` to see the report tables.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use rayon::prelude::*;
use surfcraft_cam::*;
use surfcraft_core::{Board, DVec3, surface};

const NAMES: [&str; 3] = ["shortboard", "funboard", "longboard"];

fn root() -> String {
    format!("{}/../..", env!("CARGO_MANIFEST_DIR"))
}

fn load(name: &str) -> Board {
    let bytes = std::fs::read(format!("{}/tests/fixtures/{name}.brd", root())).unwrap();
    surfcraft_io::brd::read(&bytes).unwrap()
}

fn boards() -> &'static [(&'static str, Board)] {
    static B: OnceLock<Vec<(&'static str, Board)>> = OnceLock::new();
    B.get_or_init(|| NAMES.iter().map(|&n| (n, load(n))).collect())
}

fn stl_text(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/cutters/{name}", root())).unwrap()
}

// ------------------------------------------------------------------------------------------------ G-code parser

#[derive(Clone, Debug)]
struct PMove {
    /// Machine position before the move (None when unknown, e.g. after G28).
    from: Option<DVec3>,
    to: Option<DVec3>,
    a: Option<f64>,
    feed: f64,
    line: usize,
    spindle: bool,
}

#[derive(Debug, Default)]
struct Parsed {
    moves: Vec<PMove>,
    dwell: f64,
    errors: Vec<String>,
    /// Motion lines after which X, Y and Z are all known (what `Stats::points` counts).
    known_points: usize,
    m3_count: usize,
    m5_count: usize,
    lines_with_a: usize,
}

/// Tiny LinuxCNC-subset parser: G0/G1/G4/G20/G21/G28/G28.1/G90/G91, M2/M3/M5, X/Y/Z/A/F/P words, (comments).
fn parse_gcode(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut metric = false;
    let mut absolute = false;
    let mut motion: Option<u32> = None;
    let mut feed: Option<f64> = None;
    let mut pos: [Option<f64>; 3] = [None; 3];
    let mut a_pos: Option<f64> = None;
    let mut spindle = false;
    let mut ended = false;
    for (ln, raw) in text.lines().enumerate() {
        let ln = ln + 1;
        let mut code = String::new();
        let mut depth = 0;
        for ch in raw.chars() {
            match ch {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                ')' => out.errors.push(format!("line {ln}: unbalanced ')'")),
                c if depth == 0 => code.push(c),
                _ => {}
            }
        }
        if depth != 0 {
            out.errors.push(format!("line {ln}: unclosed comment"));
        }
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        if ended {
            out.errors
                .push(format!("line {ln}: code after M2: {code:?}"));
        }
        let mut gs: Vec<String> = Vec::new();
        let mut ms: Vec<String> = Vec::new();
        let mut axes: [Option<f64>; 3] = [None; 3];
        let mut a_word: Option<f64> = None;
        let mut f_word: Option<f64> = None;
        let mut p_word: Option<f64> = None;
        for tok in code.split_whitespace() {
            let (letter, num) = tok.split_at(1);
            let lower = num.to_ascii_lowercase();
            if lower.contains("nan") || lower.contains("inf") {
                out.errors.push(format!("line {ln}: non-finite {tok:?}"));
                continue;
            }
            let Ok(v) = num.parse::<f64>() else {
                out.errors.push(format!("line {ln}: bad word {tok:?}"));
                continue;
            };
            match letter.to_ascii_uppercase().as_str() {
                "G" => gs.push(num.to_string()),
                "M" => ms.push(num.to_string()),
                "X" => axes[0] = Some(v),
                "Y" => axes[1] = Some(v),
                "Z" => axes[2] = Some(v),
                "A" => a_word = Some(v),
                "F" => f_word = Some(v),
                "P" => p_word = Some(v),
                _ => out
                    .errors
                    .push(format!("line {ln}: unexpected word {tok:?}")),
            }
        }
        let mut home = false;
        let mut dwell = false;
        for g in &gs {
            let g = g.trim_start_matches('0');
            match g {
                "21" => metric = true,
                "20" => out.errors.push(format!("line {ln}: inch mode G20")),
                "90" => absolute = true,
                "91" => absolute = false,
                "1" => motion = Some(1),
                "" => motion = Some(0),
                "4" => dwell = true,
                "28" | "28.1" => home = true,
                other => out.errors.push(format!("line {ln}: unexpected G{other}")),
            }
        }
        for m in &ms {
            match m.trim_start_matches('0') {
                "3" => {
                    spindle = true;
                    out.m3_count += 1;
                }
                "5" => {
                    spindle = false;
                    out.m5_count += 1;
                }
                "2" => ended = true,
                other => out.errors.push(format!("line {ln}: unexpected M{other}")),
            }
        }
        if let Some(f) = f_word {
            if f <= 0.0 {
                out.errors.push(format!("line {ln}: feed F{f} <= 0"));
            }
            feed = Some(f);
        }
        if dwell {
            match p_word {
                Some(p) if p >= 0.0 => out.dwell += p,
                _ => out.errors.push(format!("line {ln}: G04 without P")),
            }
            continue;
        }
        if home {
            if axes.iter().any(Option::is_some) {
                out.errors.push(format!(
                    "line {ln}: G28 with axis words (intermediate point)"
                ));
            }
            pos = [None; 3];
            continue;
        }
        if axes.iter().all(Option::is_none) && a_word.is_none() {
            continue;
        }
        if !metric {
            out.errors.push(format!("line {ln}: motion before G21"));
        }
        if !absolute {
            out.errors.push(format!("line {ln}: motion not in G90"));
        }
        let Some(mode) = motion else {
            out.errors
                .push(format!("line {ln}: axis words without a motion mode"));
            continue;
        };
        let eff_feed = feed.unwrap_or(0.0);
        if mode == 1 && eff_feed <= 0.0 {
            out.errors
                .push(format!("line {ln}: G1 without an effective feed"));
        }
        let from = match pos {
            [Some(x), Some(y), Some(z)] => Some(DVec3::new(x, y, z)),
            _ => None,
        };
        for i in 0..3 {
            if axes[i].is_some() {
                pos[i] = axes[i];
            }
        }
        if a_word.is_some() {
            a_pos = a_word;
            out.lines_with_a += 1;
        }
        let to = match pos {
            [Some(x), Some(y), Some(z)] => Some(DVec3::new(x, y, z)),
            _ => None,
        };
        if to.is_some() {
            out.known_points += 1;
        }
        out.moves.push(PMove {
            from,
            to,
            a: a_pos,
            feed: eff_feed,
            line: ln,
            spindle,
        });
    }
    if !ended {
        out.errors.push("program does not end with M2".into());
    }
    if spindle {
        out.errors.push("spindle still on at the end".into());
    }
    out
}

#[test]
fn parser_self_test() {
    let p = parse_gcode("(c)\nG21\nG90\nM3\nG04 P2\nG01 X1 Y2 Z3 F100\nX2\nM5\nG01 Z50\nM2\n");
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(!p.moves[2].spindle);
    assert_eq!(p.moves.len(), 3);
    assert_eq!(p.dwell, 2.0);
    let bad = parse_gcode("G21\nG90\nM3\nG01 X1 Y2 Z3\nXNaN\nM5\nM2\n");
    assert!(bad.errors.iter().any(|e| e.contains("effective feed")));
    assert!(bad.errors.iter().any(|e| e.contains("non-finite")));
}

// ------------------------------------------------------------------------------------------------ geometry helpers

/// Rotation about X (vecmath rotX).
fn rot_x(angle: f64, p: DVec3) -> DVec3 {
    let (s, c) = angle.sin_cos();
    DVec3::new(p.x, c * p.y - s * p.z, s * p.y + c * p.z)
}

/// Machine position (mm, A degrees) to board space (mm), and the tool axis direction in board space.
fn to_board(prog: &Program, pos: DVec3, a: Option<f64>) -> (DVec3, DVec3) {
    let mut p = pos + prog.cutter_offset;
    let mut axis = DVec3::Z;
    if let Some(a) = a {
        p = rot_x(-a.to_radians(), p);
        axis = rot_x(-a.to_radians(), axis);
    }
    let pl = &prog.placement;
    let b = pl.inverse_point(p) * 10.0;
    let o = pl.inverse_point(DVec3::ZERO);
    let u = (pl.inverse_point(axis * 10.0) - o).normalize();
    (b, u)
}

/// Lower envelope of a rotationally symmetric cutter: tip-relative height of the tool bottom at radius d (mm).
#[derive(Clone, Debug)]
struct ToolShape {
    radius: f64,
    height: f64,
    /// (d, z) samples, d increasing, for table shapes.
    table: Vec<(f64, f64)>,
    /// (R, r) of a bullnose.
    bull: Option<(f64, f64)>,
}

impl ToolShape {
    fn bullnose(c: &BullnoseCutter) -> Self {
        ToolShape {
            radius: c.diameter * 5.0,
            height: c.height * 10.0,
            table: vec![],
            bull: Some((c.diameter * 5.0, c.corner_radius * 10.0)),
        }
    }

    fn flat(c: &FlatCutter) -> Self {
        ToolShape {
            radius: c.radius * 10.0,
            height: c.height * 10.0,
            table: vec![(0.0, 0.0), (c.radius * 10.0, 0.0)],
            bull: None,
        }
    }

    /// From an ASCII STL (mm, tip at origin, axis +Z): sample every facet, keep the lowest z per radius bin.
    fn stl(text: &str) -> Self {
        let mut verts: Vec<DVec3> = Vec::new();
        for l in text.lines() {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w.first() == Some(&"vertex") {
                verts.push(DVec3::new(
                    w[1].parse().unwrap(),
                    w[2].parse().unwrap(),
                    w[3].parse().unwrap(),
                ));
            }
        }
        let radius = verts.iter().map(|v| v.x.hypot(v.y)).fold(0.0, f64::max);
        let zmax = verts.iter().map(|v| v.z).fold(f64::MIN, f64::max);
        let bins = 200usize;
        let mut low = vec![f64::MAX; bins + 1];
        for t in verts.chunks(3) {
            if t.len() < 3 {
                continue;
            }
            let n = 12;
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
                    let p = t[0] + (t[1] - t[0]) * u + (t[2] - t[0]) * v;
                    let d = p.x.hypot(p.y);
                    let b = (((d / radius) * bins as f64).round() as usize).min(bins);
                    low[b] = low[b].min(p.z);
                }
            }
        }
        let mut table = Vec::new();
        let mut last = low.iter().cloned().find(|z| *z < f64::MAX).unwrap_or(0.0);
        let mut running = f64::MIN;
        for (b, z) in low.iter().enumerate() {
            let z = if *z < f64::MAX { *z } else { last };
            last = z;
            // Convex cutter: the envelope only rises outward.
            running = running.max(z);
            table.push((b as f64 / bins as f64 * radius, running));
        }
        ToolShape {
            radius,
            height: zmax,
            table,
            bull: None,
        }
    }

    fn of(c: &Cutter) -> Self {
        match c {
            Cutter::Bullnose(b) => Self::bullnose(b),
            Cutter::Flat(f) => Self::flat(f),
            Cutter::Stl(_) => Self::stl(&stl_text("bullnose.stl")),
            Cutter::RotatingBoard(inner) => Self::of(inner),
        }
    }

    /// Radius of the tool body at height `a` above the tip.
    fn radius_at(&self, a: f64) -> f64 {
        if let Some((rr, r)) = self.bull {
            if a >= r {
                return rr;
            }
            let q = r * r - (r - a) * (r - a);
            return rr - r + q.max(0.0).sqrt();
        }
        let i = self.table.partition_point(|e| e.1 <= a);
        if i == 0 {
            0.0
        } else if i >= self.table.len() {
            self.radius
        } else {
            self.table[i - 1].0
        }
    }

    fn zb(&self, d: f64) -> Option<f64> {
        if d > self.radius {
            return None;
        }
        if let Some((rr, r)) = self.bull {
            let f = rr - r;
            if d <= f {
                return Some(0.0);
            }
            let q = r * r - (d - f) * (d - f);
            return Some(r - q.max(0.0).sqrt());
        }
        let i = self.table.partition_point(|e| e.0 < d);
        if i == 0 {
            return Some(self.table[0].1);
        }
        if i >= self.table.len() {
            return Some(self.table.last().unwrap().1);
        }
        let (d0, z0) = self.table[i - 1];
        let (d1, z1) = self.table[i];
        Some(z0 + (z1 - z0) * (d - d0) / (d1 - d0).max(1e-12))
    }
}

/// Board surface samples in board space (mm), bucketed on a 3D grid.
struct Cloud {
    pts: Vec<DVec3>,
    /// True for the deck region (centre to rail apex), false for the bottom region (apex to centre).
    deck: Vec<bool>,
    lo: DVec3,
    cell: f64,
    dims: [usize; 3],
    start: Vec<u32>,
    idx: Vec<u32>,
}

impl Cloud {
    fn new(board: &Board) -> Self {
        let len = board.length();
        let step = 0.5; // cm along the board
        let n = 96; // samples per half section and region
        let mut xs: Vec<f64> = (0..)
            .map(|i| 0.1 + i as f64 * step)
            .take_while(|&x| x <= len - 0.1)
            .collect();
        xs.push(len - 0.1);
        let rows: Vec<Vec<(DVec3, bool)>> = xs
            .par_iter()
            .map(|&x| {
                let mut v = Vec::new();
                for (range, deck) in [(surface::DECK, true), (surface::BOTTOM, false)] {
                    for i in 0..=n {
                        let s = i as f64 / n as f64;
                        let p = board.surface_point(x, s, range.0, range.1) * 10.0;
                        if p.is_finite() {
                            v.push((p, deck));
                            if p.y.abs() > 1e-6 {
                                v.push((DVec3::new(p.x, -p.y, p.z), deck));
                            }
                        }
                    }
                }
                v
            })
            .collect();
        let mut pts = Vec::new();
        let mut deck = Vec::new();
        for r in rows {
            for (p, d) in r {
                pts.push(p);
                deck.push(d);
            }
        }
        let min = pts.iter().fold(DVec3::splat(f64::MAX), |a, p| a.min(*p));
        let max = pts.iter().fold(DVec3::splat(f64::MIN), |a, p| a.max(*p));
        let lo = min - DVec3::splat(1.0);
        let hi = max + DVec3::splat(1.0);
        let cell = 8.0;
        let dims = [
            ((hi.x - lo.x) / cell).ceil() as usize + 1,
            ((hi.y - lo.y) / cell).ceil() as usize + 1,
            ((hi.z - lo.z) / cell).ceil() as usize + 1,
        ];
        let ncell = dims[0] * dims[1] * dims[2];
        let cid = |p: &DVec3| {
            let i = ((p.x - lo.x) / cell) as usize;
            let j = ((p.y - lo.y) / cell) as usize;
            let k = ((p.z - lo.z) / cell) as usize;
            (i * dims[1] + j) * dims[2] + k
        };
        let mut start = vec![0u32; ncell + 1];
        for p in &pts {
            start[cid(p) + 1] += 1;
        }
        for i in 0..ncell {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut idx = vec![0u32; pts.len()];
        for (n, p) in pts.iter().enumerate() {
            let c = cid(p);
            idx[fill[c] as usize] = n as u32;
            fill[c] += 1;
        }
        Cloud {
            pts,
            deck,
            lo,
            cell,
            dims,
            start,
            idx,
        }
    }

    fn range(&self, lo: f64, hi: f64, origin: f64, n: usize) -> Option<(usize, usize)> {
        let a = ((lo - origin) / self.cell).floor();
        let b = ((hi - origin) / self.cell).floor();
        if b < 0.0 || a > (n - 1) as f64 {
            return None;
        }
        Some((a.max(0.0) as usize, (b as usize).min(n - 1)))
    }

    /// Visit the points in cells overlapping the box.
    fn visit(&self, a: DVec3, b: DVec3, mut f: impl FnMut(usize)) {
        let lo = a.min(b);
        let hi = a.max(b);
        let Some((i0, i1)) = self.range(lo.x, hi.x, self.lo.x, self.dims[0]) else {
            return;
        };
        let Some((j0, j1)) = self.range(lo.y, hi.y, self.lo.y, self.dims[1]) else {
            return;
        };
        let Some((k0, k1)) = self.range(lo.z, hi.z, self.lo.z, self.dims[2]) else {
            return;
        };
        for i in i0..=i1 {
            for j in j0..=j1 {
                let base = (i * self.dims[1] + j) * self.dims[2];
                let s = self.start[base + k0] as usize;
                let e = self.start[base + k1 + 1] as usize;
                for &n in &self.idx[s..e] {
                    f(n as usize);
                }
            }
        }
    }
}

fn clouds() -> &'static [Cloud] {
    static C: OnceLock<Vec<Cloud>> = OnceLock::new();
    C.get_or_init(|| boards().iter().map(|(_, b)| Cloud::new(b)).collect())
}

/// Tool samples in board space along the program (mm), with the tool axis.
fn tool_samples(prog: &Program, step: f64) -> Vec<(DVec3, DVec3, u32)> {
    let moves: Vec<&Move> = prog.moves().collect();
    let mut out = Vec::new();
    for (i, m) in moves.iter().enumerate() {
        if i == 0 {
            let (p, u) = to_board(prog, m.pos, m.a);
            out.push((p, u, 0));
            continue;
        }
        let p = moves[i - 1];
        let d = p.pos.distance(m.pos);
        let da = match (p.a, m.a) {
            (Some(x), Some(y)) => (y - x).abs(),
            _ => 0.0,
        };
        // Rotations sweep the tool too: about 3 mm per degree at the board's radius.
        let n = ((d.max(da * 3.0)) / step).ceil().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let pos = p.pos.lerp(m.pos, t);
            let a = match (p.a, m.a) {
                (Some(x), Some(y)) => Some(x + (y - x) * t),
                _ => m.a,
            };
            let (b, u) = to_board(prog, pos, a);
            out.push((b, u, i as u32));
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
struct Sim {
    /// Largest penetration of the tool body below the surface, mm, and where (board space, mm).
    gouge: f64,
    gouge_at: DVec3,
    gouged_points_05: usize,
    gouged_points_01: usize,
    /// Largest gouge per pass kind of the move that made it.
    gouge_by_kind: Vec<(String, f64, DVec3)>,
    /// Largest axial distance between the surface and the nearest tool bottom in the cut region, mm.
    leftover_max: f64,
    leftover_p95: f64,
    leftover_at: DVec3,
    /// Points in the cut region the tool never passed over.
    uncovered: usize,
    region_points: usize,
}

/// Coarse cut simulation: for each tool sample, every surface point inside the tool body (radius, cutting height)
/// gets a clearance `bottom(d) - a`; negative clearance is a gouge. `region_deck` picks the region whose leftover
/// is measured (deck program: the deck; bottom program: the bottom), excluding the stringer strip and the ends.
fn simulate(
    prog: &Program,
    board: &Board,
    cloud: &Cloud,
    shape: &ToolShape,
    region_deck: bool,
) -> Sim {
    let step = std::env::var("QA_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let samples = tool_samples(prog, step);
    let n = cloud.pts.len();
    let chunks: Vec<Vec<(f64, u32)>> = samples
        .par_chunks(4096)
        .map(|chunk| {
            let mut clear = vec![(f64::INFINITY, 0u32); n];
            for &(tip, u, src) in chunk {
                let top = tip + u * shape.height;
                let r = DVec3::splat(shape.radius);
                cloud.visit(tip.min(top) - r, tip.max(top) + r, |i| {
                    let v = cloud.pts[i] - tip;
                    let a = v.dot(u);
                    if a < -shape.radius || a > shape.height {
                        return;
                    }
                    let d = (v - u * a).length();
                    if let Some(zb) = shape.zb(d) {
                        // Inside the tool: penetration is the shorter way out (down, or sideways out of the
                        // body), so a point grazing the side of the shank is not reported as an axial gouge.
                        let c = if a > zb {
                            -(a - zb).min(shape.radius_at(a) - d)
                        } else {
                            zb - a
                        };
                        if c < clear[i].0 {
                            clear[i] = (c, src);
                        }
                    }
                });
            }
            clear
        })
        .collect();
    let mut clear = vec![(f64::INFINITY, 0u32); n];
    for c in chunks {
        for (a, b) in clear.iter_mut().zip(c) {
            if b.0 < a.0 {
                *a = b;
            }
        }
    }
    let moves: Vec<&Move> = prog.moves().collect();
    let kind_of = |src: u32| -> String {
        moves[src as usize]
            .pass
            .map(|p| format!("{:?}", prog.toolpath.passes[p].kind))
            .unwrap_or_else(|| "none".into())
    };
    let mut s = Sim::default();
    let len = board.length() * 10.0;
    let mut left = Vec::new();
    for (i, &(c, src)) in clear.iter().enumerate() {
        let p = cloud.pts[i];
        if c < 0.0 {
            if -c > s.gouge {
                s.gouge = -c;
                s.gouge_at = p;
            }
            if -c > 0.1 {
                let k = kind_of(src);
                match s.gouge_by_kind.iter_mut().find(|e| e.0 == k) {
                    Some(e) => {
                        if -c > e.1 {
                            e.1 = -c;
                            e.2 = p;
                        }
                    }
                    None => s.gouge_by_kind.push((k, -c, p)),
                }
            }
        }
        if c < -0.5 {
            s.gouged_points_05 += 1;
        }
        if c < -0.1 {
            s.gouged_points_01 += 1;
        }
        let in_region = cloud.deck[i] == region_deck
            && p.y.abs() >= 20.0
            && p.x >= 50.0
            && p.x <= len - 50.0
            // Away from the rail apex, where deck and bottom regions meet.
            && p.y.abs() <= board.width_at(p.x / 10.0) * 5.0 - 10.0;
        if in_region {
            s.region_points += 1;
            if c.is_finite() {
                left.push((c.max(0.0), p));
            } else {
                s.uncovered += 1;
            }
        }
    }
    s.gouge_by_kind.sort_by(|a, b| b.1.total_cmp(&a.1));
    left.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(&(m, p)) = left.last() {
        s.leftover_max = m;
        s.leftover_at = p;
        s.leftover_p95 = left[(left.len() as f64 * 0.95) as usize].0;
    }
    s
}

// ------------------------------------------------------------------------------------------------ program checks

#[derive(Debug, Default, Clone)]
struct Check {
    errors: Vec<String>,
    moves: usize,
    /// Lowest tool tip z minus the lowest board z in machine space, mm (negative = below the blank).
    below_blank: f64,
    clamp_hits: usize,
    cut_length: f64,
}

/// End-clamp boxes in machine space (mm), without the safe zone: (tail, nose).
fn clamp_boxes(c: &EndClampsConfig, blank: &Board) -> [(DVec3, DVec3); 2] {
    let tail_max = blank.max_deck_at_tail();
    let nose_max = blank.max_deck_at_nose();
    let l = blank.length();
    let tail_bottom = blank.bottom_at_xy(0.0, 0.0);
    let nose_bottom = blank.bottom_at_xy(l - surfcraft_core::math::ZERO, 0.0);
    let rot = (nose_max.y - tail_max.y).atan2(nose_max.x - tail_max.x);
    let rotated_length = rot.sin() * (nose_bottom - tail_bottom) + rot.cos() * l;
    let len = c.clamp_length * 10.0;
    let h = c.clamp_height * 10.0;
    let w = c.clamp_width * 10.0;
    let far = rotated_length * 10.0 - 2.0 * c.clamp_overlap * 10.0;
    [
        (
            DVec3::new(-len, -w / 2.0, -h / 2.0),
            DVec3::new(0.0, w / 2.0, h / 2.0),
        ),
        (
            DVec3::new(far, -w / 2.0, -h / 2.0),
            DVec3::new(far + len, w / 2.0, h / 2.0),
        ),
    ]
}

fn cylinder_hits_box(tip: DVec3, r: f64, h: f64, b: &(DVec3, DVec3)) -> bool {
    if tip.z + h < b.0.z || tip.z > b.1.z {
        return false;
    }
    let dx = tip.x - tip.x.clamp(b.0.x, b.1.x);
    let dy = tip.y - tip.y.clamp(b.0.y, b.1.y);
    // Strictly inside: touching the face is not a collision.
    dx * dx + dy * dy < r * r - 1e-6
}

fn check_program(
    board: &Board,
    cloud: &Cloud,
    cfg: Option<&MachineConfig>,
    prog: &Program,
) -> Check {
    let text = prog.to_gcode();
    let parsed = parse_gcode(&text);
    let mut c = Check {
        errors: parsed.errors.clone(),
        moves: parsed.moves.len(),
        ..Default::default()
    };
    if parsed.moves.is_empty() {
        c.errors.push("no moves".into());
        return c;
    }
    if parsed.m3_count != 1 || parsed.m5_count != 1 {
        c.errors.push(format!(
            "M3 x{} / M5 x{} (expected one each)",
            parsed.m3_count, parsed.m5_count
        ));
    }
    let stats = prog.stats();
    // Stats against the parsed text.
    let mut len = 0.0;
    let mut time = parsed.dwell;
    for m in &parsed.moves {
        if let (Some(f), Some(t)) = (m.from, m.to) {
            let d = f.distance(t);
            len += d;
            time += d / (m.feed / 60.0);
        }
    }
    c.cut_length = len;
    let tol = 1e-6 * len + 2e-4 * parsed.moves.len() as f64;
    if (len - stats.cut_length).abs() > tol {
        c.errors.push(format!(
            "cut length {len:.3} mm from G-code vs stats {:.3}",
            stats.cut_length
        ));
    }
    if (time - stats.estimated_time).abs() > 1e-3 * time.max(1.0) {
        c.errors.push(format!(
            "time {time:.1} s from G-code vs stats {:.1}",
            stats.estimated_time
        ));
    }
    if parsed.known_points != stats.points {
        c.errors.push(format!(
            "{} motion lines with a known position vs stats.points {}",
            parsed.known_points, stats.points
        ));
    }
    // Envelope and blank bottom, machine space.
    let rotating = parsed.lines_with_a > 0;
    let (mut bmin, mut bmax) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for p in cloud.pts.iter().step_by(7) {
        let m = prog.placement.point(*p / 10.0) - prog.cutter_offset;
        bmin = bmin.min(m);
        bmax = bmax.max(m);
    }
    let mut lowest = f64::MAX;
    for m in &parsed.moves {
        let Some(t) = m.to else { continue };
        lowest = lowest.min(t.z);
        if !m.spindle && !rotating && t.z < bmax.z {
            c.errors.push(format!(
                "line {}: move into the blank with the spindle off",
                m.line
            ));
        }
        let slack = 400.0;
        if rotating {
            let r = t.y.hypot(t.z);
            let rmax = bmin
                .y
                .abs()
                .max(bmax.y.abs())
                .hypot(bmin.z.abs().max(bmax.z.abs()));
            if r > rmax + slack || t.x < bmin.x - slack || t.x > bmax.x + slack {
                c.errors
                    .push(format!("line {}: {t:?} outside the envelope", m.line));
            }
        } else if t.x < bmin.x - slack
            || t.x > bmax.x + slack
            || t.y < bmin.y - 100.0
            || t.y > bmax.y + 100.0
            || t.z > bmax.z + 300.0
        {
            c.errors.push(format!(
                "line {}: {t:?} outside the envelope {bmin:?}..{bmax:?}",
                m.line
            ));
        }
    }
    c.below_blank = lowest - bmin.z;
    if rotating {
        c.below_blank = 0.0;
        if parsed.lines_with_a != parsed.moves.len() {
            c.errors.push(format!(
                "A written on {} of {} motion lines",
                parsed.lines_with_a,
                parsed.moves.len()
            ));
        }
        // Degrees, not radians: the rail passes rotate the board past 60 degrees.
        let amax = parsed
            .moves
            .iter()
            .filter_map(|m| m.a)
            .fold(0.0f64, |m, v| m.max(v.abs()));
        let has_rails = cfg.is_some_and(|c| {
            matches!(c.generator, GeneratorKind::SurfaceSplits)
                && (c.deck_rail_cuts > 0 || c.bottom_cuts > 0)
        });
        if has_rails && amax < 60.0 {
            c.errors
                .push(format!("A axis max {amax}: radians, not degrees?"));
        }
        // A rotation of more than 45 degrees in one linear move sweeps the tool through the board unless the
        // tool is clear of it.
        for w in parsed.moves.windows(2) {
            if let (Some(a0), Some(a1), Some(t)) = (w[0].a, w[1].a, w[1].to)
                && (a1 - a0).abs() > 45.0
            {
                c.errors.push(format!(
                    "line {}: A jumps {a0} -> {a1} in one move (to {t:?})",
                    w[1].line
                ));
                break;
            }
        }
    } else if parsed.lines_with_a > 0 {
        c.errors.push("A axis on a 3-axis program".into());
    }
    // Clamp boxes.
    if let Some(cfg) = cfg
        && let HoldingSystem::EndClamps(ec) | HoldingSystem::RotatingBoard(ec) = &cfg.holding
    {
        let boxes = clamp_boxes(ec, cfg.blank.as_ref().unwrap_or(board));
        let (r, h) = match &cfg.cutter {
            Cutter::Bullnose(b) => (b.diameter * 5.0, b.height * 10.0),
            Cutter::Flat(f) => (f.radius * 10.0, f.height * 10.0),
            Cutter::Stl(s) => (s.bounds().0, s.bounds().2),
            Cutter::RotatingBoard(inner) => match inner.as_ref() {
                Cutter::Bullnose(b) => (b.diameter * 5.0, b.height * 10.0),
                _ => (10.0, 65.0),
            },
        };
        for m in &parsed.moves {
            let (Some(f), Some(t)) = (m.from, m.to) else {
                continue;
            };
            let n = (f.distance(t) / 2.0).ceil().max(1.0) as usize;
            let mut hit = false;
            for k in 0..=n {
                let p = f.lerp(t, k as f64 / n as f64);
                if boxes.iter().any(|b| cylinder_hits_box(p, r, h, b)) {
                    hit = true;
                }
            }
            if hit {
                c.clamp_hits += 1;
                if c.clamp_hits <= 3 {
                    c.errors.push(format!(
                        "line {}: move {f:?} -> {t:?} passes through a clamp",
                        m.line
                    ));
                }
            }
        }
    }
    c
}

// ------------------------------------------------------------------------------------------------ configurations

fn cutters() -> Vec<(&'static str, Cutter)> {
    let stl = Arc::new(StlCutter::from_ascii(&stl_text("bullnose.stl"), DVec3::ONE).unwrap());
    vec![
        ("bullnose", Cutter::Bullnose(BullnoseCutter::default())),
        ("flat", Cutter::Flat(FlatCutter::default())),
        ("stl-bullnose", Cutter::Stl(stl)),
        (
            "rotating",
            Cutter::RotatingBoard(Box::new(Cutter::Bullnose(BullnoseCutter::default()))),
        ),
    ]
}

fn holdings(cutter: &Cutter) -> Vec<(&'static str, HoldingSystem)> {
    if matches!(cutter, Cutter::RotatingBoard(_)) {
        vec![(
            "rot-clamps",
            HoldingSystem::RotatingBoard(EndClampsConfig::default()),
        )]
    } else {
        vec![
            (
                "supports",
                HoldingSystem::Supports(SupportsConfig::default()),
            ),
            (
                "end-clamps",
                HoldingSystem::EndClamps(EndClampsConfig::default()),
            ),
        ]
    }
}

/// Every generator x cutter x holding x board x side at a coarse step: G-code sanity checks.
#[test]
fn qa_every_combination_parses_and_is_sane() {
    let mut failures = Vec::new();
    let mut rows = Vec::new();
    for (bi, (bname, board)) in boards().iter().enumerate() {
        let cloud = &clouds()[bi];
        for (cname, cutter) in cutters() {
            for (hname, holding) in holdings(&cutter) {
                for (gname, generator) in [
                    ("surface", GeneratorKind::SurfaceSplits),
                    ("width", GeneratorKind::WidthSplits { splits: 12 }),
                ] {
                    for side in [BoardSide::Deck, BoardSide::Bottom] {
                        let cfg = MachineConfig {
                            generator: generator.clone(),
                            cutter: cutter.clone(),
                            holding: holding.clone(),
                            lengthwise_resolution: 2.0,
                            deck_cuts: 10,
                            deck_rail_cuts: 4,
                            bottom_cuts: 10,
                            ..Default::default()
                        };
                        let id = format!("{bname}/{cname}/{hname}/{gname}/{side:?}");
                        let t = Instant::now();
                        match generate(board, &cfg, side) {
                            Err(e) => failures.push(format!("{id}: generate failed: {e}")),
                            Ok(p) => {
                                let c = check_program(board, cloud, Some(&cfg), &p);
                                rows.push(format!(
                                    "{id:<52} moves {:>6} below-blank {:>7.1} mm clamp-hits {:>4} errors {:>3} ({:.0} ms)",
                                    c.moves,
                                    c.below_blank,
                                    c.clamp_hits,
                                    c.errors.len(),
                                    t.elapsed().as_secs_f64() * 1000.0
                                ));
                                for e in c.errors.iter().take(3) {
                                    failures.push(format!("{id}: {e}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        for kind in [HotwireKind::Profile, HotwireKind::Outline] {
            for begin_at_nose in [true, false] {
                let cfg = HotwireConfig {
                    splits: 800,
                    begin_at_nose,
                    kerf: 1.0,
                    expand: 0.5,
                    ..Default::default()
                };
                let id = format!("{bname}/hotwire/{kind:?}/nose={begin_at_nose}");
                match generate_hotwire(board, &cfg, kind) {
                    Err(e) => failures.push(format!("{id}: {e}")),
                    Ok(p) => {
                        let parsed = parse_gcode(&p.to_gcode());
                        for e in parsed.errors.iter().take(3) {
                            failures.push(format!("{id}: {e}"));
                        }
                        let s = p.stats();
                        let len: f64 = parsed
                            .moves
                            .iter()
                            .filter_map(|m| Some(m.from?.distance(m.to?)))
                            .sum();
                        if (len - s.cut_length).abs() > 1e-3 * len {
                            failures.push(format!("{id}: length {len} vs stats {}", s.cut_length));
                        }
                        // The wire follows the curve: board-space points within kerf/2 + expand of the curve.
                        let back = p.tool_path_in_board_space();
                        let mut worst: f64 = 0.0;
                        for q in &back {
                            let x = q.x.clamp(0.0, board.length());
                            let dev = match kind {
                                HotwireKind::Profile => {
                                    let d1 = (q.z - board.deck_at(x)).abs();
                                    let d2 = (q.z - board.rocker_at(x)).abs();
                                    d1.min(d2)
                                }
                                HotwireKind::Outline => (q.z.abs() - board.width_at(x) / 2.0).abs(),
                            };
                            if x > 2.0 && x < board.length() - 2.0 {
                                worst = worst.max(dev);
                            }
                        }
                        // expand (0.5 cm on the deck / 0.25 cm per side) + kerf/2 (0.05 cm) + slack.
                        if worst > 0.5 + 0.05 + 0.05 {
                            failures.push(format!("{id}: wire leaves the curve by {worst:.3} cm"));
                        }
                        rows.push(format!(
                            "{id:<52} moves {:>6} max deviation from curve {:.3} cm",
                            parsed.moves.len(),
                            worst
                        ));
                    }
                }
            }
        }
    }
    println!("{}", rows.join("\n"));
    // Rotating-board A sweeps and clamp hits used to be known failures here (see the BUG tests below); they are
    // fixed and checked like everything else.
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Gouge and leftover with default settings (resolution 1 cm, default cutter and holding), plus each cutter.
#[test]
fn qa_gouge_and_leftover_report() {
    let mut rows = Vec::new();
    let mut gouges = Vec::new();
    for (bi, (bname, board)) in boards().iter().enumerate() {
        let cloud = &clouds()[bi];
        let mut configs: Vec<(String, MachineConfig)> =
            vec![("default".into(), MachineConfig::default())];
        for (cname, cutter) in cutters().into_iter().skip(1) {
            let holding = holdings(&cutter)[0].1.clone();
            configs.push((
                cname.to_string(),
                MachineConfig {
                    cutter,
                    holding,
                    lengthwise_resolution: 2.0,
                    ..Default::default()
                },
            ));
        }
        configs.push((
            "width-splits-30".into(),
            MachineConfig {
                generator: GeneratorKind::WidthSplits { splits: 30 },
                lengthwise_resolution: 2.0,
                ..Default::default()
            },
        ));
        configs.push((
            "no-stringer-avoid".into(),
            MachineConfig {
                stay_away_from_stringer: false,
                lengthwise_resolution: 2.0,
                ..Default::default()
            },
        ));
        for (cname, cfg) in &configs {
            for side in [BoardSide::Deck, BoardSide::Bottom] {
                let t = Instant::now();
                let p = generate(board, cfg, side).unwrap();
                let gen_ms = t.elapsed().as_secs_f64() * 1000.0;
                let shape = ToolShape::of(&cfg.cutter);
                let s = simulate(&p, board, cloud, &shape, side == BoardSide::Deck);
                let st = p.stats();
                rows.push(format!(
                    "{bname:<10} {cname:<18} {:<6} gouge {:>6.2} mm at ({:>6.0},{:>5.0},{:>5.0}) n>0.1:{:>5} n>0.5:{:>5} | leftover max {:>5.2} p95 {:>5.2} mm uncovered {:>4}/{:<5} | {:>6} moves {:>6.1} m {:>5.0} min gen {:.0} ms",
                    format!("{side:?}"),
                    s.gouge,
                    s.gouge_at.x,
                    s.gouge_at.y,
                    s.gouge_at.z,
                    s.gouged_points_01,
                    s.gouged_points_05,
                    s.leftover_max,
                    s.leftover_p95,
                    s.uncovered,
                    s.region_points,
                    st.points,
                    st.cut_length / 1000.0,
                    st.estimated_time / 60.0,
                    gen_ms,
                ));
                rows.push(format!(
                    "{:>40} by pass kind: {}",
                    "",
                    s.gouge_by_kind
                        .iter()
                        .map(|(k, g, at)| format!(
                            "{k} {g:.1} at ({:.0},{:.0},{:.0})",
                            at.x, at.y, at.z
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                gouges.push((format!("{bname}/{cname}/{side:?}"), s));
            }
        }
    }
    println!("{}", rows.join("\n"));
    // Report, with a guard against regressions. The rotating board is included now that its BUG tests pass, and
    // the limit is down from 15 mm: after the fixes the largest gouge here is ~7.0 mm (width splits, which were
    // not reworked, at the tail tip); surface splits stay below ~4.9 mm (STL cutter at the nose end face,
    // x within 1 mm of the tip), except the rotating board at the tail clamp edge (6.1 mm on the longboard, see
    // `qa_bug_rotating_hold_height_below_wide_tail`).
    let bad: Vec<String> = gouges
        .iter()
        .filter(|(_, s)| s.gouge > 8.0)
        .map(|(n, s)| format!("{n}: {:.2} mm at {:?}", s.gouge, s.gouge_at))
        .collect();
    assert!(bad.is_empty(), "programs gouge:\n{}", bad.join("\n"));
}

fn gouge_of(board_index: usize, cfg: &MachineConfig, side: BoardSide) -> Sim {
    let (_, board) = &boards()[board_index];
    let p = generate(board, cfg, side).unwrap();
    simulate(
        &p,
        board,
        &clouds()[board_index],
        &ToolShape::of(&cfg.cutter),
        side == BoardSide::Deck,
    )
}

/// The deck program's rail passes ran to 130 degrees, past the rail apex into the tuck. A vertical 3-axis tool
/// cannot touch a surface whose normal points down without its shank cutting the apex above it: the default deck
/// program gouged the rail apex by 4-5 mm on all three fixture boards.
/// Fixed: 3-axis cutters stop the deck passes at the apex (90 degrees); the bottom program cuts the tuck.
#[test]
fn qa_bug_deck_rail_passes_gouge_the_apex() {
    for bi in 0..3 {
        let s = gouge_of(bi, &MachineConfig::default(), BoardSide::Deck);
        let rail = s
            .gouge_by_kind
            .iter()
            .find(|k| k.0 == "Rail")
            .map_or(0.0, |k| k.1);
        assert!(
            rail < 1.0,
            "{}: rail passes gouge {rail:.2} mm",
            boards()[bi].0
        );
    }
}

/// The bottom program spaced its passes by cross-section normal angle from 360 (bottom centre) to 90 (apex). On a
/// flat bottom every point has a 360 degree normal, so the start of the range landed at the edge of the flat part:
/// on the funboard about 22 % of the bottom was never touched, and the S-blend between sections parametrised over
/// (360, 90) is a different surface from the modelled one (3D view, STL: (-360, 360)) - up to ~9 mm off at
/// x = 52.6 cm - so the hard rail edge was gouged by ~11 mm.
///
/// Fixed: passes lie on the modelled surface, spaced by arc length from the bottom centre. (The simulator's
/// reference surface, the (90, 360) bottom range, itself differed from the modelled (-360, 360) surface by up to
/// 0.65 mm on the funboard fixture of the time, near the tail.)
#[test]
fn qa_bug_flat_bottom_is_not_covered() {
    let s = gouge_of(1, &MachineConfig::default(), BoardSide::Bottom);
    assert_eq!(
        s.uncovered, 0,
        "{} of {} bottom points never cut",
        s.uncovered, s.region_points
    );
}

/// The bottom program of the funboard must not gouge (the hard rail edge was gouged by ~11 mm, see
/// `qa_bug_flat_bottom_is_not_covered`).
///
/// Open: the current funboard has a crease at its hard bottom edge (the section edge knot has a kink). A regular
/// lengthwise pass at a constant arc-length fraction crosses that crease between two stations: pass 23 touches the
/// rail side at x = 76 cm (y = 26.24 cm, normal tilted 33 degrees) and the flat side at x = 77 cm (y = 26.03 cm,
/// normal vertical). The straight move between them takes the flat face of the cutter into the bottom near
/// (76.5, 26.1) cm: 1.01 mm with the default 2 mm simulation step, 1.07 mm at 1 mm. The contact normals are exact
/// and the reference surface agrees with the modelled one there to 0.02 mm, so this is not a simulator artefact.
#[test]
#[ignore = "product bug: ~1 mm gouge where a bottom pass crosses the funboard's hard-edge crease between stations"]
fn qa_bug_bottom_pass_crossing_hard_edge_gouges() {
    let s = gouge_of(1, &MachineConfig::default(), BoardSide::Bottom);
    assert!(s.gouge < 1.0, "gouge {:.2} mm at {:?}", s.gouge, s.gouge_at);
}

fn rotating_cfg() -> MachineConfig {
    MachineConfig {
        cutter: Cutter::RotatingBoard(Box::new(Cutter::Bullnose(BullnoseCutter::default()))),
        holding: HoldingSystem::RotatingBoard(EndClampsConfig::default()),
        lengthwise_resolution: 2.0,
        ..Default::default()
    }
}

/// `ClampGuard::process` builds the hold point at the safe height in machine coordinates and passes it through
/// `Cutter::tip`, which for `Cutter::RotatingBoard` rotates it by the A angle a second time. On the longboard
/// bottom program the "lift" goes from Z151.8 down to Z-127.3 next to the nose clamp; on every board the held
/// positions near the tail clamp sit inside the board (~12 mm gouges at x = 13-15 cm, even without
/// interpolating moves).
/// Fixed: the hold point is offset by the inner cutter only, at the current A; the rotating clamps are guarded
/// as the square around the circle they sweep.
#[test]
fn qa_bug_rotating_clamp_hold_point() {
    let cfg = rotating_cfg();
    let clamps = EndClampsConfig::default();
    let top = clamps.clamp_height * 10.0 / 2.0 + clamps.clamp_safe_zone * 10.0;
    // The rotating clamps sweep a circle about the X axis: half the clamp diagonal, plus the safe zone.
    let reach = (clamps.clamp_height * 10.0).hypot(clamps.clamp_width * 10.0) / 2.0
        + clamps.clamp_safe_zone * 10.0;
    for (name, board) in boards() {
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            let p = generate(board, &cfg, side).unwrap();
            for m in p.moves() {
                // Over the tail clamp the cutter must be held above the clamp box. A tail wider than the swept
                // clamp (the longboard's) is cut beside it, at any height.
                if m.pos.x < clamps.clamp_safe_zone * 10.0 + 12.7 && m.pos.y.abs() < reach + 12.7 {
                    assert!(
                        m.pos.z > top,
                        "{name} {side:?}: held move below the clamp top: {m:?}"
                    );
                }
            }
        }
    }
}

/// Open: the hold height at the tail clamp edge (machine Z211.4, from the swept clamp box) ignores the board, which
/// turns under the held cutter. Where the tail is wider than that (rail more than ~211 mm from the A axis), the
/// rail rotates into the cutter: on the longboard deck program the Outline pass is held at X62.7 Z211.4 while A goes
/// from 79.7 to 87.3 degrees, and the tool sits ~6 mm inside the rail at x = 14.3-14.6 cm, y = 21 cm (6.1 mm in the
/// gouge report, deck and bottom programs); the funboard Cleanup pass does the same by 1.7 mm.
#[test]
#[ignore = "product bug: rotating-board hold height at the tail clamp is below the rail of wide tails (6.1 mm gouge on the longboard)"]
fn qa_bug_rotating_hold_height_below_wide_tail() {
    for (bi, (name, _)) in boards().iter().enumerate() {
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            let s = gouge_of(bi, &rotating_cfg(), side);
            assert!(
                s.gouge < 1.0,
                "{name} {side:?}: gouge {:.2} mm at {:?}",
                s.gouge,
                s.gouge_at
            );
        }
    }
}

/// The A axis could turn by ~90 degrees in a single linear move while the tool was at the surface (pass
/// transitions, side change), so the tool swept through the board while the board rotated under it.
/// Fixed: turns of more than 5 degrees retract the cutter clear of the board and clamps, turn in steps of at most
/// 15 degrees there, and plunge again.
#[test]
fn qa_bug_rotating_large_a_steps() {
    let cfg = MachineConfig {
        holding: HoldingSystem::RotatingBoard(EndClampsConfig {
            // Clamps far away, so only the toolpath itself is tested.
            clamp_length: 1.0,
            clamp_overlap: -50.0,
            ..Default::default()
        }),
        ..rotating_cfg()
    };
    for (name, board) in boards() {
        for side in [BoardSide::Deck, BoardSide::Bottom] {
            let p = generate(board, &cfg, side).unwrap();
            let moves: Vec<&Move> = p.moves().collect();
            for w in moves.windows(2) {
                let da = (w[1].a.unwrap() - w[0].a.unwrap()).abs();
                assert!(
                    da < 30.0,
                    "{name} {side:?}: A {:?} -> {:?} in one move",
                    w[0],
                    w[1]
                );
            }
        }
    }
}

/// The surface-splits side change only ended when its step counter passed `side_change_steps` (10), but the
/// counter was reset whenever it passed the number of lengthwise splits. With fewer than 10 splits
/// (resolution > length / 9) the state machine never finished and allocated without bound. The UI and MCP
/// clamp the resolution to 20 cm, which reaches this for any board shorter than 9 x 20 = 180 cm (5'11").
/// Fixed: both programs finish, for resolutions up to longer than the board, and absurdly fine resolutions are
/// refused with `TooManyPoints` instead of allocating.
#[test]
fn qa_bug_coarse_resolution_never_terminates() {
    let mut board = shortboard().clone();
    board.scale(170.0, board.max_width(), board.max_thickness());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ok = true;
        for res in [20.0, 50.0, 169.0, 1000.0] {
            let cfg = MachineConfig {
                lengthwise_resolution: res,
                ..Default::default()
            };
            for side in [BoardSide::Deck, BoardSide::Bottom] {
                ok &= generate(&board, &cfg, side).is_ok();
            }
        }
        let tiny = MachineConfig {
            lengthwise_resolution: 1e-6,
            ..Default::default()
        };
        ok &= matches!(generate_deck(&board, &tiny), Err(Error::TooManyPoints));
        let _ = tx.send(ok);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(ok) => assert!(ok, "a coarse-resolution program failed"),
        Err(_) => {
            // The runaway thread allocates gigabytes per second: end the process instead of letting it exhaust
            // memory.
            eprintln!(
                "BUG reproduced: programs for a 170 cm board at coarse resolution did not finish in 20 s; exiting"
            );
            std::process::exit(101);
        }
    }
}

// ------------------------------------------------------------------------------------------------ odd settings

fn shortboard() -> &'static Board {
    &boards()[0].1
}

#[test]
fn qa_odd_settings() {
    let board = shortboard();
    let cloud = &clouds()[0];
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut run = |name: &str, cfg: MachineConfig, side: BoardSide| {
        eprintln!("odd settings: {name} {side:?}");
        let t = Instant::now();
        let r = generate(board, &cfg, side);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match r {
            Err(e) => rows.push(format!("{name:<34} {side:?}: error {e} ({ms:.0} ms)")),
            Ok(p) => {
                let c = check_program(board, cloud, Some(&cfg), &p);
                let s = p.stats();
                rows.push(format!(
                    "{name:<34} {side:?}: {} passes {} moves {:.1} m, errors {} ({ms:.0} ms)",
                    s.passes,
                    s.points,
                    s.cut_length / 1000.0,
                    c.errors.len()
                ));
                for e in c.errors.iter().take(2) {
                    failures.push(format!("{name} {side:?}: {e}"));
                }
            }
        }
    };
    for side in [BoardSide::Deck, BoardSide::Bottom] {
        run(
            "0 cuts (rails/outline still on)",
            MachineConfig {
                deck_cuts: 0,
                bottom_cuts: 0,
                ..Default::default()
            },
            side,
        );
        run(
            "everything 0",
            MachineConfig {
                deck_cuts: 0,
                deck_rail_cuts: 0,
                bottom_cuts: 0,
                bottom_rail_cuts: 0,
                deck_cleanup_cuts: 0,
                bottom_cleanup_cuts: 0,
                deck_outline_cut: false,
                bottom_outline_cut: false,
                deck_stringer_cut: false,
                bottom_stringer_cut: false,
                ..Default::default()
            },
            side,
        );
        run(
            "1 cut",
            MachineConfig {
                deck_cuts: 1,
                deck_rail_cuts: 1,
                bottom_cuts: 1,
                bottom_rail_cuts: 1,
                ..Default::default()
            },
            side,
        );
        run(
            "500 cuts, 5 cm step",
            MachineConfig {
                deck_cuts: 500,
                bottom_cuts: 500,
                lengthwise_resolution: 5.0,
                ..Default::default()
            },
            side,
        );
        run(
            "width splits, resolution 1000 cm",
            MachineConfig {
                lengthwise_resolution: 1000.0,
                generator: GeneratorKind::WidthSplits { splits: 30 },
                ..Default::default()
            },
            side,
        );
        run(
            "resolution 20 cm (UI/MCP max)",
            MachineConfig {
                lengthwise_resolution: 20.0,
                ..Default::default()
            },
            side,
        );
        run(
            "resolution 0.05 cm (UI/MCP minimum)",
            MachineConfig {
                lengthwise_resolution: 0.05,
                ..Default::default()
            },
            side,
        );
        run(
            "width splits 1",
            MachineConfig {
                generator: GeneratorKind::WidthSplits { splits: 1 },
                ..Default::default()
            },
            side,
        );
        run(
            "deck angle > deck rail angle",
            MachineConfig {
                deck_angle: 150.0,
                deck_rail_angle: 130.0,
                bottom_angle: 10.0,
                ..Default::default()
            },
            side,
        );
        run(
            "cut-offs on, start at nose/prev",
            MachineConfig {
                cut_off_nose: true,
                cut_off_tail: true,
                deck_start: DeckStart::Nose,
                bottom_start: BottomStart::Previous,
                lengthwise_resolution: 2.0,
                ..Default::default()
            },
            side,
        );
        run(
            "lift clear before/after",
            MachineConfig {
                before_deck_cut: MachineAction::LiftClear,
                after_deck_cut: MachineAction::LiftClear,
                before_bottom_cut: MachineAction::LiftClear,
                after_bottom_cut: MachineAction::LiftClear,
                lengthwise_resolution: 2.0,
                ..Default::default()
            },
            side,
        );
    }
    println!("{}", rows.join("\n"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Timing of very fine resolutions (the library has no lower limit; UI and MCP clamp at 0.05 cm).
#[test]
fn qa_tiny_resolution_timing() {
    let board = shortboard();
    for res in [0.05, 0.02] {
        let cfg = MachineConfig {
            lengthwise_resolution: res,
            ..Default::default()
        };
        let t = Instant::now();
        let p = generate_deck(board, &cfg).unwrap();
        let gen_t = t.elapsed();
        let t = Instant::now();
        let g = p.to_gcode();
        let write = t.elapsed();
        let t = Instant::now();
        let s = p.stats();
        let stats = t.elapsed();
        println!(
            "resolution {res} cm: {} moves, {:.1} MB G-code, generate {:.2} s, to_gcode {:.2} s, stats {:.3} s",
            s.points,
            g.len() as f64 / 1e6,
            gen_t.as_secs_f64(),
            write.as_secs_f64(),
            stats.as_secs_f64()
        );
    }
}

/// A cutter wider than the board is thick: the program must stay finite and must not gouge more than a normal one.
#[test]
fn qa_cutter_bigger_than_board_thickness() {
    let board = shortboard();
    let cloud = &clouds()[0];
    let big = BullnoseCutter {
        diameter: 10.0,
        corner_radius: 2.0,
        height: 10.0,
        ..Default::default()
    };
    assert!(big.diameter > board.max_thickness());
    for side in [BoardSide::Deck, BoardSide::Bottom] {
        let cfg = MachineConfig {
            cutter: Cutter::Bullnose(big.clone()),
            lengthwise_resolution: 2.0,
            ..Default::default()
        };
        let p = generate(board, &cfg, side).unwrap();
        let c = check_program(board, cloud, Some(&cfg), &p);
        let s = simulate(
            &p,
            board,
            cloud,
            &ToolShape::bullnose(&big),
            side == BoardSide::Deck,
        );
        println!(
            "100 mm cutter {side:?}: gouge {:.2} mm at {:?}, leftover max {:.2} p95 {:.2}, below blank {:.1} mm, errors {:?}",
            s.gouge,
            s.gouge_at,
            s.leftover_max,
            s.leftover_p95,
            c.below_blank,
            c.errors.iter().take(3).collect::<Vec<_>>()
        );
        assert!(c.errors.is_empty(), "{:?}", c.errors);
    }
}

/// MCP `generate_toolpath {cutter_diameter: 0.4}` keeps the default 0.5 cm corner radius: a corner radius larger
/// than the cutter radius describes no real tool. The library accepts it and moves the tip *outward* from the
/// contact point by a negative flat radius.
/// Fixed: `Cutter::validate` rejects it with `InvalidConfig`.
#[test]
fn qa_corner_radius_larger_than_radius_is_rejected() {
    let board = shortboard();
    let cfg = MachineConfig {
        cutter: Cutter::Bullnose(BullnoseCutter {
            diameter: 0.4,
            corner_radius: 0.5,
            ..Default::default()
        }),
        lengthwise_resolution: 5.0,
        ..Default::default()
    };
    assert!(
        matches!(generate_deck(board, &cfg), Err(Error::InvalidConfig(_))),
        "corner radius 5 mm on a 4 mm cutter was accepted"
    );
}

/// Feeds were not validated by the library: zero speed wrote `F0` on G1 moves, which LinuxCNC rejects.
/// Fixed: `MachineConfig::validate` rejects it (and other impossible settings) with `InvalidConfig`.
#[test]
fn qa_zero_feed_is_rejected() {
    let board = shortboard();
    let cfg = MachineConfig {
        cutting_speed: 0.0,
        lengthwise_resolution: 5.0,
        ..Default::default()
    };
    match generate_deck(board, &cfg) {
        Err(_) => {}
        Ok(p) => {
            let parsed = parse_gcode(&p.to_gcode());
            assert!(
                parsed.errors.is_empty(),
                "{:?}",
                &parsed.errors[..parsed.errors.len().min(3)]
            );
        }
    }
}

/// Debug aid: dump G-code lines of one sanity-test combination, `QA_DUMP=board/cutter/side/from-to`.
#[test]
#[ignore = "debug aid, run explicitly"]
fn qa_dump() {
    let spec =
        std::env::var("QA_DUMP").unwrap_or_else(|_| "longboard/rotating/Bottom/2000-2010".into());
    let parts: Vec<&str> = spec.split('/').collect();
    let board = &boards().iter().find(|b| b.0 == parts[0]).unwrap().1;
    let (_, cutter) = cutters().into_iter().find(|c| c.0 == parts[1]).unwrap();
    let holding = holdings(&cutter)[0].1.clone();
    let side = if parts[2] == "Deck" {
        BoardSide::Deck
    } else {
        BoardSide::Bottom
    };
    let cfg = MachineConfig {
        cutter,
        holding,
        lengthwise_resolution: 2.0,
        deck_cuts: 10,
        deck_rail_cuts: 4,
        bottom_cuts: 10,
        ..Default::default()
    };
    let p = generate(board, &cfg, side).unwrap();
    let (a, b) = parts[3].split_once('-').unwrap();
    let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
    let g = p.to_gcode();
    let lines: Vec<&str> = g.lines().collect();
    let first_move_line = lines.iter().position(|l| l.starts_with("G01 X")).unwrap() + 1;
    let moves: Vec<&Move> = p.moves().collect();
    for (i, l) in lines.iter().enumerate().take(b).skip(a.saturating_sub(1)) {
        let ln = i + 1;
        let pass = ln
            .checked_sub(first_move_line)
            .and_then(|k| moves.get(k))
            .and_then(|m| m.pass)
            .map(|i| {
                let ps = &p.toolpath.passes[i];
                format!("{:?} {:?} #{}", ps.kind, ps.side, ps.index)
            });
        println!("{ln:>6}: {l:<60} {}", pass.unwrap_or_default());
    }
}

/// Debug aid: cross-section of the shortboard at x = 86 cm and the deck program's rail/cleanup contacts and tool
/// tips near it (board space, mm).
#[test]
#[ignore = "debug aid, run explicitly"]
fn qa_dump_rail_section() {
    let spec = std::env::var("QA_SECTION").unwrap_or_else(|_| "shortboard/86/Deck/180".into());
    let parts: Vec<&str> = spec.split('/').collect();
    let board = &boards().iter().find(|b| b.0 == parts[0]).unwrap().1;
    let x: f64 = parts[1].parse().unwrap();
    let side = if parts[2] == "Deck" {
        BoardSide::Deck
    } else {
        BoardSide::Bottom
    };
    let ymin: f64 = parts[3].parse().unwrap();
    println!("section at x = {x} cm (y, z mm), right half:");
    for (range, name) in [(surface::DECK, "deck"), (surface::BOTTOM, "bottom")] {
        for i in (0..=40).map(|i| i as f64 / 40.0) {
            let p = board.surface_point(x, i, range.0, range.1) * 10.0;
            let n = board.surface_normal(x, i, range.0, range.1);
            if p.y > ymin {
                println!(
                    "  {name:<6} s {i:.3}: y {:>7.2} z {:>6.2}  n ({:.2},{:.2},{:.2})",
                    p.y, p.z, n.x, n.y, n.z
                );
            }
        }
    }
    println!("same x, whole section (-360..360, as the 3D mesh and STL export):");
    for i in (0..=60).map(|i| i as f64 / 60.0) {
        let q = board.surface_point(x, i, surface::WHOLE.0, surface::WHOLE.1) * 10.0;
        if q.y > ymin {
            println!("  whole s {i:.3}: y {:>7.2} z {:>6.2}", q.y, q.z);
        }
    }
    println!("same x, bottom parametrised from 360 to 90 degrees (as the bottom program does):");
    for i in (0..=20).map(|i| i as f64 / 20.0) {
        let q = board.surface_point(x, i, 360.0, 90.0) * 10.0;
        if q.y > ymin {
            println!("  360..90 s {i:.3}: y {:>7.2} z {:>6.2}", q.y, q.z);
        }
    }
    let p = generate(board, &MachineConfig::default(), side).unwrap();
    let moves: Vec<&Move> = p.moves().collect();
    for (pi, pass) in p.toolpath.passes.iter().enumerate() {
        if pass.side != Side::Right {
            continue;
        }
        if let Some(tp) = pass
            .points
            .iter()
            .min_by(|a, b| (a.pos.x - x).abs().total_cmp(&(b.pos.x - x).abs()))
        {
            let tip = moves
                .iter()
                .filter(|m| m.pass == Some(pi))
                .map(|m| to_board(&p, m.pos, m.a).0)
                .min_by(|a, b| (a.x - x * 10.0).abs().total_cmp(&(b.x - x * 10.0).abs()))
                .unwrap();
            let n = tp.normal;
            let angle = n.y.atan2(n.z).to_degrees();
            println!(
                "{:?} #{}: contact y {:.2} z {:.2} normal angle from up {angle:.1} deg -> tip y {:.2} z {:.2}",
                pass.kind,
                pass.index,
                tp.pos.y * 10.0,
                tp.pos.z * 10.0,
                tip.y,
                tip.z
            );
        }
    }
}

/// Debug aid: generate one program at `QA_RES` cm resolution and report its size and time.
#[test]
#[ignore = "debug aid, run explicitly"]
fn qa_probe_resolution() {
    let res: f64 = std::env::var("QA_RES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000.0);
    let cfg = MachineConfig {
        lengthwise_resolution: res,
        ..Default::default()
    };
    let t = Instant::now();
    let p = generate_deck(shortboard(), &cfg).unwrap();
    eprintln!(
        "generated in {:?}: {} passes, {} points",
        t.elapsed(),
        p.toolpath.passes.len(),
        p.toolpath.point_count()
    );
    let s = p.stats();
    eprintln!("stats {s:?}");
    let g = p.to_gcode();
    eprintln!("gcode {} bytes", g.len());
}
