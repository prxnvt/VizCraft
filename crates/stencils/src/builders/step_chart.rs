//! Displacement-step chart builder: the sequence chart drawn beside a pneumatic circuit.
//!
//! Provenance: own design, drawn from the written description of VDI 3260. One row per actuator,
//! its code at the left; two levels per row, extended (1) above retracted (0); the step
//! boundaries as vertical lines numbered at the top; the actuator's state as a line through the
//! boundaries, sloped across a step where the state changes. The rows come from a short text,
//! one line per actuator (`1A1 0 1 1 0`), read by `vizcraft_notation::fluid::steps`.

use vizcraft_notation::fluid::steps;

use super::{BuilderSpec, Def, Draw, ParamKind, ParamSpec, Params};

const SOURCE: &str = "own design, drawn from the written description of VDI 3260";

/// Modules for the code column.
const CODE_W: f64 = 6.0;
/// Modules per step.
const STEP_W: f64 = 4.0;
/// Modules per row.
const ROW_H: f64 = 4.0;
/// Modules for the step numbers.
const HEAD_H: f64 = 2.0;

pub(super) const SPEC: BuilderSpec = BuilderSpec {
    name: "stepChart",
    label: "Displacement-Step Chart",
    description: "A VDI 3260 displacement-step chart: one row per actuator from a text of one line each (`1A1 0 1 1 0`: the code, then 0 retracted or 1 extended at each step boundary), step boundaries numbered across the top, the motion drawn between them. At most 16 rows and 32 steps.",
    params: &[ParamSpec {
        key: "rows",
        label: "Rows",
        kind: ParamKind::Text { max: steps::MAX_TEXT },
        default: Def::Text("1A1 0 1 0"),
        description: "One line per actuator: its code, then its state at each step boundary.",
    }],
    sources: &[("VDI 3260", SOURCE)],
    build,
};

fn build(p: &Params, d: &mut Draw) {
    d.standard("iso1219");
    let chart = steps::parse(p.text("rows"));
    for w in chart.problems.iter().take(4) {
        d.warn(format!("`rows`: {w}"));
    }
    let n = chart.states().max(2);
    let rows = chart.rows.len().max(1);
    let w = CODE_W + STEP_W * (n - 1) as f64 + 2.0;
    let h = HEAD_H + ROW_H * rows as f64;
    d.size(w, h);
    d.rect(0.0, HEAD_H, w, h);
    for i in 0..n {
        let x = CODE_W + STEP_W * i as f64;
        d.line(&[(x, HEAD_H), (x, h)]);
        d.label(&format!("s{}", i + 1), &(i + 1).to_string(), x, HEAD_H / 2.0, 0.8, "middle");
    }
    for (r, row) in chart.rows.iter().enumerate() {
        let top = HEAD_H + ROW_H * r as f64;
        if r > 0 {
            d.line(&[(0.0, top), (w, top)]);
        }
        d.label(&format!("r{}", r + 1), &row.code, 0.5, top + ROW_H / 2.0, 0.8, "start");
        let level = |on: bool| if on { top + 1.0 } else { top + 3.0 };
        let last = row.states.last().copied().unwrap_or(false);
        let pts: Vec<(f64, f64)> = (0..n).map(|i| (CODE_W + STEP_W * i as f64, level(row.states.get(i).copied().unwrap_or(last)))).collect();
        d.line(&pts);
    }
}
