//! Asymmetric boards in the command interface: the `set_asymmetric` command and the per-rail figures added to
//! `board_summary` (see [`surfcraft_core::asym`]).

use serde_json::{Value, json};
use surfcraft_core::{Board, Rail};

use crate::commands::{Context, R, ToolSpec, arg, measure, obj, string};
use crate::document::Document;

pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "set_asymmetric",
        description: "Give the board a separate outline for each rail (on: the left rail starts as a copy of the outline; then edit curve \"outline\" for the rider's right rail and \"outline_left\" for the rider's left rail, standing on the deck facing the nose) or make it symmetric again (off: keep = \"left\" or \"right\", the rail whose outline both sides take; default right). One undo step.",
        input_schema: obj(
            json!({
                "on": {"type": "boolean"},
                "keep": {"type": "string", "enum": ["left", "right"], "description": "When turning off: which rail's outline to keep for both sides (default right)."}
            }),
            &["on"],
        ),
    }]
}

/// `set_asymmetric {on, keep}`.
pub(crate) fn set_asymmetric(doc: &mut Document, ctx: &mut Context, a: &Value) -> R {
    let on = arg(a, "on")
        .and_then(Value::as_bool)
        .ok_or("on: expected true or false")?;
    let keep = match string(a, "keep").unwrap_or("right") {
        "left" => Rail::Left,
        "right" => Rail::Right,
        o => return Err(format!("keep: expected left or right, got {o:?}")),
    };
    if on != doc.board.is_asymmetric() {
        doc.edit(|b| {
            b.set_asymmetric(on, keep);
        });
    }
    Ok(rails(&doc.board, ctx))
}

/// Asymmetry and per-rail widths for `board_summary`.
pub(crate) fn rails(board: &Board, ctx: &Context) -> Value {
    let mut v = json!({"asymmetric": board.is_asymmetric()});
    if board.is_asymmetric() {
        let l = board.length();
        for rail in Rail::BOTH {
            let (w, x) = board.max_half_width(rail);
            v[format!("{}_rail", rail.name())] = json!({
                "max_half_width": measure(ctx, w),
                "max_half_width_from_tail": measure(ctx, x),
                "half_width_middle": measure(ctx, board.half_width_at(l / 2.0, rail)),
            });
        }
    }
    v
}
