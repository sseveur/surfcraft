//! Compare against reference values: the fixture boards measured by the reference program (BoardCAD 3.0.1,
//! run offline). Keys ending in `_java` are values the reference program gets wrong; the corrected value is
//! checked instead.

use serde_json::Value;
use surfcraft_core::{Board, SurfaceModel};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

fn load(name: &str, model: SurfaceModel) -> (Board, Value) {
    let brd = std::fs::read(format!("{FIXTURES}/{name}.brd")).unwrap();
    let mut board = surfcraft_io::brd::read(&brd).unwrap();
    board.model = model;
    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{FIXTURES}/{name}.golden.json")).unwrap(),
    )
    .unwrap();
    let key = match model {
        SurfaceModel::SBlend => "slinear",
        SurfaceModel::ControlPoint => "control_point",
    };
    (board, golden[key].clone())
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn arr(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(f).collect()
}

#[track_caller]
fn close(what: &str, got: f64, want: f64, abs: f64, rel: f64) {
    let tol = abs.max(want.abs() * rel);
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got}, want {want} (diff {:.3e}, tol {tol:.3e})",
        (got - want).abs()
    );
}

fn check_1d(name: &str) {
    let (b, g) = load(name, SurfaceModel::SBlend);
    close("length", b.length(), f(&g["length"]), 1e-9, 0.0);
    close(
        "length over curve",
        b.length_over_curve(),
        f(&g["length_over_curve"]),
        0.01,
        0.0,
    );
    close(
        "center width",
        b.center_width(),
        f(&g["center_width"]),
        0.005,
        0.0,
    );
    close(
        "center thickness",
        b.center_thickness(),
        f(&g["thickness"]),
        0.005,
        0.0,
    );
    let (t, tpos) = b.max_thickness_and_pos();
    close("max thickness", t, f(&g["max_thickness"]), 0.005, 0.0);
    close(
        "max thickness pos",
        tpos,
        f(&g["max_thickness_pos"]),
        0.5,
        0.0,
    );
    close(
        "max width pos",
        b.max_width_pos(),
        f(&g["max_width_pos"]),
        0.5,
        0.0,
    );
    // The original skipped the last outline segment; ours can only be equal or wider.
    assert!(b.max_width() >= f(&g["max_width_java"]) - 0.005);
    close(
        "max width",
        b.max_width(),
        f(&g["max_width_java"]),
        0.05,
        0.0,
    );
    assert!(b.max_rocker() >= f(&g["max_rocker_java"]) - 0.005);
    close("area", b.area(), f(&g["area"]), 0.0, 1e-4);

    let stations = arr(&g["stations"]);
    let cmp = |what: &str, key: &str, get: &dyn Fn(f64) -> f64, tol: f64| {
        for (x, want) in stations.iter().zip(arr(&g[key])) {
            close(&format!("{what} at {x:.2}"), get(*x), want, tol, 0.0);
        }
    };
    cmp("width", "width_at", &|x| b.width_at(x), 0.005);
    cmp("rocker", "rocker_at", &|x| b.rocker_at(x), 0.005);
    cmp("deck", "deck_at", &|x| b.deck_at(x), 0.005);
    cmp("thickness", "thickness_at", &|x| b.thickness_at(x), 0.01);
    cmp(
        "over bottom",
        "from_tail_over_bottom",
        &|x| b.from_tail_over_bottom(x),
        0.05,
    );
}

fn check_sections(name: &str) {
    let (b, g) = load(name, SurfaceModel::SBlend);
    let rows = g["sections"].as_array().unwrap();
    let real: Vec<_> = b.real_sections().map(|i| &b.sections[i]).collect();
    assert_eq!(real.len(), rows.len());
    for (s, r) in real.iter().zip(rows) {
        let at = format!("section {:.2}", s.position);
        close(
            &format!("{at} position"),
            s.position,
            f(&r["position"]),
            1e-9,
            0.0,
        );
        close(
            &format!("{at} width"),
            s.width(),
            f(&r["width"]),
            0.005,
            0.0,
        );
        close(
            &format!("{at} thickness"),
            s.center_thickness(),
            f(&r["center_thickness"]),
            1e-9,
            0.0,
        );
        close(
            &format!("{at} spline length"),
            s.spline.length(),
            f(&r["spline_length"]),
            0.01,
            0.0,
        );
        let apex = s.spline.s_by_normal_reverse(90f64.to_radians(), true);
        close(&format!("{at} s apex"), apex, f(&r["s_apex"]), 0.003, 0.0);
        let tuck = s.spline.s_by_normal_reverse(175f64.to_radians(), true);
        close(&format!("{at} s tuck"), tuck, f(&r["s_tuck"]), 0.003, 0.0);
        close(
            &format!("{at} release"),
            s.release_angle(),
            f(&r["release_angle"]),
            0.01,
            0.0,
        );
        close(
            &format!("{at} tuck radius"),
            s.tuck_radius(),
            f(&r["tuck_radius"]),
            0.02,
            0.02,
        );
    }
}

fn check_surface(name: &str, model: SurfaceModel) {
    let (b, g) = load(name, model);
    let n = g["surface_n"].as_u64().unwrap() as usize;
    let stations = arr(&g["stations"]);
    let deck = g["deck_points"].as_array().unwrap();
    let bottom = g["bottom_points"].as_array().unwrap();
    let normals = g["deck_normals"].as_array().unwrap();
    let mut k = 0;
    for &x in &stations {
        for i in 0..=n {
            let s = i as f64 / n as f64;
            for (what, want, (lo, hi)) in [
                ("deck", &deck[k], (-90.0, 90.0)),
                ("bottom", &bottom[k], (90.0, 360.0)),
            ] {
                let want = arr(want);
                let got = b.surface_point(x, s, lo, hi);
                let d = ((got.x - want[0]).powi(2)
                    + (got.y - want[1]).powi(2)
                    + (got.z - want[2]).powi(2))
                .sqrt();
                assert!(
                    d < 0.02,
                    "{model:?} {what} x={x:.2} s={s}: got {got:?}, want {want:?}"
                );
            }
            let want = arr(&normals[k]);
            let got = b.surface_normal(x, s, -90.0, 90.0);
            let dot = got.x * want[0] + got.y * want[1] + got.z * want[2];
            assert!(
                dot > 0.999,
                "{model:?} normal x={x:.2} s={s}: got {got:?}, want {want:?}"
            );
            k += 1;
        }
    }
    let areas = arr(&g["section_area_at"]);
    for (x, want) in stations.iter().zip(areas) {
        close(
            &format!("{model:?} section area at {x:.2}"),
            b.section_area_at(*x, 10),
            want,
            0.05,
            0.002,
        );
    }
}

fn check_integrals(name: &str, model: SurfaceModel) {
    let (b, g) = load(name, model);
    close(
        &format!("{model:?} volume"),
        b.volume(),
        f(&g["volume"]),
        0.0,
        0.002,
    );
    let com = b.center_of_mass();
    close(
        &format!("{model:?} centre of mass"),
        com,
        f(&g["center_of_mass"]),
        0.2,
        0.0,
    );
    close(
        &format!("{model:?} moment of inertia"),
        b.moment_of_inertia(com, 0.0),
        f(&g["moment_of_inertia"]),
        0.0,
        0.01,
    );
}

macro_rules! golden {
    ($($name:ident),*) => {$(
        mod $name {
            use super::*;
            #[test] fn measurements() { check_1d(stringify!($name)); }
            #[test] fn sections() { check_sections(stringify!($name)); }
            #[test] fn surface_sblend() { check_surface(stringify!($name), SurfaceModel::SBlend); }
            #[test] fn surface_control_point() { check_surface(stringify!($name), SurfaceModel::ControlPoint); }
            #[test] fn integrals_sblend() { check_integrals(stringify!($name), SurfaceModel::SBlend); }
            #[test] fn integrals_control_point() { check_integrals(stringify!($name), SurfaceModel::ControlPoint); }
        }
    )*};
}

golden!(shortboard, funboard, longboard);
