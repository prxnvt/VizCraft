//! Dependency layering rules (docs/architecture.md, "Layers").
//!
//! The rules work on a small model built from `cargo metadata --no-deps` so they can be unit
//! tested without cargo. Crates are classified by name suffix and manifest path, so the rules
//! hold for every `vizcraft-*` crate without listing them.

use serde_json::Value;
use std::path::Path;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Model layer (diagram, stencils, formats and the leaf libraries in `MODEL_SUFFIXES`): plain data and geometry below the engine; no UI toolkit,
    /// no engine, no front end.
    Model,
    /// L4: the model, undo, command registry; knows nothing about any UI toolkit.
    Engine,
    /// L6: the egui front end; uses egui but not a window host.
    UiEgui,
    /// Platform glue (macos-open): no workspace dependencies, no UI toolkit.
    Platform,
    /// The MCP server (`*-mcp`): above the engine, below the apps, and never uses a UI toolkit.
    Mcp,
    /// Binaries under apps/: the only crates allowed to use eframe, rfd, muda.
    App,
    /// xtask: repository tooling, no UI toolkit.
    Tooling,
}

/// A crate that may sit in L6 or above: egui itself and its satellite crates.
fn is_egui(name: &str) -> bool {
    name == "egui" || name.starts_with("egui_") || name.starts_with("egui-")
}

/// Window hosts and desktop services that only the apps may use.
const APP_ONLY: &[&str] = &["eframe", "winit", "rfd", "muda"];

/// Everything the engine and the model layer must stay clear of (the app-only set, egui, and the
/// clipboard).
fn forbidden_in_engine(name: &str) -> bool {
    is_egui(name) || APP_ONLY.contains(&name) || name == "arboard"
}

/// External crates the rules look at (shown in the `layers` table).
pub fn watched(name: &str) -> bool {
    forbidden_in_engine(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// `true` if the dependency is a workspace member.
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    /// Path of the crate's Cargo.toml, used to recognise apps (`apps/<name>/Cargo.toml`).
    pub manifest_path: String,
    pub deps: Vec<Dep>,
}

/// Name suffixes of the model-layer crates: the diagram model and catalog, the file formats, and
/// the leaf libraries they build on (Mermaid parser, draw.io codec, auto-layout, connector router).
const MODEL_SUFFIXES: &[&str] = &["-diagram", "-stencils", "-formats", "-mermaid", "-drawio", "-layout", "-routing"];

/// Classify a crate by its name suffix, falling back to its directory for apps.
pub fn classify(c: &Crate) -> Option<Class> {
    if c.name == "xtask" {
        return Some(Class::Tooling);
    }
    if MODEL_SUFFIXES.iter().any(|s| c.name.ends_with(s)) {
        return Some(Class::Model);
    }
    if c.name.ends_with("-engine") {
        return Some(Class::Engine);
    }
    if c.name.ends_with("-ui-egui") {
        return Some(Class::UiEgui);
    }
    if c.name.ends_with("-macos-open") {
        return Some(Class::Platform);
    }
    if c.name.ends_with("-mcp") {
        return Some(Class::Mcp);
    }
    let under_apps = Path::new(&c.manifest_path).components().any(|p| p.as_os_str() == "apps");
    if under_apps { Some(Class::App) } else { None }
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Model) => "model".into(),
        Some(Class::Engine) => "L4".into(),
        Some(Class::UiEgui) => "L6".into(),
        Some(Class::Platform) => "platform".into(),
        Some(Class::Mcp) => "mcp".into(),
        Some(Class::App) => "app".into(),
        Some(Class::Tooling) => "tooling".into(),
        None => "UNKNOWN".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A crate the rules cannot place: not a model suffix (`*-diagram`, `*-formats`, `*-drawio`, ...), `*-engine`, `*-ui-egui`, `*-macos-open`, `xtask` or under `apps/`.
    Unregistered { krate: String },
    /// The engine pulls in a UI toolkit or desktop service.
    EngineUsesUi { krate: String, dep: String },
    /// A model crate (diagram, stencils) pulls in a UI toolkit or desktop service.
    ModelUsesUi { krate: String, dep: String },
    /// A non-app crate uses a window host or desktop service (eframe, winit, rfd, muda).
    AppOnlyDep { krate: String, dep: String },
    /// A workspace dependency that points the wrong way (engine -> ui, anything -> app, platform -> anything).
    Upward { krate: String, dep: String },
    /// Something depends on xtask.
    DependsOnTooling { krate: String },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Unregistered { krate } => write!(
                f,
                "{krate}: unknown workspace crate; name it with a model suffix (-diagram, -stencils, -formats, -mermaid, -drawio, -layout, -routing), <craft>-engine, <craft>-ui-egui, <craft>-macos-open, put it under apps/, or extend xtask/src/layers.rs"
            ),
            Violation::EngineUsesUi { krate, dep } => {
                write!(f, "{krate} depends on `{dep}`; the engine and the MCP server must not know about egui/eframe/winit/rfd/muda/arboard")
            }
            Violation::ModelUsesUi { krate, dep } => {
                write!(f, "{krate} (model) depends on `{dep}`; the model layer must not know about egui/eframe/winit/rfd/muda/arboard")
            }
            Violation::AppOnlyDep { krate, dep } => write!(f, "{krate} depends on `{dep}`; only apps/* may use eframe, winit, rfd, muda"),
            Violation::Upward { krate, dep } => {
                write!(f, "{krate} -> {dep}: dependencies must point down the layer table (model <- engine <- ui-egui <- apps)")
            }
            Violation::DependsOnTooling { krate } => write!(f, "{krate} depends on xtask; tooling is not a library"),
        }
    }
}

/// Rank used for the direction check: a crate may only depend on strictly lower ranks (model
/// crates may also depend on each other: stencils builds on diagram).
fn rank(class: Class) -> u8 {
    match class {
        Class::Model => 0,
        Class::Platform | Class::Engine => 1,
        // Same rank as ui-egui, so neither can depend on the other.
        Class::UiEgui | Class::Mcp => 2,
        Class::App => 3,
        Class::Tooling => 4,
    }
}

/// Check all rules. Violations come back sorted and deduplicated for stable output.
pub fn check(crates: &[Crate]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify(c) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        for d in &c.deps {
            // Self dev-dependencies (to enable features in tests) are fine.
            if d.name == c.name {
                continue;
            }
            if d.workspace {
                let target = crates.iter().find(|x| x.name == d.name).and_then(classify);
                match target {
                    Some(Class::Tooling) => out.push(Violation::DependsOnTooling { krate: c.name.clone() }),
                    Some(Class::Model) if class == Class::Model => {}
                    Some(t) if class == Class::Platform || rank(t) >= rank(class) => {
                        out.push(Violation::Upward { krate: c.name.clone(), dep: d.name.clone() });
                    }
                    // Unknown dependencies are reported on their own entry.
                    _ => {}
                }
                continue;
            }
            match class {
                Class::Engine | Class::Mcp if forbidden_in_engine(&d.name) => {
                    out.push(Violation::EngineUsesUi { krate: c.name.clone(), dep: d.name.clone() })
                }
                Class::Model if forbidden_in_engine(&d.name) => out.push(Violation::ModelUsesUi { krate: c.name.clone(), dep: d.name.clone() }),
                Class::UiEgui | Class::Platform | Class::Tooling if APP_ONLY.contains(&d.name.as_str()) => {
                    out.push(Violation::AppOnlyDep { krate: c.name.clone(), dep: d.name.clone() });
                }
                _ => {}
            }
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

/// Build the model from `cargo metadata --format-version 1 --no-deps`.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members: Vec<&str> = pkgs.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().ok_or("package without name")?.to_owned();
        let manifest_path = p["manifest_path"].as_str().unwrap_or_default().to_owned();
        let mut deps = Vec::new();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dname = d["name"].as_str().unwrap_or_default().to_owned();
            let kind = match d["kind"].as_str() {
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                _ => DepKind::Normal,
            };
            let workspace = members.contains(&dname.as_str()) || d["path"].is_string();
            deps.push(Dep { name: dname, kind, workspace });
        }
        out.push(Crate { name, manifest_path, deps });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use DepKind::*;

    fn c(name: &str, manifest: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate {
            name: name.into(),
            manifest_path: manifest.into(),
            deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect(),
        }
    }

    fn baseline(prefix: &str) -> Vec<Crate> {
        let p = |s: &str| format!("{prefix}-{s}");
        vec![
            c(
                &p("engine"),
                "/r/crates/engine/Cargo.toml",
                &[("serde", Normal, false), ("thiserror", Normal, false), (&p("diagram"), Normal, true), (&p("stencils"), Normal, true)],
            ),
            c(&p("macos-open"), "/r/crates/macos-open/Cargo.toml", &[("objc2", Normal, false)]),
            c(
                &p("ui-egui"),
                "/r/crates/ui-egui/Cargo.toml",
                &[(&p("engine"), Normal, true), ("egui", Normal, false), ("egui_kittest", Dev, false), ("arboard", Normal, false)],
            ),
            c(
                prefix,
                "/r/apps/vizcraft/Cargo.toml",
                &[
                    (&p("engine"), Normal, true),
                    (&p("ui-egui"), Normal, true),
                    ("eframe", Normal, false),
                    ("rfd", Normal, false),
                    ("muda", Normal, false),
                ],
            ),
            c(&p("cli"), "/r/apps/vizcraft-cli/Cargo.toml", &[(&p("engine"), Normal, true)]),
            c("xtask", "/r/xtask/Cargo.toml", &[("serde_json", Normal, false)]),
            c(&p("diagram"), "/r/crates/diagram/Cargo.toml", &[("serde", Normal, false), ("proptest", Dev, false)]),
            c(&p("stencils"), "/r/crates/stencils/Cargo.toml", &[(&p("diagram"), Normal, true)]),
        ]
    }

    #[test]
    fn baseline_graph_passes_for_any_craft_name() {
        for prefix in ["vizcraft", "notecraft"] {
            let g = baseline(prefix);
            assert!(check(&g).is_empty(), "{prefix}: {:?}", check(&g));
        }
    }

    #[test]
    fn classification() {
        let g = baseline("vizcraft");
        let classes: Vec<_> = g.iter().map(classify).collect();
        assert_eq!(
            classes,
            vec![
                Some(Class::Engine),
                Some(Class::Platform),
                Some(Class::UiEgui),
                Some(Class::App),
                Some(Class::App),
                Some(Class::Tooling),
                Some(Class::Model),
                Some(Class::Model)
            ]
        );
        assert_eq!(classify(&c("vizcraft-web", "/r/apps/vizcraft-web/Cargo.toml", &[])), Some(Class::App));
        assert_eq!(classify(&c("vizcraft-mystery", "/r/crates/mystery/Cargo.toml", &[])), None);
    }

    #[test]
    fn engine_must_not_use_ui_crates() {
        for dep in ["egui", "egui_extras", "eframe", "winit", "rfd", "muda", "arboard"] {
            let v = check(&[c("vizcraft-engine", "/r/crates/engine/Cargo.toml", &[(dep, Normal, false)])]);
            assert!(matches!(&v[..], [Violation::EngineUsesUi { dep: d, .. }] if d == dep), "{dep}: {v:?}");
        }
        // Dev-dependencies count too: the engine's tests must stay headless.
        assert!(!check(&[c("vizcraft-engine", "/r/crates/engine/Cargo.toml", &[("egui_kittest", Dev, false)])]).is_empty());
        assert!(check(&[c("vizcraft-engine", "/r/crates/engine/Cargo.toml", &[("serde_json", Normal, false)])]).is_empty());
    }

    #[test]
    fn model_must_not_use_ui_crates_or_point_up() {
        for name in [
            "vizcraft-diagram",
            "vizcraft-stencils",
            "vizcraft-formats",
            "vizcraft-mermaid",
            "vizcraft-drawio",
            "vizcraft-layout",
            "vizcraft-routing",
        ] {
            assert_eq!(classify(&c(name, "/r/crates/x/Cargo.toml", &[])), Some(Class::Model), "{name}");
            let manifest = format!("/r/crates/{}/Cargo.toml", name.trim_start_matches("vizcraft-"));
            for dep in ["egui", "egui_kittest", "eframe", "winit", "rfd", "muda", "arboard"] {
                let v = check(&[c(name, &manifest, &[(dep, Normal, false)])]);
                assert!(matches!(&v[..], [Violation::ModelUsesUi { dep: d, .. }] if d == dep), "{name} {dep}: {v:?}");
            }
            assert!(check(&[c(name, &manifest, &[("serde", Normal, false), ("proptest", Dev, false)])]).is_empty());
        }
        // The model sits below the engine and the UI: depending on either points up.
        for up in ["vizcraft-engine", "vizcraft-ui-egui"] {
            let mut g = baseline("vizcraft");
            g[6].deps.push(Dep { name: up.into(), kind: Normal, workspace: true });
            assert!(matches!(&check(&g)[..], [Violation::Upward { krate, dep }] if krate == "vizcraft-diagram" && dep == up), "{up}");
        }
        // Stencils on diagram is sideways within the model layer and allowed; the reverse is too,
        // since cargo itself rejects a cycle.
        assert!(check(&baseline("vizcraft")).is_empty());
        // The UI may read the model directly.
        let mut g = baseline("vizcraft");
        g[2].deps.push(Dep { name: "vizcraft-diagram".into(), kind: Normal, workspace: true });
        assert!(check(&g).is_empty());
        // Platform glue stays free of every workspace crate, the model included.
        let mut g = baseline("vizcraft");
        g[1].deps.push(Dep { name: "vizcraft-diagram".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "vizcraft-macos-open"));
    }

    #[test]
    fn ui_egui_may_use_egui_but_not_a_window_host() {
        let ok = c(
            "vizcraft-ui-egui",
            "/r/crates/ui-egui/Cargo.toml",
            &[("egui", Normal, false), ("egui_extras", Normal, false), ("arboard", Normal, false)],
        );
        assert!(check(&[ok]).is_empty());
        for dep in ["eframe", "winit", "rfd", "muda"] {
            let v = check(&[c("vizcraft-ui-egui", "/r/crates/ui-egui/Cargo.toml", &[(dep, Normal, false)])]);
            assert!(matches!(&v[..], [Violation::AppOnlyDep { dep: d, .. }] if d == dep), "{dep}: {v:?}");
        }
    }

    #[test]
    fn platform_and_tooling_stay_clear_of_hosts() {
        assert!(!check(&[c("vizcraft-macos-open", "/r/crates/macos-open/Cargo.toml", &[("winit", Normal, false)])]).is_empty());
        assert!(!check(&[c("xtask", "/r/xtask/Cargo.toml", &[("rfd", Normal, false)])]).is_empty());
    }

    #[test]
    fn upward_and_sideways_workspace_deps_flagged() {
        let mut g = baseline("vizcraft");
        g[0].deps.push(Dep { name: "vizcraft-ui-egui".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, dep }] if krate == "vizcraft-engine" && dep == "vizcraft-ui-egui"));

        let mut g = baseline("vizcraft");
        g[2].deps.push(Dep { name: "vizcraft".into(), kind: Dev, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "vizcraft-ui-egui"));

        let mut g = baseline("vizcraft");
        g[1].deps.push(Dep { name: "vizcraft-engine".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "vizcraft-macos-open"));

        let mut g = baseline("vizcraft");
        g[0].deps.push(Dep { name: "vizcraft-macos-open".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "vizcraft-engine"));
    }

    #[test]
    fn nothing_depends_on_xtask_and_unknown_crates_are_errors() {
        let mut g = baseline("vizcraft");
        g[4].deps.push(Dep { name: "xtask".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::DependsOnTooling { krate }] if krate == "vizcraft-cli"));

        let v = check(&[c("vizcraft-mystery", "/r/crates/mystery/Cargo.toml", &[])]);
        assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "vizcraft-mystery"));
    }

    #[test]
    fn mcp_sits_between_engine_and_apps() {
        let mut g = baseline("vizcraft");
        g.push(c("vizcraft-mcp", "/r/crates/mcp/Cargo.toml", &[("vizcraft-engine", Normal, true), ("serde_json", Normal, false)]));
        let idx = |g: &[Crate], name: &str| g.iter().position(|x| x.name == name).unwrap();
        let cli = idx(&g, "vizcraft-cli");
        g[cli].deps.push(Dep { name: "vizcraft-mcp".into(), kind: Normal, workspace: true });
        assert!(check(&g).is_empty(), "{:?}", check(&g));
        let mcp = idx(&g, "vizcraft-mcp");
        assert_eq!(classify(&g[mcp]), Some(Class::Mcp));

        g[mcp].deps.push(Dep { name: "vizcraft-ui-egui".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "vizcraft-mcp"));
        let v = check(&[c("vizcraft-mcp", "/r/crates/mcp/Cargo.toml", &[("egui", Normal, false)])]);
        assert!(matches!(&v[..], [Violation::EngineUsesUi { .. }]), "{v:?}");
    }

    #[test]
    fn self_dev_dependency_ignored() {
        assert!(check(&[c("vizcraft-engine", "/r/crates/engine/Cargo.toml", &[("vizcraft-engine", Dev, true)])]).is_empty());
    }

    #[test]
    fn metadata_parsing() {
        let meta: Value = serde_json::from_str(
            r#"{"packages":[
                {"name":"vizcraft-ui-egui","manifest_path":"/x/crates/ui-egui/Cargo.toml","dependencies":[
                    {"name":"vizcraft-engine","kind":null,"path":"/x/crates/engine"},
                    {"name":"egui","kind":null},
                    {"name":"egui_kittest","kind":"dev"}]},
                {"name":"vizcraft-engine","manifest_path":"/x/crates/engine/Cargo.toml","dependencies":[]}
            ]}"#,
        )
        .unwrap();
        let g = from_metadata(&meta).unwrap();
        assert_eq!(g.len(), 2);
        let ui = g.iter().find(|c| c.name == "vizcraft-ui-egui").unwrap();
        assert!(ui.deps[0].workspace && !ui.deps[1].workspace);
        assert_eq!(ui.deps[2].kind, Dev);
        assert!(check(&g).is_empty());
    }
}
