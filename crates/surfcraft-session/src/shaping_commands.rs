//! JSON commands of the section-shaping tools (see [`crate::shaping`]): `apply_bottom_contour` and `set_rails`.

use serde_json::{Value, json};

use crate::commands::{
    Context, R, ToolSpec, arg, len_schema, length, measure, number, obj, string,
};
use crate::document::Document;
use crate::shaping::{
    self, ContourKind, ContourZone, DEFAULT_SPACING, RAIL_PRESETS, RailParams, RailSpec, RunOut,
    ShapeReport,
};

pub(crate) fn specs() -> Vec<ToolSpec> {
    let kinds: Vec<&str> = ContourKind::ALL
        .iter()
        .map(|k| k.slug())
        .chain(["remove"])
        .collect();
    let presets: Vec<&str> = RAIL_PRESETS.iter().map(|p| p.0).collect();
    let rail_props = json!({
        "preset": {"type": "string", "enum": presets, "description": "Starting point: 50/50 (soft, apex in the middle), 60/40, 70/30, 80/20 (hard, low apex). Other fields override it."},
        "apex": {"type": "number", "description": "Apex height as a fraction of the rail height from the bottom (0.5 = 50/50, 0.4 = 60/40, 0.3 = 70/30)"},
        "softness": {"type": "number", "description": "0 = hard edge, 1 = soft rounded rail"},
        "tuck": len_schema("how far the edge sits inboard of the apex"),
        "band": len_schema("distance in from the edge where the rail meets the deck"),
    });
    let mut rails = rail_props.as_object().cloned().unwrap_or_default();
    rails.insert(
        "start".into(),
        len_schema("only sections from here (from tail)"),
    );
    rails.insert(
        "end".into(),
        len_schema("only sections up to here (from tail)"),
    );
    let zone = json!({"type": "object", "properties": rail_props});
    rails.insert(
        "tail".into(),
        json!({"type": "object", "description": "Rail at the tail (up to 20 % of the length), blending into the main one by mid-board. Same fields as the main spec.", "properties": zone["properties"]}),
    );
    rails.insert(
        "nose".into(),
        json!({"type": "object", "description": "Rail at the nose (from 80 % of the length), blending from the main one. Same fields as the main spec.", "properties": zone["properties"]}),
    );
    vec![
        ToolSpec {
            name: "apply_bottom_contour",
            description: "Shape the bottom of every cross-section in a zone: flat, single_concave, double_concave \
                          (two concaves inside a single), vee or channels; `remove` flattens the bottom back to \
                          level. The stringer rocker is kept: depth is measured with a straight edge across the \
                          run-out points (a concave ends `depth` below the stringer, a vee `depth` above). \
                          Interpolated sections are added inside the zone where needed. Undoable as one step; \
                          returns the volume before and after.",
            input_schema: obj(
                json!({
                    "type": {"type": "string", "enum": kinds},
                    "depth": len_schema("concave / vee depth, or channel depth (default 1/8\" concave, 1/4\" vee, 3/16\" channels)"),
                    "inner_depth": len_schema("depth of the two inner concaves of a double concave (default 1/16\")"),
                    "start": len_schema("zone start from the tail (default 0)"),
                    "end": len_schema("zone end from the tail"),
                    "fade_in": len_schema("length over which the contour grows from the zone start"),
                    "fade_out": len_schema("length over which the contour dies away before the zone end"),
                    "run_out_from_rail": len_schema("where the contour ends: distance in from the rail edge (default 1\")"),
                    "run_out_fraction": {"type": "number", "description": "where the contour ends: fraction of the local half-width from the stringer (0.1..1), instead of run_out_from_rail"},
                    "channel_count": {"type": "integer", "description": "channels per side (1..6, default 2)"},
                    "channel_width": len_schema("width of each channel (default 1\")"),
                    "section_spacing": len_schema("largest gap between sections inside the zone (default 15 cm)"),
                }),
                &["type"],
            ),
        },
        ToolSpec {
            name: "set_rails",
            description: "Rebuild the rail of every cross-section (or those between start and end): apex height, \
                          softness and tuck, from a preset (50/50, 60/40, 70/30, 80/20) and/or explicit values; \
                          optional tail and nose specs blend along the board. Width, thickness and the bottom \
                          contour are kept. Undoable as one step; returns the volume before and after.",
            input_schema: obj(Value::Object(rails), &[]),
        },
    ]
}

fn report_json(r: &ShapeReport, ctx: &Context) -> Value {
    let l = |cm3: f64| (cm3 / 10.0).round() / 100.0;
    json!({
        "sections_changed": r.sections_changed,
        "sections_added": r.sections_added,
        "skipped_sections_at": r.skipped.iter().map(|&x| measure(ctx, x)).collect::<Vec<_>>(),
        "volume_before_litres": l(r.volume_before),
        "volume_after_litres": l(r.volume_after),
        "volume_change_litres": l(r.volume_after - r.volume_before),
    })
}

pub(crate) fn apply_bottom_contour(doc: &mut Document, ctx: &mut Context, a: &Value) -> R {
    let ty = string(a, "type").ok_or("missing type")?;
    let board_len = doc.board.length();
    let remove = ty == "remove";
    let kind = if remove {
        ContourKind::Flat
    } else {
        ContourKind::from_slug(ty).ok_or_else(|| format!("unknown type {ty:?}"))?
    };
    let mut z = ContourZone::for_board(kind, &doc.board);
    if remove {
        z.end = board_len;
    }
    if let Some(v) = length(a, "depth")? {
        z.depth = v;
    }
    if let Some(v) = length(a, "inner_depth")? {
        z.inner_depth = v;
    }
    if let Some(v) = length(a, "start")? {
        z.start = v;
    }
    if let Some(v) = length(a, "end")? {
        z.end = v;
    }
    if let Some(v) = length(a, "fade_in")? {
        z.fade_in = v;
    }
    if let Some(v) = length(a, "fade_out")? {
        z.fade_out = v;
    }
    match (
        length(a, "run_out_from_rail")?,
        number(a, "run_out_fraction")?,
    ) {
        (Some(_), Some(_)) => {
            return Err("give run_out_from_rail or run_out_fraction, not both".into());
        }
        (Some(d), None) => z.run_out = RunOut::FromRail(d),
        (None, Some(f)) => z.run_out = RunOut::Fraction(f),
        (None, None) => {}
    }
    if let Some(v) = arg(a, "channel_count") {
        let n = v
            .as_u64()
            .filter(|n| *n <= 100)
            .ok_or("channel_count: expected a small non-negative integer")?;
        z.channel_count = n as u32;
    }
    if let Some(v) = length(a, "channel_width")? {
        z.channel_width = v;
    }
    let spacing = length(a, "section_spacing")?.unwrap_or(DEFAULT_SPACING);
    let mut b = doc.board.clone();
    let r = if remove {
        z.validate()?;
        shaping::remove_contours(&mut b, z.start, z.end, z.run_out)?
    } else {
        shaping::apply_contours(&mut b, &[z], spacing)?
    };
    doc.edit(|d| *d = b);
    let mut out = report_json(&r, ctx);
    out["type"] = json!(ty);
    Ok(out)
}

fn rail_spec(a: &Value, base: RailSpec) -> Result<RailSpec, String> {
    let mut s = match string(a, "preset") {
        Some(p) => RailSpec::preset(p).ok_or_else(|| format!("unknown preset {p:?}"))?,
        None if arg(a, "preset").is_some() => return Err("preset: expected a string".into()),
        None => base,
    };
    if let Some(v) = number(a, "apex")? {
        s.apex = v;
    }
    if let Some(v) = number(a, "softness")? {
        s.softness = v;
    }
    if let Some(v) = length(a, "tuck")? {
        s.tuck = v;
    }
    if let Some(v) = length(a, "band")? {
        s.band = v;
    }
    s.validate()?;
    Ok(s)
}

pub(crate) fn set_rails(doc: &mut Document, ctx: &mut Context, a: &Value) -> R {
    let middle = rail_spec(a, RailSpec::default())?;
    let mut p = RailParams::uniform(middle);
    for (key, slot) in [("tail", &mut p.tail), ("nose", &mut p.nose)] {
        if let Some(v) = arg(a, key) {
            if !v.is_object() {
                return Err(format!("{key}: expected an object"));
            }
            *slot = rail_spec(v, middle)?;
            p.zoned = true;
        }
    }
    let (start, end) = (length(a, "start")?, length(a, "end")?);
    if start.is_some() || end.is_some() {
        p.range = Some((start.unwrap_or(0.0), end.unwrap_or(doc.board.length())));
    }
    let mut b = doc.board.clone();
    let r = shaping::apply_rails(&mut b, &p)?;
    doc.edit(|d| *d = b);
    let mut out = report_json(&r, ctx);
    out["apex"] = json!(middle.apex);
    out["softness"] = json!(middle.softness);
    out["tuck"] = measure(ctx, middle.tuck);
    Ok(out)
}
