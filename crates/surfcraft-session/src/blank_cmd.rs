//! JSON commands for foam blanks: `list_blanks` and `fit_blank` (see [`surfcraft_core::blank`]).

use serde_json::{Value, json};
use surfcraft_core::blank::{self, Blank, BlankSetup, FitReport, FitView, Measure};
use surfcraft_core::units::FOOT;

use crate::commands::{
    Context, R, ToolSpec, arg, len_schema, length, measure, number, obj, string,
};
use crate::document::Document;

pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_blanks",
            description: "Built-in foam blank library (published catalogue dimensions with their measured stations, plus a few labelled generic presets) and the blank chosen for the open board, if any.",
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "fit_blank",
            description: "Check that the board fits inside a foam blank: a library blank by name, explicit dimensions, or the blank stored with the board. Places the board centred (or at tail_offset / tilt / lift, or auto = search shift and tilt for the largest clearance) and reports whether it fits, the smallest foam clearance in outline (per side), deck and bottom with where they occur, foam left at nose and tail, and every place the board sticks out. store = true saves the blank and placement in the board (undoable).",
            input_schema: obj(
                json!({
                    "blank": {"type": "string", "description": "Library blank name (see list_blanks); a unique part of the name is enough"},
                    "length": len_schema("blank length tip to tip"),
                    "width": len_schema("blank widest width"),
                    "thickness": len_schema("blank thickness at the middle"),
                    "nose_rocker": len_schema("blank nose rocker"),
                    "tail_rocker": len_schema("blank tail rocker"),
                    "wide_point": len_schema("wide point from the middle, positive toward the nose"),
                    "nose_width": len_schema("blank width 12\" from the nose"),
                    "tail_width": len_schema("blank width 12\" from the tail"),
                    "name": {"type": "string", "description": "Name for explicit dimensions"},
                    "tail_offset": len_schema("distance from the blank's tail to the board's tail (default: centred)"),
                    "tilt": {"type": "number", "description": "Tilt in the profile plane, degrees, positive lifts the nose (default 0)"},
                    "lift": len_schema("height of the board above the blank's bottom (default: centred)"),
                    "auto": {"type": "boolean", "description": "Search the shift and tilt that give the largest smallest clearance"},
                    "store": {"type": "boolean", "description": "Save the blank and the placement with the board"},
                }),
                &[],
            ),
        },
    ]
}

fn dims(b: &Blank, ctx: &Context) -> Value {
    let opt = |v: Option<f64>| v.map(|v| measure(ctx, v));
    let stations: Vec<Value> = b
        .stations
        .iter()
        .map(|s| {
            json!({
                "from": if s.from_nose { "nose" } else { "tail" },
                "distance": measure(ctx, s.dist),
                "width": opt(s.width),
                "thickness": opt(s.thickness),
                "rocker": opt(s.rocker),
            })
        })
        .collect();
    json!({
        "name": b.name,
        "length": measure(ctx, b.length),
        "width": measure(ctx, b.width),
        "wide_point": measure(ctx, b.wide_point),
        "thickness": measure(ctx, b.thickness),
        "nose_rocker": measure(ctx, b.nose_rocker),
        "tail_rocker": measure(ctx, b.tail_rocker),
        "stations": stations,
    })
}

pub(crate) fn list_blanks(doc: &Document, ctx: &Context) -> R {
    let lib: Vec<Value> = blank::library().iter().map(|b| dims(b, ctx)).collect();
    Ok(json!({
        "library": lib,
        "board_blank": doc.board.blank.as_ref().map(|s| dims(&s.blank, ctx)),
        "note": "User blanks saved in the app are not listed here; give their dimensions to fit_blank.",
    }))
}

fn view_name(v: FitView) -> &'static str {
    match v {
        FitView::Outline => "outline",
        FitView::Deck => "deck",
        FitView::Bottom => "bottom",
        FitView::Length => "length",
    }
}

/// The blank from the arguments: a library name, explicit dimensions (over the named blank, if any), or the
/// board's stored blank.
fn blank_from_args(doc: &Document, a: &Value) -> Result<Blank, String> {
    let main = ["length", "width", "thickness"];
    let given = main.iter().filter(|k| arg(a, k).is_some()).count();
    let mut b = match (string(a, "blank"), &doc.board.blank) {
        (Some(n), _) => blank::find(n)
            .ok_or_else(|| format!("blank: no library blank matches {n:?} (see list_blanks)"))?,
        (None, _) if given == main.len() => Blank {
            name: "Custom blank".into(),
            ..Blank::default()
        },
        (None, Some(s)) if given == 0 => s.blank.clone(),
        _ => {
            return Err(
                "give a library blank name, or the blank's length, width and thickness".into(),
            );
        }
    };
    for (k, f) in [
        ("length", &mut b.length),
        ("width", &mut b.width),
        ("thickness", &mut b.thickness),
        ("nose_rocker", &mut b.nose_rocker),
        ("tail_rocker", &mut b.tail_rocker),
        ("wide_point", &mut b.wide_point),
    ] {
        if let Some(v) = length(a, k)? {
            *f = v;
        }
    }
    for (k, nose) in [("nose_width", true), ("tail_width", false)] {
        if let Some(v) = length(a, k)? {
            b.set_station(nose, FOOT, Measure::Width, Some(v));
        }
    }
    if let Some(n) = string(a, "name") {
        b.name = n.chars().take(200).collect();
    }
    b.validate().map_err(|e| format!("blank: {e}"))?;
    Ok(b)
}

pub(crate) fn report_json(r: &FitReport, ctx: &Context) -> Value {
    let clear =
        |c: &blank::Clearance| json!({"min": measure(ctx, c.min), "at": measure(ctx, c.at)});
    let problems: Vec<Value> = r
        .problems
        .iter()
        .map(|p| {
            json!({
                "view": view_name(p.view),
                "from": measure(ctx, p.from),
                "to": measure(ctx, p.to),
                "sticks_out_by": measure(ctx, p.depth),
                "worst_at": measure(ctx, p.at),
            })
        })
        .collect();
    json!({
        "fits": r.fits,
        "min_clearance": measure(ctx, r.min_clearance()),
        "outline_per_side": clear(&r.outline),
        "deck": clear(&r.deck),
        "bottom": clear(&r.bottom),
        "tail_spare": measure(ctx, r.tail_spare),
        "nose_spare": measure(ctx, r.nose_spare),
        "placement": {
            "tail_offset": measure(ctx, r.tail_offset),
            "tilt_degrees": (r.tilt * 1000.0).round() / 1000.0,
            "lift": measure(ctx, r.lift),
        },
        "problems": problems,
        "positions": "from the board's tail",
    })
}

pub(crate) fn fit_blank(doc: &mut Document, ctx: &Context, a: &Value) -> R {
    let b = blank_from_args(doc, a)?;
    let stored = doc
        .board
        .blank
        .as_ref()
        .filter(|s| s.blank == b)
        .map(|s| s.placement);
    let auto = matches!(arg(a, "auto"), Some(Value::Bool(true)));
    let mut p = if auto {
        blank::auto_fit(&doc.board, &b)
    } else {
        stored.unwrap_or_default()
    };
    if let Some(v) = length(a, "tail_offset")? {
        p.tail_offset = Some(v);
    }
    if let Some(v) = number(a, "tilt")? {
        if v.abs() > 30.0 {
            return Err("tilt must be between -30 and 30 degrees".into());
        }
        p.tilt = v;
    }
    if let Some(v) = length(a, "lift")? {
        p.lift = Some(v);
    }
    let r = blank::fit(&doc.board, &b, &p);
    let mut out = report_json(&r, ctx);
    out["blank"] = dims(&b, ctx);
    if matches!(arg(a, "store"), Some(Value::Bool(true))) {
        let setup = BlankSetup {
            blank: b,
            placement: p,
        };
        doc.edit(|board| board.blank = Some(setup));
        out["stored"] = json!(true);
    }
    Ok(out)
}
