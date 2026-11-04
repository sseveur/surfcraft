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
    /// Planform and foil, relative to the base (used for the 3D model and its export).
    pub shape: FinShape,
}

/// The shape of a fin, as fractions of its base length so one shape fits every size.
///
/// The planform (seen from the side) has a trailing edge that curves back from the rear of the base to the tip
/// and a chord that narrows from the base to the tip width, slowly near the root on a full fin and steadily on
/// a slender one. The tip is closed by a cap that is anything from square-cut to fully round. The foil can be
/// symmetric (50/50), 80/20 or flat on the inside face (side fins only; a fin on the stringer is always
/// symmetric).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinShape {
    /// How far the rear of the tip sits behind the rear of the base (rake).
    pub rake: f64,
    /// Width of the tip, front to back (0 = pointed; above 1 the fin widens towards the tip, as on a hatchet).
    pub tip: f64,
    /// Rounding of the tip: 0 = square-cut, 1 = a full round cap.
    pub tip_round: f64,
    /// How long the leading edge stays forward going down the fin (the exponent of its sweep back): 1 for a
    /// straight leading edge, about 2 for a typical convex one, 4 or more for a full, wide fin.
    pub fullness: f64,
    /// Foil thickness at the root.
    pub thickness: f64,
    /// Share of the thickness on the inside face (towards the stringer): 0.5 = symmetric, 0.2 = 80/20, 0 = flat
    /// inside.
    pub inside: f64,
}

impl Default for FinShape {
    fn default() -> Self {
        FinShape::DEFAULT
    }
}

/// Spanwise station of a fin planform, see [`FinShape::stations`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinStation {
    /// Height below the root.
    pub h: f64,
    /// Trailing and leading edge, along the base from its rear end (negative = behind it).
    pub trailing: f64,
    pub leading: f64,
    /// Half the foil thickness at this height.
    pub half: f64,
}

type Range = std::ops::RangeInclusive<f64>;

/// Exponent of the trailing edge's curve back to the tip (above 1: hollow).
const TRAILING_HOLLOW: f64 = 1.6;

impl FinShape {
    /// A swept fin with a small round tip (a typical thruster template).
    pub const DEFAULT: FinShape = FinShape {
        rake: 0.32,
        tip: 0.12,
        tip_round: 1.0,
        fullness: 2.2,
        thickness: 0.072,
        inside: 0.5,
    };

    /// Named starting points (English names; the app translates).
    pub const TEMPLATES: [(&'static str, FinShape); 5] = [
        ("Raked (swept, small round tip)", FinShape::DEFAULT),
        (
            "Upright (wide round tip)",
            FinShape {
                rake: 0.16,
                tip: 0.32,
                tip_round: 0.8,
                fullness: 2.6,
                ..FinShape::DEFAULT
            },
        ),
        (
            "Square tip",
            FinShape {
                rake: 0.2,
                tip: 0.34,
                tip_round: 0.15,
                fullness: 2.4,
                ..FinShape::DEFAULT
            },
        ),
        (
            "Pointed",
            FinShape {
                rake: 0.38,
                tip: 0.0,
                tip_round: 0.0,
                fullness: 1.6,
                ..FinShape::DEFAULT
            },
        ),
        (
            "Keel (long, low, swept)",
            FinShape {
                rake: 0.6,
                tip: 0.12,
                tip_round: 1.0,
                fullness: 1.4,
                thickness: 0.06,
                inside: 0.5,
            },
        ),
    ];

    pub const RAKE: Range = 0.0..=0.75;
    pub const TIP: Range = 0.0..=1.6;
    pub const FULLNESS: Range = 0.6..=14.0;
    pub const THICKNESS: Range = 0.02..=0.15;

    /// The shape with every value in its valid range (non-finite values take the default).
    pub fn clamped(self) -> FinShape {
        let c = |v: f64, r: Range, d: f64| {
            if v.is_finite() {
                v.clamp(*r.start(), *r.end())
            } else {
                d
            }
        };
        let d = FinShape::DEFAULT;
        FinShape {
            rake: c(self.rake, FinShape::RAKE, d.rake),
            tip: c(self.tip, FinShape::TIP, d.tip),
            tip_round: c(self.tip_round, 0.0..=1.0, d.tip_round),
            fullness: c(self.fullness, FinShape::FULLNESS, d.fullness),
            thickness: c(self.thickness, FinShape::THICKNESS, d.thickness),
            inside: c(self.inside, 0.0..=0.5, d.inside),
        }
    }

    /// The name of the template this planform is (the foil is ignored), if any.
    pub fn template(&self) -> Option<&'static str> {
        let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
        FinShape::TEMPLATES.iter().find_map(|(name, t)| {
            (same(t.rake, self.rake)
                && same(t.tip, self.tip)
                && same(t.tip_round, self.tip_round)
                && same(t.fullness, self.fullness)
                && same(t.thickness, self.thickness))
            .then_some(*name)
        })
    }

    /// Height of the rounded tip cap for a fin of this base and depth.
    fn cap_height(&self, base: f64, depth: f64) -> f64 {
        (self.tip_round * self.tip * base / 2.0).min(0.4 * depth)
    }

    /// Spanwise stations from the root (first) to the tip (last) for a fin of the given base and depth:
    /// `body` intervals up to the start of the tip cap, then `cap` more around a rounded tip. The last station
    /// of a rounded tip is nearly a point; a square-cut tip ends with its full width.
    pub fn stations(&self, base: f64, depth: f64, body: usize, cap: usize) -> Vec<FinStation> {
        let s = self.clamped();
        let (rake, tip) = (s.rake * base, s.tip * base);
        let cap_height = s.cap_height(base, depth);
        let body_height = depth - cap_height;
        let root_half = s.thickness * base / 2.0;
        let body = body.max(1);
        let mut out = Vec::with_capacity(body + cap + 1);
        for i in 0..=body {
            let t = i as f64 / body as f64;
            // A slightly hollow trailing edge, and a leading edge that stays forward longer the fuller the fin
            // (convex above 1), both reaching the tip at the same height.
            let trailing = -rake * t.powf(TRAILING_HOLLOW);
            let leading = (base - (base + rake - tip) * t.powf(s.fullness)).max(trailing);
            out.push(FinStation {
                h: body_height * t,
                trailing,
                leading,
                half: root_half * (1.0 - 0.8 * t),
            });
        }
        if cap_height > 1e-9 {
            let last = out[body];
            let mid = (last.trailing + last.leading) / 2.0;
            let a = (last.leading - last.trailing) / 2.0;
            for j in 1..=cap {
                // Stop just short of the end so the last ring keeps a little size for its closing fan.
                let phi = std::f64::consts::FRAC_PI_2 * 0.97 * j as f64 / cap as f64;
                let (sin, cos) = phi.sin_cos();
                out.push(FinStation {
                    h: body_height + cap_height * sin,
                    trailing: mid - a * cos,
                    leading: mid + a * cos,
                    half: last.half * cos,
                });
            }
        }
        out
    }

    /// The planform as a closed polygon of `(along the base from its rear end, height below the root)` points:
    /// up the trailing edge, around the tip, down the leading edge.
    pub fn outline(&self, base: f64, depth: f64) -> Vec<DVec2> {
        const BODY: usize = 24;
        let st = self.stations(base, depth, BODY, 10);
        let mut pts: Vec<DVec2> = st.iter().map(|s| DVec2::new(s.trailing, s.h)).collect();
        if st.len() > BODY + 1 {
            // A rounded cap ends at the full depth, in the middle of the tip.
            let l = st[st.len() - 1];
            pts.push(DVec2::new((l.trailing + l.leading) / 2.0, depth));
        }
        pts.extend(st.iter().rev().map(|s| DVec2::new(s.leading, s.h)));
        pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
        pts
    }

    /// Area of the planform (side view), in the square of the base's unit.
    pub fn area(&self, base: f64, depth: f64) -> f64 {
        let p = self.outline(base, depth);
        let n = p.len();
        ((0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>() / 2.0).abs()
    }

    /// Sweep angle in degrees as FCS draws it: the angle between a line square to the base at the middle of the
    /// base and the line from there to the tip (the middle of the tip at full depth).
    pub fn sweep(&self, base: f64, depth: f64) -> f64 {
        let s = self.clamped();
        let tip_middle = (s.tip / 2.0 - s.rake) * base;
        ((base / 2.0 - tip_middle) / depth).atan() / DEG
    }

    /// A shape matching published fin specs: base and depth (in one unit), planform area (that unit squared)
    /// and sweep angle in degrees (FCS's, see [`FinShape::sweep`]), the last two optional. The sweep sets the rake and
    /// the area sets the fullness (and the tip width, if even the fullest fin is too small). The tip rounding,
    /// thickness and foil are kept from `self`.
    pub fn fitted(&self, base: f64, depth: f64, area: Option<f64>, sweep: Option<f64>) -> FinShape {
        let mut s = self.clamped();
        if !(base > 0.0 && depth > 0.0 && base.is_finite() && depth.is_finite()) {
            return s;
        }
        let sweep = sweep.filter(|v| v.is_finite() && (0.0..80.0).contains(v));
        let swept = |mut s: FinShape| {
            if let Some(sweep) = sweep {
                // The tip middle sits depth * tan(sweep) behind the middle of the base.
                let behind = depth * (sweep * DEG).tan() / base;
                s.rake = (behind - 0.5 + s.tip / 2.0)
                    .clamp(*FinShape::RAKE.start(), *FinShape::RAKE.end());
            }
            s
        };
        s = swept(s);
        let Some(target) = area.filter(|a| a.is_finite() && *a > 0.0) else {
            return s;
        };
        // The area grows with the fullness and with the tip width: bisect on each in turn.
        let bisect = |s: FinShape, set: fn(&mut FinShape, f64), range: Range| {
            let (mut lo, mut hi) = (*range.start(), *range.end());
            let at = |v: f64| {
                let mut t = s;
                set(&mut t, v);
                swept(t)
            };
            for _ in 0..50 {
                let mid = (lo + hi) / 2.0;
                if at(mid).area(base, depth) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            at((lo + hi) / 2.0)
        };
        s = bisect(s, |s, v| s.fullness = v, FinShape::FULLNESS);
        if s.area(base, depth) < 0.995 * target {
            s = bisect(s, |s, v| s.tip = v, s.tip..=*FinShape::TIP.end());
        }
        s
    }

    /// Text form for `.brd` p64: `rake tip tip_round fullness thickness inside`.
    pub fn encode(&self) -> String {
        [
            self.rake,
            self.tip,
            self.tip_round,
            self.fullness,
            self.thickness,
            self.inside,
        ]
        .map(num)
        .join(" ")
    }

    /// Parse [`FinShape::encode`] output.
    pub fn decode(text: &str) -> Option<FinShape> {
        let v: Vec<f64> = text
            .split_whitespace()
            .map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect::<Option<_>>()?;
        let [rake, tip, tip_round, fullness, thickness, inside] = v[..] else {
            return None;
        };
        Some(FinShape {
            rake,
            tip,
            tip_round,
            fullness,
            thickness,
            inside,
        })
    }
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
    /// Typical fin base and depth for this role (inches), see `places`.
    base: f64,
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
    let side = |from_tail, from_rail, toe, cant, (base, depth)| Place {
        role: Role::Sides,
        from_tail,
        from_rail,
        toe,
        cant,
        base,
        depth,
    };
    let centre = |from_tail, (base, depth)| Place {
        role: Role::Centre,
        from_tail,
        from_rail: 0.0,
        toe: 0.0,
        cant: 0.0,
        base,
        depth,
    };
    // Fin sizes (base, depth in inches) for each role: medians of the fins of that setup and role in the fin
    // catalogue (data/fin-catalog.csv, docs/fins.md), rounded; a longboard 2+1 centre and single from the
    // longboard fins.
    const THRUSTER_SIDE: (f64, f64) = (4.45, 4.55);
    const THRUSTER_CENTRE: (f64, f64) = (4.4, 4.5);
    const QUAD_REAR: (f64, f64) = (3.95, 4.05);
    const TWIN: (f64, f64) = (5.65, 5.35);
    const TWIN_PLUS_ONE_SIDE: (f64, f64) = (5.05, 5.3);
    const STABILISER: (f64, f64) = (3.25, 3.3);
    const SIDE_BITE: (f64, f64) = (3.9, 3.6);
    const TWO_PLUS_ONE_CENTRE: (f64, f64) = (4.8, 7.0);
    const LONGBOARD_SINGLE: (f64, f64) = (6.5, 9.0);
    // Front/side fins 1 1/4" from the rail at the trailing edge, 1/4" toe, 6.5° cant; quad rears 3/16" toe and
    // less cant (Lundquist, Fiberglass Hawaii, FCS; docs/fins.md).
    let (front, rear) = (
        side(11.0, 1.25, 0.25, 6.5, THRUSTER_SIDE),
        side(5.5, 1.25, 0.1875, 4.0, QUAD_REAR),
    );
    match preset {
        Preset::Custom => (Scaling::Short, Vec::new()),
        Preset::Single => (Scaling::Long, vec![centre(5.0, LONGBOARD_SINGLE)]),
        Preset::Twin => (Scaling::Short, vec![side(8.0, 1.25, 0.1875, 6.5, TWIN)]),
        Preset::TwinTrailer => (
            Scaling::Short,
            vec![
                side(10.5, 1.25, 0.1875, 6.5, TWIN_PLUS_ONE_SIDE),
                centre(4.0, STABILISER),
            ],
        ),
        Preset::Thruster => (Scaling::Short, vec![front, centre(3.5, THRUSTER_CENTRE)]),
        Preset::Quad => (Scaling::Short, vec![front, rear]),
        Preset::TwoPlusOne => (
            Scaling::Long,
            vec![
                side(15.5, 1.25, 0.1875, 6.5, SIDE_BITE),
                centre(5.5, TWO_PLUS_ONE_CENTRE),
            ],
        ),
        Preset::Five => (
            Scaling::Short,
            vec![front, rear, centre(3.5, THRUSTER_CENTRE)],
        ),
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
            let base = p.base * INCH;
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
                shape: FinShape::DEFAULT,
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
                shape: FinShape::DEFAULT,
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
                shape: FinShape::DEFAULT,
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
                shape: FinShape::DEFAULT,
            });
        }
        Some(setup)
    }

    /// One-line text form of the fin shapes, stored in `.brd` files under `p64` (kept apart from p60 so older
    /// readers of p60 still read the fins): `1` followed by `; <shape>` per fin, in p60's order.
    pub fn encode_shapes(&self) -> String {
        let mut s = String::from("1");
        for f in &self.fins {
            s.push_str("; ");
            s.push_str(&f.shape.encode());
        }
        s
    }

    /// Apply [`FinSetup::encode_shapes`] output. Text that does not match the fins is ignored (the fins keep
    /// the default shape); returns whether it was applied.
    pub fn apply_shapes(&mut self, text: &str) -> bool {
        let mut parts = text.split(';');
        if parts.next().map(str::trim) != Some("1") {
            return false;
        }
        let Some(shapes) = parts.map(FinShape::decode).collect::<Option<Vec<_>>>() else {
            return false;
        };
        if shapes.len() != self.fins.len() {
            return false;
        }
        for (f, s) in self.fins.iter_mut().zip(shapes) {
            f.shape = s;
        }
        true
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

    /// Triangle mesh of the fins hanging under the bottom (empty without fins): one closed shell per fin, with
    /// its own [`FinShape`], tilted out towards its rail by its cant and turned by its toe. The root is sunk
    /// [`FIN_EMBED`] into the board so no gap shows on a curved bottom, and closed so each shell is watertight
    /// (for export, 3D printing and flow simulation).
    pub fn fin_mesh(&self) -> crate::Mesh {
        let mut mesh = crate::Mesh::default();
        for fin in &self.fin_setup().fins {
            add_fin(self, fin, &mut mesh);
        }
        mesh.compute_normals();
        mesh
    }
}

/// Depth the fin root is sunk into the board, cm.
pub const FIN_EMBED: f64 = 0.3;

/// NACA 4-digit thickness distribution with a closed trailing edge, normalised to 1 at its thickest point;
/// `x` runs from 0 at the leading edge to 1 at the trailing edge.
fn foil(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let t =
        0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x.powi(3) - 0.1036 * x.powi(4);
    (t / 0.1).max(0.0)
}

fn add_fin(board: &Board, fin: &Fin, mesh: &mut crate::Mesh) {
    use glam::DVec3;
    const BODY: usize = 20;
    const CAP: usize = 8;
    /// Points along each face, trailing edge to leading edge (both edges shared by the two faces).
    const CHORD: usize = 16;
    let (base, depth) = (fin.base, fin.depth);
    if !(base > 0.0 && depth > 0.0 && base.is_finite() && depth.is_finite()) {
        return;
    }
    let shape = fin.shape.clamped();
    let d = fin.dir();
    let along = DVec3::new(d.x, d.y, 0.0);
    // Out towards the fin's own rail; a centre fin has no cant.
    let out = DVec2::new(-d.y, d.x) * fin.sign();
    let c = fin.cant * DEG;
    let down = DVec3::new(out.x * c.sin(), out.y * c.sin(), -c.cos());
    // `side` points to the fin's left; for a left fin that is its outside face, for a right fin its inside.
    let side = along.cross(down).normalize_or_zero();
    // Thickness share of the +side and -side faces.
    let inside = if fin.sign() == 0.0 { 0.5 } else { shape.inside };
    let (plus, minus) = if fin.sign() > 0.0 {
        (1.0 - inside, inside)
    } else if fin.sign() < 0.0 {
        (inside, 1.0 - inside)
    } else {
        (0.5, 0.5)
    };
    let first = mesh.positions.len() as u32;
    let ring = 2 * CHORD - 2;
    let stations = shape.stations(base, depth, BODY, CAP);
    for st in &stations {
        let point = |w: f64, offset: f64| {
            let u = st.trailing + (st.leading - st.trailing) * w;
            let plan = fin.rear() + d * u;
            let root = board.bottom_at_xy(plan.x, plan.y) + FIN_EMBED;
            DVec3::new(plan.x, plan.y, root)
                + down * st.h
                + side * (offset * 2.0 * st.half * foil(1.0 - w))
        };
        // Cosine spacing: more points near the edges, where the foil curves most.
        let w =
            |k: usize| (1.0 - (std::f64::consts::PI * k as f64 / (CHORD - 1) as f64).cos()) / 2.0;
        for k in 0..CHORD {
            mesh.positions.push(point(w(k), plus));
        }
        for k in (1..CHORD - 1).rev() {
            mesh.positions.push(point(w(k), -minus));
        }
    }
    let rings = stations.len() as u32;
    let r = ring as u32;
    let start = mesh.triangles.len();
    for i in 0..rings - 1 {
        let (a, b) = (first + i * r, first + (i + 1) * r);
        for k in 0..r {
            let k1 = (k + 1) % r;
            mesh.triangles.push([a + k, a + k1, b + k1]);
            mesh.triangles.push([a + k, b + k1, b + k]);
        }
    }
    // Close the root and the tip with fans around the middle of their rings.
    for (ring_start, flip) in [(first, true), (first + (rings - 1) * r, false)] {
        let pts = &mesh.positions[ring_start as usize..(ring_start + r) as usize];
        let centre = pts.iter().copied().sum::<DVec3>() / r as f64;
        let c = mesh.positions.len() as u32;
        mesh.positions.push(centre);
        for k in 0..r {
            let (p, q) = (ring_start + k, ring_start + (k + 1) % r);
            mesh.triangles
                .push(if flip { [c, q, p] } else { [c, p, q] });
        }
    }
    // Face outwards whatever the handedness of the fin's frame.
    let tris = &mut mesh.triangles[start..];
    let volume: f64 = tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            a.dot(b.cross(c))
        })
        .sum();
    if volume < 0.0 {
        for t in tris.iter_mut() {
            t.swap(1, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fin_mesh_hangs_under_the_bottom_at_each_fin() {
        let mut b = crate::mesh::tests::toy_board();
        let setup = FinSetup::preset(Preset::Five, BoxSystem::FcsII, &b);
        b.fin_setup = None;
        b.fins = [0.0; 9];
        assert!(b.fin_mesh().triangles.is_empty());
        b.set_fin_setup(setup.clone());
        let m = b.fin_mesh();
        assert!(!m.triangles.is_empty());
        assert_eq!(m.normals.len(), m.positions.len());
        assert!(m.positions.iter().all(|p| p.is_finite()));
        for fin in &setup.fins {
            // Some vertex reaches the fin's depth below the bottom, near the fin.
            let bottom = b.bottom_at_xy(fin.middle().x, fin.middle().y);
            let lowest = m
                .positions
                .iter()
                .filter(|p| DVec2::new(p.x, p.y).distance(fin.middle()) < fin.base + fin.depth)
                .map(|p| p.z)
                .fold(f64::MAX, f64::min);
            assert!(
                lowest < bottom - 0.8 * fin.depth,
                "{fin:?}: {lowest} vs {bottom}"
            );
        }
        // Side fins cant out towards their own rail.
        let left = setup
            .fins
            .iter()
            .find(|f| f.y > 0.0 && f.cant > 0.0)
            .unwrap();
        let tip = m
            .positions
            .iter()
            .filter(|p| DVec2::new(p.x, p.y).distance(left.middle()) < left.base)
            .min_by(|a, b| a.z.total_cmp(&b.z))
            .unwrap();
        assert!(tip.y > left.middle().y, "tip {tip:?} of {left:?}");
    }

    #[test]
    fn fin_shells_are_closed_and_face_outwards() {
        let mut b = crate::mesh::tests::toy_board();
        for (name, shape) in FinShape::TEMPLATES {
            for inside in [0.5, 0.2, 0.0] {
                let mut setup = FinSetup::preset(Preset::Thruster, BoxSystem::FcsII, &b);
                for f in &mut setup.fins {
                    f.shape = FinShape { inside, ..shape };
                }
                b.set_fin_setup(setup.clone());
                let m = b.fin_mesh();
                assert!(m.is_watertight(), "{name} {inside}");
                // Every edge is used by exactly two triangles.
                let mut edges = std::collections::HashMap::<(u32, u32), u32>::new();
                for t in &m.triangles {
                    for k in 0..3 {
                        let (p, q) = (t[k], t[(k + 1) % 3]);
                        *edges.entry((p.min(q), p.max(q))).or_default() += 1;
                    }
                }
                assert!(edges.values().all(|&n| n == 2), "{name} {inside}");
                // The shells enclose a positive volume close to planform area x mean thickness.
                let v = m.volume();
                let expected: f64 = setup
                    .fins
                    .iter()
                    .map(|f| f.shape.area(f.base, f.depth) * f.shape.thickness * f.base * 0.4)
                    .sum();
                assert!(
                    v > 0.4 * expected && v < 2.0 * expected,
                    "{name} {inside}: {v} vs {expected}"
                );
            }
        }
    }

    #[test]
    fn tip_width_and_rounding_shape_the_planform() {
        let (base, depth) = (11.0, 11.5);
        let square = FinShape {
            tip: 0.3,
            tip_round: 0.0,
            ..FinShape::DEFAULT
        };
        let st = square.stations(base, depth, 20, 8);
        let tip = st.last().unwrap();
        assert!((tip.h - depth).abs() < 1e-9);
        assert!((tip.leading - tip.trailing - 0.3 * base).abs() < 1e-9);
        assert!((tip.trailing + square.rake * base).abs() < 1e-9);
        // A round tip of the same width ends in (nearly) a point at full depth.
        let round = FinShape {
            tip_round: 1.0,
            ..square
        };
        let st = round.stations(base, depth, 20, 8);
        let tip = st.last().unwrap();
        assert!(tip.leading - tip.trailing < 0.1 * 0.3 * base);
        assert!(tip.h < depth && tip.h > depth - 0.01 * depth);
        assert!(round.area(base, depth) < square.area(base, depth));
        // A pointed tip has less area than a wide one.
        let pointed = FinShape { tip: 0.0, ..square };
        assert!(pointed.area(base, depth) < square.area(base, depth));
        // Base x depth bounds the area.
        for (_, s) in FinShape::TEMPLATES {
            let a = s.area(base, depth);
            assert!(a > 0.3 * base * depth && a < base * depth, "{a}");
        }
    }

    #[test]
    fn shapes_fit_published_specs() {
        // Base, depth (in), area (sq in), sweep (deg) of a typical side fin.
        let (base, depth, area, sweep) = (4.6, 5.14, 19.69, 33.0);
        let s = FinShape::DEFAULT.fitted(base, depth, Some(area), Some(sweep));
        assert!((s.area(base, depth) - area).abs() / area < 0.01, "{s:?}");
        assert!((s.sweep(base, depth) - sweep).abs() < 0.5, "{s:?}");
        // Area only: the rake is kept.
        let s = FinShape::DEFAULT.fitted(base, depth, Some(area), None);
        assert!((s.rake - FinShape::DEFAULT.rake).abs() < 1e-12);
        assert!((s.area(base, depth) - area).abs() / area < 0.01);
        // Nothing to fit, or nonsense: the shape is unchanged.
        assert_eq!(
            FinShape::DEFAULT.fitted(base, depth, None, None),
            FinShape::DEFAULT
        );
        assert_eq!(
            FinShape::DEFAULT.fitted(0.0, depth, Some(area), None),
            FinShape::DEFAULT
        );
    }

    #[test]
    fn flat_inside_foil_keeps_the_inside_face_flat() {
        let mut b = crate::mesh::tests::toy_board();
        let mut setup = FinSetup::preset(Preset::Twin, BoxSystem::None, &b);
        for f in &mut setup.fins {
            f.shape.inside = 0.0;
            f.cant = 0.0;
            f.toe = 0.0;
        }
        b.set_fin_setup(setup.clone());
        let m = b.fin_mesh();
        for f in &setup.fins {
            // With no toe or cant the fin's mid plane is y = f.y: nothing crosses it towards the stringer.
            let near: Vec<_> = m
                .positions
                .iter()
                .filter(|p| (p.x - f.middle().x).abs() < f.base && (p.y - f.y).abs() < 2.0)
                .collect();
            assert!(!near.is_empty());
            for p in near {
                assert!((p.y - f.y) * f.y.signum() > -1e-9, "{p:?} {f:?}");
            }
        }
    }

    #[test]
    fn shapes_round_trip_as_text() {
        let mut s = FinSetup::preset(
            Preset::Quad,
            BoxSystem::Futures,
            &crate::mesh::tests::toy_board(),
        );
        s.fins[0].shape = FinShape::TEMPLATES[2].1;
        s.fins[3].shape.inside = 0.2;
        let text = s.encode_shapes();
        let mut back = s.clone();
        for f in &mut back.fins {
            f.shape = FinShape::DEFAULT;
        }
        assert!(back.apply_shapes(&text));
        assert_eq!(back, s);
        assert!(
            !back.apply_shapes("1; 0.3 0.1 1 2 0.07 0.5"),
            "wrong fin count"
        );
        assert!(!back.apply_shapes("2; 0.3 0.1 1 2 0.07 0.5"));
        assert!(!back.apply_shapes("1; x"));
        assert_eq!(
            FinShape::TEMPLATES[1].1.template(),
            Some(FinShape::TEMPLATES[1].0)
        );
    }

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
                shape: FinShape::DEFAULT,
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
                    shape: FinShape::DEFAULT,
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
