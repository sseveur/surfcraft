use surfcraft_core::{Board, DVec2};
use surfcraft_export::Drawing;
use surfcraft_export::drawing::{Item, Style};
use surfcraft_export::woodwork::{
    self, ChamberedOptions, HwsOptions, SandwichOptions, paths_with_style,
};

fn load(name: &str) -> Board {
    let path = format!(
        "{}/../../tests/fixtures/{name}.brd",
        env!("CARGO_MANIFEST_DIR")
    );
    surfcraft_io::brd::read(&std::fs::read(path).unwrap()).unwrap()
}

fn boards() -> Vec<(&'static str, Board)> {
    vec![
        ("shortboard", load("shortboard")),
        ("longboard", load("longboard")),
    ]
}

fn cm(v: f64) -> String {
    format!("{v:.1} cm")
}

fn assert_finite(name: &str, d: &Drawing) {
    assert!(
        d.width.is_finite() && d.height.is_finite() && d.width > 0.0 && d.height > 0.0,
        "{name}: size {} x {}",
        d.width,
        d.height
    );
    for item in &d.items {
        match item {
            Item::Path { points, .. } => {
                for p in points {
                    assert!(p.is_finite(), "{name}: NaN/inf path point {p:?}");
                    assert!(
                        p.x >= -0.01
                            && p.y >= -0.01
                            && p.x <= d.width + 0.01
                            && p.y <= d.height + 0.01,
                        "{name}: point {p:?} outside {} x {}",
                        d.width,
                        d.height
                    );
                }
            }
            Item::Text { at, size_mm, .. } => {
                assert!(
                    at.is_finite() && size_mm.is_finite(),
                    "{name}: text at {at:?}"
                );
            }
        }
    }
}

fn cut_bounds(d: &Drawing) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::MAX);
    let mut hi = DVec2::splat(f64::MIN);
    for pts in paths_with_style(d, Style::CUT) {
        for p in pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    (lo, hi)
}

/// Heights where the closed polygon crosses the vertical line at `x`.
fn crossings(poly: &[DVec2], x: f64) -> Vec<f64> {
    let mut ys = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.x <= x && b.x > x) || (b.x <= x && a.x > x) {
            let t = (x - a.x) / (b.x - a.x);
            ys.push(a.y + t * (b.y - a.y));
        }
    }
    ys.sort_by(f64::total_cmp);
    ys
}

fn largest_cut(d: &Drawing) -> Vec<DVec2> {
    paths_with_style(d, Style::CUT)
        .max_by_key(|p| p.len())
        .unwrap()
        .to_vec()
}

#[test]
fn sandwich_profile_is_inset_by_skin() {
    for (name, b) in boards() {
        let skin = 0.3;
        let bare = woodwork::sandwich_profile(
            &b,
            &SandwichOptions {
                skin_thickness: 0.0,
                ..Default::default()
            },
            &cm,
        );
        let core = woodwork::sandwich_profile(&b, &SandwichOptions::default(), &cm);
        assert_finite(name, &bare);
        assert_finite(name, &core);
        let bare_poly = largest_cut(&bare);
        let core_poly = largest_cut(&core);
        // The bare profile starts at the tail (x = 0); both drawings share the layout origin (baseline).
        let x0 = bare_poly.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        let (lo_b, _) = bare.bounds().unwrap();
        let (lo_c, _) = core.bounds().unwrap();
        assert!((lo_b - lo_c).length() < 1e-6, "{name}: layouts differ");
        let length = b.length();
        for x in [length * 0.25, length * 0.5, length * 0.75] {
            let span = |poly: &[DVec2]| {
                let ys = crossings(poly, x0 + x * 10.0);
                ys.last().unwrap() - ys.first().unwrap()
            };
            let (t_bare, t_core) = (span(&bare_poly), span(&core_poly));
            assert!(
                (t_bare - b.thickness_at(x) * 10.0).abs() < 0.5,
                "{name} at {x}: bare {t_bare} vs thickness {}",
                b.thickness_at(x) * 10.0
            );
            let inset = t_bare - t_core;
            assert!(
                inset >= 2.0 * skin * 10.0 - 0.05 && inset < 2.0 * skin * 10.0 * 1.3,
                "{name} at {x}: core inset {inset} mm"
            );
        }
        let flat = woodwork::sandwich_profile(
            &b,
            &SandwichOptions {
                flatten: true,
                ..Default::default()
            },
            &cm,
        );
        assert_finite(name, &flat);
    }
}

#[test]
fn sandwich_rail_and_skins() {
    for (name, b) in boards() {
        let opts = SandwichOptions::default();
        let rail = woodwork::sandwich_rail(&b, &opts, &cm);
        assert_finite(name, &rail);
        let (lo, hi) = cut_bounds(&rail);
        let usable = (b.length() - opts.tail_offset - opts.nose_offset) * 10.0;
        // Unrolled along the curved rail line: a bit longer than straight, never much shorter.
        assert!(
            hi.x - lo.x > usable * 0.85 && hi.x - lo.x < usable * 1.1,
            "{name}: rail length {} vs {usable}",
            hi.x - lo.x
        );
        // Rail height is under the board's max thickness.
        assert!(hi.y - lo.y > 10.0);
        let flat = woodwork::sandwich_rail(
            &b,
            &SandwichOptions {
                flatten: true,
                ..opts
            },
            &cm,
        );
        assert_finite(name, &flat);

        let deck = woodwork::sandwich_deck_skin(&b, &opts, &cm);
        let bottom = woodwork::sandwich_bottom_skin(&b, &opts, &cm);
        assert_finite(name, &deck);
        assert_finite(name, &bottom);
        let (dlo, dhi) = cut_bounds(&deck);
        let (blo, bhi) = cut_bounds(&bottom);
        // Half width over the crown is at least the flat half width at the rail line and not much more.
        let half = (b.max_width() / 2.0 - opts.distance_to_rail) * 10.0;
        for (what, w) in [("deck", dhi.y - dlo.y), ("bottom", bhi.y - blo.y)] {
            assert!(
                w > half - 2.0 && w < half * 1.15,
                "{name} {what}: half width {w} vs {half}"
            );
        }
        // Lengths over the curve are close to the board length.
        for (what, l) in [("deck", dhi.x - dlo.x), ("bottom", bhi.x - blo.x)] {
            assert!(
                l > b.length() * 10.0 * 0.85 && l < b.length() * 10.0 * 1.1,
                "{name} {what}: length {l}"
            );
        }
        assert_ne!(
            largest_cut(&deck),
            largest_cut(&bottom),
            "{name}: bottom skin is the deck skin"
        );
    }
}

#[test]
fn hws_stringer_and_rail() {
    for (name, b) in boards() {
        let opts = HwsOptions::default();
        let stringer = woodwork::hws_stringer(&b, &opts, &cm);
        assert_finite(name, &stringer);
        let outline = largest_cut(&stringer);
        let (lo, hi) = outline.iter().fold(
            (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        // Board length less the end offsets, a little shorter where the skins meet at the very ends.
        let usable = (b.length() - opts.tail_offset - opts.nose_offset) * 10.0;
        assert!(
            hi.x - lo.x <= usable + 1.0 && hi.x - lo.x > usable - 30.0,
            "{name}: stringer {} mm vs {usable}",
            hi.x - lo.x
        );
        // The whole template spans the board length (zero-rocker baseline 5 mm past each end).
        let (lo, hi) = stringer.bounds().unwrap();
        assert!(
            (hi.x - lo.x - (b.length() * 10.0 + 10.0)).abs() < 0.5,
            "{name}: stringer template {} mm",
            hi.x - lo.x
        );
        // Webbing holes plus the two end-piece slots.
        let cuts = paths_with_style(&stringer, Style::CUT).count();
        assert!(cuts >= 1 + 2 + 4, "{name}: only {cuts} stringer cuts");

        let rail = woodwork::hws_rail(&b, &opts, &cm);
        assert_finite(name, &rail);
        let (lo, hi) = cut_bounds(&rail);
        assert!(
            hi.x - lo.x > usable * 0.85 && hi.x - lo.x < usable * 1.1,
            "{name}: rail {} mm",
            hi.x - lo.x
        );
        // One slot per rib at least.
        let ribs = woodwork::hws_rib_stations(&b).len();
        assert!(paths_with_style(&rail, Style::CUT).count() > ribs);

        let deck = woodwork::hws_deck_skin(&b, &opts, &cm);
        let bottom = woodwork::hws_bottom_skin(&b, &opts, &cm);
        assert_finite(name, &deck);
        assert_finite(name, &bottom);
        assert_ne!(largest_cut(&deck), largest_cut(&bottom), "{name}");
    }
}

#[test]
fn hws_ribs_fit_inside_rail_strip() {
    for (name, b) in boards() {
        let opts = HwsOptions::default();
        let stations = woodwork::hws_rib_stations(&b);
        let expected = ((b.length() - woodwork::HWS_PIECE_LENGTH) / 30.48).floor() as usize;
        assert_eq!(stations.len(), expected, "{name}");
        assert!(!stations.is_empty());
        for &x in &stations {
            let rib = woodwork::hws_rib(&b, x, &opts, &cm);
            assert_finite(name, &rib);
            let (lo, hi) = cut_bounds(&rib);
            // Tab tips reach the outer face of the rail strip.
            let sin = b.outline.tangent_at(x).abs().sin().max(0.2);
            let expect = (b.width_at(x) - 2.0 * opts.distance_from_rail / sin) * 10.0;
            assert!(
                (hi.x - lo.x - expect).abs() < 0.5,
                "{name} rib at {x}: {} mm vs {expect}",
                hi.x - lo.x
            );
            // Inside the finished section (the long dashed paths), inset by the skin top and bottom.
            let section: Vec<DVec2> = paths_with_style(&rib, Style::GUIDE)
                .filter(|p| p.len() > 50)
                .flatten()
                .copied()
                .collect();
            let top = section.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            let bottom = section.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let skin = opts.skin_thickness * 10.0;
            assert!(
                hi.y <= top - skin + 0.3 && lo.y >= bottom + skin - 0.3,
                "{name} rib at {x}: {}..{} vs section {bottom}..{top}",
                lo.y,
                hi.y
            );
        }
        let all = woodwork::hws_ribs(&b, &opts, &cm);
        assert_finite(name, &all);
    }
}

#[test]
fn hws_end_pieces() {
    for (name, b) in boards() {
        let opts = HwsOptions::default();
        for (what, d) in [
            ("tail", woodwork::hws_tail_piece(&b, &opts, &cm)),
            ("nose", woodwork::hws_nose_piece(&b, &opts, &cm)),
        ] {
            assert_finite(name, &d);
            let outline = largest_cut(&d);
            assert!(outline.len() > 10, "{name} {what} piece is empty");
            let (lo, hi) = cut_bounds(&d);
            let len = hi.x - lo.x;
            // About 9" less the end offset; shorter where the end is pointed.
            let nominal = (woodwork::HWS_PIECE_LENGTH - 3.5) * 10.0;
            assert!(
                len > nominal * 0.6 && len < nominal * 1.1,
                "{name} {what}: {len}"
            );
            // Symmetric about the stringer line.
            let mid = (lo.y + hi.y) / 2.0;
            for p in &outline {
                let q = DVec2::new(p.x, 2.0 * mid - p.y);
                assert!(
                    outline.iter().any(|r| (*r - q).length() < 0.01),
                    "{name} {what}: not symmetric at {p:?}"
                );
            }
        }
    }
}

#[test]
fn chambered_planks_cover_half_width() {
    let opts = ChamberedOptions::default();
    for (name, b) in boards() {
        let offsets = woodwork::chambered_plank_offsets(&b, &opts);
        let expected = (b.max_width() / 2.0 / opts.plank_thickness).ceil() as usize;
        assert_eq!(offsets.len(), expected, "{name}");

        // Centre plank: the full stringer profile with chambers inside it.
        let centre = woodwork::chambered_plank(&b, 0.0, &opts, &cm);
        assert_finite(name, &centre);
        let outline = largest_cut(&centre);
        let (lo, hi) = outline.iter().fold(
            (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        assert!(
            (hi.x - lo.x - b.length() * 10.0).abs() < 5.0,
            "{name}: centre plank {} mm",
            hi.x - lo.x
        );
        let chambers: Vec<_> = paths_with_style(&centre, Style::CUT)
            .filter(|p| p.len() != outline.len())
            .collect();
        assert!(chambers.len() >= 4, "{name}: {} chambers", chambers.len());
        for c in &chambers {
            for p in c.iter() {
                let ys = crossings(&outline, p.x);
                assert!(
                    ys.len() >= 2
                        && p.y >= ys[0] + opts.shell_thickness * 10.0 - 0.5
                        && p.y <= ys[ys.len() - 1] - opts.shell_thickness * 10.0 + 0.5,
                    "{name}: chamber point {p:?} outside the shell"
                );
            }
        }
        // Planks further out are shorter.
        let outer = woodwork::chambered_plank(&b, offsets[offsets.len() / 2], &opts, &cm);
        assert_finite(name, &outer);
        let (olo, ohi) = cut_bounds(&outer);
        assert!(ohi.x - olo.x < hi.x - lo.x);
    }
    let (name, b) = &boards()[0];
    let all = woodwork::chambered_planks(b, &opts, &cm);
    assert_eq!(all.len(), woodwork::chambered_plank_offsets(b, &opts).len());
    for d in &all {
        assert_finite(name, d);
    }
    let sheet = woodwork::stack(&all, 10.0);
    assert_finite(name, &sheet);
}
