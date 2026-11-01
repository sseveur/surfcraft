//! Blank holding systems: placement of the board on the machine and the obstacles the tool must avoid
//! (`cam.md` §4.4, §9).

use surfcraft_core::{Board, DVec2, DVec3};

use crate::config::{BoardSide, EndClampsConfig, HoldingSystem, SupportsConfig};
use crate::eval::profile;

/// Side frame (cm) to reference frame (mm): mirror z for the bottom (`flip`), rotation `angle` (radians) about Y,
/// then `offset` (cm), then `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub angle: f64,
    pub offset: DVec3,
    pub scale: f64,
    /// Board turned over (bottom program): board-frame z is negated before the rotation.
    pub flip: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            angle: 0.0,
            offset: DVec3::ZERO,
            scale: 10.0,
            flip: false,
        }
    }
}

impl Placement {
    fn rotate(&self, p: DVec3) -> DVec3 {
        let (s, c) = self.angle.sin_cos();
        DVec3::new(c * p.x + s * p.z, p.y, -s * p.x + c * p.z)
    }

    fn unrotate(&self, p: DVec3) -> DVec3 {
        let (s, c) = self.angle.sin_cos();
        DVec3::new(c * p.x - s * p.z, p.y, s * p.x + c * p.z)
    }

    fn mirror(&self, p: DVec3) -> DVec3 {
        if self.flip {
            DVec3::new(p.x, p.y, -p.z)
        } else {
            p
        }
    }

    /// Board-frame point (cm) to the reference frame (mm).
    pub fn point(&self, p: DVec3) -> DVec3 {
        self.scale * (self.rotate(self.mirror(p)) + self.offset)
    }

    /// Board-frame direction to the reference frame (unscaled).
    pub fn vector(&self, v: DVec3) -> DVec3 {
        self.rotate(self.mirror(v))
    }

    /// Reference-frame point (mm) back to the board frame (cm).
    pub fn inverse_point(&self, p: DVec3) -> DVec3 {
        self.mirror(self.unrotate(p / self.scale - self.offset))
    }

    /// Reference-frame direction back to the board frame.
    pub fn inverse_vector(&self, v: DVec3) -> DVec3 {
        self.mirror(self.unrotate(v))
    }
}

/// Block the tool must stay out of (reference frame, or the turned frame of the rotating board, mm): the tool, a
/// vertical cylinder unbounded upwards, collides when its tip is below `top` and its section overlaps the
/// rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Obstacle {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub top: f64,
}

impl Obstacle {
    /// Whether a tool of `radius` at `(x, y)` is over the block.
    #[inline]
    pub fn under(&self, x: f64, y: f64, radius: f64) -> bool {
        let dx = x - x.clamp(self.x0, self.x1);
        let dy = y - y.clamp(self.y0, self.y1);
        dx * dx + dy * dy < radius * radius
    }
}

/// Side-frame (x, z) curve of the board's lowest centreline for a side: the rocker for the deck program, the
/// mirrored deck for the bottom program.
fn resting_curve(board: &Board, side: BoardSide) -> impl Fn(f64) -> f64 + '_ {
    move |x| match side {
        BoardSide::Deck => profile(&board.bottom, x).0,
        BoardSide::Bottom => -profile(&board.deck, x).0,
    }
}

/// Side-frame points of the tail end, for the tail-stop contact.
fn tail_points(board: &Board, side: BoardSide) -> Vec<DVec2> {
    let l = board.length();
    let mut v = Vec::new();
    for i in 0..=20 {
        let x = (l * 0.02).min(2.0) * i as f64 / 20.0;
        let (d, b) = (profile(&board.deck, x).0, profile(&board.bottom, x).0);
        match side {
            BoardSide::Deck => {
                v.push(DVec2::new(x, d));
                v.push(DVec2::new(x, b));
            }
            BoardSide::Bottom => {
                v.push(DVec2::new(x, -d));
                v.push(DVec2::new(x, -b));
            }
        }
    }
    v
}

/// Rotation and offset (cm) that put `curve` on both supports with the rearmost of `rear` (shifted by `rear_shift`
/// along X after placement) against the tail stop.
fn rest_on_supports(
    s: &SupportsConfig,
    len: f64,
    curve: impl Fn(f64) -> f64,
    rear: &[DVec2],
    rear_shift: f64,
) -> (f64, DVec3) {
    let xt = s.tailstop_pos / 10.0;
    let (x1, h1) = (
        xt + s.support1_from_tailstop / 10.0,
        s.support1_height / 10.0,
    );
    let (x2, h2) = (
        xt + s.support2_from_tailstop / 10.0,
        s.support2_height / 10.0,
    );
    let solve = |theta: f64| -> (f64, f64, f64) {
        let (sn, cs) = theta.sin_cos();
        let rearmost = rear
            .iter()
            .map(|p| cs * p.x + sn * p.y)
            .fold(f64::MAX, f64::min);
        let ox = xt - rearmost + rear_shift;
        let big_x = |x: f64| cs * x + sn * curve(x) + ox;
        let at = |target: f64| {
            let (mut a, mut b) = (0.0, len);
            for _ in 0..60 {
                let m = (a + b) / 2.0;
                if big_x(m) < target { a = m } else { b = m }
            }
            (a + b) / 2.0
        };
        let z = |x: f64| -sn * x + cs * curve(x);
        let (c1, c2) = (at(x1), at(x2));
        let oz = h1 - z(c1);
        (z(c2) + oz - h2, ox, oz)
    };
    let mut t0 = (h1 - h2).atan2(x2 - x1);
    let mut f0 = solve(t0).0;
    let mut t1 = t0 + 0.01;
    for _ in 0..60 {
        let f1 = solve(t1).0;
        if f1.abs() < 1e-10 || (f1 - f0).abs() < 1e-15 {
            break;
        }
        let t2 = t1 - f1 * (t1 - t0) / (f1 - f0);
        (t0, f0, t1) = (t1, f1, t2.clamp(-1.2, 1.2));
    }
    let (_, ox, oz) = solve(t1);
    (t1, DVec3::new(ox, 0.0, oz))
}

/// Blank on end clamps (`cam.md` §9.2.1): deck high points at the vertical offset from the clamp axis, the tail
/// `clamp_overlap` inside the tail clamp. Returns the deck placement and the rotated length.
fn clamped(c: &EndClampsConfig, blank: &Board) -> (Placement, f64) {
    let t = blank.max_deck_at_tail();
    let n = blank.max_deck_at_nose();
    let theta = (n.y - t.y).atan2(n.x - t.x);
    let (sn, cs) = theta.sin_cos();
    let v = cs * t.y - sn * t.x - c.blank_vertical_offset;
    let h = -sn * t.y;
    let l = blank.length();
    let rotated = sn * (profile(&blank.bottom, l).0 - profile(&blank.bottom, 0.0).0) + cs * l;
    (
        Placement {
            angle: theta,
            offset: DVec3::new(h - c.clamp_overlap, 0.0, -v),
            ..Default::default()
        },
        rotated,
    )
}

/// Default placement of a board inside a placed blank (`cam.md` §9.1.3): tail-aligned, decks level at the tail.
fn inside_blank(board: &Board, blank: &Board, blank_pl: &Placement) -> Placement {
    let l = board.length();
    let (xt, xn) = (1.0f64.min(l / 2.0), (l - 0.5).max(l / 2.0));
    let tb = blank_pl.angle;
    let rot = |x: f64, z: f64, a: f64| z * a.cos() - x * a.sin();
    let (bd_t, bd_n) = (profile(&blank.deck, xt).0, profile(&blank.deck, xn).0);
    let (d_t, d_n) = (profile(&board.deck, xt).0, profile(&board.deck, xn).0);
    let blank_chord = (rot(xn, bd_n, tb) - rot(xt, bd_t, tb)).atan2(xn - xt);
    let board_chord = (d_n - d_t).atan2(xn - xt);
    let theta = board_chord - blank_chord;
    let oz = blank_pl.offset.z + rot(xt, bd_t, tb) - rot(xt, d_t, theta);
    Placement {
        angle: theta,
        offset: DVec3::new(blank_pl.offset.x, 0.0, oz),
        ..Default::default()
    }
}

/// The mirror of a deck placement about the reference plane Z = 0 (the board turned over between clamps).
fn mirrored(p: Placement) -> Placement {
    Placement {
        angle: -p.angle,
        offset: DVec3::new(p.offset.x, p.offset.y, -p.offset.z),
        flip: true,
        ..p
    }
}

impl HoldingSystem {
    /// Placement of one side of `board` on the machine; with a `blank`, the board lies inside the placed blank.
    pub fn placement(&self, board: &Board, blank: Option<&Board>, side: BoardSide) -> Placement {
        self.placements(board, blank, side).0
    }

    /// Placements of the board and of the blank (the board itself when there is none) for `side`.
    pub(crate) fn placements(
        &self,
        board: &Board,
        blank: Option<&Board>,
        side: BoardSide,
    ) -> (Placement, Placement) {
        match self {
            HoldingSystem::Supports(s) => {
                let blank_board = blank.unwrap_or(board);
                let (ta, oa) = rest_on_supports(
                    s,
                    blank_board.length(),
                    resting_curve(blank_board, BoardSide::Deck),
                    &tail_points(blank_board, BoardSide::Deck),
                    0.0,
                );
                let blank_deck = Placement {
                    angle: ta,
                    offset: oa,
                    ..Default::default()
                };
                let board_deck = match blank {
                    Some(b) => inside_blank(board, b, &blank_deck),
                    None => blank_deck,
                };
                match side {
                    BoardSide::Deck => (board_deck, blank_deck),
                    BoardSide::Bottom => {
                        // The machined deck rests on the supports; the blank's tail keeps its distance from the
                        // board's tail along X.
                        let shift = match blank {
                            Some(_) => {
                                let board_tail = board_deck.point(DVec3::ZERO).x / 10.0;
                                let blank_tail = blank_deck.point(DVec3::ZERO).x / 10.0;
                                board_tail - blank_tail
                            }
                            None => 0.0,
                        };
                        let (tb, ob) = rest_on_supports(
                            s,
                            board.length(),
                            resting_curve(board, BoardSide::Bottom),
                            &tail_points(board, BoardSide::Bottom),
                            shift,
                        );
                        let board_bottom = Placement {
                            angle: tb,
                            offset: ob,
                            flip: true,
                            ..Default::default()
                        };
                        let blank_bottom = match blank {
                            Some(_) => Placement {
                                offset: ob - DVec3::new(shift, 0.0, 0.0),
                                ..board_bottom
                            },
                            None => board_bottom,
                        };
                        (board_bottom, blank_bottom)
                    }
                }
            }
            HoldingSystem::EndClamps(c) | HoldingSystem::RotatingBoard(c) => {
                let blank_board = blank.unwrap_or(board);
                let (blank_deck, _) = clamped(c, blank_board);
                let board_deck = match blank {
                    Some(b) => inside_blank(board, b, &blank_deck),
                    None => blank_deck,
                };
                match side {
                    BoardSide::Deck => (board_deck, blank_deck),
                    BoardSide::Bottom => (mirrored(board_deck), mirrored(blank_deck)),
                }
            }
        }
    }

    /// Blocks the tool must avoid, in the frame in which the tool axis is vertical (mm).
    pub(crate) fn obstacles(&self, board: &Board, blank: Option<&Board>) -> Vec<Obstacle> {
        const FAR: f64 = 1e7;
        match self {
            HoldingSystem::Supports(s) => {
                let xt = s.tailstop_pos;
                let x1 = xt + s.support1_from_tailstop;
                let x2 = xt + s.support2_from_tailstop;
                let half = s.support_length / 2.0;
                let mut v = Vec::new();
                if s.tailstop_height > 0.0 {
                    v.push(Obstacle {
                        x0: -FAR,
                        x1: xt,
                        y0: -FAR,
                        y1: FAR,
                        top: s.support1_height + s.tailstop_height,
                    });
                }
                v.extend([
                    Obstacle {
                        x0: x1 - half,
                        x1: x1 + half,
                        y0: -FAR,
                        y1: FAR,
                        top: s.support1_height,
                    },
                    Obstacle {
                        x0: x2 - half,
                        x1: x2 + half,
                        y0: -FAR,
                        y1: FAR,
                        top: s.support2_height,
                    },
                ]);
                v
            }
            HoldingSystem::EndClamps(c) | HoldingSystem::RotatingBoard(c) => {
                let (_, rotated) = clamped(c, blank.unwrap_or(board));
                let (len, s) = (c.clamp_length * 10.0, c.clamp_safe_zone * 10.0);
                let (half_w, half_h) = (c.clamp_width * 5.0 + s, c.clamp_height * 5.0 + s);
                let far = rotated * 10.0 - 2.0 * c.clamp_overlap * 10.0;
                // The rotating clamps sweep a circle about the axis: guard the square around it.
                let (half_w, top) = if matches!(self, HoldingSystem::RotatingBoard(_)) {
                    let r = half_w.hypot(half_h);
                    (r, r)
                } else {
                    (half_w, half_h)
                };
                vec![
                    Obstacle {
                        x0: -len - s,
                        x1: s,
                        y0: -half_w,
                        y1: half_w,
                        top,
                    },
                    Obstacle {
                        x0: far - s,
                        x1: far + len + s,
                        y0: -half_w,
                        y1: half_w,
                        top,
                    },
                ]
            }
        }
    }

    /// One-line description for the program header.
    pub(crate) fn describe(&self) -> String {
        match self {
            HoldingSystem::Supports(s) => format!(
                "Supports, tail stop at {:.1} mm, support 1 {:.1} mm from it at {:.1} mm high, support 2 {:.1} mm \
                 from it at {:.1} mm high",
                s.tailstop_pos,
                s.support1_from_tailstop,
                s.support1_height,
                s.support2_from_tailstop,
                s.support2_height
            ),
            HoldingSystem::EndClamps(c) => format!("End clamps, {}", clamps_text(c)),
            HoldingSystem::RotatingBoard(c) => {
                format!("Rotating board system, {}", clamps_text(c))
            }
        }
    }
}

fn clamps_text(c: &EndClampsConfig) -> String {
    format!(
        "clamps {:.1} x {:.1} x {:.1} cm, safe zone {:.1} cm, overlap {:.1} cm, blank offset {:.1} cm",
        c.clamp_length,
        c.clamp_width,
        c.clamp_height,
        c.clamp_safe_zone,
        c.clamp_overlap,
        c.blank_vertical_offset
    )
}
