//! The starter workflow patterns over the Flowchart pack, after the control-flow patterns of the
//! Workflow Patterns catalogue (van der Aalst, ter Hofstede, Kiepuszewski and Barros):
//!
//! | Pattern | Drawing | Layout |
//! |---|---|---|
//! | `workflow.parallelSplit` | a split fans out to N branches that join at a step | pipeline with a parallel band |
//! | `workflow.exclusiveChoice` | a Decision with N labelled branches to steps that rejoin | pipeline with a parallel band |
//! | `workflow.nOfMJoin` | M branches into one join step named "N of M" | N-channel voter |
//! | `workflow.multipleInstances` | one step repeated N times between a split and a join | pipeline with a parallel band |
//! | `workflow.cancelRegion` | a region of steps with a cancelling Decision whose flow leaves it | nested container |
//!
//! Flowchart has no fork symbol, and only a Decision may have more than one exit (ISO 5807), so
//! a split is drawn as a Decision named for what it does; the joins are Process steps.

use vizcraft_layout::Direction;
use vizcraft_layout::{Nest, Parallel, Primitive};

use crate::emit::Emission;
use crate::schema::{Args, Param, Pattern};
use crate::{Builtin, PatternError};

const PROCESS: &str = "flowchart.Process";
const DECISION: &str = "flowchart.Decision";
const FLOW: &str = "flowchart.Flow";

/// Most branches, instances or steps any starter pattern takes.
pub const MAX_BRANCHES: u32 = 32;

fn pattern(id: &str, name: &str, description: &str, params: Vec<Param>) -> Pattern {
    Pattern {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        pack: "flowchart".into(),
        audience: vec!["general".into(), "enterprise".into()],
        params,
    }
}

/// The starter patterns with their generators.
pub fn builtins() -> Vec<Builtin> {
    vec![
        Builtin {
            pattern: pattern(
                "workflow.parallelSplit",
                "Parallel Split",
                "A step fans out to several branches that run at the same time and join again at one step.",
                vec![
                    Param::count("branches", "Branches", 2, MAX_BRANCHES, 3),
                    Param::labels("labels", "Branch names", &[]),
                    Param::text("split", "Split step", "Split"),
                    Param::text("join", "Join step", "Join"),
                    Param::tag_prefix("tagPrefix", "Tag prefix"),
                ],
            ),
            generate: parallel_split,
        },
        Builtin {
            pattern: pattern(
                "workflow.exclusiveChoice",
                "Exclusive Choice",
                "A decision sends the flow down exactly one of several labelled branches, which then rejoin.",
                vec![
                    Param::count("branches", "Branches", 2, MAX_BRANCHES, 2),
                    Param::text("question", "Decision", "Condition?"),
                    Param::labels("labels", "Branch labels", &["Yes", "No"]),
                    Param::text("merge", "Merge step", "Merge"),
                ],
            ),
            generate: exclusive_choice,
        },
        Builtin {
            pattern: pattern(
                "workflow.nOfMJoin",
                "N-of-M Join",
                "Several branches flow into one join step that continues once N of the M branches have finished.",
                vec![
                    Param::count("branches", "Branches (M)", 2, MAX_BRANCHES, 3),
                    Param::count("required", "Required (N)", 1, MAX_BRANCHES, 2),
                    Param::labels("labels", "Branch names", &[]),
                ],
            ),
            generate: n_of_m_join,
        },
        Builtin {
            pattern: pattern(
                "workflow.multipleInstances",
                "Multiple Instances",
                "One step runs as several instances side by side between a split and a join.",
                vec![
                    Param::count("instances", "Instances", 2, MAX_BRANCHES, 3),
                    Param::text("task", "Step", "Task"),
                    Param::text("split", "Split step", "Split"),
                    Param::text("join", "Join step", "Join"),
                ],
            ),
            generate: multiple_instances,
        },
        Builtin {
            pattern: pattern(
                "workflow.cancelRegion",
                "Cancel Region",
                "A region of steps ends in a decision that can cancel the work; both of its flows leave the region.",
                vec![
                    Param::count("steps", "Steps", 1, MAX_BRANCHES, 2),
                    Param::labels("labels", "Step names", &[]),
                    Param::text("region", "Region", "Cancel region"),
                    Param::text("question", "Decision", "Cancel?"),
                    Param::text("cancelled", "Cancelled step", "Cancelled"),
                    Param::text("next", "Next step", "Continue"),
                ],
            ),
            generate: cancel_region,
        },
    ]
}

/// The text argument `key`, or `fallback` when it is blank.
fn named(args: &Args, key: &str, fallback: &str) -> String {
    let t = args.text(key).trim();
    if t.is_empty() { fallback.to_string() } else { t.to_string() }
}

/// A fan out from `from` to `members` and back to `to`, laid out as a pipeline's parallel band.
fn band(from: &str, to: &str, members: Vec<String>) -> Primitive<String> {
    Primitive::Pipeline { parallel: Some(Parallel { from: from.to_string(), to: to.to_string(), members }) }
}

fn parallel_split(args: &Args) -> Result<Emission, PatternError> {
    let n = args.count("branches");
    let roles: Vec<String> = (1..=n).map(|i| format!("branch.{i}")).collect();
    let mut e = Emission::new("", band("split", "join", roles.clone()), Direction::LeftRight);
    e.tag_prefix = args.text("tagPrefix").to_string();
    e.element("split", DECISION, named(args, "split", "Split"));
    for (i, role) in roles.iter().enumerate() {
        e.element(role.clone(), PROCESS, args.label_or("labels", i, || format!("Branch {}", i.saturating_add(1))));
    }
    e.element("join", PROCESS, named(args, "join", "Join"));
    for role in &roles {
        e.relate("flow", FLOW, "split", role, "");
        e.relate("flow", FLOW, role, "join", "");
    }
    Ok(e)
}

fn exclusive_choice(args: &Args) -> Result<Emission, PatternError> {
    let n = args.count("branches");
    let roles: Vec<String> = (1..=n).map(|i| format!("branch.{i}")).collect();
    let mut e = Emission::new("", band("choice", "merge", roles.clone()), Direction::LeftRight);
    e.element("choice", DECISION, named(args, "question", "Condition?"));
    for (i, role) in roles.iter().enumerate() {
        e.element(role.clone(), PROCESS, format!("Step {}", i.saturating_add(1)));
    }
    e.element("merge", PROCESS, named(args, "merge", "Merge"));
    for (i, role) in roles.iter().enumerate() {
        e.relate("flow", FLOW, "choice", role, args.label_or("labels", i, || format!("Choice {}", i.saturating_add(1))));
        e.relate("flow", FLOW, role, "merge", "");
    }
    Ok(e)
}

fn n_of_m_join(args: &Args) -> Result<Emission, PatternError> {
    let m = args.count("branches");
    let n = args.count("required");
    if n > m {
        return Err(PatternError::Args(format!("`required`: {n} is more than the {m} branches")));
    }
    let roles: Vec<String> = (1..=m).map(|i| format!("branch.{i}")).collect();
    let layout = Primitive::Voter { channels: roles.iter().map(|r| vec![r.clone()]).collect(), voter: "join".into() };
    let mut e = Emission::new("", layout, Direction::TopBottom);
    for (i, role) in roles.iter().enumerate() {
        e.element(role.clone(), PROCESS, args.label_or("labels", i, || format!("Branch {}", i.saturating_add(1))));
    }
    e.element("join", PROCESS, format!("{n} of {m}"));
    for role in &roles {
        e.relate("flow", FLOW, role, "join", "");
    }
    Ok(e)
}

fn multiple_instances(args: &Args) -> Result<Emission, PatternError> {
    let n = args.count("instances");
    let roles: Vec<String> = (1..=n).map(|i| format!("instance.{i}")).collect();
    let mut e = Emission::new("", band("split", "join", roles.clone()), Direction::LeftRight);
    let task = named(args, "task", "Task");
    e.element("split", DECISION, named(args, "split", "Split"));
    for (i, role) in roles.iter().enumerate() {
        e.element(role.clone(), PROCESS, format!("{task} {}", i.saturating_add(1)));
    }
    e.element("join", PROCESS, named(args, "join", "Join"));
    for role in &roles {
        e.relate("flow", FLOW, "split", role, "");
        e.relate("flow", FLOW, role, "join", "");
    }
    Ok(e)
}

fn cancel_region(args: &Args) -> Result<Emission, PatternError> {
    let n = args.count("steps");
    let steps: Vec<String> = (1..=n).map(|i| format!("step.{i}")).collect();
    let mut inside = steps.clone();
    inside.push("cancel".into());
    let layout = Primitive::Nested { containers: vec![Nest { id: "region".into(), members: inside.clone(), padding: None, header: None }] };
    let mut e = Emission::new("", layout, Direction::LeftRight);
    for (i, role) in steps.iter().enumerate() {
        e.element(role.clone(), PROCESS, args.label_or("labels", i, || format!("Step {}", i.saturating_add(1))));
    }
    e.element("cancel", DECISION, named(args, "question", "Cancel?"));
    e.element("cancelled", PROCESS, named(args, "cancelled", "Cancelled"));
    e.element("next", PROCESS, named(args, "next", "Continue"));
    for pair in inside.windows(2) {
        if let [a, b] = pair {
            e.relate("flow", FLOW, a, b, "");
        }
    }
    e.relate("flow", FLOW, "cancel", "cancelled", "Yes");
    e.relate("flow", FLOW, "cancel", "next", "No");
    e.regions.push(crate::emit::Region { role: "region".into(), label: named(args, "region", "Cancel region"), members: inside });
    Ok(e)
}
