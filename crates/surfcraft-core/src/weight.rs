//! Finished-weight estimate of a foam, stringer and fibreglass board.

use crate::asym::Rail;
use crate::board::Board;
use crate::math::{ZERO, simpson, simpson_par};
use crate::units::FOOT;

/// Materials of the layup. Densities in kg/l, glass unit weights in kg/m², widths in cm, masses in kg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layup {
    pub stringer_width: f64,
    pub stringer_density: f64,
    pub foam_density: f64,
    pub deck_glass: f64,
    pub deck_lap_width: f64,
    pub bottom_glass: f64,
    pub bottom_lap_width: f64,
    /// Resin mass per glass mass.
    pub resin_ratio: f64,
    pub hotcoat: f64,
    pub plugs_and_fins: f64,
}

impl Layup {
    /// Usual layup for the board's length: longer boards get a wider stringer and heavier cloth.
    pub fn defaults_for(board: &Board) -> Layup {
        let l = board.length();
        let (stringer_width, deck_glass, bottom_glass) = if l > 9.0 * FOOT {
            (0.8, 0.400, 0.200)
        } else if l > 7.0 * FOOT {
            (0.5, 0.400, 0.200)
        } else {
            (0.3, 0.270, 0.135)
        };
        Layup {
            stringer_width,
            stringer_density: 0.4,
            foam_density: 0.045,
            deck_glass,
            deck_lap_width: 2.54,
            bottom_glass,
            bottom_lap_width: 2.54,
            resin_ratio: 1.5,
            hotcoat: 0.03 * (l / FOOT),
            plugs_and_fins: 0.2,
        }
    }
}

/// Estimated masses (kg) and glassed areas (cm²).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weights {
    pub stringer: f64,
    pub foam: f64,
    pub deck_glass: f64,
    pub deck_lap: f64,
    pub bottom_glass: f64,
    pub bottom_lap: f64,
    /// All glass: deck and bottom cloth with their laps.
    pub glass: f64,
    pub resin: f64,
    pub hotcoat: f64,
    pub plugs_and_fins: f64,
    pub total: f64,
    pub deck_glass_area: f64,
    pub bottom_glass_area: f64,
    pub stringer_volume: f64,
    pub foam_volume: f64,
}

/// Estimate the finished weight of `board` with volume `volume` (cm³, usually [`Board::volume`]).
pub fn estimate(board: &Board, volume: f64, layup: &Layup) -> Weights {
    let l = board.length();
    let stringer_area =
        simpson(|x| board.deck_at(x), 0.0, l, 20) - simpson(|x| board.rocker_at(x), 0.0, l, 20);
    let stringer_volume = stringer_area * layup.stringer_width;
    let stringer = stringer_volume / 1000.0 * layup.stringer_density;
    let foam_volume = volume - stringer_volume;
    let foam = foam_volume / 1000.0 * layup.foam_density;
    let girth = |x: f64, deck: bool| {
        if board.is_asymmetric() {
            (board.half_girth_on(x, deck, Rail::Left) + board.half_girth_on(x, deck, Rail::Right))
                / 2.0
        } else {
            board.half_girth(x, deck)
        }
    };
    let deck_glass_area = 2.0 * simpson_par(|x| girth(x, true), ZERO, l - ZERO, 10);
    let bottom_glass_area = 2.0 * simpson_par(|x| girth(x, false), ZERO, l - ZERO, 10);
    let deck_glass = deck_glass_area / 10000.0 * layup.deck_glass;
    let bottom_glass = bottom_glass_area / 10000.0 * layup.bottom_glass;
    let rail = match &board.outline_left {
        None => board.outline.length(),
        Some(left) => (board.outline.length() + left.length()) / 2.0,
    };
    let deck_lap = rail * layup.deck_lap_width * 2.0 / 10000.0 * layup.deck_glass;
    let bottom_lap = rail * layup.bottom_lap_width * 2.0 / 10000.0 * layup.bottom_glass;
    let glass = deck_glass + deck_lap + bottom_glass + bottom_lap;
    let resin = glass * layup.resin_ratio;
    let total = stringer + foam + glass + resin + layup.hotcoat + layup.plugs_and_fins;
    Weights {
        stringer,
        foam,
        deck_glass,
        deck_lap,
        bottom_glass,
        bottom_lap,
        glass,
        resin,
        hotcoat: layup.hotcoat,
        plugs_and_fins: layup.plugs_and_fins,
        total,
        deck_glass_area,
        bottom_glass_area,
        stringer_volume,
        foam_volume,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::toy_board;

    #[test]
    fn defaults_follow_length() {
        let mut b = toy_board();
        assert_eq!(Layup::defaults_for(&b).stringer_width, 0.3);
        b.scale(8.0 * FOOT, 55.0, 7.0);
        assert_eq!(Layup::defaults_for(&b).stringer_width, 0.5);
        b.scale(10.0 * FOOT, 55.0, 7.0);
        let l = Layup::defaults_for(&b);
        assert_eq!((l.stringer_width, l.deck_glass), (0.8, 0.4));
    }

    #[test]
    fn total_adds_up() {
        let b = toy_board();
        let l = Layup::defaults_for(&b);
        let w = estimate(&b, b.volume(), &l);
        let sum = w.stringer + w.foam + w.glass + w.resin + w.hotcoat + w.plugs_and_fins;
        assert!((w.total - sum).abs() < 1e-12);
        assert!(w.deck_glass_area > b.area() / 2.0);
        assert!((w.resin - w.glass * 1.5).abs() < 1e-12);
    }
}
