//! Volume advisor: a starting-point board volume for a rider, by the "Guild factor" method used by most surfboard
//! volume calculators (litres = rider weight in kg x a factor that falls with skill), adjusted for age, fitness,
//! wave type and board type. Values and sources: `docs/volume-advisor.md`.

/// Surfing ability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skill {
    Beginner,
    #[default]
    Intermediate,
    Advanced,
    Pro,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fitness {
    Low,
    #[default]
    Average,
    High,
}

/// The waves the board is mostly ridden in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Waves {
    /// Small, weak, mushy.
    Small,
    #[default]
    Average,
    /// Overhead, powerful, hollow.
    Powerful,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardType {
    #[default]
    Shortboard,
    /// Fish, hybrid, groveller.
    Fish,
    /// Mid-length, funboard, egg.
    MidLength,
    Longboard,
}

impl Skill {
    pub const ALL: [Skill; 4] = [
        Skill::Beginner,
        Skill::Intermediate,
        Skill::Advanced,
        Skill::Pro,
    ];

    /// Guild factor range (litres per kg) for a performance shortboard, average fitness and waves.
    pub fn factor(self) -> (f64, f64) {
        match self {
            Skill::Beginner => (0.50, 0.60),
            Skill::Intermediate => (0.40, 0.45),
            Skill::Advanced => (0.36, 0.40),
            Skill::Pro => (0.33, 0.36),
        }
    }
}

impl Fitness {
    pub const ALL: [Fitness; 3] = [Fitness::Low, Fitness::Average, Fitness::High];

    /// Multiplier on the volume.
    pub fn multiplier(self) -> f64 {
        match self {
            Fitness::Low => 1.10,
            Fitness::Average => 1.0,
            Fitness::High => 0.95,
        }
    }
}

impl Waves {
    pub const ALL: [Waves; 3] = [Waves::Small, Waves::Average, Waves::Powerful];

    /// Multiplier on the volume.
    pub fn multiplier(self) -> f64 {
        match self {
            Waves::Small => 1.08,
            Waves::Average => 1.0,
            Waves::Powerful => 0.93,
        }
    }
}

impl BoardType {
    pub const ALL: [BoardType; 4] = [
        BoardType::Shortboard,
        BoardType::Fish,
        BoardType::MidLength,
        BoardType::Longboard,
    ];

    /// Litres added to the shortboard volume.
    pub fn extra_litres(self) -> f64 {
        match self {
            BoardType::Shortboard => 0.0,
            BoardType::Fish => 4.0,
            BoardType::MidLength => 10.0,
            BoardType::Longboard => 20.0,
        }
    }
}

/// Multiplier for the rider's age in years (0 = not given): paddling power falls with age.
pub fn age_multiplier(age: u32) -> f64 {
    match age {
        0..35 => 1.0,
        35..50 => 1.04,
        50..60 => 1.08,
        _ => 1.12,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rider {
    pub weight_kg: f64,
    /// Years; 0 when not given.
    pub age: u32,
    pub skill: Skill,
    pub fitness: Fitness,
    pub waves: Waves,
    pub board: BoardType,
}

impl Default for Rider {
    fn default() -> Self {
        Self {
            weight_kg: 75.0,
            age: 0,
            skill: Skill::default(),
            fitness: Fitness::default(),
            waves: Waves::default(),
            board: BoardType::default(),
        }
    }
}

/// Suggested volume range, litres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeAdvice {
    pub min: f64,
    pub max: f64,
}

impl VolumeAdvice {
    pub fn middle(&self) -> f64 {
        (self.min + self.max) / 2.0
    }

    pub fn contains(&self, litres: f64) -> bool {
        (self.min..=self.max).contains(&litres)
    }
}

/// Suggested volume for a rider; `None` for a weight outside 10–250 kg.
pub fn suggest_volume(r: &Rider) -> Option<VolumeAdvice> {
    if !(10.0..=250.0).contains(&r.weight_kg) {
        return None;
    }
    let (lo, hi) = r.skill.factor();
    let m = r.fitness.multiplier() * r.waves.multiplier() * age_multiplier(r.age);
    let extra = r.board.extra_litres();
    Some(VolumeAdvice {
        min: r.weight_kg * lo * m + extra,
        max: r.weight_kg * hi * m + extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rider(kg: f64, skill: Skill) -> Rider {
        Rider {
            weight_kg: kg,
            skill,
            ..Default::default()
        }
    }

    #[test]
    fn guild_factor_baseline() {
        // 75 kg intermediate on a shortboard: 0.40-0.45 L/kg.
        let a = suggest_volume(&rider(75.0, Skill::Intermediate)).unwrap();
        assert!((a.min - 30.0).abs() < 1e-9 && (a.max - 33.75).abs() < 1e-9);
        assert!((a.middle() - 31.875).abs() < 1e-9);
        assert!(a.contains(32.0) && !a.contains(29.0));
        // 70 kg pro: about 23-25 L, as pro shortboards are.
        let p = suggest_volume(&rider(70.0, Skill::Pro)).unwrap();
        assert!(p.min > 22.0 && p.max < 26.0, "{p:?}");
    }

    #[test]
    fn more_skill_less_volume() {
        let v: Vec<f64> = Skill::ALL
            .iter()
            .map(|&s| suggest_volume(&rider(80.0, s)).unwrap().middle())
            .collect();
        assert!(v.windows(2).all(|w| w[0] > w[1]), "{v:?}");
    }

    #[test]
    fn adjustments_go_the_right_way() {
        let base = rider(80.0, Skill::Advanced);
        let mid = |r: Rider| suggest_volume(&r).unwrap().middle();
        let b = mid(base);
        assert!(
            mid(Rider {
                fitness: Fitness::Low,
                ..base
            }) > b
        );
        assert!(
            mid(Rider {
                fitness: Fitness::High,
                ..base
            }) < b
        );
        assert!(
            mid(Rider {
                waves: Waves::Small,
                ..base
            }) > b
        );
        assert!(
            mid(Rider {
                waves: Waves::Powerful,
                ..base
            }) < b
        );
        assert_eq!(mid(Rider { age: 25, ..base }), b);
        assert!(mid(Rider { age: 55, ..base }) > mid(Rider { age: 40, ..base }));
        assert!(mid(Rider { age: 70, ..base }) > mid(Rider { age: 55, ..base }));
        let types: Vec<f64> = BoardType::ALL
            .iter()
            .map(|&t| mid(Rider { board: t, ..base }))
            .collect();
        assert!(types.windows(2).all(|w| w[0] < w[1]));
        assert!((types[3] - b - 20.0).abs() < 1e-9);
    }

    #[test]
    fn heavier_rider_more_volume_and_bad_weights_rejected() {
        let a = suggest_volume(&rider(60.0, Skill::Beginner)).unwrap();
        let b = suggest_volume(&rider(90.0, Skill::Beginner)).unwrap();
        assert!(b.min > a.max);
        assert!(suggest_volume(&rider(0.0, Skill::Beginner)).is_none());
        assert!(suggest_volume(&rider(f64::NAN, Skill::Beginner)).is_none());
        assert!(suggest_volume(&rider(400.0, Skill::Beginner)).is_none());
    }
}
