//! JSON command interface over a [`Document`]: used by the MCP server, the GUI automation bridge and tests.
//!
//! Lengths are centimetres when given as numbers; strings may carry units (`"6'2\""`, `"18 1/2\""`, `"47cm"`).
//! Results always report centimetres plus a formatted value in the session's display unit.

use std::collections::BTreeSet;
use std::path::PathBuf;

use surfcraft_core::units::FOOT;
use surfcraft_core::weight::{self, Layup};
use surfcraft_core::{Board, DVec2, Handle, LengthUnit, Mesh, MeshDetail, SurfaceModel};
use surfcraft_export::drawing::{Paper, fit_page, tile};
use surfcraft_export::{dxf, mesh_formats, pdf, svg, templates, woodwork};

/// Names accepted by the `export` command's `what`.
fn export_names() -> Vec<&'static str> {
    let mut v = vec![
        "outline",
        "outline_full",
        "spin",
        "profile",
        "sections",
        "spec_sheet",
        "model",
    ];
    v.extend(woodwork::TEMPLATES.iter().map(|(n, _)| *n));
    v
}
use serde_json::{Value, json};

use crate::document::{Document, write_with_backup};
use crate::edit::{self, CurveId, Selection};

/// Session state beyond the document.
#[derive(Clone, Debug)]
pub struct Context {
    pub unit: LengthUnit,
    pub fraction: u32,
    pub current_section: usize,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            unit: LengthUnit::FeetInchesFraction,
            fraction: 16,
            current_section: 1,
        }
    }
}

impl Context {
    fn fmt(&self, cm: f64) -> String {
        self.unit.format(cm, self.fraction)
    }
}

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

type R = Result<Value, String>;

fn len_schema(desc: &str) -> Value {
    json!({"type": ["number", "string"], "description": format!("{desc} (cm, or a string with units like \"6'2\\\"\" or \"47cm\")")})
}

fn curve_schema() -> Value {
    json!({
        "curve": {"type": "string", "enum": ["outline", "deck", "bottom", "section"], "description": "Which curve. Outline: x from tail, y half width. Deck/bottom: x from tail, y height. Section: x from stringer, y height."},
        "section_index": {"type": "integer", "description": "For curve=section: index into board.sections (real sections are 1..n-2). Defaults to the current section."}
    })
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

fn merge(a: Value, b: Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    if let Some(bm) = b.as_object() {
        for (k, v) in bm {
            m.insert(k.clone(), v.clone());
        }
    }
    Value::Object(m)
}

/// All commands with descriptions and JSON input schemas.
pub fn tool_specs() -> Vec<ToolSpec> {
    let handle = json!({"type": "string", "enum": ["point", "prev", "next"], "description": "point = the knot itself; prev/next = its tangent handles"});
    vec![
        ToolSpec {
            name: "board_summary",
            description: "Main dimensions and volume of the open board, its metadata and cross-section positions.",
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "sample_board",
            description: "Width, thickness, rocker and deck height at stations along the board (default every 15 cm plus 1 ft from each end).",
            input_schema: obj(
                json!({"stations": {"type": "array", "items": {"type": ["number", "string"]}, "description": "Positions from the tail"}}),
                &[],
            ),
        },
        ToolSpec {
            name: "get_curve",
            description: "Control points (knots) of a curve: position, tangent handles and whether the knot is smooth.",
            input_schema: obj(curve_schema(), &["curve"]),
        },
        ToolSpec {
            name: "set_knot",
            description: "Move a knot or one of its tangent handles to an exact position. Smooth knots keep their other handle aligned. Undoable.",
            input_schema: obj(
                merge(
                    curve_schema(),
                    json!({"index": {"type": "integer"}, "handle": handle, "x": len_schema("x"), "y": len_schema("y")}),
                ),
                &["curve", "index", "x", "y"],
            ),
        },
        ToolSpec {
            name: "move_knot",
            description: "Move a knot or handle by an offset. Undoable.",
            input_schema: obj(
                merge(
                    curve_schema(),
                    json!({"index": {"type": "integer"}, "handle": handle, "dx": len_schema("dx"), "dy": len_schema("dy")}),
                ),
                &["curve", "index"],
            ),
        },
        ToolSpec {
            name: "set_smooth",
            description: "Make a knot smooth (continuous tangents) or a corner.",
            input_schema: obj(
                merge(
                    curve_schema(),
                    json!({"index": {"type": "integer"}, "smooth": {"type": "boolean"}}),
                ),
                &["curve", "index", "smooth"],
            ),
        },
        ToolSpec {
            name: "smooth_curve",
            description: "Remove kinks: make knots smooth with both handles along one tangent, keeping handle \
                          lengths. With `indices`, smooth those knots; without, smooth every small kink (corners \
                          turning less than 30 degrees; deliberate corners such as a squash tail stay). End knots \
                          never change. Returns how many knots changed.",
            input_schema: obj(
                merge(
                    curve_schema(),
                    json!({"indices": {"type": "array", "items": {"type": "integer"}}}),
                ),
                &["curve"],
            ),
        },
        ToolSpec {
            name: "add_knot",
            description: "Insert a knot on the curve at the point nearest (x, y) without changing its shape.",
            input_schema: obj(
                merge(
                    curve_schema(),
                    json!({"x": len_schema("x"), "y": len_schema("y")}),
                ),
                &["curve", "x", "y"],
            ),
        },
        ToolSpec {
            name: "delete_knot",
            description: "Delete a knot (not the end knots).",
            input_schema: obj(
                merge(curve_schema(), json!({"index": {"type": "integer"}})),
                &["curve", "index"],
            ),
        },
        ToolSpec {
            name: "section_info",
            description: "Cross-section details: width, thickness, rail release angle and tuck radius.",
            input_schema: obj(json!({"section_index": {"type": "integer"}}), &[]),
        },
        ToolSpec {
            name: "add_section",
            description: "Add a cross-section at a position, interpolated from its neighbours.",
            input_schema: obj(
                json!({"position": len_schema("distance from tail")}),
                &["position"],
            ),
        },
        ToolSpec {
            name: "move_section",
            description: "Move a cross-section to a new position.",
            input_schema: obj(
                json!({"section_index": {"type": "integer"}, "position": len_schema("distance from tail")}),
                &["section_index", "position"],
            ),
        },
        ToolSpec {
            name: "remove_section",
            description: "Remove a cross-section (at least one remains).",
            input_schema: obj(
                json!({"section_index": {"type": "integer"}}),
                &["section_index"],
            ),
        },
        ToolSpec {
            name: "scale_board",
            description: "Scale to new overall length, width and/or thickness (omitted ones are kept). Optional target volume in litres adjusts thickness afterwards.",
            input_schema: obj(
                json!({"length": len_schema("length"), "width": len_schema("max width"), "thickness": len_schema("max thickness"), "volume_litres": {"type": "number"}, "scale_fins": {"type": "boolean", "default": true}}),
                &[],
            ),
        },
        ToolSpec {
            name: "new_board",
            description: "Start a new board from a template (shortboard, funboard, longboard), optionally at a target size and volume. Replaces the open board.",
            input_schema: obj(
                json!({"template": {"type": "string", "enum": ["shortboard", "funboard", "longboard"]}, "length": len_schema("length"), "width": len_schema("max width"), "thickness": len_schema("max thickness"), "volume_litres": {"type": "number"}}),
                &["template"],
            ),
        },
        ToolSpec {
            name: "open_board",
            description: "Open a .brd file (also .s3d/.srf/.stp when supported).",
            input_schema: obj(json!({"path": {"type": "string"}}), &["path"]),
        },
        ToolSpec {
            name: "save_board",
            description: "Save as .brd (keeps a .bak of an existing file). Without path, saves to the current file.",
            input_schema: obj(json!({"path": {"type": "string"}}), &[]),
        },
        ToolSpec {
            name: "set_info",
            description: "Set board metadata.",
            input_schema: obj(
                json!({"name": {"type": "string"}, "model": {"type": "string"}, "designer": {"type": "string"}, "author": {"type": "string"}, "surfer": {"type": "string"}, "fin_type": {"type": "string"}, "description": {"type": "string"}, "comments": {"type": "string"}}),
                &[],
            ),
        },
        ToolSpec {
            name: "set_fins",
            description: "Fin positions: side fin rear/front (x from tail, y from stringer), centre fin rear/front x, depths, splay (degrees).",
            input_schema: obj(
                json!({
                    "side_rear_x": len_schema("Side fin trailing edge, from tail"),
                    "side_rear_y": len_schema("Side fin trailing edge, out from stringer"),
                    "side_front_x": len_schema("Side fin leading edge, from tail"),
                    "side_front_y": len_schema("Side fin leading edge, out from stringer"),
                    "centre_rear_x": len_schema("Centre fin trailing edge, from tail"),
                    "centre_front_x": len_schema("Centre fin leading edge, from tail"),
                    "centre_depth": len_schema("Centre fin depth"),
                    "side_depth": len_schema("Side fin depth"),
                    "splay_degrees": {"type": "number", "description": "Side fin splay, degrees (-20 to 20)"}
                }),
                &[],
            ),
        },
        ToolSpec {
            name: "estimate_weight",
            description: "Finished weight estimate (kg) for a foam/stringer/glass layup. Optional overrides: foam_density, stringer_density (kg/L), deck_glass, bottom_glass (kg/m²), resin_ratio.",
            input_schema: obj(
                json!({"foam_density": {"type": "number"}, "stringer_density": {"type": "number"}, "deck_glass": {"type": "number"}, "bottom_glass": {"type": "number"}, "resin_ratio": {"type": "number"}}),
                &[],
            ),
        },
        ToolSpec {
            name: "export",
            description: "Write a template, spec sheet or 3D model. PDF is tiled 1:1 over pages (spec sheet: one page); SVG/DXF are one 1:1 sheet; STL/OBJ in mm. Also woodworking templates for sandwich, hollow wood (hws_*) and chambered construction.",
            input_schema: obj(
                json!({"what": {"type": "string", "enum": export_names()}, "format": {"type": "string", "enum": ["pdf", "svg", "dxf", "stl", "obj", "step"], "description": "model: stl, obj or step (STEP holds the exact Bezier curves); drawings: pdf, svg or dxf"}, "path": {"type": "string"}, "paper": {"type": "string", "enum": ["A4", "A3", "Letter", "Tabloid"]}}),
                &["what", "format", "path"],
            ),
        },
        ToolSpec {
            name: "generate_toolpath",
            description: "CNC program (LinuxCNC G-code) for the deck or bottom, or a hot-wire profile/outline. Returns stats (passes, cut length, estimated time); writes the G-code when path is given. Optional overrides of the machine defaults.",
            input_schema: obj(
                json!({
                    "job": {"type": "string", "enum": ["deck", "bottom", "hotwire_profile", "hotwire_outline"]},
                    "path": {"type": "string", "description": "Where to write the .nc file"},
                    "deck_cuts": {"type": "integer"}, "deck_rail_cuts": {"type": "integer"},
                    "bottom_cuts": {"type": "integer"}, "bottom_rail_cuts": {"type": "integer"},
                    "lengthwise_resolution": len_schema("step along the board"),
                    "cutter_diameter": len_schema("bullnose cutter diameter"),
                    "corner_radius": len_schema("bullnose corner radius"),
                    "cutting_speed_mm_s": {"type": "number"},
                    "holding": {"type": "string", "enum": ["supports", "end_clamps"]},
                }),
                &["job"],
            ),
        },
        ToolSpec {
            name: "set_surface_model",
            description: "How cross-sections are blended: sblend (default, smooth) or control_point (knot matching, as BoardCAD's original default).",
            input_schema: obj(
                json!({"model": {"type": "string", "enum": ["sblend", "control_point"]}}),
                &["model"],
            ),
        },
        ToolSpec {
            name: "flip_board",
            description: "Swap nose and tail.",
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "undo",
            description: "Undo the last change.",
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "redo",
            description: "Redo the last undone change.",
            input_schema: obj(json!({}), &[]),
        },
    ]
}

// --------------------------------------------------------------------------------------------- helpers

fn arg<'a>(a: &'a Value, k: &str) -> Option<&'a Value> {
    a.get(k).filter(|v| !v.is_null())
}

/// Largest coordinate accepted anywhere (cm). A 10 m board is already absurd; this keeps arithmetic and
/// length-proportional sampling bounded.
const MAX_COORD: f64 = 1000.0;

fn length(a: &Value, k: &str) -> Result<Option<f64>, String> {
    let v = match arg(a, k) {
        None => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => Some(
            LengthUnit::Centimetres
                .parse(s)
                .ok_or_else(|| format!("{k}: cannot read length {s:?}"))?,
        ),
        Some(v) => return Err(format!("{k}: expected a number or string, got {v}")),
    };
    match v {
        Some(v) if v.is_finite() && v.abs() <= MAX_COORD => Ok(Some(v)),
        _ => Err(format!(
            "{k}: must be a finite length within ±{MAX_COORD} cm"
        )),
    }
}

/// Optional plain number; an argument of the wrong type is an error, not silently ignored.
fn number(a: &Value, k: &str) -> Result<Option<f64>, String> {
    match arg(a, k) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Some)
            .ok_or_else(|| format!("{k}: expected a number, got {v}")),
    }
}

fn need_length(a: &Value, k: &str) -> Result<f64, String> {
    length(a, k)?.ok_or_else(|| format!("missing {k}"))
}

fn index(a: &Value, k: &str) -> Result<Option<usize>, String> {
    match arg(a, k) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .map(|n| Some(n as usize))
            .ok_or_else(|| format!("{k}: expected a non-negative integer")),
    }
}

fn string<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    arg(a, k).and_then(Value::as_str)
}

fn curve(a: &Value, board: &Board, ctx: &Context) -> Result<CurveId, String> {
    let id = match string(a, "curve").ok_or("missing curve")? {
        "outline" => CurveId::Outline,
        "deck" => CurveId::Deck,
        "bottom" => CurveId::Bottom,
        "section" => {
            let i = index(a, "section_index")?.unwrap_or(ctx.current_section);
            if !board.real_sections().contains(&i) {
                return Err(format!(
                    "section_index {i} is not a real section ({:?})",
                    board.real_sections()
                ));
            }
            CurveId::Section(i)
        }
        other => return Err(format!("unknown curve {other:?}")),
    };
    Ok(id)
}

fn handle(a: &Value) -> Result<Handle, String> {
    match string(a, "handle").unwrap_or("point") {
        "point" => Ok(Handle::Point),
        "prev" => Ok(Handle::Prev),
        "next" => Ok(Handle::Next),
        other => Err(format!("unknown handle {other:?}")),
    }
}

fn knot_index(board: &Board, id: CurveId, i: usize) -> Result<usize, String> {
    let n = id.spline(board).map_or(0, |s| s.len());
    if i < n {
        Ok(i)
    } else {
        Err(format!("index {i} out of range (curve has {n} knots)"))
    }
}

fn measure(ctx: &Context, cm: f64) -> Value {
    json!({"cm": (cm * 1000.0).round() / 1000.0, "text": ctx.fmt(cm)})
}

fn pt(p: DVec2) -> Value {
    json!([
        (p.x * 10000.0).round() / 10000.0,
        (p.y * 10000.0).round() / 10000.0
    ])
}

pub fn summary(board: &Board, ctx: &Context) -> Value {
    let l = board.length();
    let (t, tpos) = board.max_thickness_and_pos();
    let volume = board.volume();
    let i = &board.info;
    json!({
        "name": i.name, "model": i.model, "designer": i.designer, "surfer": i.surfer, "fin_type": i.fin_type,
        "length": measure(ctx, l),
        "length_over_curve": measure(ctx, board.length_over_curve()),
        "max_width": measure(ctx, board.max_width()),
        "max_width_from_tail": measure(ctx, board.max_width_pos()),
        "max_thickness": measure(ctx, t),
        "max_thickness_from_tail": measure(ctx, tpos),
        "nose_width_1ft": measure(ctx, board.width_at(l - FOOT)),
        "tail_width_1ft": measure(ctx, board.width_at(FOOT)),
        "nose_rocker": measure(ctx, board.rocker_at(l)),
        "tail_rocker": measure(ctx, board.rocker_at(0.0)),
        "volume_litres": (volume / 10.0).round() / 100.0,
        "planform_area_cm2": board.area().round(),
        "center_of_mass_from_tail": measure(ctx, board.center_of_mass()),
        "surface_model": match board.model { SurfaceModel::SBlend => "sblend", SurfaceModel::ControlPoint => "control_point" },
        "sections": board.real_sections().map(|i| json!({"index": i, "position": measure(ctx, board.sections[i].position)})).collect::<Vec<_>>(),
        "knot_counts": {"outline": board.outline.len(), "deck": board.deck.len(), "bottom": board.bottom.len()},
    })
}

fn template(name: &str) -> Result<Board, String> {
    let idx = match name {
        "shortboard" => 0,
        "funboard" => 1,
        "longboard" => 2,
        other => return Err(format!("unknown template {other:?}")),
    };
    Ok(surfcraft_io::templates::TEMPLATES[idx].board())
}

fn scale_to(b: &mut Board, a: &Value) -> Result<(), String> {
    let (l0, w0) = (b.length(), b.max_width());
    let l = length(a, "length")?.unwrap_or(l0);
    let w = length(a, "width")?.unwrap_or(w0);
    let t = length(a, "thickness")?.unwrap_or_else(|| b.max_thickness());
    if l <= 10.0 || w <= 1.0 || t <= 0.5 {
        return Err("dimensions too small".into());
    }
    b.scale(l, w, t);
    if arg(a, "scale_fins")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        b.scale_fins(l / l0, w / w0);
    }
    if let Some(v) = arg(a, "volume_litres") {
        let v = match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().replace(',', ".").parse::<f64>().ok(),
            _ => None,
        }
        .filter(|v| v.is_finite())
        .ok_or("volume_litres must be a number of litres")?;
        if !(1.0..=500.0).contains(&v) {
            return Err("volume_litres out of range".into());
        }
        solve_volume(b, v);
    }
    Ok(())
}

/// Adjust thickness (keeping length and width) to reach `litres`.
pub fn solve_volume(board: &mut Board, litres: f64) -> f64 {
    let target = litres * 1000.0;
    let (l, w) = (board.length(), board.max_width());
    let mut t = board.max_thickness();
    let mut v = board.volume();
    for _ in 0..6 {
        if v <= 0.0 || (v - target).abs() / target < 0.001 {
            break;
        }
        t *= target / v;
        board.scale(l, w, t);
        v = board.volume();
    }
    v / 1000.0
}

/// Render an export to bytes.
pub fn render_export(
    board: &Board,
    what: &str,
    format: &str,
    paper: Paper,
    ctx: &Context,
    title: &str,
) -> Result<Vec<u8>, String> {
    let fmt = |cm: f64| ctx.fmt(cm);
    if what == "model" && format == "step" {
        return Ok(surfcraft_io::step::write(board).into_bytes());
    }
    if what == "model" {
        let mesh = Mesh::of_board(
            board,
            MeshDetail {
                stations: 160,
                half_section: 40,
            },
        );
        return match format {
            "stl" => Ok(mesh_formats::stl(&mesh, title)),
            "obj" => Ok(mesh_formats::obj(&mesh, title).into_bytes()),
            f => Err(format!("model exports are stl, obj or step, not {f}")),
        };
    }
    let d = match what {
        "outline" => templates::outline(board, templates::OutlineOptions::default(), &fmt),
        "outline_full" => templates::outline(
            board,
            templates::OutlineOptions {
                full: true,
                ..Default::default()
            },
            &fmt,
        ),
        "spin" => templates::spin(board, &fmt),
        "profile" => templates::profile(board, &fmt),
        "sections" => templates::sections(board, &fmt),
        "spec_sheet" => {
            let p = paper.size();
            templates::spec_sheet(board, board.volume(), DVec2::new(p.y, p.x), &fmt)
        }
        w => woodwork::by_name(board, w, &fmt).ok_or_else(|| format!("unknown export {w:?}"))?,
    };
    match format {
        "pdf" => {
            let pages = if what == "spec_sheet" {
                vec![fit_page(&d, DVec2::new(d.width, d.height))]
            } else {
                tile(&d, paper, 10.0, 10.0, &format!("{title} {what}"))
            };
            Ok(pdf::write(&d, &pages, title))
        }
        "svg" => Ok(svg::write(&d).into_bytes()),
        "dxf" => Ok(dxf::write(&d).into_bytes()),
        f => Err(format!("drawings export as pdf, svg or dxf, not {f}")),
    }
}

/// Read any supported board file; the flag is true for native `.brd` (safe to save back in place).
pub fn read_board(path: &std::path::Path) -> Result<(Board, bool), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let native = ext.is_empty() || ext == "brd";
    surfcraft_io::read_any(if native { "brd" } else { &ext }, &bytes)
        .map(|b| (b, native))
        .map_err(|e| format!("{}: {e}", path.display()))
}

// --------------------------------------------------------------------------------------------- execute

/// Run one command. Edits are applied as single undo steps.
pub fn execute(doc: &mut Document, ctx: &mut Context, name: &str, a: &Value) -> R {
    let a = if a.is_null() { &json!({}) } else { a };
    match name {
        "board_summary" => Ok(summary(&doc.board, ctx)),
        "sample_board" => {
            let b = &doc.board;
            let l = b.length();
            let stations = match arg(a, "stations") {
                None => None,
                Some(Value::Array(list)) if list.len() <= 1000 => Some(list),
                Some(_) => return Err("stations: expected an array of up to 1000 positions".into()),
            };
            let xs: Vec<f64> = match stations {
                Some(list) => list
                    .iter()
                    .map(|v| {
                        let x = match v {
                            Value::Number(n) => n.as_f64(),
                            Value::String(s) => LengthUnit::Centimetres.parse(s),
                            _ => None,
                        };
                        x.filter(|x| (0.0..=l).contains(x)).ok_or_else(|| {
                            format!(
                                "bad station {v}: must be a position on the board (0 to {l:.1} cm)"
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
                None => {
                    let mut v: Vec<f64> = (0..)
                        .map(|i| i as f64 * 15.0)
                        .take_while(|&x| x <= l)
                        .collect();
                    v.extend([FOOT, l - FOOT, l]);
                    v.sort_by(f64::total_cmp);
                    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                    v
                }
            };
            Ok(json!(
                xs.iter()
                    .map(|&x| json!({
                        "from_tail": measure(ctx, x),
                        "width": measure(ctx, b.width_at(x)),
                        "thickness": measure(ctx, b.thickness_at(x)),
                        "rocker": measure(ctx, b.rocker_at(x)),
                        "deck": measure(ctx, b.deck_at(x)),
                    }))
                    .collect::<Vec<_>>()
            ))
        }
        "get_curve" => {
            let id = curve(a, &doc.board, ctx)?;
            let s = id.spline(&doc.board).ok_or("no such curve")?;
            Ok(json!({
                "curve": id.label(),
                "length": measure(ctx, s.length()),
                "knots": s.knots().iter().enumerate().map(|(i, k)| json!({
                    "index": i, "point": pt(k.point()), "prev": pt(k.prev()), "next": pt(k.next()),
                    "smooth": k.continuous, "end_knot": i == 0 || i + 1 == s.len(),
                })).collect::<Vec<_>>(),
            }))
        }
        "set_knot" | "move_knot" => {
            let id = curve(a, &doc.board, ctx)?;
            let i = knot_index(&doc.board, id, index(a, "index")?.ok_or("missing index")?)?;
            let which = handle(a)?;
            let current = id.spline(&doc.board).unwrap().knots()[i].get(which);
            let target = if name == "set_knot" {
                DVec2::new(need_length(a, "x")?, need_length(a, "y")?)
            } else {
                current
                    + DVec2::new(
                        length(a, "dx")?.unwrap_or(0.0),
                        length(a, "dy")?.unwrap_or(0.0),
                    )
            };
            doc.edit(|b| edit::set_handle(b, id, i, which, target));
            let k = &id.spline(&doc.board).unwrap().knots()[i];
            Ok(
                json!({"point": pt(k.point()), "prev": pt(k.prev()), "next": pt(k.next()), "note": if k.get(which) != target { "constrained by end-point or tangent locks" } else { "" }}),
            )
        }
        "set_smooth" => {
            let id = curve(a, &doc.board, ctx)?;
            let i = knot_index(&doc.board, id, index(a, "index")?.ok_or("missing index")?)?;
            let smooth = arg(a, "smooth")
                .and_then(Value::as_bool)
                .ok_or("missing smooth")?;
            doc.edit(|b| edit::set_continuous(b, id, i, smooth));
            Ok(json!({"smooth": smooth}))
        }
        "smooth_curve" => {
            let id = curve(a, &doc.board, ctx)?;
            let picked = match arg(a, "indices") {
                None | Some(Value::Null) => None,
                Some(Value::Array(items)) => {
                    let mut set = BTreeSet::new();
                    for v in items {
                        let i = v.as_u64().ok_or("indices must be non-negative integers")?;
                        set.insert(knot_index(&doc.board, id, i as usize)?);
                    }
                    Some(set)
                }
                Some(_) => return Err("indices must be an array of integers".into()),
            };
            // Only record an undo step when something changes.
            let mut probe = doc.board.clone();
            if edit::smooth_knots(&mut probe, id, picked.as_ref()) == 0 {
                return Ok(json!({"smoothed": 0}));
            }
            let mut n = 0;
            doc.edit(|b| n = edit::smooth_knots(b, id, picked.as_ref()));
            Ok(json!({"smoothed": n}))
        }
        "add_knot" => {
            let id = curve(a, &doc.board, ctx)?;
            let p = DVec2::new(need_length(a, "x")?, need_length(a, "y")?);
            let mut out = None;
            doc.edit(|b| out = edit::add_knot(b, id, p));
            out.map(|i| json!({"index": i}))
                .ok_or_else(|| "could not add knot".into())
        }
        "delete_knot" => {
            let id = curve(a, &doc.board, ctx)?;
            let i = index(a, "index")?.ok_or("missing index")?;
            let mut ok = false;
            doc.edit(|b| ok = edit::delete_knots(b, id, &BTreeSet::from([i])));
            if ok {
                Ok(json!({"deleted": i}))
            } else {
                Err("cannot delete that knot (end knot or too few knots)".into())
            }
        }
        "section_info" => {
            let i = index(a, "section_index")?.unwrap_or(ctx.current_section);
            let b = &doc.board;
            if !b.real_sections().contains(&i) {
                return Err(format!(
                    "section_index {i} is not a real section ({:?})",
                    b.real_sections()
                ));
            }
            let s = &b.sections[i];
            Ok(json!({
                "index": i, "position": measure(ctx, s.position), "width": measure(ctx, s.width()),
                "thickness": measure(ctx, s.center_thickness()),
                "release_angle_degrees": (s.release_angle().to_degrees() * 10.0).round() / 10.0,
                "tuck_radius": measure(ctx, s.tuck_radius()), "knots": s.spline.len(),
            }))
        }
        "add_section" => {
            let x = need_length(a, "position")?;
            let mut out = None;
            doc.edit(|b| out = edit::add_section(b, x));
            let i = out
                .ok_or("cannot add a section there (too close to an end or to another section)")?;
            ctx.current_section = i;
            Ok(json!({"section_index": i}))
        }
        "move_section" => {
            let i = index(a, "section_index")?.ok_or("missing section_index")?;
            if !doc.board.real_sections().contains(&i) {
                return Err(format!("section_index {i} is not a real section"));
            }
            let x = need_length(a, "position")?;
            if !edit::section_position_free(&doc.board, i, x) {
                return Err(format!(
                    "another cross-section already sits at {}",
                    ctx.fmt(x)
                ));
            }
            let mut new = i;
            doc.edit(|b| new = edit::move_section(b, i, x));
            ctx.current_section = new;
            Ok(json!({"section_index": new}))
        }
        "remove_section" => {
            let i = index(a, "section_index")?.ok_or("missing section_index")?;
            let mut ok = false;
            doc.edit(|b| ok = edit::remove_section(b, i));
            if ok {
                let r = doc.board.real_sections();
                ctx.current_section = ctx
                    .current_section
                    .clamp(r.start, r.end.saturating_sub(1).max(r.start));
                Ok(json!({"removed": i, "current_section": ctx.current_section}))
            } else {
                Err("cannot remove that section".into())
            }
        }
        "scale_board" => {
            let mut b = doc.board.clone();
            scale_to(&mut b, a)?;
            doc.edit(|d| *d = b);
            Ok(summary(&doc.board, ctx))
        }
        "new_board" | "open_board"
            if doc.is_dirty()
                && !arg(a, "discard_unsaved")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
        {
            Err(
                "the open board has unsaved changes: save it first or pass discard_unsaved: true"
                    .into(),
            )
        }
        "new_board" => {
            let mut b = template(string(a, "template").ok_or("missing template")?)?;
            b.model = doc.board.model;
            if ["length", "width", "thickness", "volume_litres"]
                .iter()
                .any(|k| arg(a, k).is_some())
            {
                scale_to(&mut b, a)?;
            }
            b.info.name.clear();
            *doc = Document::new(b, None);
            ctx.current_section = doc
                .board
                .nearest_section_index(doc.board.length() / 2.0)
                .unwrap_or(1);
            Ok(summary(&doc.board, ctx))
        }
        "open_board" => {
            let path = PathBuf::from(string(a, "path").ok_or("missing path")?);
            let (mut b, native) = read_board(&path)?;
            b.model = doc.board.model;
            if !native && b.info.name.is_empty() {
                b.info.name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            // Imported formats open untitled so save_board never writes .brd text over them.
            *doc = Document::new(b, native.then_some(path));
            ctx.current_section = doc
                .board
                .nearest_section_index(doc.board.length() / 2.0)
                .unwrap_or(1);
            Ok(summary(&doc.board, ctx))
        }
        "save_board" => {
            let path = match string(a, "path") {
                Some(p) => PathBuf::from(p),
                None => doc
                    .path
                    .clone()
                    .ok_or("no path: the board has never been saved")?,
            };
            doc.save_to(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(json!({"saved": path.display().to_string()}))
        }
        "set_info" => {
            let mut info = doc.board.info.clone();
            for (k, f) in [
                ("name", &mut info.name),
                ("model", &mut info.model),
                ("designer", &mut info.designer),
                ("author", &mut info.author),
                ("surfer", &mut info.surfer),
                ("fin_type", &mut info.fin_type),
                ("description", &mut info.description),
                ("comments", &mut info.comments),
            ] {
                match arg(a, k) {
                    None => {}
                    Some(Value::String(s)) if s.len() <= 10_000 => *f = s.clone(),
                    Some(_) => return Err(format!("{k}: expected text (up to 10 000 characters)")),
                }
            }
            doc.edit(|b| b.info = info);
            Ok(json!({"ok": true}))
        }
        "set_fins" => {
            let mut fins = doc.board.fins;
            for (i, k) in [
                "side_rear_x",
                "side_rear_y",
                "side_front_x",
                "side_front_y",
                "centre_rear_x",
                "centre_front_x",
                "centre_depth",
                "side_depth",
            ]
            .iter()
            .enumerate()
            {
                if let Some(v) = length(a, k)? {
                    let l = doc.board.length();
                    // Positions along the board must be on it; offsets and depths within a board width.
                    let ok = if k.ends_with("_x") {
                        (0.0..=l).contains(&v)
                    } else {
                        v.abs() <= 100.0
                    };
                    if !ok {
                        return Err(format!("{k}: {v} cm is outside the board"));
                    }
                    fins[i] = v;
                }
            }
            if let Some(v) = number(a, "splay_degrees")? {
                if v.abs() > 20.0 {
                    return Err("splay_degrees must be between -20 and 20".into());
                }
                fins[8] = v;
            }
            doc.edit(|b| b.fins = fins);
            Ok(json!({"fins": fins}))
        }
        "estimate_weight" => {
            let mut l = Layup::defaults_for(&doc.board);
            for (k, f) in [
                ("foam_density", &mut l.foam_density),
                ("stringer_density", &mut l.stringer_density),
                ("deck_glass", &mut l.deck_glass),
                ("bottom_glass", &mut l.bottom_glass),
                ("resin_ratio", &mut l.resin_ratio),
            ] {
                if let Some(v) = arg(a, k) {
                    let v = v
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.0 && *v < 100.0);
                    *f = v.ok_or_else(|| format!("{k} must be a non-negative number"))?;
                }
            }
            let w = weight::estimate(&doc.board, doc.board.volume(), &l);
            let r = |v: f64| (v * 1000.0).round() / 1000.0;
            Ok(json!({
                "total_kg": r(w.total), "foam_kg": r(w.foam), "stringer_kg": r(w.stringer), "glass_kg": r(w.glass),
                "resin_kg": r(w.resin), "hotcoat_kg": r(w.hotcoat), "plugs_and_fins_kg": r(w.plugs_and_fins),
                "deck_area_cm2": w.deck_glass_area.round(), "bottom_area_cm2": w.bottom_glass_area.round(),
            }))
        }
        "export" => {
            let what = string(a, "what").ok_or("missing what")?;
            let format = string(a, "format").ok_or("missing format")?;
            let path = PathBuf::from(string(a, "path").ok_or("missing path")?);
            let paper = match string(a, "paper")
                .unwrap_or("A4")
                .to_ascii_lowercase()
                .as_str()
            {
                "a4" => Paper::A4,
                "a3" => Paper::A3,
                "letter" => Paper::Letter,
                "tabloid" => Paper::Tabloid,
                p => {
                    return Err(format!(
                        "unknown paper {p:?}: use A4, A3, Letter or Tabloid"
                    ));
                }
            };
            let title = doc
                .title()
                .trim_end_matches(" *")
                .trim_end_matches(".brd")
                .to_string();
            let bytes = render_export(&doc.board, what, format, paper, ctx, &title)?;
            write_with_backup(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(json!({"written": path.display().to_string(), "bytes": bytes.len()}))
        }
        "generate_toolpath" => {
            use surfcraft_cam::{
                BoardSide, Cutter, HoldingSystem, HotwireConfig, HotwireKind, MachineConfig,
            };
            let mut cfg = MachineConfig::default();
            for (k, f) in [
                ("deck_cuts", &mut cfg.deck_cuts),
                ("deck_rail_cuts", &mut cfg.deck_rail_cuts),
                ("bottom_cuts", &mut cfg.bottom_cuts),
                ("bottom_rail_cuts", &mut cfg.bottom_rail_cuts),
            ] {
                if let Some(v) = index(a, k)? {
                    *f = v.min(500);
                }
            }
            if let Some(v) = length(a, "lengthwise_resolution")? {
                cfg.lengthwise_resolution = v.clamp(0.05, 20.0);
            }
            if let Cutter::Bullnose(b) = &mut cfg.cutter {
                if let Some(v) = length(a, "cutter_diameter")? {
                    b.diameter = v.max(0.1);
                }
                if let Some(v) = length(a, "corner_radius")? {
                    b.corner_radius = v.max(0.0);
                }
                // A corner cannot be larger than the cutter radius (also after only the diameter changed).
                b.corner_radius = b.corner_radius.min(b.diameter / 2.0);
            }
            if let Some(v) = number(a, "cutting_speed_mm_s")? {
                cfg.cutting_speed = v.clamp(1.0, 2000.0);
            }
            match string(a, "holding") {
                Some("end_clamps") => cfg.holding = HoldingSystem::EndClamps(Default::default()),
                Some("supports") | None => {}
                Some(o) => return Err(format!("unknown holding {o:?}")),
            }
            let hot = HotwireConfig::default();
            let program = match string(a, "job").ok_or("missing job")? {
                "deck" => surfcraft_cam::generate(&doc.board, &cfg, BoardSide::Deck),
                "bottom" => surfcraft_cam::generate(&doc.board, &cfg, BoardSide::Bottom),
                "hotwire_profile" => {
                    surfcraft_cam::generate_hotwire(&doc.board, &hot, HotwireKind::Profile)
                }
                "hotwire_outline" => {
                    surfcraft_cam::generate_hotwire(&doc.board, &hot, HotwireKind::Outline)
                }
                o => return Err(format!("unknown job {o:?}")),
            }
            .map_err(|e| e.to_string())?;
            let s = program.stats();
            let mut out = json!({
                "passes": s.passes, "moves": s.points,
                "cut_length_m": (s.cut_length / 100.0).round() / 10.0,
                "estimated_minutes": (s.estimated_time / 6.0).round() / 10.0,
                "passes_by_kind": s.passes_by_kind.iter().map(|(k, n)| json!({"kind": format!("{k:?}"), "count": n})).collect::<Vec<_>>(),
            });
            if let Some(p) = string(a, "path") {
                let path = PathBuf::from(p);
                write_with_backup(&path, program.to_gcode().as_bytes())
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                out["written"] = json!(path.display().to_string());
            }
            Ok(out)
        }
        "set_surface_model" => {
            let m = match string(a, "model").ok_or("missing model")? {
                "sblend" => SurfaceModel::SBlend,
                "control_point" => SurfaceModel::ControlPoint,
                o => return Err(format!("unknown model {o:?}")),
            };
            doc.edit(|b| b.model = m);
            Ok(json!({"volume_litres": (doc.board.volume() / 10.0).round() / 100.0}))
        }
        "flip_board" => {
            doc.edit(Board::flip);
            Ok(json!({"ok": true}))
        }
        "undo" => {
            let can = doc.can_undo();
            doc.undo();
            Ok(json!({"undone": can}))
        }
        "redo" => {
            let can = doc.can_redo();
            doc.redo();
            Ok(json!({"redone": can}))
        }
        other => Err(format!("unknown command {other:?}")),
    }
}

/// Selection helper for callers that want to highlight what a command touched.
pub fn selection_for(curve: CurveId, index: usize) -> Selection {
    Selection::single(curve, index, Handle::Point)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Document, Context) {
        (
            Document::new(surfcraft_io::templates::TEMPLATES[0].board(), None),
            Context::default(),
        )
    }

    fn run(doc: &mut Document, ctx: &mut Context, name: &str, a: Value) -> Value {
        execute(doc, ctx, name, &a).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// Volume of the built-in shortboard in litres, as computed by the reference program (its golden file).
    fn reference_volume_litres() -> f64 {
        let golden: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/shortboard.golden.json"
        ))
        .unwrap();
        golden["slinear"]["volume"].as_f64().unwrap() / 1000.0
    }

    #[test]
    fn every_command_has_a_schema_and_runs() {
        let names: Vec<_> = tool_specs().iter().map(|t| t.name).collect();
        assert!(names.len() >= 20);
        let (mut doc, mut ctx) = session();
        let s = run(&mut doc, &mut ctx, "board_summary", json!({}));
        assert_eq!(s["length"]["text"], "6' 0\"");
        assert!((s["volume_litres"].as_f64().unwrap() - reference_volume_litres()).abs() < 0.01);
        run(
            &mut doc,
            &mut ctx,
            "sample_board",
            json!({"stations": [30.48, "3'"]}),
        );
        let c = run(&mut doc, &mut ctx, "get_curve", json!({"curve": "outline"}));
        assert_eq!(
            c["knots"].as_array().unwrap().len(),
            doc.board.outline.len()
        );
        // Move the wide point (the outline knot furthest from the stringer) 1 cm outward: 2 cm wider.
        let width = doc.board.max_width();
        let wide = (0..doc.board.outline.len())
            .max_by(|&a, &b| {
                let y = |i: usize| doc.board.outline.knots()[i].point().y;
                y(a).total_cmp(&y(b))
            })
            .unwrap();
        run(
            &mut doc,
            &mut ctx,
            "move_knot",
            json!({"curve": "outline", "index": wide, "dy": 1.0}),
        );
        assert!(doc.board.max_width() > width + 1.9);
        run(&mut doc, &mut ctx, "undo", json!({}));
        assert_eq!(doc.board.max_width(), width);
        run(&mut doc, &mut ctx, "redo", json!({}));
        run(
            &mut doc,
            &mut ctx,
            "set_smooth",
            json!({"curve": "deck", "index": 1, "smooth": false}),
        );
        let k = run(
            &mut doc,
            &mut ctx,
            "add_knot",
            json!({"curve": "deck", "x": 100, "y": 7}),
        );
        run(
            &mut doc,
            &mut ctx,
            "delete_knot",
            json!({"curve": "deck", "index": k["index"]}),
        );
        let s = run(&mut doc, &mut ctx, "add_section", json!({"position": "4'"}));
        let i = s["section_index"].as_u64().unwrap();
        run(
            &mut doc,
            &mut ctx,
            "section_info",
            json!({"section_index": i}),
        );
        run(
            &mut doc,
            &mut ctx,
            "set_knot",
            json!({"curve": "section", "index": 1, "x": 20, "y": 1}),
        );
        let m = run(
            &mut doc,
            &mut ctx,
            "move_section",
            json!({"section_index": i, "position": 125}),
        );
        run(
            &mut doc,
            &mut ctx,
            "remove_section",
            json!({"section_index": m["section_index"]}),
        );
        let s = run(
            &mut doc,
            &mut ctx,
            "scale_board",
            json!({"length": "5'10\"", "volume_litres": 26}),
        );
        assert!((s["volume_litres"].as_f64().unwrap() - 26.0).abs() < 0.1);
        run(
            &mut doc,
            &mut ctx,
            "set_info",
            json!({"name": "Test", "designer": "Me"}),
        );
        run(
            &mut doc,
            &mut ctx,
            "set_fins",
            json!({"side_rear_x": 25, "splay_degrees": 4}),
        );
        let w = run(&mut doc, &mut ctx, "estimate_weight", json!({}));
        assert!(w["total_kg"].as_f64().unwrap() > 1.5);
        run(
            &mut doc,
            &mut ctx,
            "set_surface_model",
            json!({"model": "control_point"}),
        );
        run(&mut doc, &mut ctx, "flip_board", json!({}));
        let exported_length = doc.board.length();
        let dir = std::env::temp_dir().join("surfcraft-session-test");
        std::fs::create_dir_all(&dir).unwrap();
        for (what, format) in [
            ("outline", "pdf"),
            ("spec_sheet", "pdf"),
            ("profile", "svg"),
            ("sections", "dxf"),
            ("model", "stl"),
            ("model", "obj"),
            ("spin", "pdf"),
            ("outline_full", "dxf"),
            ("model", "step"),
        ] {
            let p = dir.join(format!("{what}.{format}"));
            run(
                &mut doc,
                &mut ctx,
                "export",
                json!({"what": what, "format": format, "path": p}),
            );
            assert!(std::fs::metadata(&p).unwrap().len() > 100);
        }
        let p = dir.join("board.brd");
        run(&mut doc, &mut ctx, "save_board", json!({"path": p}));
        run(
            &mut doc,
            &mut ctx,
            "new_board",
            json!({"template": "longboard", "length": "9'2\""}),
        );
        let s = run(&mut doc, &mut ctx, "open_board", json!({"path": p}));
        assert_eq!(s["name"], "Test");
        // STEP round trip through open_board: opens untitled, same length.
        let s = run(
            &mut doc,
            &mut ctx,
            "open_board",
            json!({"path": dir.join("model.step")}),
        );
        assert!(doc.path.is_none());
        assert!((s["length"]["cm"].as_f64().unwrap() - exported_length).abs() < 0.01);
    }

    #[test]
    fn generates_every_toolpath_job() {
        let (mut doc, mut ctx) = session();
        let dir = std::env::temp_dir().join("surfcraft-session-cam");
        std::fs::create_dir_all(&dir).unwrap();
        for job in ["deck", "bottom", "hotwire_profile", "hotwire_outline"] {
            let p = dir.join(format!("{job}.nc"));
            let r = run(
                &mut doc,
                &mut ctx,
                "generate_toolpath",
                json!({"job": job, "path": p, "deck_cuts": 10, "holding": "end_clamps"}),
            );
            assert!(r["passes"].as_u64().unwrap() > 0, "{job}: {r}");
            assert!(std::fs::read_to_string(&p).unwrap().contains("M2"), "{job}");
        }
        assert!(
            execute(
                &mut doc,
                &mut ctx,
                "generate_toolpath",
                &json!({"job": "rails"})
            )
            .is_err()
        );
    }

    #[test]
    fn exports_every_woodwork_template() {
        let (mut doc, mut ctx) = session();
        let dir = std::env::temp_dir().join("surfcraft-session-wood");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, _) in woodwork::TEMPLATES {
            let p = dir.join(format!("{name}.svg"));
            run(
                &mut doc,
                &mut ctx,
                "export",
                json!({"what": name, "format": "svg", "path": p}),
            );
            let svg = std::fs::read_to_string(&p).unwrap();
            assert!(svg.matches("<polyline").count() > 0, "{name} is empty");
        }
    }

    #[test]
    fn errors_are_reported_not_panics() {
        let (mut doc, mut ctx) = session();
        for (name, a) in [
            ("get_curve", json!({"curve": "rail"})),
            (
                "set_knot",
                json!({"curve": "outline", "index": 99, "x": 1, "y": 1}),
            ),
            ("delete_knot", json!({"curve": "outline", "index": 0})),
            ("section_info", json!({"section_index": 0})),
            ("scale_board", json!({"length": "banana"})),
            (
                "export",
                json!({"what": "model", "format": "pdf", "path": "/tmp/x"}),
            ),
            ("open_board", json!({"path": "/nonexistent.brd"})),
            ("nope", json!({})),
        ] {
            assert!(
                execute(&mut doc, &mut ctx, name, &a).is_err(),
                "{name} should fail"
            );
        }
    }
}
