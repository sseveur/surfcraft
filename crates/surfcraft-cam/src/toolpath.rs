//! Contact-point toolpaths: surface splits (`cam.md` §5), width splits (§6) and the hot-wire curves (§7).
//!
//! Toolpath points are in the board frame (cm): the contact point on the board surface and its outward unit
//! normal. The bottom program's points are on the bottom (normals pointing down); its placement turns the board
//! over.

use rayon::prelude::*;
use surfcraft_core::{Board, DVec2, DVec3};

use crate::config::{
    BoardSide, BottomStart, DeckStart, GeneratorKind, HotwireConfig, HotwireKind, MachineConfig,
    OutlineDepth,
};
use crate::eval::{Slice, Surf};
use crate::{Error, MAX_POINTS};

/// What a pass does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassKind {
    Approach,
    Stringer,
    Regular,
    Rail,
    Outline,
    Cleanup,
    SideChange,
    NoseCutOff,
    TailCutOff,
    WidthSplit,
    HotwireProfile,
    HotwireOutline,
}

/// Board half a pass runs on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Side {
    #[default]
    Right,
    Left,
    Center,
}

impl Side {
    /// +1 right, -1 left, 0 centre.
    pub fn sign(self) -> f64 {
        match self {
            Side::Right => 1.0,
            Side::Left => -1.0,
            Side::Center => 0.0,
        }
    }
}

/// Contact point on the board surface (board frame, cm) with its outward unit normal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolPoint {
    pub pos: DVec3,
    pub normal: DVec3,
}

/// One pass of contact points.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub kind: PassKind,
    pub side: Side,
    /// Number of this pass among the passes of its kind and side.
    pub index: usize,
    /// Leading transition points before the pass proper.
    pub lead_in: usize,
    pub points: Vec<ToolPoint>,
}

/// All passes of one program, in cutting order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub passes: Vec<Pass>,
}

impl Toolpath {
    /// Every contact point, in order.
    pub fn points(&self) -> impl Iterator<Item = &ToolPoint> + '_ {
        self.passes.iter().flat_map(|p| p.points.iter())
    }

    /// Number of passes of `kind`, on `side` when given.
    pub fn count(&self, kind: PassKind, side: Option<Side>) -> usize {
        self.passes
            .iter()
            .filter(|p| p.kind == kind && side.is_none_or(|s| p.side == s))
            .count()
    }

    pub fn point_count(&self) -> usize {
        self.passes.iter().map(|p| p.points.len()).sum()
    }
}

/// A toolpath with the notes that go into the program header.
pub(crate) struct Planned {
    pub toolpath: Toolpath,
    pub notes: Vec<String>,
}

fn map<T: Send, R: Send>(
    parallel: bool,
    items: Vec<T>,
    f: impl Fn(T) -> R + Sync + Send,
) -> Vec<R> {
    if parallel {
        items.into_par_iter().map(f).collect()
    } else {
        items.into_iter().map(f).collect()
    }
}

fn mirror(p: ToolPoint) -> ToolPoint {
    ToolPoint {
        pos: DVec3::new(p.pos.x, -p.pos.y, p.pos.z),
        normal: DVec3::new(p.normal.x, -p.normal.y, p.normal.z),
    }
}

fn on(side: Side, p: ToolPoint) -> ToolPoint {
    if side == Side::Left { mirror(p) } else { p }
}

fn tp((pos, normal): (DVec3, DVec3)) -> ToolPoint {
    ToolPoint { pos, normal }
}

/// Lengthwise stations of a pass: `x_k = clamp(k Δ, 0.1, L - 0.1)`, `k = 0 ..= N`.
fn stations(len: f64, res: f64) -> Result<Vec<f64>, Error> {
    let lp = len - 0.1;
    let n = (lp / res).floor() + 1.0;
    if !(n.is_finite()) || n > MAX_POINTS as f64 {
        return Err(Error::TooManyPoints);
    }
    let n = n as usize;
    Ok((0..=n)
        .map(|k| (k as f64 * res).clamp(0.1f64.min(lp), lp))
        .collect())
}

/// Board shape check shared by the generators.
pub(crate) fn check_board(board: &Board) -> Result<(), Error> {
    let l = board.length();
    if board.is_empty()
        || l.is_nan()
        || l <= 1.0
        || board.sections.len() < 3
        || board.deck.segment_count() == 0
        || board.bottom.segment_count() == 0
        || board.outline.segment_count() == 0
        || board.max_width().is_nan()
        || board.max_width() <= 0.0
    {
        return Err(Error::InvalidBoard);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ surface splits

/// One band of passes between two normal angles.
#[derive(Clone, Copy, Debug)]
struct Band {
    a: f64,
    b: f64,
    n: usize,
}

impl Band {
    fn passes(&self) -> usize {
        if (self.a - self.b).abs() < 1e-9 {
            0
        } else {
            self.n
        }
    }
}

/// Settings of one program resolved for the generator.
struct Setup {
    side: BoardSide,
    main: Band,
    rail: Band,
    stringer: bool,
    outline: bool,
    cleanups: usize,
    /// Normal angle of the outline pass height.
    outline_angle: f64,
    /// Normal angle of the bottom clean-up reference point.
    cleanup_angle: f64,
    /// Outer angle of the last band with passes.
    last_angle: f64,
}

fn setup(cfg: &MachineConfig, side: BoardSide, notes: &mut Vec<String>) -> Setup {
    let three = !cfg.four_axis();
    let mut limit = |name: &str, v: f64, deck: bool| -> f64 {
        let lim = if deck { v.min(90.0) } else { v.max(90.0) };
        if three && lim != v {
            notes.push(format!(
                "{name} {v} limited to {lim} degrees: a 3-axis tool cannot reach past the rail apex"
            ));
            lim
        } else {
            v
        }
    };
    match side {
        BoardSide::Deck => {
            let da = limit("Deck angle", cfg.deck_angle, true);
            let dr = if cfg.deck_rail_cuts > 0 {
                limit("Deck rail angle", cfg.deck_rail_angle, true)
            } else {
                da
            };
            let main = Band {
                a: 0.0,
                b: da,
                n: cfg.deck_cuts,
            };
            let rail = Band {
                a: da,
                b: dr,
                n: cfg.deck_rail_cuts,
            };
            let bottom_angle = if three {
                cfg.bottom_angle.max(90.0)
            } else {
                cfg.bottom_angle
            };
            Setup {
                side,
                main,
                rail,
                stringer: cfg.deck_stringer_cut,
                outline: cfg.deck_outline_cut,
                cleanups: cfg.deck_cleanup_cuts,
                outline_angle: match cfg.deck_outline_depth {
                    OutlineDepth::RailApex => 90.0,
                    OutlineDepth::Bottom => bottom_angle,
                },
                cleanup_angle: 90.0,
                last_angle: if rail.passes() > 0 { dr } else { da },
            }
        }
        BoardSide::Bottom => {
            let ba = limit("Bottom angle", cfg.bottom_angle, false);
            let br = if cfg.bottom_rail_cuts > 0 {
                limit("Bottom rail angle", cfg.bottom_rail_angle, false)
            } else {
                ba
            };
            let main = Band {
                a: 180.0,
                b: ba,
                n: cfg.bottom_cuts,
            };
            let rail = Band {
                a: ba,
                b: br,
                n: cfg.bottom_rail_cuts,
            };
            Setup {
                side,
                main,
                rail,
                stringer: cfg.bottom_stringer_cut,
                outline: cfg.bottom_outline_cut,
                cleanups: cfg.bottom_cleanup_cuts,
                outline_angle: match cfg.deck_outline_depth {
                    OutlineDepth::RailApex => 90.0,
                    OutlineDepth::Bottom => {
                        if three {
                            cfg.bottom_angle.max(90.0)
                        } else {
                            cfg.bottom_angle
                        }
                    }
                },
                cleanup_angle: br,
                last_angle: if rail.passes() > 0 { br } else { ba },
            }
        }
    }
}

/// Lengthwise pass identity within one slice's contact list.
#[derive(Clone, Copy, Debug)]
enum Lane {
    Stringer,
    Main(usize),
    Rail(usize),
    Outline,
    Cleanup(usize),
}

/// Contacts of every lengthwise pass at one station (right half).
struct SliceContacts {
    stringer: ToolPoint,
    main: Vec<ToolPoint>,
    rail: Vec<ToolPoint>,
    outline: ToolPoint,
    cleanup: Vec<ToolPoint>,
}

impl SliceContacts {
    fn get(&self, lane: Lane) -> ToolPoint {
        match lane {
            Lane::Stringer => self.stringer,
            Lane::Main(i) => self.main[i - 1],
            Lane::Rail(i) => self.rail[i - 1],
            Lane::Outline => self.outline,
            Lane::Cleanup(i) => self.cleanup[i],
        }
    }
}

fn band_s(sl: &Slice, band: &Band, f: f64) -> f64 {
    sl.band(sl.s_at_angle(band.a), sl.s_at_angle(band.b), f)
}

fn stringer_contact(surf: &Surf, x: f64, side: BoardSide) -> ToolPoint {
    match side {
        BoardSide::Deck => {
            let (z, d) = surf.deck(x);
            ToolPoint {
                pos: DVec3::new(x, 0.0, z),
                normal: DVec3::new(-d, 0.0, 1.0).normalize(),
            }
        }
        BoardSide::Bottom => {
            let (z, d) = surf.rocker(x);
            ToolPoint {
                pos: DVec3::new(x, 0.0, z),
                normal: DVec3::new(d, 0.0, -1.0).normalize(),
            }
        }
    }
}

fn slice_contacts(surf: &Surf, sl: &Slice, st: &Setup) -> SliceContacts {
    let x = sl.x();
    let band = |b: &Band| -> Vec<ToolPoint> {
        let n = b.passes();
        if n == 0 {
            return Vec::new();
        }
        let (sa, sb) = (sl.s_at_angle(b.a), sl.s_at_angle(b.b));
        (1..=n)
            .map(|i| tp(sl.contact(sl.band(sa, sb, i as f64 / n as f64))))
            .collect()
    };
    let (half, slope) = surf.outline(x);
    let apex = sl.contact(sl.s_at_angle(90.0));
    let outline = {
        let z = if st.outline_angle == 90.0 {
            apex.0.z
        } else {
            sl.contact(sl.s_at_angle(st.outline_angle)).0.z
        };
        ToolPoint {
            pos: DVec3::new(x, half.max(apex.0.y), z),
            normal: DVec3::new(-slope, 1.0, 0.0).normalize(),
        }
    };
    let cleanup = (0..st.cleanups)
        .map(|i| match st.side {
            BoardSide::Deck => ToolPoint {
                pos: DVec3::new(apex.0.x, apex.0.y, surf.rocker(x).0 + i as f64),
                normal: apex.1,
            },
            BoardSide::Bottom => {
                let (p, n) = sl.contact(sl.s_at_angle(st.cleanup_angle));
                ToolPoint {
                    pos: p + DVec3::new(0.0, (i + 1) as f64, 0.0),
                    normal: n,
                }
            }
        })
        .collect();
    SliceContacts {
        stringer: stringer_contact(surf, x, st.side),
        main: band(&st.main),
        rail: band(&st.rail),
        outline,
        cleanup,
    }
}

/// End of the board where a program finishes, from its sequence alone.
fn finishes_at_nose(cfg: &MachineConfig, side: BoardSide, start_at_nose: bool) -> bool {
    let mut notes = Vec::new();
    let st = setup(cfg, side, &mut notes);
    let mut at_nose = start_at_nose;
    let lengthwise = usize::from(st.stringer)
        + 2 * (st.main.passes() + st.rail.passes() + usize::from(st.outline) + st.cleanups);
    if lengthwise % 2 == 1 {
        at_nose = !at_nose;
    }
    at_nose
}

fn sandwich(
    cfg: &MachineConfig,
    board: &Board,
    side: BoardSide,
    kind: PassKind,
    p: ToolPoint,
) -> ToolPoint {
    let s = &cfg.sandwich;
    let (on, t, limit) = match (kind, side) {
        (PassKind::Outline, _) if s.rail => (true, s.rail_thickness, 0.0),
        (
            PassKind::Stringer
            | PassKind::Regular
            | PassKind::Rail
            | PassKind::Outline
            | PassKind::Cleanup
            | PassKind::WidthSplit,
            BoardSide::Deck,
        ) => (s.deck, s.deck_thickness, s.deck_rail_limit),
        (
            PassKind::Stringer
            | PassKind::Regular
            | PassKind::Rail
            | PassKind::Outline
            | PassKind::Cleanup
            | PassKind::WidthSplit,
            BoardSide::Bottom,
        ) => (s.bottom, s.bottom_thickness, s.bottom_rail_limit),
        _ => (false, 0.0, 0.0),
    };
    if !on || t == 0.0 {
        return p;
    }
    if limit > 0.0 {
        let q = DVec2::new(p.pos.x, p.pos.y.abs());
        let half = crate::eval::profile(&board.outline, q.x).0;
        if half - q.y < limit || board.outline.distance_to(q) < limit {
            return p;
        }
    }
    ToolPoint {
        pos: p.pos - t * p.normal,
        normal: p.normal,
    }
}

struct Builder<'a> {
    cfg: &'a MachineConfig,
    board: &'a Board,
    side: BoardSide,
    passes: Vec<Pass>,
    counts: Vec<(PassKind, Side, usize)>,
}

impl Builder<'_> {
    fn push(&mut self, kind: PassKind, side: Side, lead_in: usize, points: Vec<ToolPoint>) {
        let index = match self.counts.iter_mut().find(|c| c.0 == kind && c.1 == side) {
            Some(c) => {
                c.2 += 1;
                c.2 - 1
            }
            None => {
                self.counts.push((kind, side, 1));
                0
            }
        };
        let points = points
            .into_iter()
            .map(|p| sandwich(self.cfg, self.board, self.side, kind, p))
            .collect();
        self.passes.push(Pass {
            kind,
            side,
            index,
            lead_in,
            points,
        });
    }

    fn last(&self) -> Option<ToolPoint> {
        self.passes.last().and_then(|p| p.points.last().copied())
    }
}

/// Surface-splits toolpath of one side.
pub(crate) fn surface_splits(
    board: &Board,
    cfg: &MachineConfig,
    side: BoardSide,
) -> Result<Planned, Error> {
    let mut notes = Vec::new();
    let st = setup(cfg, side, &mut notes);
    let surf = Surf::new(board);
    let len = surf.len;
    let xs = stations(len, cfg.lengthwise_resolution)?;
    let lanes_per_side =
        st.main.passes() + st.rail.passes() + usize::from(st.outline) + st.cleanups;
    let cut_offs = match side {
        BoardSide::Deck => cfg.cut_off_nose,
        BoardSide::Bottom => cfg.cut_off_tail,
    };
    if lanes_per_side == 0 && !st.stringer && !cut_offs {
        return Err(Error::NothingToCut);
    }
    let lanes = usize::from(st.stringer) + 2 * lanes_per_side;
    if (lanes as f64) * (xs.len() as f64 + 2.0 + cfg.move_between_steps as f64) > MAX_POINTS as f64
    {
        return Err(Error::TooManyPoints);
    }
    let n = xs.len() - 1;
    let slices: Vec<SliceContacts> = map(cfg.parallel, xs.clone(), |x| {
        let sl = Slice::new(&surf, x);
        slice_contacts(&surf, &sl, &st)
    });
    let ends = [Slice::new(&surf, xs[0]), Slice::new(&surf, xs[n])];
    let end_slice = |at_nose: bool| &ends[usize::from(at_nose)];

    let mut b = Builder {
        cfg,
        board,
        side,
        passes: Vec::new(),
        counts: Vec::new(),
    };
    let lengthwise = |lane: Lane, side: Side, forward: bool, extra: bool| -> Vec<ToolPoint> {
        let mut v: Vec<ToolPoint> = if forward {
            (0..=n).map(|k| on(side, slices[k].get(lane))).collect()
        } else {
            (0..=n)
                .rev()
                .map(|k| on(side, slices[k].get(lane)))
                .collect()
        };
        if extra && let Some(&l) = v.last() {
            v.push(l);
        }
        v
    };
    let cut_off = |nose: bool| -> Vec<ToolPoint> {
        let (d, r) = (
            cfg.cutter.cutting_radius() / 5.0,
            cfg.cutter.corner_radius() / 10.0,
        );
        let w = cfg.stringer_width;
        let (xq, q, up) = if nose {
            let x = len - 1.0;
            (x, DVec3::new(x, 0.0, surf.deck(x).0), 1.0)
        } else {
            (1.0, DVec3::new(1.0, 0.0, surf.rocker(1.0).0), -1.0)
        };
        let t = surf.deck(xq).0 - surf.rocker(xq).0;
        let lift = 5.0;
        let last = if nose {
            (-(d / 2.0 + 1.0), t + lift)
        } else {
            (-(d / 2.0 - 0.1), t + r + lift)
        };
        let steps = [
            (w + d, t + lift),
            (w + d, -r),
            (d / 2.0 + 0.1, -r),
            (d / 2.0 + 0.1, t + lift),
            (-(w + d), t + lift),
            (-(w + d), -r),
            (-(d / 2.0 + 0.1), -r),
            last,
        ];
        let normal = DVec3::new(if nose { 1.0 } else { -1.0 }, 0.0, 0.0);
        steps
            .iter()
            .map(|&(dy, dz)| ToolPoint {
                pos: q + DVec3::new(0.0, dy, up * dz),
                normal,
            })
            .collect()
    };

    // Start: stringer and cut-off at the chosen end.
    let mut at_nose = match side {
        BoardSide::Deck => cfg.deck_start == DeckStart::Nose,
        BoardSide::Bottom => match cfg.bottom_start {
            BottomStart::Nose => true,
            BottomStart::Tail => false,
            BottomStart::Previous => {
                finishes_at_nose(cfg, BoardSide::Deck, cfg.deck_start == DeckStart::Nose)
            }
        },
    };
    let (cut_kind, cut_at_nose) = match side {
        BoardSide::Deck => (PassKind::NoseCutOff, true),
        BoardSide::Bottom => (PassKind::TailCutOff, false),
    };
    let stringer_first = at_nose != cut_at_nose;
    if stringer_first && st.stringer {
        b.push(
            PassKind::Stringer,
            Side::Center,
            0,
            lengthwise(Lane::Stringer, Side::Center, !at_nose, true),
        );
        at_nose = !at_nose;
    }
    if cut_offs {
        b.push(cut_kind, Side::Center, 0, cut_off(cut_at_nose));
        at_nose = cut_at_nose;
    }
    if !stringer_first && st.stringer {
        b.push(
            PassKind::Stringer,
            Side::Center,
            0,
            lengthwise(Lane::Stringer, Side::Center, !at_nose, true),
        );
        at_nose = !at_nose;
    }

    for half in [Side::Right, Side::Left] {
        if lanes_per_side == 0 {
            break;
        }
        if half == Side::Left {
            // Side change at the current end.
            let sl = end_slice(at_nose);
            let steps = cfg.side_change_steps;
            let (s0, s_last) = match side {
                BoardSide::Deck => (0.0, sl.s_at_angle(st.last_angle)),
                BoardSide::Bottom => (1.0, sl.s_at_angle(st.last_angle)),
            };
            let mut pts = Vec::with_capacity(steps + 1);
            if cfg.stay_away_from_stringer {
                let past = match side {
                    BoardSide::Deck => st.last_angle > 90.0,
                    BoardSide::Bottom => st.last_angle < 90.0,
                };
                let apex = sl.s_at_angle(90.0);
                let mut prev = b.last().unwrap_or_else(|| tp(sl.contact(s_last)));
                for j in 0..steps - 1 {
                    if past {
                        let f = (steps - j) as f64 / steps as f64;
                        prev = tp(sl.contact(sl.band(apex, s_last, f)));
                    }
                    pts.push(prev);
                }
                let up = match side {
                    BoardSide::Deck => 1.0,
                    BoardSide::Bottom => -1.0,
                };
                let normal = DVec3::new(if at_nose { 1.0 } else { -1.0 }, 0.0, 0.0);
                let raised = prev.pos + DVec3::new(0.0, 0.0, 5.0 * up);
                pts.push(ToolPoint {
                    pos: raised,
                    normal,
                });
                let y = match side {
                    BoardSide::Deck => -2.5 * cfg.stringer_width,
                    BoardSide::Bottom => -0.5 * cfg.stringer_width,
                };
                pts.push(ToolPoint {
                    pos: DVec3::new(raised.x, y, raised.z),
                    normal,
                });
            } else {
                for j in 0..=steps {
                    let f = (steps - j) as f64 / steps as f64;
                    pts.push(tp(sl.contact(sl.band(s0, s_last, f))));
                }
            }
            b.push(PassKind::SideChange, Side::Center, 0, pts);
        }
        for (band, rail) in [(&st.main, false), (&st.rail, true)] {
            let n_b = band.passes();
            let kind = if rail {
                PassKind::Rail
            } else {
                PassKind::Regular
            };
            let m = cfg.move_between_steps;
            for i in 1..=n_b {
                let sl = end_slice(at_nose);
                let mut pts: Vec<ToolPoint> = (0..m)
                    .map(|k| {
                        let f = (((i - 1) * m + k) as f64 / (n_b * m) as f64).min(1.0);
                        on(half, tp(sl.contact(band_s(sl, band, f))))
                    })
                    .collect();
                let lane = if rail { Lane::Rail(i) } else { Lane::Main(i) };
                pts.extend(lengthwise(lane, half, !at_nose, false));
                b.push(kind, half, m, pts);
                at_nose = !at_nose;
            }
        }
        if st.outline {
            b.push(
                PassKind::Outline,
                half,
                0,
                lengthwise(Lane::Outline, half, !at_nose, true),
            );
            at_nose = !at_nose;
        }
        for i in 0..st.cleanups {
            b.push(
                PassKind::Cleanup,
                half,
                0,
                lengthwise(Lane::Cleanup(i), half, !at_nose, true),
            );
            at_nose = !at_nose;
        }
    }
    if b.passes.iter().all(|p| p.points.is_empty()) {
        return Err(Error::NothingToCut);
    }
    let passes = with_approach(b.passes, side);
    Ok(Planned {
        toolpath: Toolpath { passes },
        notes,
    })
}

/// Prepend the approach pass: 5.08 cm above the first point on the machined side.
fn with_approach(passes: Vec<Pass>, side: BoardSide) -> Vec<Pass> {
    let first = passes
        .iter()
        .find_map(|p| p.points.first().copied().map(|q| (q, p.side)));
    let mut out = Vec::with_capacity(passes.len() + 1);
    if let Some((q, s)) = first {
        let up = match side {
            BoardSide::Deck => 1.0,
            BoardSide::Bottom => -1.0,
        };
        out.push(Pass {
            kind: PassKind::Approach,
            side: s,
            index: 0,
            lead_in: 0,
            points: vec![ToolPoint {
                pos: q.pos + DVec3::new(0.0, 0.0, 5.08 * up),
                normal: q.normal,
            }],
        });
    }
    out.extend(passes);
    out
}

// ------------------------------------------------------------------------------------------------ width splits

/// Width-splits toolpath (`cam.md` §6): a centre pass and `splits - 1` passes per side at fractions of the
/// half-width, serpentine from the tail, sampled at the lengthwise resolution.
pub(crate) fn width_splits(
    board: &Board,
    cfg: &MachineConfig,
    side: BoardSide,
    splits: usize,
) -> Result<Planned, Error> {
    let surf = Surf::new(board);
    let xs = stations(surf.len, cfg.lengthwise_resolution)?;
    let passes_n = 2 * splits - 1;
    if passes_n as f64 * xs.len() as f64 > MAX_POINTS as f64 {
        return Err(Error::TooManyPoints);
    }
    let deck = side == BoardSide::Deck;
    // Per station: the right-side contacts at fractions 0 .. splits-1.
    let rows: Vec<Vec<ToolPoint>> = map(cfg.parallel, xs.clone(), |x| {
        let sl = Slice::new(&surf, x);
        let half = surf.width(sl.x()) / 2.0;
        (0..splits)
            .map(|k| {
                if k == 0 {
                    return stringer_contact(&surf, sl.x(), side);
                }
                let y = half * k as f64 / splits as f64;
                tp(sl.contact(sl.s_at_lateral(y, deck)))
            })
            .collect()
    });
    let mut b = Builder {
        cfg,
        board,
        side,
        passes: Vec::new(),
        counts: Vec::new(),
    };
    let mut forward = true;
    let order = std::iter::once((0, Side::Center))
        .chain((1..splits).map(|k| (k, Side::Right)))
        .chain((1..splits).map(|k| (k, Side::Left)));
    for (k, s) in order {
        let mut pts: Vec<ToolPoint> = rows.iter().map(|r| on(s, r[k])).collect();
        if !forward {
            pts.reverse();
        }
        b.push(PassKind::WidthSplit, s, 0, pts);
        forward = !forward;
    }
    Ok(Planned {
        toolpath: Toolpath {
            passes: with_approach(b.passes, side),
        },
        notes: Vec::new(),
    })
}

/// Contact-point toolpath of one milling program.
pub(crate) fn plan(board: &Board, cfg: &MachineConfig, side: BoardSide) -> Result<Planned, Error> {
    check_board(board)?;
    cfg.validate(side)?;
    match cfg.generator {
        GeneratorKind::SurfaceSplits => surface_splits(board, cfg, side),
        GeneratorKind::WidthSplits { splits } => width_splits(board, cfg, side, splits),
    }
}

// ------------------------------------------------------------------------------------------------ hot wire

/// Hot-wire curves (`cam.md` §7.1, §7.2), board frame (cm): points `(x, 0, z)` with the profile height or the
/// outline half-width as z, offset outward by the kerf and the extra material.
pub(crate) fn hotwire(board: &Board, cfg: &HotwireConfig, kind: HotwireKind) -> Toolpath {
    let n = cfg.splits;
    let kerf = cfg.kerf / 20.0;
    // Samples of a curve at the segment-index parameter, with the outward unit normal (`up` true: the side of
    // +y in the curve plane).
    let curve = |spline: &surfcraft_core::Spline,
                 g0: f64,
                 g1: f64,
                 count: usize,
                 up: bool|
     -> Vec<(DVec2, DVec2)> {
        let m = spline.segment_count();
        (0..=count)
            .map(|j| {
                let g = if count == 0 {
                    g0
                } else {
                    g0 + (g1 - g0) * j as f64 / count as f64
                };
                let gm = (g.clamp(0.0, 1.0) * m as f64).min(m as f64);
                let i = (gm.floor() as usize).min(m - 1);
                let seg = spline.segment(i);
                let t = gm - i as f64;
                let d = seg.derivative(t);
                let mut nrm = DVec2::new(-d.y, d.x).normalize_or_zero();
                if (nrm.y < 0.0) == up && nrm != DVec2::ZERO {
                    nrm = -nrm;
                }
                (seg.point(t), nrm)
            })
            .collect()
    };
    // The extra material and the kerf widen the shape across the curve plane: up on the deck, down on the bottom,
    // outward from the stringer on the outline.
    let pt = |p: DVec2, nrm: DVec2, extra: f64| -> ToolPoint {
        let q = p + DVec2::new(0.0, nrm.y.signum() * (extra + kerf));
        let normal = DVec3::new(nrm.x, 0.0, nrm.y);
        ToolPoint {
            pos: DVec3::new(q.x, 0.0, q.y),
            normal: if normal == DVec3::ZERO {
                DVec3::Z
            } else {
                normal
            },
        }
    };
    let mk = |kind: PassKind, side: Side, index: usize, points: Vec<ToolPoint>| Pass {
        kind,
        side,
        index,
        lead_in: 0,
        points,
    };
    match kind {
        HotwireKind::Profile => {
            let k = PassKind::HotwireProfile;
            let (a, bb) = if cfg.begin_at_nose {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            let deck: Vec<ToolPoint> = curve(&board.deck, a, bb, n, true)
                .into_iter()
                .map(|(p, nr)| pt(p, nr, cfg.expand))
                .collect();
            let bottom: Vec<ToolPoint> = curve(&board.bottom, bb, a, n, false)
                .into_iter()
                .map(|(p, nr)| pt(p, nr, 0.0))
                .collect();
            // Close the loop: re-trace the last 1/m of the deck from where the bottom ended (m deck knots).
            let m = board.deck.len().max(1);
            let count = n / m;
            let span = count as f64 / n as f64;
            let end = if cfg.begin_at_nose { 1.0 - span } else { span };
            let closing: Vec<ToolPoint> = curve(&board.deck, a, end, count, true)
                .into_iter()
                .map(|(p, nr)| pt(p, nr, cfg.expand))
                .collect();
            Toolpath {
                passes: vec![
                    mk(k, Side::Center, 0, deck),
                    mk(k, Side::Center, 1, bottom),
                    mk(k, Side::Center, 2, closing),
                ],
            }
        }
        HotwireKind::Outline => {
            let k = PassKind::HotwireOutline;
            let out = cfg.expand / 2.0;
            let (a, bb) = if cfg.begin_at_nose {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            let right: Vec<ToolPoint> = curve(&board.outline, a, bb, n, true)
                .into_iter()
                .map(|(p, nr)| pt(p, nr, out))
                .collect();
            let left: Vec<ToolPoint> = right
                .iter()
                .rev()
                .map(|q| ToolPoint {
                    pos: DVec3::new(q.pos.x, 0.0, -q.pos.z),
                    normal: DVec3::new(q.normal.x, 0.0, -q.normal.z),
                })
                .collect();
            Toolpath {
                passes: vec![mk(k, Side::Right, 0, right), mk(k, Side::Left, 0, left)],
            }
        }
    }
}
