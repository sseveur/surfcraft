//! From contact points to a machine program: placement, 4th-axis turn, cutter compensation, stay-away, gouge
//! verification, holding-system avoidance, linking moves, machine offset (`cam.md` §1 pipeline), then the
//! program text (§10) and statistics (§11).

use rayon::prelude::*;
use surfcraft_core::{Board, DVec2, DVec3};

use crate::Error;
use crate::config::{
    AxisScaling, BoardSide, Cutter, HotwireConfig, HotwireKind, MachineAction, MachineConfig,
};
use crate::cutter::{Profile, rot_x, unwrap_near};
use crate::eval::Part3;
use crate::gouge::{Envelope, cloud};
use crate::holding::{Obstacle, Placement};
use crate::toolpath::{PassKind, Planned, Side, Toolpath};

/// One linear cutting move (machine frame, mm; feed mm/min; A degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub pos: DVec3,
    pub feed: f64,
    pub a: Option<f64>,
    /// Index of the toolpath pass the move belongs to.
    pub pass: Option<usize>,
}

/// One program step.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Comment(String),
    /// G-code text written verbatim (scripts, preamble).
    Raw(String),
    Home,
    SpindleOn,
    SpindleOff,
    /// Dwell, seconds.
    Dwell(f64),
    /// Vertical move to `z` (mm) at `feed` (mm/min).
    MoveZ {
        z: f64,
        feed: f64,
    },
    Move(Move),
    End,
}

/// Summary of a program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Passes, not counting the approach.
    pub passes: usize,
    /// Motion lines with a known position.
    pub points: usize,
    /// Path length, mm.
    pub cut_length: f64,
    /// Seconds.
    pub estimated_time: f64,
    pub passes_by_kind: Vec<(PassKind, usize)>,
    /// Positions raised to avoid cutting into the board.
    pub raised: usize,
    /// Positions lifted over the holding system.
    pub held: usize,
}

/// A generated machine program.
#[derive(Clone, Debug)]
pub struct Program {
    pub side: BoardSide,
    pub toolpath: Toolpath,
    pub placement: Placement,
    /// Program zero in the reference frame, mm.
    pub cutter_offset: DVec3,
    pub commands: Vec<Command>,
    /// Output word scaling.
    pub scaling: AxisScaling,
    stats: Stats,
}

impl Program {
    fn new(
        side: BoardSide,
        toolpath: Toolpath,
        placement: Placement,
        cutter_offset: DVec3,
        commands: Vec<Command>,
        scaling: AxisScaling,
    ) -> Program {
        let mut p = Program {
            side,
            toolpath,
            placement,
            cutter_offset,
            commands,
            scaling,
            stats: Stats::default(),
        };
        p.stats = p.compute_stats();
        p
    }

    /// The cutting moves, in order.
    pub fn moves(&self) -> impl Iterator<Item = &Move> + '_ {
        self.commands.iter().filter_map(|c| match c {
            Command::Move(m) => Some(m),
            _ => None,
        })
    }

    /// Output value of a coordinate word: scaled, rounded to the written precision.
    fn word(v: f64, scale: f64) -> f64 {
        let r = (v * scale * 1e4).round() / 1e4;
        if r == 0.0 { 0.0 } else { r }
    }

    fn scaled(&self, p: DVec3) -> DVec3 {
        let s = &self.scaling;
        DVec3::new(
            Self::word(p.x, s.x),
            Self::word(p.y, s.y),
            Self::word(p.z, s.z),
        )
    }

    pub fn to_gcode(&self) -> String {
        let mut out = String::with_capacity(self.commands.len() * 40);
        let mut g01 = false;
        let mut feed: Option<i64> = None;
        let fmt = |v: f64| {
            let s = format!("{v:.4}");
            if s == "-0.0000" {
                "0.0000".to_string()
            } else {
                s
            }
        };
        for c in &self.commands {
            match c {
                Command::Comment(t) => {
                    for line in t.lines() {
                        let clean: String = line
                            .chars()
                            .map(|ch| match ch {
                                '(' => '[',
                                ')' => ']',
                                c if c.is_ascii_graphic() || c == ' ' => c,
                                _ => '?',
                            })
                            .collect();
                        out.push('(');
                        out.push_str(&clean);
                        out.push_str(")\n");
                    }
                }
                Command::Raw(t) => {
                    for line in t.lines() {
                        out.push_str(line);
                        out.push('\n');
                    }
                    // A script may change the motion mode or the feed.
                    g01 = false;
                    feed = None;
                }
                Command::Home => out.push_str("G28\n"),
                Command::SpindleOn => out.push_str("M3\n"),
                Command::SpindleOff => out.push_str("M5\n"),
                Command::Dwell(s) => out.push_str(&format!("G04 P{s}\n")),
                Command::MoveZ { z, feed: f } => {
                    let f = f.trunc() as i64;
                    out.push_str(&format!(
                        "G01 Z{} F{f}\n",
                        fmt(Self::word(*z, self.scaling.z))
                    ));
                    g01 = true;
                    feed = Some(f);
                }
                Command::Move(m) => {
                    let p = self.scaled(m.pos);
                    if !g01 {
                        out.push_str("G01 ");
                        g01 = true;
                    }
                    out.push_str(&format!("X{} Y{} Z{}", fmt(p.x), fmt(p.y), fmt(p.z)));
                    if let Some(a) = m.a {
                        out.push_str(&format!(" A{}", fmt(Self::word(a, self.scaling.a))));
                    }
                    let f = m.feed.trunc() as i64;
                    if feed != Some(f) {
                        out.push_str(&format!(" F{f}"));
                        feed = Some(f);
                    }
                    out.push('\n');
                }
                Command::End => out.push_str("M2\n"),
            }
        }
        out
    }

    fn compute_stats(&self) -> Stats {
        let mut s = Stats::default();
        let mut pos: [Option<f64>; 3] = [None; 3];
        let step = |to: [Option<f64>; 3], f: f64, pos: &mut [Option<f64>; 3], s: &mut Stats| {
            let from = match *pos {
                [Some(x), Some(y), Some(z)] => Some(DVec3::new(x, y, z)),
                _ => None,
            };
            for i in 0..3 {
                if to[i].is_some() {
                    pos[i] = to[i];
                }
            }
            if let [Some(x), Some(y), Some(z)] = *pos {
                s.points += 1;
                if let Some(a) = from {
                    let d = a.distance(DVec3::new(x, y, z));
                    s.cut_length += d;
                    if f > 0.0 {
                        s.estimated_time += d / (f / 60.0);
                    }
                }
            }
        };
        for c in &self.commands {
            match c {
                Command::Home => pos = [None; 3],
                Command::Dwell(t) => s.estimated_time += t,
                Command::MoveZ { z, feed: f } => {
                    step(
                        [None, None, Some(Self::word(*z, self.scaling.z))],
                        f.trunc(),
                        &mut pos,
                        &mut s,
                    );
                }
                Command::Move(m) => {
                    let p = self.scaled(m.pos);
                    step(
                        [Some(p.x), Some(p.y), Some(p.z)],
                        m.feed.trunc(),
                        &mut pos,
                        &mut s,
                    );
                }
                _ => {}
            }
        }
        let mut kinds: Vec<(PassKind, usize)> = Vec::new();
        for p in &self.toolpath.passes {
            if p.kind == PassKind::Approach {
                continue;
            }
            s.passes += 1;
            match kinds.iter_mut().find(|k| k.0 == p.kind) {
                Some(k) => k.1 += 1,
                None => kinds.push((p.kind, 1)),
            }
        }
        s.passes_by_kind = kinds;
        s
    }

    pub fn stats(&self) -> Stats {
        self.stats.clone()
    }

    /// Tool positions mapped back to board space (cm), for previews.
    pub fn tool_path_in_board_space(&self) -> Vec<DVec3> {
        self.moves()
            .map(|m| {
                let mut p = m.pos + self.cutter_offset;
                if let Some(a) = m.a {
                    p = rot_x(-a.to_radians(), p);
                }
                self.placement.inverse_point(p)
            })
            .collect()
    }
}

// ------------------------------------------------------------------------------------------------ pipeline

/// How a toolpath point becomes a tool position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Surface contact: cutter compensation.
    Contact,
    /// Cut-off: the tool side tangent to a plane across the board, no vertical compensation.
    CutOff,
}

/// A tool position in the frame where the tool axis is +Z (mm, before the machine offset).
#[derive(Clone, Copy, Debug)]
struct Pos {
    tip: DVec3,
    a: Option<f64>,
    feed: f64,
    pass: Option<usize>,
    /// Reference-frame contact and normal, side sign, for re-evaluating between two positions.
    contact: Option<(DVec3, DVec3, f64)>,
    mode: Mode,
    held: bool,
}

/// Feed (mm/min, output units) of a pass kind.
fn feed_of(cfg: &MachineConfig, kind: PassKind, side: Side, index: usize) -> f64 {
    let v = match kind {
        PassKind::Stringer | PassKind::NoseCutOff | PassKind::TailCutOff => cfg.stringer_speed,
        PassKind::Rail => cfg.rail_speed,
        PassKind::Outline => cfg.outline_speed,
        PassKind::WidthSplit if side == Side::Center && index == 0 => cfg.stringer_speed,
        _ => cfg.cutting_speed,
    };
    (v * 60.0 * cfg.scaling.f).trunc().max(1.0)
}

/// Everything the position pipeline needs.
struct Machine<'a> {
    cfg: &'a MachineConfig,
    placement: Placement,
    four: bool,
    obstacles: Vec<Obstacle>,
    radius: f64,
    profile: Profile,
    envelope: Option<Envelope>,
    tol: f64,
    /// Height above which the tool clears board and holding system at any A (4-axis).
    safe_turn: f64,
}

/// Margin kept above an obstacle when lifting over it, mm.
const OBSTACLE_MARGIN: f64 = 1.0;
/// A turns larger than this retract the tool, degrees.
const RETRACT_TURN: f64 = 5.0;
/// Largest A step while retracted, degrees.
const RETRACTED_A_STEP: f64 = 15.0;

impl Machine<'_> {
    /// Lowest tip height that keeps the tool off the obstacles at plan position `at`.
    fn obstacle_floor(&self, at: DVec2) -> f64 {
        self.obstacles
            .iter()
            .filter(|o| o.under(at.x, at.y, self.radius))
            .map(|o| o.top + OBSTACLE_MARGIN)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Penetration into the board at `tip` and the lowest clear tip height there (3-axis).
    fn check(&self, tip: DVec3) -> (f64, f64) {
        self.envelope
            .as_ref()
            .map_or((0.0, f64::NEG_INFINITY), |e| e.check(tip, &self.profile))
    }

    /// Tool position for a reference-frame contact.
    fn position(
        &self,
        contact: DVec3,
        normal: DVec3,
        side: f64,
        mode: Mode,
    ) -> (DVec3, Option<f64>) {
        let stay = self
            .cfg
            .stay_away_from_stringer
            .then_some(self.cfg.stringer_width);
        match mode {
            Mode::CutOff => {
                let inner = self.cfg.cutter.mounted();
                let h = DVec3::new(normal.x, normal.y, 0.0).normalize_or_zero();
                let mut tip = contact + inner.cutting_radius() * h;
                if let Some(w) = stay {
                    tip = inner.stay_away(tip, w * 10.0, side);
                }
                (tip, self.four.then_some(0.0))
            }
            Mode::Contact => {
                let t = self.cfg.cutter.tip(contact, normal, stay, side, None);
                (t.pos, t.a)
            }
        }
    }
}

/// Header line describing a cutter.
fn describe_cutter(c: &Cutter) -> String {
    match c {
        Cutter::Bullnose(b) => format!(
            "Bullnose cutter, diameter {:.1} mm, corner radius {:.1} mm, height {:.1} mm",
            b.diameter * 10.0,
            b.corner_radius * 10.0,
            b.height * 10.0
        ),
        Cutter::Flat(f) => format!(
            "Flat cutter, diameter {:.1} mm, height {:.1} mm",
            f.radius * 20.0,
            f.height * 10.0
        ),
        Cutter::Stl(s) => {
            let (r, lo, hi) = s.bounds();
            format!(
                "Stl cutter, diameter {:.1} mm, height {:.1} mm",
                2.0 * r,
                hi - lo
            )
        }
        Cutter::RotatingBoard(inner) => {
            format!("Rotating board system with {}", describe_cutter(inner))
        }
    }
}

fn describe_board(b: &Board) -> String {
    let name = if b.info.name.trim().is_empty() {
        "unnamed"
    } else {
        b.info.name.trim()
    };
    format!(
        "{name} {:.1} x {:.1} x {:.1} cm",
        b.length(),
        b.max_width(),
        b.max_thickness()
    )
}

fn action(a: &MachineAction, z: f64, feed: f64, out: &mut Vec<Command>) {
    match a {
        MachineAction::Nothing => {}
        MachineAction::Home => out.push(Command::Home),
        MachineAction::HomeRef => out.push(Command::Raw("G28.1".into())),
        MachineAction::LiftClear => out.push(Command::MoveZ { z, feed }),
        MachineAction::Script(text) => out.push(Command::Raw(text.clone())),
    }
}

/// Points of a board's section samples used for the clouds, cm.
const CLOUD_STEP: f64 = 0.05;
const CLOUD_SPACING: f64 = 0.05;

/// Milling program from a contact toolpath.
pub(crate) fn milling(
    board: &Board,
    cfg: &MachineConfig,
    side: BoardSide,
    planned: Planned,
) -> Result<Program, Error> {
    let Planned { toolpath, notes } = planned;
    let blank = cfg.blank.as_ref();
    let (placement, blank_placement) = cfg.holding.placements(board, blank, side);
    let four = cfg.four_axis();
    let inner = cfg.cutter.mounted();
    let profile = inner.profile();
    let radius = cfg.cutter.radius();
    let par = cfg.parallel;
    let up_part = match side {
        BoardSide::Deck => Part3::Deck,
        BoardSide::Bottom => Part3::Bottom,
    };
    let obstacles = cfg.holding.obstacles(board, blank);

    // Board (and blank) clouds in the reference frame.
    let (envelope, board_top, board_radius) = if four {
        let pts = cloud(board, &placement, Part3::Whole, 0.5, 0.3, par);
        let r = pts.iter().map(|p| p.y.hypot(p.z)).fold(0.0, f64::max);
        (None, f64::NEG_INFINITY, r)
    } else {
        let pts = cloud(board, &placement, up_part, CLOUD_STEP, CLOUD_SPACING, par);
        let env = Envelope::new(pts, profile.radius() / 2.0);
        let top = env.top;
        (Some(env), top, 0.0)
    };
    let blank_env = blank.map(|b| {
        let pts = cloud(
            b,
            &blank_placement,
            if four { Part3::Whole } else { up_part },
            0.5,
            0.3,
            par,
        );
        let r = pts.iter().map(|p| p.y.hypot(p.z)).fold(0.0, f64::max);
        (Envelope::new(pts, profile.radius() / 2.0), r)
    });
    let obstacle_top = obstacles
        .iter()
        .map(|o| o.top)
        .fold(f64::NEG_INFINITY, f64::max);
    let safe_turn = board_radius
        .max(blank_env.as_ref().map_or(0.0, |b| b.1))
        .max(obstacle_top)
        + cfg.retract_clearance;
    let m = Machine {
        cfg,
        placement,
        four,
        obstacles,
        radius,
        profile,
        envelope,
        tol: cfg.gouge_tolerance,
        safe_turn,
    };

    // Raw positions, in parallel.
    let passes = &toolpath.passes;
    let first_kind = passes.get(1).map_or(PassKind::Regular, |p| p.kind);
    let mut items = Vec::with_capacity(toolpath.point_count());
    for (pi, pass) in passes.iter().enumerate() {
        let kind = if pass.kind == PassKind::Approach {
            first_kind
        } else {
            pass.kind
        };
        let mode = match kind {
            PassKind::NoseCutOff | PassKind::TailCutOff => Mode::CutOff,
            _ => Mode::Contact,
        };
        let next = passes.get(pi + 1);
        let feed = if pass.kind == PassKind::Approach {
            next.map_or(feed_of(cfg, kind, pass.side, 0), |n| {
                feed_of(cfg, n.kind, n.side, n.index)
            })
        } else {
            feed_of(cfg, kind, pass.side, pass.index)
        };
        for p in &pass.points {
            items.push((pi, p.pos, p.normal, pass.side.sign(), mode, feed));
        }
    }
    let compute = |&(pi, pos, normal, side, mode, feed): &(usize, DVec3, DVec3, f64, Mode, f64)| {
        let c = m.placement.point(pos);
        let n = m.placement.vector(normal);
        let (mut tip, a) = m.position(c, n, side, mode);
        let mut raised = false;
        let mut held = false;
        if !four {
            if mode == Mode::Contact {
                let (pen, z) = m.check(tip);
                if pen > m.tol {
                    tip.z = z;
                    raised = true;
                }
            }
            let floor = m.obstacle_floor(tip.truncate());
            if tip.z < floor {
                tip.z = floor;
                held = true;
            }
        } else if tip.z < m.obstacle_floor(tip.truncate()) {
            tip.z = m.safe_turn;
            held = true;
        }
        (
            Pos {
                tip,
                a,
                feed,
                pass: Some(pi),
                contact: (mode == Mode::Contact).then_some((c, n, side)),
                mode,
                held,
            },
            raised,
        )
    };
    let raw: Vec<(Pos, bool)> = if par {
        items.par_iter().map(compute).collect()
    } else {
        items.iter().map(compute).collect()
    };
    let mut raised = raw.iter().filter(|r| r.1).count();
    let held = raw.iter().filter(|r| r.0.held).count();
    let mut pos: Vec<Pos> = raw.into_iter().map(|r| r.0).collect();
    if pos.is_empty() {
        return Err(Error::NothingToCut);
    }
    // Approach: above the first point, clear of the blank.
    if let Some((env, _)) = &blank_env
        && !four
    {
        let z = env.check(pos[0].tip, &m.profile).1 + 5.0;
        if pos[0].tip.z < z {
            pos[0].tip.z = z;
        }
    }
    // Plunge feed for the first cutting position.
    if pos.len() > 1 {
        let plunge = (pos[1].feed * cfg.plunge_speed_factor).trunc().max(1.0);
        pos[1].feed = plunge;
    }
    // Unwrap A.
    if four {
        let mut prev: Option<f64> = None;
        for p in &mut pos {
            if let (Some(a), Some(q)) = (p.a, prev) {
                p.a = Some(unwrap_near(a, q));
            }
            prev = p.a.or(prev);
        }
    }

    // Linking moves.
    let links: Vec<Vec<Pos>> = {
        let link = |i: usize| -> (Vec<Pos>, usize) {
            let (a, b) = (&pos[i], &pos[i + 1]);
            if four {
                link_4(&m, a, b)
            } else {
                link_3(&m, a, b)
            }
        };
        let r: Vec<(Vec<Pos>, usize)> = if par {
            (0..pos.len() - 1).into_par_iter().map(link).collect()
        } else {
            (0..pos.len() - 1).map(link).collect()
        };
        raised += r.iter().map(|x| x.1).sum::<usize>();
        r.into_iter().map(|x| x.0).collect()
    };
    let mut seq: Vec<Pos> = Vec::with_capacity(pos.len() * 2);
    for (i, p) in pos.iter().enumerate() {
        if i > 0 {
            seq.extend(links[i - 1].iter().copied());
        }
        seq.push(*p);
    }

    // Safe height and the end of the program.
    let top_tip = seq
        .iter()
        .map(|p| p.tip.z)
        .fold(f64::NEG_INFINITY, f64::max);
    let z_safe = if four {
        safe_turn.max(top_tip)
    } else {
        top_tip
            .max(board_top)
            .max(obstacle_top)
            .max(blank_env.as_ref().map_or(f64::NEG_INFINITY, |b| b.0.top))
            + cfg.retract_clearance
    };
    let last = *seq.last().unwrap();
    seq.push(Pos {
        tip: DVec3::new(last.tip.x, last.tip.y, z_safe.max(last.tip.z)),
        pass: None,
        contact: None,
        held: false,
        ..last
    });

    // Commands.
    let offset = cfg.cutter_offset * 10.0;
    let mut cmds = Vec::with_capacity(seq.len() + 32);
    cmds.push(Command::Comment("Generated with SurfCraft".into()));
    cmds.push(Command::Comment(
        match side {
            BoardSide::Deck => "Deck cut",
            BoardSide::Bottom => "Bottom cut",
        }
        .into(),
    ));
    cmds.push(Command::Comment(format!(
        "Board: {}",
        describe_board(board)
    )));
    cmds.push(Command::Comment(match blank {
        Some(b) => format!("Blank: {}", describe_board(b)),
        None => "Blank: none".into(),
    }));
    cmds.push(Command::Comment(format!(
        "Cutter: {}",
        describe_cutter(&cfg.cutter)
    )));
    let pl = &m.placement;
    cmds.push(Command::Comment(format!(
        "Holding system: {}. Board offset {:.3}, {:.3}, {:.3} cm, angle {:.6} rad",
        cfg.holding.describe(),
        pl.offset.x,
        pl.offset.y,
        pl.offset.z,
        pl.angle
    )));
    for n in &notes {
        cmds.push(Command::Comment(n.clone()));
    }
    if raised > 0 {
        cmds.push(Command::Comment(format!(
            "{raised} positions raised to keep the tool out of the board"
        )));
    }
    if held > 0 {
        cmds.push(Command::Comment(format!(
            "{held} positions lifted over the blank holding system"
        )));
    }
    cmds.push(Command::Raw("G21".into()));
    cmds.push(Command::Raw("G90".into()));
    let (before, after) = match side {
        BoardSide::Deck => (&cfg.before_deck_cut, &cfg.after_deck_cut),
        BoardSide::Bottom => (&cfg.before_bottom_cut, &cfg.after_bottom_cut),
    };
    let lift_feed = (cfg.stringer_speed.max(cfg.cutting_speed) * 60.0 * cfg.scaling.f)
        .trunc()
        .max(1.0);
    let first_z = seq[0].tip.z - offset.z;
    action(
        before,
        first_z + cfg.clear_height * 10.0,
        lift_feed,
        &mut cmds,
    );
    cmds.push(Command::SpindleOn);
    cmds.push(Command::Dwell(2.0));
    let mut prev_key: Option<[i64; 4]> = None;
    let s = &cfg.scaling;
    for p in &seq {
        let q = p.tip - offset;
        let key = [
            (q.x * s.x * 1e4).round() as i64,
            (q.y * s.y * 1e4).round() as i64,
            (q.z * s.z * 1e4).round() as i64,
            p.a.map_or(0, |a| (a * s.a * 1e4).round() as i64),
        ];
        if prev_key == Some(key) {
            continue;
        }
        prev_key = Some(key);
        cmds.push(Command::Move(Move {
            pos: q,
            feed: p.feed,
            a: p.a,
            pass: p.pass,
        }));
    }
    cmds.push(Command::SpindleOff);
    let last_z = seq.last().unwrap().tip.z - offset.z;
    action(
        after,
        last_z + cfg.clear_height * 10.0,
        lift_feed,
        &mut cmds,
    );
    cmds.push(Command::End);
    let mut program = Program::new(
        side,
        toolpath,
        m.placement,
        offset,
        cmds,
        cfg.scaling.clone(),
    );
    program.stats.raised = raised;
    program.stats.held = held;
    Ok(program)
}

/// Linking positions between two 3-axis positions: nothing when the straight move is clear; raised intermediate
/// points over small gouges; otherwise a retract over the obstacle or the board.
fn link_3(m: &Machine, a: &Pos, b: &Pos) -> (Vec<Pos>, usize) {
    if a.mode == Mode::CutOff && b.mode == Mode::CutOff && a.pass == b.pass {
        return (Vec::new(), 0);
    }
    // A purely vertical move between two clear positions is clear (the tool is unbounded upwards).
    let horizontal = (b.tip.truncate() - a.tip.truncate()).length();
    if horizontal < 1e-9 {
        return (Vec::new(), 0);
    }
    let d = a.tip.distance(b.tip);
    let k = ((d / m.cfg.linking_spacing).ceil() as usize).max(2);
    let mut need = Vec::with_capacity(k - 1);
    let mut worst = 0.0f64;
    let mut obstacle = false;
    for j in 1..k {
        let t = j as f64 / k as f64;
        let p = a.tip.lerp(b.tip, t);
        let floor = m.obstacle_floor(p.truncate());
        let (pen, z) = m.check(p);
        if p.z < floor {
            obstacle = true;
        }
        let gouge = pen > m.tol;
        if gouge {
            worst = worst.max(z - p.z);
        }
        let req = floor.max(if gouge { z } else { p.z });
        need.push((t, p, req));
    }
    if !obstacle && worst == 0.0 {
        return (Vec::new(), 0);
    }
    let base = Pos {
        contact: None,
        held: false,
        mode: Mode::Contact,
        pass: None,
        ..*b
    };
    if obstacle || worst > 3.0 {
        let h = need
            .iter()
            .map(|e| e.2)
            .fold(a.tip.z.max(b.tip.z), f64::max)
            + 2.0 * OBSTACLE_MARGIN;
        let up = Pos {
            tip: DVec3::new(a.tip.x, a.tip.y, h),
            ..base
        };
        let over = Pos {
            tip: DVec3::new(b.tip.x, b.tip.y, h),
            ..base
        };
        return (vec![up, over], 0);
    }
    // Follow the envelope where the straight move would cut in: every sample from one before the first to one
    // after the last violation, at least at the gouge-free height.
    let bad: Vec<usize> = need
        .iter()
        .enumerate()
        .filter(|(_, e)| e.1.z < e.2)
        .map(|(i, _)| i)
        .collect();
    let (first, last) = (
        bad[0].saturating_sub(1),
        (bad[bad.len() - 1] + 1).min(need.len() - 1),
    );
    let pts: Vec<Pos> = need[first..=last]
        .iter()
        .map(|e| Pos {
            tip: DVec3::new(e.1.x, e.1.y, e.1.z.max(e.2)),
            ..base
        })
        .collect();
    let n = bad.len();
    (pts, n)
}

/// Linking positions between two 4-axis positions: small turns are split by re-evaluating the contact between
/// the two positions; large turns and moves to or from held positions go through the safe height, where the
/// board turns freely.
fn link_4(m: &Machine, a: &Pos, b: &Pos) -> (Vec<Pos>, usize) {
    let (aa, ba) = (a.a.unwrap_or(0.0), b.a.unwrap_or(0.0));
    let turn = (ba - aa).abs();
    let base = Pos {
        contact: None,
        held: false,
        mode: Mode::Contact,
        pass: None,
        ..*b
    };
    let retract = |both_high: bool| -> Vec<Pos> {
        let h = m.safe_turn;
        let mut v = Vec::new();
        if !both_high {
            v.push(Pos {
                tip: DVec3::new(a.tip.x, a.tip.y, h.max(a.tip.z)),
                a: Some(aa),
                ..base
            });
        }
        let steps = ((turn / RETRACTED_A_STEP).ceil() as usize).max(1);
        for j in 1..=steps {
            let t = j as f64 / steps as f64;
            let xy = a.tip.truncate().lerp(b.tip.truncate(), t);
            v.push(Pos {
                tip: DVec3::new(xy.x, xy.y, h.max(b.tip.z)),
                a: Some(aa + (ba - aa) * t),
                ..base
            });
        }
        v
    };
    if a.held || b.held {
        if a.held && b.held && turn <= RETRACTED_A_STEP {
            return (Vec::new(), 0);
        }
        return (retract(a.held && b.held), 0);
    }
    if turn <= m.cfg.max_a_step {
        return (Vec::new(), 0);
    }
    if turn > RETRACT_TURN {
        return (retract(false), 0);
    }
    let (Some((ca, na, sa)), Some((cb, nb, _))) = (a.contact, b.contact) else {
        return (retract(false), 0);
    };
    let k = (turn / m.cfg.max_a_step).ceil() as usize;
    let mut v = Vec::with_capacity(k);
    let mut prev = aa;
    for j in 1..k {
        let t = j as f64 / k as f64;
        let c = ca.lerp(cb, t);
        let n = na.lerp(nb, t).normalize_or(nb);
        let (tip, an) = m.position(c, n, sa, Mode::Contact);
        let an = an.map(|x| unwrap_near(x, prev));
        prev = an.unwrap_or(prev);
        if tip.z < m.obstacle_floor(tip.truncate()) {
            return (retract(false), 0);
        }
        v.push(Pos { tip, a: an, ..base });
    }
    (v, 0)
}

// ------------------------------------------------------------------------------------------------ hot wire

/// Hot-wire program (`cam.md` §7.1, §7.2, §10.7): pass-through tool, no scaling.
pub(crate) fn hotwire_program(
    board: &Board,
    cfg: &HotwireConfig,
    kind: HotwireKind,
    toolpath: Toolpath,
) -> Program {
    let placement = match kind {
        HotwireKind::Profile => {
            let t = board.max_deck_at_tail();
            let n = board.max_deck_at_nose();
            let theta = (n.y - t.y).atan2(n.x - t.x);
            let zb0 = crate::eval::profile(&board.bottom, 0.0).0;
            Placement {
                angle: theta,
                offset: DVec3::new(-zb0 * theta.sin(), 0.0, -t.y * theta.cos()),
                ..Default::default()
            }
        }
        HotwireKind::Outline => Placement::default(),
    };
    let feed = (cfg.speed * 60.0).trunc().max(1.0);
    let mut cmds = vec![
        Command::Comment("Generated with SurfCraft".into()),
        Command::Comment(
            match kind {
                HotwireKind::Profile => "Hot-wire profile",
                HotwireKind::Outline => "Hot-wire outline",
            }
            .into(),
        ),
        Command::Comment(format!("Board: {}", describe_board(board))),
        Command::Raw("G21".into()),
        Command::Raw("G90".into()),
        Command::SpindleOn,
    ];
    let mut prev: Option<[i64; 3]> = None;
    for (pi, pass) in toolpath.passes.iter().enumerate() {
        for p in &pass.points {
            let q = placement.point(p.pos);
            let key = [
                (q.x * 1e4).round() as i64,
                (q.y * 1e4).round() as i64,
                (q.z * 1e4).round() as i64,
            ];
            if prev == Some(key) {
                continue;
            }
            prev = Some(key);
            cmds.push(Command::Move(Move {
                pos: q,
                feed,
                a: None,
                pass: Some(pi),
            }));
        }
    }
    cmds.push(Command::SpindleOff);
    cmds.push(Command::End);
    Program::new(
        BoardSide::Deck,
        toolpath,
        placement,
        DVec3::ZERO,
        cmds,
        AxisScaling::default(),
    )
}
