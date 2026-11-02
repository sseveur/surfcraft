//! The fin-setup part of the `set_fins` command: presets and box systems, and the fin list it returns.

use serde_json::{Value, json};
use surfcraft_core::Board;
use surfcraft_core::fins::{BoxSystem, FinSetup, Preset, Side};

use crate::document::Document;

/// Names accepted for `preset`.
pub const PRESETS: [&str; 7] = [
    "single",
    "twin",
    "twin_trailer",
    "thruster",
    "quad",
    "2+1",
    "five",
];

/// Names accepted for `box_system`.
pub const BOX_SYSTEMS: [&str; 5] = ["fcs1", "fcs2", "futures", "usbox", "none"];

/// Apply `preset` and/or `box_system` from the arguments. `Ok(false)` when neither is given.
pub(crate) fn apply(doc: &mut Document, a: &Value) -> Result<bool, String> {
    let get = |k: &str| a.get(k).filter(|v| !v.is_null());
    let preset = match get("preset") {
        None => None,
        Some(Value::String(s)) => Some(Preset::from_code(s).ok_or_else(|| {
            format!(
                "preset: unknown {s:?}, expected one of {}",
                PRESETS.join(", ")
            )
        })?),
        Some(v) => return Err(format!("preset: expected text, got {v}")),
    };
    let system = match get("box_system") {
        None => None,
        Some(Value::String(s)) => Some(BoxSystem::from_code(s).ok_or_else(|| {
            format!(
                "box_system: unknown {s:?}, expected one of {}",
                BOX_SYSTEMS.join(", ")
            )
        })?),
        Some(v) => return Err(format!("box_system: expected text, got {v}")),
    };
    if preset.is_none() && system.is_none() {
        return Ok(false);
    }
    let current = doc.board.fin_setup();
    let system = system.unwrap_or(current.system);
    let setup = match preset {
        Some(Preset::Custom) => return Err("preset: \"custom\" cannot be applied".into()),
        Some(p) => FinSetup::preset(p, system, &doc.board),
        None => current.with_system(system),
    };
    doc.edit(|b| b.set_fin_setup(setup));
    Ok(true)
}

/// The fins of `board` as JSON (lengths in cm, angles in degrees).
pub(crate) fn setup_json(board: &Board) -> Value {
    let s = board.fin_setup();
    let fins: Vec<Value> = s
        .fins
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let side = match f.side() {
                Side::Left => "left",
                Side::Right => "right",
                Side::Centre => "centre",
            };
            json!({
                "fin": s.role_label(i),
                "side": side,
                "from_tail": f.x,
                "from_stringer": f.y,
                "from_rail": if f.side() == Side::Centre { Value::Null } else { json!(f.from_rail(board)) },
                "base": f.base,
                "toe_degrees": f.toe,
                "cant_degrees": f.cant,
                "depth": f.depth,
                "box_system": f.system.code(),
            })
        })
        .collect();
    json!({
        "preset": s.preset.code(),
        "box_system": s.system.code(),
        "stored": board.fin_setup.is_some(),
        "fins": fins,
    })
}
