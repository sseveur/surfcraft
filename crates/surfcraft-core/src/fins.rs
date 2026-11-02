//! Fin setups: the fins of a board with their positions, toe, cant and fin-box system, the standard presets
//! (single, twin, twin + trailer, thruster, quad, 2+1, five boxes) and the box footprints drawn on the plan view
//! and printed on the outline template.
//!
//! Lengths are centimetres, angles degrees. Every fin is stored explicitly (both sides), with a signed lateral
//! offset, so asymmetric boards need nothing new. Sources of every dimension: `docs/fins.md`.
//!
//! Frame: x from the tail along the stringer, y across; y > 0 is the rider's left when standing on the deck
//! facing the nose (the side drawn above the stringer in the outline view), y < 0 the right.

use glam::DVec2;

use crate::Board;
use crate::asym::Rail;
use crate::fin as legacy;
use crate::math::DEG;
use crate::units::INCH;

/// How a fin is attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoxSystem {
    /// Position only (glassed-on fin, or just a mark).
    #[default]
    None,
    /// FCS I: two round plugs per fin.
    FcsI,
    /// FCS II: one long plug ("Fusion" / "Tab" box).
    FcsII,
    /// Futures: one long box.
    Futures,
    /// Single-fin "US box" (10" longboard box), the fin slides in it.
    UsBox,
}

impl BoxSystem {
    pub const ALL: [BoxSystem; 5] = [
        BoxSystem::FcsI,
        BoxSystem::FcsII,
        BoxSystem::Futures,
        BoxSystem::UsBox,
        BoxSystem::None,
    ];

    /// Short identifier used in files and commands.
    pub fn code(self) -> &'static str {
        match self {
            BoxSystem::None => "none",
            BoxSystem::FcsI => "fcs1",
            BoxSystem::FcsII => "fcs2",
            BoxSystem::Futures => "futures",
            BoxSystem::UsBox => "usbox",
        }
    }

    /// Parse a code; also accepts a few spellings ("FCS II", "fcs_2", "us_box", "single").
    pub fn from_code(s: &str) -> Option<BoxSystem> {
        let k: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        Some(match k.as_str() {
            "none" | "" | "glassed" | "glasson" => BoxSystem::None,
            "fcs1" | "fcsi" | "fcs" => BoxSystem::FcsI,
            "fcs2" | "fcsii" => BoxSystem::FcsII,
            "futures" | "future" => BoxSystem::Futures,
            "usbox" | "us" | "longboard" | "longboardbox" | "single" => BoxSystem::UsBox,
            _ => return None,
        })
    }

    /// English display name.
    pub fn label(self) -> &'static str {
        match self {
            BoxSystem::None => "None",
            BoxSystem::FcsI => "FCS I",
            BoxSystem::FcsII => "FCS II",
            BoxSystem::Futures => "Futures",
            BoxSystem::UsBox => "US box",
        }
    }

    /// Footprint dimensions, `None` for [`BoxSystem::None`].
    pub fn spec(self) -> Option<&'static BoxSpec> {
        match self {
            BoxSystem::None => None,
            BoxSystem::FcsI => Some(&FCS1),
            BoxSystem::FcsII => Some(&FCS2),
            BoxSystem::Futures => Some(&FUTURES),
            BoxSystem::UsBox => Some(&US_BOX),
        }
    }
}

/// Plan-view footprint of a fin box (what is routed or drilled into the blank), in cm. The footprint is
/// centred on the middle of the fin base and aligned with it. Not every number is published by the
/// manufacturers: `docs/fins.md` gives the source of each value and says which ones are approximate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxSpec {
    /// Length of the routed pocket along the fin (0 for plug-only systems).
    pub length: f64,
    /// Width of the routed pocket across the fin.
    pub width: f64,
    /// Corner radius of the pocket.
    pub corner_radius: f64,
    /// Depth of the hole or box below the bottom surface, where a source gives one.
    pub depth: Option<f64>,
    /// Round plugs: diameter of the drilled hole (0 if none).
    pub plug_diameter: f64,
    /// Round plugs: centre positions along the fin, from the middle of the fin base (+ towards the nose).
    pub plug_offsets: [f64; 2],
    /// Base length of a typical fin for this box.
    pub fin_base: f64,
    /// Whether every footprint dimension above comes from a manufacturer or retailer specification (false: some
    /// are forum measurements or estimates; check against the real box before routing).
    pub confirmed: bool,
}

impl BoxSpec {
    pub fn has_slot(&self) -> bool {
        self.length > 0.0 && self.width > 0.0
    }
    pub fn has_plugs(&self) -> bool {
        self.plug_diameter > 0.0
    }
}

const MM: f64 = 0.1;

// Box dimensions. Every value is documented, with its source, in docs/fins.md.

/// FCS I (original): two round plugs, 53 mm centre to centre (two forum measurements agree), drilled with a
/// 1 1/8" (28.6 mm) hole saw, 7/8" deep (plug 20 mm high + about 1/8"). X-2 plugs use a 30 mm saw, 5/8" deep.
/// Fin: FCS M5 base 4.37".
pub const FCS1: BoxSpec = BoxSpec {
    length: 0.0,
    width: 0.0,
    corner_radius: 0.0,
    depth: Some(0.875 * INCH),
    plug_diameter: 1.125 * INCH,
    plug_offsets: [-26.5 * MM, 26.5 * MM],
    fin_base: 4.37 * INCH,
    confirmed: false,
};

/// FCS II plug: maximum length 110 mm, maximum width 37.5 mm (FCS figures quoted by a retailer); drawn as a
/// slot with round ends enclosing the plug. Fin: FCS II Performer M base 4.37".
pub const FCS2: BoxSpec = BoxSpec {
    length: 110.0 * MM,
    width: 37.5 * MM,
    corner_radius: 37.5 * MM / 2.0,
    depth: None,
    plug_diameter: 0.0,
    plug_offsets: [0.0, 0.0],
    fin_base: 4.37 * INCH,
    confirmed: true,
};

/// Futures ILT box (1/2" and 3/4" boxes share the footprint): routed flange 5 7/8" x 1 1/4" (149.8 x 31.8 mm)
/// with fully round ends (Cleanline listing; a router jig modelled on the box agrees within 1 mm). Fin base 4.5".
pub const FUTURES: BoxSpec = BoxSpec {
    length: 149.8 * MM,
    width: 31.8 * MM,
    corner_radius: 31.8 * MM / 2.0,
    depth: None,
    plug_diameter: 0.0,
    plug_offsets: [0.0, 0.0],
    fin_base: 4.5 * INCH,
    confirmed: false,
};

/// Single-fin US box (Fins Unlimited 10.5" box): 26.6 cm long, 2.5 cm wide and deep; corners of a 1/4" bit.
/// Fin: 9" longboard fin, base 163 mm.
pub const US_BOX: BoxSpec = BoxSpec {
    length: 26.6,
    width: 2.5,
    corner_radius: 0.125 * INCH,
    depth: Some(2.5),
    plug_diameter: 0.0,
    plug_offsets: [0.0, 0.0],
    fin_base: 163.0 * MM,
    confirmed: true,
};

/// Standard fin arrangements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Edited by hand or read from an old file.
    #[default]
    Custom,
    Single,
    Twin,
    TwinTrailer,
    Thruster,
    Quad,
    TwoPlusOne,
    /// Five boxes: a thruster plus the rear quad boxes.
    Five,
}

impl Preset {
    /// The presets offered to the user, in menu order.
    pub const ALL: [Preset; 7] = [
        Preset::Single,
        Preset::Twin,
        Preset::TwinTrailer,
        Preset::Thruster,
        Preset::Quad,
        Preset::TwoPlusOne,
        Preset::Five,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Preset::Custom => "custom",
            Preset::Single => "single",
            Preset::Twin => "twin",
            Preset::TwinTrailer => "twin_trailer",
            Preset::Thruster => "thruster",
            Preset::Quad => "quad",
            Preset::TwoPlusOne => "2+1",
            Preset::Five => "five",
        }
    }

    /// Parse a code; also accepts "Twin + trailer", "2_plus_1", "5", "5 fins", "tri".
    pub fn from_code(s: &str) -> Option<Preset> {
        let k: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+')
            .collect::<String>()
            .to_ascii_lowercase();
        Some(match k.as_str() {
            "custom" => Preset::Custom,
            "single" | "1" => Preset::Single,
            "twin" | "2" => Preset::Twin,
            "twintrailer" | "twin+trailer" | "twinstabiliser" | "twinstabilizer" => {
                Preset::TwinTrailer
            }
            "thruster" | "tri" | "3" => Preset::Thruster,
            "quad" | "4" => Preset::Quad,
            "2+1" | "2plus1" | "21" => Preset::TwoPlusOne,
            "five" | "5" | "5fin" | "5fins" | "fivefin" | "fivefins" => Preset::Five,
            _ => return None,
        })
    }

    /// English display name.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Custom => "Custom",
            Preset::Single => "Single",
            Preset::Twin => "Twin",
            Preset::TwinTrailer => "Twin + trailer",
            Preset::Thruster => "Thruster",
            Preset::Quad => "Quad",
            Preset::TwoPlusOne => "2+1",
            Preset::Five => "5 fins",
        }
    }
}

/// One fin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fin {
    /// Rear end of the fin base (trailing edge), from the tail along the stringer.
    pub x: f64,
    /// Lateral offset of that point from the stringer, signed (see the module frame); 0 for a centre fin.
    pub y: f64,
    /// Length of the fin base.
    pub base: f64,
    /// Toe-in, degrees: the leading edge turned towards the stringer.
    pub toe: f64,
    /// Cant, degrees: the tip tilted out towards the rail.
    pub cant: f64,
    /// Fin depth (height below the bottom).
    pub depth: f64,
    pub system: BoxSystem,
}

/// Which side of the stringer a fin is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Centre,
    Right,
}

/// Lateral offsets smaller than this count as on the stringer.
const ON_STRINGER: f64 = 1e-6;

impl Fin {
    pub fn side(&self) -> Side {
        if self.y > ON_STRINGER {
            Side::Left
        } else if self.y < -ON_STRINGER {
            Side::Right
        } else {
            Side::Centre
        }
    }

    fn sign(&self) -> f64 {
        match self.side() {
            Side::Left => 1.0,
            Side::Right => -1.0,
            Side::Centre => 0.0,
        }
    }

    /// Rear end of the base.
    pub fn rear(&self) -> DVec2 {
        DVec2::new(self.x, self.y)
    }

    /// Unit vector along the base, rear to front (towards the nose, turned in by the toe).
    pub fn dir(&self) -> DVec2 {
        let t = self.toe * DEG;
        DVec2::new(t.cos(), -self.sign() * t.sin())
    }

    /// Front end of the base (leading edge).
    pub fn front(&self) -> DVec2 {
        self.rear() + self.dir() * self.base
    }

    /// Middle of the base.
    pub fn middle(&self) -> DVec2 {
        self.rear() + self.dir() * (self.base / 2.0)
    }

    /// A point given in the fin's frame (u along the base from its middle, v across, + to the left of the
    /// direction of travel) in board coordinates.
    pub fn to_board(&self, u: f64, v: f64) -> DVec2 {
        let d = self.dir();
        self.middle() + d * u + DVec2::new(-d.y, d.x) * v
    }

    /// Distance from the rail (outline edge) to the rear end of the base (the trailing edge, where the FCS II
    /// jig's "shaper's dot" goes), measured square to the stringer. On an asymmetric board the fin's own rail
    /// is used.
    pub fn from_rail(&self, board: &Board) -> f64 {
        board.half_width_at(self.x, Rail::of_y(self.y)) - self.y.abs()
    }

    /// Set the lateral offset so that the rear end of the base is `rail` in from the rail (the fin's own rail
    /// on an asymmetric board), keeping the side.
    pub fn set_from_rail(&mut self, board: &Board, rail: f64) {
        let s = if self.y < 0.0 { -1.0 } else { 1.0 };
        let half = board.half_width_at(self.x, Rail::of_y(self.y));
        self.y = s * (half - rail).max(2.0 * ON_STRINGER);
    }

    /// Toe-in as a distance: how much closer to the stringer the front of the base is than its rear.
    pub fn toe_offset(&self) -> f64 {
        self.base * (self.toe * DEG).sin()
    }

    /// The box footprint as closed polygons in board coordinates (empty without a box system).
    pub fn footprint(&self) -> Vec<Vec<DVec2>> {
        let Some(spec) = self.system.spec() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if spec.has_slot() {
            out.push(
                rounded_rect(spec.length, spec.width, spec.corner_radius)
                    .into_iter()
                    .map(|p| self.to_board(p.x, p.y))
                    .collect(),
            );
        }
        if spec.has_plugs() {
            for &u in &spec.plug_offsets {
                out.push(
                    circle(spec.plug_diameter / 2.0)
                        .into_iter()
                        .map(|p| self.to_board(u + p.x, p.y))
                        .collect(),
                );
            }
        }
        out
    }

    /// The fin's centre line, extended `overrun` past each end of the base or box, for marking out.
    pub fn centre_line(&self, overrun: f64) -> (DVec2, DVec2) {
        let half = self
            .system
            .spec()
            .map_or(0.0, |s| {
                if s.has_slot() {
                    s.length / 2.0
                } else {
                    s.plug_offsets[1].abs() + s.plug_diameter / 2.0
                }
            })
            .max(self.base / 2.0)
            + overrun;
        (self.to_board(-half, 0.0), self.to_board(half, 0.0))
    }

    /// Mirror image across the stringer.
    pub fn mirrored(&self) -> Fin {
        Fin {
            y: -self.y,
            ..*self
        }
    }
}

/// Polygon of a rectangle with rounded corners centred on the origin, length along x.
fn rounded_rect(length: f64, width: f64, radius: f64) -> Vec<DVec2> {
    let r = radius.clamp(0.0, width.min(length) / 2.0);
    let (hx, hy) = (length / 2.0 - r, width / 2.0 - r);
    if r <= 0.0 {
        return vec![
            DVec2::new(-hx, -hy),
            DVec2::new(hx, -hy),
            DVec2::new(hx, hy),
            DVec2::new(-hx, hy),
        ];
    }
    const STEPS: usize = 8;
    let mut pts = Vec::with_capacity(4 * (STEPS + 1));
    for (cx, cy, a0) in [
        (hx, -hy, -90.0),
        (hx, hy, 0.0),
        (-hx, hy, 90.0),
        (-hx, -hy, 180.0),
    ] {
        for k in 0..=STEPS {
            let a = (a0 + 90.0 * k as f64 / STEPS as f64) * DEG;
            pts.push(DVec2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
    pts
}

fn circle(r: f64) -> Vec<DVec2> {
    const N: usize = 36;
    (0..N)
        .map(|k| {
            let a = k as f64 / N as f64 * 360.0 * DEG;
            DVec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The fins of a board.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FinSetup {
    pub preset: Preset,
    /// The box system chosen for the setup (each fin also has its own).
    pub system: BoxSystem,
    pub fins: Vec<Fin>,
}

/// Placement of one fin of a preset on its reference board (inches; see `docs/fins.md`).
struct Place {
    role: Role,
    /// Rear of the fin base from the tail; for a fin in a US box, rear of the box.
    from_tail: f64,
    /// Rear of the fin base in from the rail (side fins only).
    from_rail: f64,
    /// Toe-in as a distance over the fin base.
    toe: f64,
    cant: f64,
    /// Fin depth used for drawing (not a placement rule).
    depth: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A pair of side fins.
    Sides,
    /// One fin on the stringer.
    Centre,
}

/// How a preset's distances from the tail follow the board length.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scaling {
    /// Shortboard family, given for 6'0": the McKee table quoted by Lundquist (front fins 10 1/4" at 4'9",
    /// 11" at 6'0", 12 1/4" at 7'0"), extended with its end slopes.
    Short,
    /// Longboard family, given for 9'0": in proportion to the length.
    Long,
}

/// Factor applied to a preset's distances from the tail on a board of length `l` (cm).
fn length_factor(scaling: Scaling, l: f64) -> f64 {
    if l <= 0.0 || !l.is_finite() {
        return 1.0;
    }
    let inches = l / INCH;
    let f = match scaling {
        Scaling::Long => inches / 108.0,
        Scaling::Short => {
            let (x0, y0, x1, y1, x2, y2) = (57.0, 10.25 / 11.0, 72.0, 1.0, 84.0, 12.25 / 11.0);
            if inches < x1 {
                y1 + (inches - x1) * (y1 - y0) / (x1 - x0)
            } else {
                y1 + (inches - x1) * (y2 - y1) / (x2 - x1)
            }
        }
    };
    f.clamp(0.6, 1.6)
}

fn places(preset: Preset) -> (Scaling, Vec<Place>) {
    let side = |from_tail, from_rail, toe, cant, depth| Place {
        role: Role::Sides,
        from_tail,
        from_rail,
        toe,
        cant,
        depth,
    };
    let centre = |from_tail, depth| Place {
        role: Role::Centre,
        from_tail,
        from_rail: 0.0,
        toe: 0.0,
        cant: 0.0,
        depth,
    };
    // Front/side fins 1 1/4" from the rail at the trailing edge, 1/4" toe, 6.5° cant; quad rears 3/16" toe and
    // less cant (Lundquist, Fiberglass Hawaii, FCS; docs/fins.md).
    let (front, rear) = (
        side(11.0, 1.25, 0.25, 6.5, 4.5),
        side(5.5, 1.25, 0.1875, 4.0, 4.0),
    );
    match preset {
        Preset::Custom => (Scaling::Short, Vec::new()),
        Preset::Single => (Scaling::Long, vec![centre(5.0, 9.0)]),
        Preset::Twin => (Scaling::Short, vec![side(8.0, 1.25, 0.1875, 6.5, 5.0)]),
        Preset::TwinTrailer => (
            Scaling::Short,
            vec![side(10.5, 1.25, 0.1875, 6.5, 4.75), centre(4.0, 3.5)],
        ),
        Preset::Thruster => (Scaling::Short, vec![front, centre(3.5, 4.5)]),
        Preset::Quad => (Scaling::Short, vec![front, rear]),
        Preset::TwoPlusOne => (
            Scaling::Long,
            vec![side(15.5, 1.25, 0.1875, 6.5, 3.5), centre(5.5, 7.0)],
        ),
        Preset::Five => (Scaling::Short, vec![front, rear, centre(3.5, 4.5)]),
    }
}

/// The box of one fin of a preset when the setup's system is `system`: a 2+1 always has its centre fin in a US
/// box; a US box is a centre box, so side fins of a US-box setup are positions only.
fn box_for(preset: Preset, centre: bool, system: BoxSystem) -> BoxSystem {
    match (centre, system) {
        (true, _) if preset == Preset::TwoPlusOne => BoxSystem::UsBox,
        (false, BoxSystem::UsBox) => BoxSystem::None,
        _ => system,
    }
}

impl FinSetup {
    pub fn is_empty(&self) -> bool {
        self.fins.is_empty()
    }

    /// A preset placed on `board` with the given box system. Distances from the tail are those of the preset's
    /// reference board (6'0" shortboard, 9'0" longboard for single and 2+1) adjusted to the board length; side
    /// fins are placed from the rail using the outline at their position. A 2+1 uses a US box for the centre
    /// fin whatever the side boxes are.
    pub fn preset(preset: Preset, system: BoxSystem, board: &Board) -> FinSetup {
        let (scaling, places) = places(preset);
        let k = length_factor(scaling, board.length());
        let mut fins = Vec::new();
        for p in places {
            let sys = box_for(preset, p.role == Role::Centre, system);
            let longboard_centre =
                p.role == Role::Centre && matches!(preset, Preset::Single | Preset::TwoPlusOne);
            let base = match sys.spec() {
                Some(s) => s.fin_base,
                None if longboard_centre => US_BOX.fin_base,
                None => FCS2.fin_base,
            };
            let mut x = p.from_tail * INCH * k;
            if sys == BoxSystem::UsBox {
                // Given to the rear of the box; the fin sits in the middle of it.
                x += (US_BOX.length - base).max(0.0) / 2.0;
            }
            let mut f = Fin {
                x,
                y: 0.0,
                base,
                toe: (p.toe * INCH / base).clamp(-1.0, 1.0).asin() / DEG,
                cant: p.cant,
                depth: p.depth * INCH,
                system: sys,
            };
            match p.role {
                Role::Centre => fins.push(f),
                Role::Sides => {
                    f.y = 1.0;
                    f.set_from_rail(board, p.from_rail * INCH);
                    fins.push(f);
                    // The right fin follows the right rail (the same offset on a symmetric board).
                    let mut r = f.mirrored();
                    r.set_from_rail(board, p.from_rail * INCH);
                    fins.push(r);
                }
            }
        }
        FinSetup {
            preset,
            system,
            fins,
        }
    }

    /// The same fins in another box system (a 2+1 keeps its centre fin in the US box; a US box takes only centre
    /// fins).
    pub fn with_system(mut self, system: BoxSystem) -> FinSetup {
        self.system = system;
        for f in &mut self.fins {
            f.system = box_for(self.preset, f.side() == Side::Centre, system);
        }
        self
    }

    /// The index of the fin mirroring fin `i` across the stringer, if any.
    pub fn partner(&self, i: usize) -> Option<usize> {
        let f = self.fins.get(i)?;
        if f.side() == Side::Centre {
            return None;
        }
        self.fins
            .iter()
            .enumerate()
            .position(|(j, g)| j != i && (g.x - f.x).abs() < 1e-6 && (g.y + f.y).abs() < 1e-6)
    }

    /// A short label for fin `i`: "Centre", "Left", "Right", or "Left front" / "Left rear" when a side has more
    /// than one fin (English; the app translates).
    pub fn role_label(&self, i: usize) -> &'static str {
        let Some(f) = self.fins.get(i) else {
            return "";
        };
        let side = f.side();
        if side == Side::Centre {
            return "Centre";
        }
        let same: Vec<&Fin> = self.fins.iter().filter(|g| g.side() == side).collect();
        let front = same.iter().all(|g| g.x <= f.x + 1e-9);
        let rear = same.iter().all(|g| g.x >= f.x - 1e-9);
        match (side, same.len() > 1, front, rear) {
            (Side::Left, false, ..) => "Left",
            (Side::Right, false, ..) => "Right",
            (Side::Left, true, true, _) => "Left front",
            (Side::Left, true, _, true) => "Left rear",
            (Side::Left, true, ..) => "Left middle",
            (Side::Right, true, true, _) => "Right front",
            (Side::Right, true, _, true) => "Right rear",
            _ => "Right middle",
        }
    }

    /// The legacy nine fin numbers (`Board::fins`, `.brd` p50) closest to this setup: the frontmost left side
    /// fin (or right, mirrored) as the side fin, the frontmost centre fin as the centre fin.
    pub fn to_legacy(&self) -> [f64; 9] {
        let mut out = [0.0; 9];
        let front = |side: Side| {
            self.fins
                .iter()
                .filter(|f| f.side() == side)
                .max_by(|a, b| a.x.total_cmp(&b.x))
        };
        if let Some(f) = front(Side::Left).or_else(|| front(Side::Right)) {
            let (r, fr) = (f.rear(), f.front());
            out[legacy::SIDE_REAR_X] = r.x;
            out[legacy::SIDE_REAR_Y] = r.y.abs();
            out[legacy::SIDE_FRONT_X] = fr.x;
            out[legacy::SIDE_FRONT_Y] = fr.y.abs();
            out[legacy::SIDE_DEPTH] = f.depth;
            out[legacy::SPLAY] = f.cant;
        }
        if let Some(f) = front(Side::Centre) {
            out[legacy::CENTER_REAR_X] = f.x;
            out[legacy::CENTER_FRONT_X] = f.x + f.base;
            out[legacy::CENTER_DEPTH] = f.depth;
        }
        out
    }

    /// The setup the legacy nine numbers describe: a pair of side fins if a side fin mark is set, a centre fin
    /// if a centre mark is set (thruster, twin or single), no boxes.
    pub fn from_legacy(f: &[f64; 9]) -> FinSetup {
        let mut fins = Vec::new();
        let has_side = f[legacy::SIDE_REAR_X] != 0.0 || f[legacy::SIDE_FRONT_X] != 0.0;
        let has_centre = f[legacy::CENTER_REAR_X] != 0.0 || f[legacy::CENTER_FRONT_X] != 0.0;
        if has_side {
            let mut r = DVec2::new(f[legacy::SIDE_REAR_X], f[legacy::SIDE_REAR_Y].abs());
            let mut fr = DVec2::new(f[legacy::SIDE_FRONT_X], f[legacy::SIDE_FRONT_Y].abs());
            if fr.x < r.x {
                std::mem::swap(&mut r, &mut fr);
            }
            let d = fr - r;
            let fin = Fin {
                x: r.x,
                y: r.y,
                base: d.length(),
                toe: (-d.y).atan2(d.x) / DEG,
                cant: f[legacy::SPLAY],
                depth: f[legacy::SIDE_DEPTH],
                system: BoxSystem::None,
            };
            let fin = if fin.y.abs() <= ON_STRINGER {
                // A side mark on the stringer: keep it as drawn, off by a hair so it stays a pair.
                Fin {
                    y: 2.0 * ON_STRINGER,
                    ..fin
                }
            } else {
                fin
            };
            fins.push(fin);
            fins.push(fin.mirrored());
        }
        if has_centre {
            let (a, b) = (f[legacy::CENTER_REAR_X], f[legacy::CENTER_FRONT_X]);
            fins.push(Fin {
                x: a.min(b),
                y: 0.0,
                base: (b - a).abs(),
                toe: 0.0,
                cant: 0.0,
                depth: f[legacy::CENTER_DEPTH],
                system: BoxSystem::None,
            });
        }
        let preset = match (has_side, has_centre) {
            (true, true) => Preset::Thruster,
            (true, false) => Preset::Twin,
            (false, true) => Preset::Single,
            (false, false) => Preset::Custom,
        };
        FinSetup {
            preset,
            system: BoxSystem::None,
            fins,
        }
    }

    /// Scale positions along the board by `rx` and across by `ry`; fin and box sizes are kept.
    pub fn scale(&mut self, rx: f64, ry: f64) {
        for f in &mut self.fins {
            f.x *= rx;
            f.y *= ry;
        }
    }

    /// Swap nose and tail on a board of length `l`: each fin's front end becomes its rear end.
    pub fn flip(&mut self, l: f64) {
        for f in &mut self.fins {
            let fr = f.front();
            f.x = l - fr.x;
            f.y = fr.y;
            f.toe = -f.toe;
        }
    }

    // ------------------------------------------------------------------------------- .brd text (p60)

    /// One-line text form stored in `.brd` files under `p60` (see `docs/spec/file-formats.md`):
    /// `1 <preset> <system>` followed by `; <system> <x> <y> <base> <toe> <cant> <depth>` per fin.
    pub fn encode(&self) -> String {
        let mut s = format!("1 {} {}", self.preset.code(), self.system.code());
        for f in &self.fins {
            s.push_str(&format!(
                "; {} {} {} {} {} {} {}",
                f.system.code(),
                num(f.x),
                num(f.y),
                num(f.base),
                num(f.toe),
                num(f.cant),
                num(f.depth)
            ));
        }
        s
    }

    /// Parse [`FinSetup::encode`] output. Unknown versions and malformed text give `None`.
    pub fn decode(text: &str) -> Option<FinSetup> {
        let mut parts = text.split(';');
        let head: Vec<&str> = parts.next()?.split_whitespace().collect();
        if head.len() != 3 || head[0] != "1" {
            return None;
        }
        let mut setup = FinSetup {
            preset: Preset::from_code(head[1])?,
            system: BoxSystem::from_code(head[2])?,
            fins: Vec::new(),
        };
        for part in parts {
            let t: Vec<&str> = part.split_whitespace().collect();
            if t.len() != 7 {
                return None;
            }
            let v = |i: usize| t[i].parse::<f64>().ok().filter(|v| v.is_finite());
            setup.fins.push(Fin {
                system: BoxSystem::from_code(t[0])?,
                x: v(1)?,
                y: v(2)?,
                base: v(3)?,
                toe: v(4)?,
                cant: v(5)?,
                depth: v(6)?,
            });
        }
        Some(setup)
    }
}

/// Shortest text that reads back as the same number.
fn num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "0".into()
    }
}

impl Board {
    /// The fin setup: the stored one, or the one described by the legacy fin marks.
    pub fn fin_setup(&self) -> FinSetup {
        self.fin_setup
            .clone()
            .unwrap_or_else(|| FinSetup::from_legacy(&self.fins))
    }

    /// Store a fin setup and update the legacy fin marks to match.
    pub fn set_fin_setup(&mut self, setup: FinSetup) {
        self.fins = setup.to_legacy();
        self.fin_setup = Some(setup);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(poly: &[DVec2], dir: DVec2) -> f64 {
        let v: Vec<f64> = poly.iter().map(|p| p.dot(dir)).collect();
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
    }

    #[test]
    fn footprints_have_the_documented_sizes() {
        for sys in BoxSystem::ALL {
            let f = Fin {
                x: 20.0,
                y: 15.0,
                base: 11.0,
                toe: 3.0,
                cant: 6.0,
                depth: 11.0,
                system: sys,
            };
            let polys = f.footprint();
            let Some(spec) = sys.spec() else {
                assert!(polys.is_empty());
                continue;
            };
            let (u, v) = (f.dir(), DVec2::new(-f.dir().y, f.dir().x));
            if spec.has_slot() {
                assert_eq!(polys.len(), 1, "{sys:?}");
                assert!((extent(&polys[0], u) - spec.length).abs() < 1e-9, "{sys:?}");
                assert!((extent(&polys[0], v) - spec.width).abs() < 1e-9, "{sys:?}");
                // Centred on the middle of the base.
                let c = polys[0].iter().copied().sum::<DVec2>() / polys[0].len() as f64;
                assert!(c.distance(f.middle()) < 1e-6, "{sys:?}");
            }
            if spec.has_plugs() {
                assert_eq!(polys.len(), 2);
                for (poly, off) in polys.iter().zip(spec.plug_offsets) {
                    assert!((extent(poly, u) - spec.plug_diameter).abs() < 1e-9);
                    let c = poly.iter().copied().sum::<DVec2>() / poly.len() as f64;
                    assert!(c.distance(f.to_board(off, 0.0)) < 1e-9);
                }
            }
        }
        // The documented constants (docs/fins.md).
        assert!((FCS2.length - 11.0).abs() < 1e-12 && (FCS2.width - 3.75).abs() < 1e-12);
        assert!((FCS1.plug_diameter - 2.8575).abs() < 1e-12);
        assert!((FCS1.plug_offsets[1] - FCS1.plug_offsets[0] - 5.3).abs() < 1e-12);
        assert!((FUTURES.length - 14.98).abs() < 1e-12 && (FUTURES.width - 3.18).abs() < 1e-12);
        assert!((US_BOX.length - 26.6).abs() < 1e-12 && (US_BOX.width - 2.5).abs() < 1e-12);
    }

    #[test]
    fn toe_turns_the_leading_edge_towards_the_stringer() {
        let f = Fin {
            x: 25.0,
            y: 20.0,
            base: 11.0,
            toe: 3.0,
            ..Default::default()
        };
        assert!(f.front().y < f.rear().y);
        assert!(f.mirrored().front().y > f.mirrored().rear().y);
        assert!((f.rear().y - f.front().y - f.toe_offset()).abs() < 1e-12);
    }

    #[test]
    fn text_form_round_trips() {
        let s = FinSetup {
            preset: Preset::TwoPlusOne,
            system: BoxSystem::Futures,
            fins: vec![
                Fin {
                    x: 1.0 / 3.0,
                    y: -12.25,
                    base: 11.43,
                    toe: 2.5,
                    cant: 6.5,
                    depth: 11.0,
                    system: BoxSystem::Futures,
                },
                Fin {
                    x: 14.0,
                    system: BoxSystem::UsBox,
                    ..Default::default()
                },
            ],
        };
        assert_eq!(FinSetup::decode(&s.encode()), Some(s.clone()));
        let empty = FinSetup::default();
        assert_eq!(FinSetup::decode(&empty.encode()), Some(empty));
        for bad in [
            "",
            "2 thruster fcs2",
            "1 nope fcs2",
            "1 quad fcs2; fcs2 1 2 3",
            "1 quad x",
        ] {
            assert_eq!(FinSetup::decode(bad), None, "{bad:?}");
        }
        for p in Preset::ALL {
            assert_eq!(Preset::from_code(p.code()), Some(p));
            assert_eq!(Preset::from_code(p.label()), Some(p));
        }
        for b in BoxSystem::ALL {
            assert_eq!(BoxSystem::from_code(b.code()), Some(b));
            assert_eq!(BoxSystem::from_code(b.label()), Some(b));
        }
    }

    #[test]
    fn legacy_marks_convert_both_ways() {
        let legacy = [27.94, 21.0, 39.0, 20.4, 7.6, 18.0, 11.0, 11.5, 6.0];
        let s = FinSetup::from_legacy(&legacy);
        assert_eq!(s.preset, Preset::Thruster);
        assert_eq!(s.fins.len(), 3);
        assert_eq!(s.partner(0), Some(1));
        assert!(s.fins[0].toe > 0.0);
        let back = s.to_legacy();
        for (a, b) in back.iter().zip(legacy) {
            assert!((a - b).abs() < 1e-9, "{back:?}");
        }
        let twin = FinSetup::from_legacy(&[20.0, 15.0, 31.0, 15.0, 0.0, 0.0, 0.0, 12.0, 0.0]);
        assert_eq!((twin.preset, twin.fins.len()), (Preset::Twin, 2));
        let single = FinSetup::from_legacy(&[0.0, 0.0, 0.0, 0.0, 10.0, 30.0, 20.0, 0.0, 0.0]);
        assert_eq!((single.preset, single.fins.len()), (Preset::Single, 1));
        assert!(FinSetup::from_legacy(&[0.0; 9]).is_empty());
    }

    #[test]
    fn length_factor_follows_the_table() {
        assert!((length_factor(Scaling::Short, 72.0 * INCH) - 1.0).abs() < 1e-12);
        assert!((length_factor(Scaling::Short, 84.0 * INCH) * 11.0 - 12.25).abs() < 1e-9);
        assert!((length_factor(Scaling::Short, 57.0 * INCH) * 11.0 - 10.25).abs() < 1e-9);
        assert!((length_factor(Scaling::Long, 108.0 * INCH) - 1.0).abs() < 1e-12);
    }
}
