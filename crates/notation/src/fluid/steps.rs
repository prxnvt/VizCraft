//! The displacement-step chart (VDI 3260): one row per actuator, its state at each step
//! boundary, 0 retracted and 1 extended.
//!
//! The chart is stored as text, one row per line or between semicolons: the actuator's code, then
//! its states separated by spaces (`1A1 0 1 1 0; 2A1 0 0 1 1`). [`parse`] reads it (never failing: what does not read is reported in
//! [`Chart::problems`]) and [`Chart::text`] writes it back in the same form. A chart holds at most
//! [`MAX_ROWS`] rows and [`MAX_STATES`] states (32 steps).

/// Most rows (actuators) a chart holds.
pub const MAX_ROWS: usize = 16;

/// Most states a row holds: 32 steps.
pub const MAX_STATES: usize = 33;

/// Fewest states a row holds: one step.
pub const MIN_STATES: usize = 2;

/// Longest code read, in characters.
pub const MAX_CODE: usize = 32;

/// Longest chart text read, in bytes; the rest is ignored.
pub const MAX_TEXT: usize = 4096;

/// One row: an actuator's code and its states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// The actuator's code (its tag).
    pub code: String,
    /// Its state at each step boundary: `true` extended.
    pub states: Vec<bool>,
}

/// A parsed chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chart {
    /// The rows, in order.
    pub rows: Vec<Row>,
    /// What did not read, one line each.
    pub problems: Vec<String>,
}

impl Chart {
    /// The number of states of the longest row.
    pub fn states(&self) -> usize {
        self.rows.iter().map(|r| r.states.len()).max().unwrap_or(0)
    }

    /// The chart as text, rows separated by `; ` (a glyph parameter keeps no line breaks), every
    /// row padded to the same number of states with its last state.
    pub fn text(&self) -> String {
        let n = self.states();
        self.rows
            .iter()
            .map(|r| {
                let last = r.states.last().copied().unwrap_or(false);
                let mut line = r.code.clone();
                for i in 0..n {
                    line.push(' ');
                    line.push(if r.states.get(i).copied().unwrap_or(last) { '1' } else { '0' });
                }
                line
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Read chart text (see the module docs). Blank lines are skipped; a state other than 0 or 1, a
/// row with fewer than two states, and anything past the limits are reported and left out.
pub fn parse(text: &str) -> Chart {
    let mut chart = Chart::default();
    let mut cut = text;
    if cut.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while end > 0 && !cut.is_char_boundary(end) {
            end -= 1;
        }
        cut = cut.get(..end).unwrap_or_default();
        chart.problems.push(format!("the chart is longer than {MAX_TEXT} bytes; the rest is ignored"));
    }
    for (n, line) in cut.split(['\n', '\r', ';']).map(str::trim).filter(|l| !l.is_empty()).enumerate() {
        if chart.rows.len() >= MAX_ROWS {
            chart.problems.push(format!("a chart holds at most {MAX_ROWS} rows; the rest is ignored"));
            break;
        }
        let mut words = line.split_whitespace();
        let code: String = words.next().unwrap_or_default().chars().take(MAX_CODE).collect();
        let mut states = Vec::new();
        let mut bad = false;
        for w in words {
            match w {
                "0" => states.push(false),
                "1" => states.push(true),
                _ => bad = true,
            }
            if states.len() > MAX_STATES {
                break;
            }
        }
        if bad {
            chart.problems.push(format!("row {} ({code}): a state is 0 (retracted) or 1 (extended)", n + 1));
        }
        if states.len() > MAX_STATES {
            states.truncate(MAX_STATES);
            chart.problems.push(format!("row {} ({code}): at most {} steps", n + 1, MAX_STATES - 1));
        }
        if states.len() < MIN_STATES {
            chart.problems.push(format!("row {} ({code}): a row needs at least two states", n + 1));
            continue;
        }
        chart.rows.push(Row { code, states });
    }
    chart
}

/// The default rows for actuators `codes` over `steps` steps: each actuator extends one step after
/// the one before it and retracts in the same order once all are out (A+ B+ A- B-) when the steps
/// allow, else every state is 0.
pub fn default_rows(codes: &[String], steps: usize) -> Chart {
    let steps = steps.clamp(MIN_STATES - 1, MAX_STATES - 1);
    let m = codes.len().min(MAX_ROWS);
    let rows = codes
        .iter()
        .take(MAX_ROWS)
        .enumerate()
        .map(|(k, code)| {
            let states = (0..=steps).map(|b| steps >= 2 * m && b > k && b <= m + k).collect();
            Row { code: code.chars().take(MAX_CODE).collect(), states }
        })
        .collect();
    Chart { rows, problems: Vec::new() }
}

/// One finding about a chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Finding {
    /// The row it is about (0-based), if one.
    pub row: Option<usize>,
    /// The step it is about (1-based: the move from state `step - 1` to state `step`), if one.
    pub step: Option<usize>,
    /// What is wrong, in plain words.
    pub message: String,
    /// The shapes of the components it names (the actuator, the valve).
    pub shapes: Vec<String>,
    /// A step in which nothing moves (an info finding) rather than a fault.
    pub idle: bool,
}

/// What is wrong with `chart` against circuit `c`: text that did not read, rows naming no
/// actuator, steps no selectable valve position can drive, and idle steps.
///
/// For each move, the valve driving the actuator is the one its moving side reaches through
/// lines and pass-through parts. Some selectable position of it (the rest, an end square with an
/// operator, the spring-centred middle) must join its supply to that side and, for a double
/// acting actuator, the other side to an exhaust.
pub fn findings(c: &super::Circuit, cat: &crate::Catalog, model: &vizcraft_model::Model, chart: &Chart) -> Vec<Finding> {
    use super::{ACTUATOR, Role, Walk, is_a};
    let mut out: Vec<Finding> = chart.problems.iter().map(|p| Finding { message: p.clone(), ..Finding::default() }).collect();
    let actuator = |code: &str| {
        c.parts
            .iter()
            .position(|p| is_a(cat, &p.type_id, ACTUATOR) && model.element(&p.element).is_some_and(|e| !code.is_empty() && e.tag.trim() == code))
    };
    let rows: Vec<Option<usize>> = chart.rows.iter().map(|r| actuator(&r.code)).collect();
    for (i, (row, a)) in chart.rows.iter().zip(&rows).enumerate() {
        if a.is_none() {
            out.push(Finding {
                row: Some(i),
                message: format!("Row {} names {}, which is no actuator's code.", i + 1, row.code),
                ..Finding::default()
            });
        }
    }
    let walk = Walk { valves: false, pressure_valves: true, ..Walk::lines() };
    let n = chart.states();
    for step in 1..n {
        let mut moved = false;
        for (i, row) in chart.rows.iter().enumerate() {
            let (Some(&before), Some(&after)) = (row.states.get(step - 1), row.states.get(step)) else { continue };
            if before == after {
                continue;
            }
            moved = true;
            let Some(ai) = rows.get(i).copied().flatten() else { continue };
            let Some(part) = c.parts.get(ai) else { continue };
            let (drive, other) = if after { ("A", "B") } else { ("B", "A") };
            let verb = if after { "extends" } else { "retracts" };
            let Some(dp) = part.port(c, drive) else { continue };
            let op = part.port(c, other);
            let valve_ports = |p: usize| -> Vec<usize> {
                c.trace(&[p], &walk)
                    .ports
                    .into_iter()
                    .filter(|&q| c.ports.get(q).and_then(|x| c.parts.get(x.part)).is_some_and(|x| x.role == Role::Valve))
                    .collect()
            };
            let drivers = valve_ports(dp);
            let at = |msg: String, shapes: Vec<String>| Finding { row: Some(i), step: Some(step), message: msg, shapes, idle: false };
            if drivers.is_empty() {
                out.push(at(format!("Step {step}: {} {verb}, but no valve drives its {drive} side.", row.code), vec![part.shape.clone()]));
                continue;
            }
            let others = op.map(valve_ports).unwrap_or_default();
            let ok = drivers.iter().any(|&vp| {
                let Some(v) = c.ports.get(vp).and_then(|x| c.parts.get(x.part)) else { return false };
                let Some(info) = &v.valve else { return false };
                let joined = |pos: usize, a: usize, b: usize| {
                    v.positions.get(pos).is_some_and(|es| es.iter().any(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a)))
                };
                v.selectable.iter().any(|&pos| {
                    let fed = info.supply.is_some_and(|s| joined(pos, s, vp));
                    let vented =
                        op.is_none() || others.iter().filter(|&&q| v.ports.contains(&q)).any(|&q| info.exhausts.iter().any(|&x| joined(pos, q, x)));
                    fed && vented
                })
            });
            if !ok {
                let valve = drivers.first().and_then(|&vp| c.ports.get(vp)).and_then(|x| c.parts.get(x.part));
                let name = valve.and_then(|v| model.element(&v.element)).map(|e| e.tag.trim().to_string()).filter(|t| !t.is_empty());
                let shapes: Vec<String> = [Some(part.shape.clone()), valve.map(|v| v.shape.clone())].into_iter().flatten().collect();
                out.push(at(
                    format!(
                        "Step {step}: {} {verb}, but no position of valve {} joins its supply to that side{}.",
                        row.code,
                        name.unwrap_or_else(|| "(uncoded)".into()),
                        if op.is_some() { " and the other side to an exhaust" } else { "" }
                    ),
                    shapes,
                ));
            }
        }
        if !moved && !chart.rows.is_empty() {
            out.push(Finding { step: Some(step), message: format!("Step {step} moves no actuator."), idle: true, ..Finding::default() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_round_trip_and_bad_text_is_reported() {
        let c = parse("1A1 0 1 1 0\n\n2A1 0 0 1 1 0\n");
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        assert_eq!(c.rows.len(), 2);
        assert_eq!(c.text(), "1A1 0 1 1 0 0; 2A1 0 0 1 1 0");
        assert_eq!(parse(&c.text()).text(), c.text());
        let bad = parse("1A1 0 x 1\n2A1 0\n");
        assert_eq!(bad.problems.len(), 2, "{:?}", bad.problems);
        assert_eq!(bad.rows.len(), 1);
        let long = parse(&"1A1 0 1\n".repeat(40));
        assert_eq!(long.rows.len(), MAX_ROWS);
        let wide = parse(&format!("1A1{}", " 1".repeat(100)));
        assert_eq!(wide.rows[0].states.len(), MAX_STATES);
        let huge = parse(&"é".repeat(70_000));
        assert!(!huge.problems.is_empty());
    }

    #[test]
    fn default_rows_extend_in_order_then_retract() {
        let c = default_rows(&["1A1".into(), "2A1".into()], 4);
        assert_eq!(c.text(), "1A1 0 1 1 0 0; 2A1 0 0 1 1 0");
        let flat = default_rows(&["1A1".into(), "2A1".into()], 2);
        assert_eq!(flat.text(), "1A1 0 0 0; 2A1 0 0 0");
    }
}
