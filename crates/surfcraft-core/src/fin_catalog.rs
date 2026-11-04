//! Commercial fins (FCS, Futures, Firewire) with the specs their makers publish, shipped with the app
//! (`data/fin-catalog.csv`, see `docs/fins.md`). Picking one sets each fin's base, depth, shape, foil and
//! (where published) cant from the specs.

use std::sync::OnceLock;

use crate::fins::{BoxSystem, Fin, FinSetup, FinShape, Preset, Side};
use crate::units::INCH;

/// Which fins of a set a catalogue row describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Side,
    Centre,
    Rear,
    Front,
    /// Every fin of the set is the same.
    All,
}

impl Role {
    fn from_code(s: &str) -> Option<Role> {
        Some(match s {
            "side" => Role::Side,
            "centre" => Role::Centre,
            "rear" => Role::Rear,
            "front" => Role::Front,
            "all" => Role::All,
            _ => return None,
        })
    }
}

/// One row of the catalogue: one fin (of one role) of one model and size. Lengths in cm, areas in cm².
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogFin {
    pub role: Role,
    pub system: BoxSystem,
    pub base: f64,
    pub depth: f64,
    pub area: Option<f64>,
    /// Sweep angle in degrees, as FCS publishes it.
    pub sweep: Option<f64>,
    /// Cant in degrees, as Futures publishes it ("Angle").
    pub cant: Option<f64>,
    /// The foil as published (e.g. "Flat", "80/20", "SYMM", "Vector 3/2").
    pub foil: String,
}

/// A fin product: one model in one size, sold as one setup, with a row per role.
#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub brand: String,
    pub model: String,
    pub size: String,
    pub setup: Preset,
    pub fins: Vec<CatalogFin>,
    pub source_url: String,
}

impl Product {
    /// "FCS Performer M" (the size left out when there is only one).
    pub fn name(&self) -> String {
        let mut s = format!("{} {}", self.brand, self.model);
        if !self.size.is_empty() {
            s.push(' ');
            s.push_str(&self.size);
        }
        s
    }

    /// The box system of the side fins (or of the centre fin for a single).
    pub fn system(&self) -> BoxSystem {
        self.fins
            .iter()
            .find(|f| f.role != Role::Centre)
            .or(self.fins.first())
            .map_or(BoxSystem::None, |f| f.system)
    }

    /// The row for a fin in the given position and box: `pairs` side fin pairs on the board, `rear` for the rear
    /// pair of two. Falls back from the exact role to a similar one, but only to a fin that fits the box: a fin
    /// in a US box takes only US box fins, any other box only fins for FCS or Futures boxes. `None` when the set
    /// has no fin for it (a set of side bites has none for the centre of a 2+1).
    pub fn row_for(&self, f: &Fin, pairs: usize, rear: bool) -> Option<&CatalogFin> {
        let order: &[Role] = match (f.side(), pairs > 1, rear) {
            (Side::Centre, ..) => &[Role::Centre, Role::All],
            (_, true, true) => &[Role::Rear, Role::Side, Role::All],
            (_, true, false) => &[Role::Front, Role::Side, Role::All],
            _ => &[Role::Side, Role::All, Role::Front, Role::Rear],
        };
        let fits = |row: &CatalogFin| match f.system {
            BoxSystem::UsBox => row.system == BoxSystem::UsBox,
            // A position without a box takes any fin of its role; a US box fin only on the stringer.
            BoxSystem::None => row.system != BoxSystem::UsBox || f.side() == Side::Centre,
            _ => row.system != BoxSystem::UsBox,
        };
        order
            .iter()
            .find_map(|r| self.fins.iter().find(|row| row.role == *r && fits(row)))
    }

    /// For each fin of `setup`, the row it takes (see [`Product::row_for`]).
    pub fn rows_for(&self, setup: &FinSetup) -> Vec<Option<&CatalogFin>> {
        let count = |side: Side| setup.fins.iter().filter(|f| f.side() == side).count();
        let pairs = count(Side::Left).max(count(Side::Right));
        setup
            .fins
            .iter()
            .enumerate()
            .map(|(i, f)| self.row_for(f, pairs, setup.role_label(i).ends_with("rear")))
            .collect()
    }

    /// Give the fins of `setup` this product's sizes and shapes, keeping their positions. Side fins in front of
    /// another pair take the front (or side) fins, the rear pair the rear fins, centre fins the centre fin, each
    /// only if it fits the fin's box; fins the set has nothing for are left as they are. Each fin gets the
    /// published cant where there is one.
    pub fn apply(&self, setup: &mut FinSetup) {
        let rows: Vec<Option<CatalogFin>> = self
            .rows_for(setup)
            .into_iter()
            .map(|r| r.cloned())
            .collect();
        for (f, row) in setup.fins.iter_mut().zip(rows) {
            if let Some(row) = row {
                row.apply(f);
            }
        }
    }
}

impl CatalogFin {
    /// Set a fin's base, depth, shape, foil, box and cant from this row. The shape starts from the default
    /// template (a small round tip) and is fitted to the published area and sweep.
    pub fn apply(&self, f: &mut Fin) {
        f.base = self.base;
        f.depth = self.depth;
        f.system = self.system;
        let mut shape = FinShape {
            inside: f.shape.inside,
            ..FinShape::DEFAULT
        };
        if let Some(inside) = inside_share(&self.foil) {
            shape.inside = inside;
        }
        f.shape = shape.fitted(self.base, self.depth, self.area, self.sweep);
        if f.side() != Side::Centre
            && let Some(c) = self.cant.filter(|c| *c > 0.0)
        {
            f.cant = c;
        }
    }
}

/// The share of the thickness on the inside face for a published foil name, if the name says.
pub fn inside_share(foil: &str) -> Option<f64> {
    let f = foil.trim().to_ascii_lowercase();
    Some(match f.as_str() {
        "symm" | "sym" | "symmetrical" | "50/50" => 0.5,
        "flat" | "flat/bev" | "bevel" => 0.0,
        "80/20" | "ift - 80/20" => 0.2,
        "70/30" => 0.3,
        // Inside foils with a slight curve on the inside face (Futures Vector, FCS IFT and the like).
        "v" | "v1" | "v2" | "vector" | "vector 3/2" | "3/2" | "ift" | "cambered" | "stretch"
        | "tow" => 0.15,
        _ => return None,
    })
}

/// Every product in the catalogue, in file order.
pub fn products() -> &'static [Product] {
    static CATALOG: OnceLock<Vec<Product>> = OnceLock::new();
    CATALOG.get_or_init(|| parse(include_str!("../data/fin-catalog.csv")))
}

/// Products where every word of `query` starts a word of the name or setup (case-insensitive: "performer m"
/// finds the Performer M, not the XS), optionally only for one box system (its side fins; a US box single counts
/// for any system).
pub fn search(query: &str, system: Option<BoxSystem>) -> Vec<&'static Product> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    products()
        .iter()
        .filter(|p| {
            let name = format!("{} {}", p.name(), p.setup.label()).to_lowercase();
            let name_words: Vec<&str> = name
                .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
                .collect();
            words
                .iter()
                .all(|w| name_words.iter().any(|n| n.starts_with(w.as_str())))
        })
        .filter(|p| match system {
            None | Some(BoxSystem::None) => true,
            Some(s) => p.system() == s || p.system() == BoxSystem::UsBox,
        })
        .collect()
}

fn parse(text: &str) -> Vec<Product> {
    let mut lines = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols: Vec<&str> = header.split(',').collect();
    let col = |name: &str| cols.iter().position(|c| *c == name);
    let idx: Option<Vec<usize>> = [
        "brand",
        "model",
        "size",
        "system",
        "setup",
        "role",
        "base_in",
        "depth_in",
        "area_sqin",
        "sweep_deg",
        "cant_deg",
        "foil",
        "source_url",
    ]
    .iter()
    .map(|n| col(n))
    .collect();
    let Some(idx) = idx else {
        return Vec::new();
    };
    let mut out: Vec<Product> = Vec::new();
    for line in lines {
        let v: Vec<&str> = line.split(',').map(str::trim).collect();
        let get = |k: usize| v.get(idx[k]).copied().unwrap_or("");
        let num = |k: usize| {
            get(k)
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x > 0.0)
        };
        let (Some(base), Some(depth), Some(role), Some(setup)) = (
            num(6),
            num(7),
            Role::from_code(get(5)),
            Preset::from_code(get(4)),
        ) else {
            continue;
        };
        let system = match get(3) {
            "us_box" => BoxSystem::UsBox,
            s => BoxSystem::from_code(s).unwrap_or(BoxSystem::None),
        };
        let fin = CatalogFin {
            role,
            system,
            base: base * INCH,
            depth: depth * INCH,
            area: num(8).map(|a| a * INCH * INCH),
            sweep: num(9),
            cant: get(10).parse::<f64>().ok().filter(|c| c.is_finite()),
            foil: get(11).to_string(),
        };
        let (brand, model, size) = (get(0), get(1), get(2));
        match out.last_mut() {
            Some(p)
                if p.brand == brand && p.model == model && p.size == size && p.setup == setup =>
            {
                p.fins.push(fin)
            }
            _ => out.push(Product {
                brand: brand.to_string(),
                model: model.to_string(),
                size: size.to_string(),
                setup,
                fins: vec![fin],
                source_url: get(12).to_string(),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_loads_every_brand_with_sane_numbers() {
        let p = products();
        assert!(p.len() > 400, "{}", p.len());
        for brand in ["FCS", "Futures", "Firewire"] {
            assert!(p.iter().any(|x| x.brand == brand), "{brand}");
        }
        for x in p {
            assert!(
                !x.fins.is_empty() && x.source_url.starts_with("https://"),
                "{x:?}"
            );
            for f in &x.fins {
                // 1.5" to 12" deep (five-fin trailers are small), bases 2" to 13".
                assert!(
                    f.depth > 1.5 * INCH && f.depth < 12.5 * INCH,
                    "{}",
                    x.name()
                );
                assert!(f.base > 2.0 * INCH && f.base < 13.0 * INCH, "{}", x.name());
                if let Some(a) = f.area {
                    let ratio = a / (f.base * f.depth);
                    assert!(ratio > 0.3 && ratio < 1.2, "{} {ratio}", x.name());
                }
            }
        }
    }

    #[test]
    fn every_fin_fits_its_published_area() {
        for x in products() {
            for f in &x.fins {
                let Some(area) = f.area else { continue };
                let s = FinShape::DEFAULT.fitted(f.base, f.depth, Some(area), f.sweep);
                let got = s.area(f.base, f.depth);
                assert!(
                    (got - area).abs() / area < 0.02,
                    "{} {:?}: {got} vs {area}",
                    x.name(),
                    f.role
                );
            }
        }
    }

    #[test]
    fn applying_a_product_sizes_each_fin_by_role() {
        let board = crate::mesh::tests::toy_board();
        // A thruster set on a thruster: side rows on the side fins, the centre row on the centre fin.
        let futures = search("futures f6 alpha", None)
            .into_iter()
            .find(|p| p.setup == Preset::Thruster && p.fins.len() == 2)
            .expect("an F6 set with side and centre rows");
        let mut setup = FinSetup::preset(Preset::Thruster, BoxSystem::Futures, &board);
        let rows: Vec<CatalogFin> = futures
            .rows_for(&setup)
            .into_iter()
            .map(|r| r.expect("a row for every fin").clone())
            .collect();
        futures.apply(&mut setup);
        for (f, row) in setup.fins.iter().zip(&rows) {
            let role = if f.side() == Side::Centre {
                Role::Centre
            } else {
                Role::Side
            };
            assert_eq!(row.role, role);
            assert!((f.base - row.base).abs() < 1e-9 && (f.depth - row.depth).abs() < 1e-9);
            if let Some(a) = row.area {
                assert!((f.shape.area(f.base, f.depth) - a).abs() / a < 0.02);
            }
        }
        // A quad set: the rear pair takes the rear fins.
        let quad = products()
            .iter()
            .find(|p| {
                p.setup == Preset::Quad
                    && p.system() == BoxSystem::FcsII
                    && p.fins.iter().any(|f| f.role == Role::Rear)
            })
            .unwrap();
        let rear = quad
            .fins
            .iter()
            .find(|f| f.role == Role::Rear)
            .unwrap()
            .clone();
        let mut setup = FinSetup::preset(Preset::Quad, BoxSystem::FcsII, &board);
        quad.apply(&mut setup);
        for (i, f) in setup.fins.iter().enumerate() {
            if setup.role_label(i).ends_with("rear") {
                assert!((f.base - rear.base).abs() < 1e-9, "{}", quad.name());
            }
        }
    }

    #[test]
    fn a_2_plus_1_takes_only_fins_that_fit_its_boxes() {
        let board = crate::mesh::tests::toy_board();
        let fresh = FinSetup::preset(Preset::TwoPlusOne, BoxSystem::FcsII, &board);
        let centre = fresh
            .fins
            .iter()
            .position(|f| f.side() == Side::Centre)
            .unwrap();
        assert_eq!(fresh.fins[centre].system, BoxSystem::UsBox);
        // Side bites only: the sides change, the US box centre fin stays as it was.
        let bites = search("skindog side byte", None)[0];
        let mut s = fresh.clone();
        bites.apply(&mut s);
        assert_eq!(s.fins[centre], fresh.fins[centre]);
        for f in s.fins.iter().filter(|f| f.side() != Side::Centre) {
            assert!((f.base - bites.fins[0].base).abs() < 1e-9);
        }
        // Then a US box single for the centre: only the centre changes, and stays in its US box.
        let single = products()
            .iter()
            .find(|p| p.setup == Preset::Single && p.system() == BoxSystem::UsBox)
            .unwrap();
        let before = s.clone();
        single.apply(&mut s);
        assert_eq!(s.fins[centre].system, BoxSystem::UsBox);
        assert!((s.fins[centre].depth - single.fins[0].depth).abs() < 1e-9);
        for (i, f) in s.fins.iter().enumerate() {
            if i != centre {
                assert_eq!(*f, before.fins[i]);
            }
        }
        // A thruster set never puts its FCS II centre fin in the US box.
        let thruster = search("fcs performer m", None)
            .into_iter()
            .find(|p| p.setup == Preset::Thruster)
            .unwrap();
        let mut t = fresh.clone();
        thruster.apply(&mut t);
        assert_eq!(t.fins[centre], fresh.fins[centre]);
    }

    #[test]
    fn foil_names_map_to_inside_shares() {
        assert_eq!(inside_share("SYMM"), Some(0.5));
        assert_eq!(inside_share("Flat"), Some(0.0));
        assert_eq!(inside_share("80/20"), Some(0.2));
        assert_eq!(inside_share("Vector 3/2"), Some(0.15));
        assert_eq!(inside_share("CUSTOM"), None);
    }

    #[test]
    fn search_matches_words_and_box_system() {
        assert!(!search("fcs performer", None).is_empty());
        assert!(
            search("fcs performer", None)
                .iter()
                .all(|p| p.brand == "FCS")
        );
        let futures_only = search("", Some(BoxSystem::Futures));
        assert!(
            futures_only
                .iter()
                .all(|p| p.system() == BoxSystem::Futures || p.system() == BoxSystem::UsBox)
        );
        assert!(search("no such fin anywhere", None).is_empty());
        // Words match from their start: "m" is the size, not the m of "performer".
        let m = search("fcs performer m", None);
        assert!(!m.is_empty());
        assert!(
            m.iter()
                .all(|p| p.model != "Performer" || p.size.to_lowercase().starts_with('m'))
        );
    }
}
