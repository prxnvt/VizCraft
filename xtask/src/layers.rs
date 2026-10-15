//! Dependency layering rules (docs/architecture.md, "Layers").
//!
//! The rules work on a small model built from `cargo metadata --no-deps` so they can be unit
//! tested without cargo. Crates are classified by name suffix and manifest path, so the same
//! rules hold for `protocraft-*` and for every craft made with `cargo xtask new-craft`.

use serde_json::Value;
use std::path::Path;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// L4: the model, undo, command registry; knows nothing about any UI toolkit.
    Engine,
    /// L6: the egui front end; uses egui but not a window host.
    UiEgui,
    /// Platform glue (macos-open): no workspace dependencies, no UI toolkit.
    Platform,
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

/// Everything the engine must stay clear of (the app-only set, egui, and the clipboard).
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

/// Classify a crate by its name suffix, falling back to its directory for apps.
pub fn classify(c: &Crate) -> Option<Class> {
    if c.name == "xtask" {
        return Some(Class::Tooling);
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
    let under_apps = Path::new(&c.manifest_path).components().any(|p| p.as_os_str() == "apps");
    if under_apps { Some(Class::App) } else { None }
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Engine) => "L4".into(),
        Some(Class::UiEgui) => "L6".into(),
        Some(Class::Platform) => "platform".into(),
        Some(Class::App) => "app".into(),
        Some(Class::Tooling) => "tooling".into(),
        None => "UNKNOWN".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A crate the rules cannot place: not `*-engine`, `*-ui-egui`, `*-macos-open`, `xtask` or under `apps/`.
    Unregistered { krate: String },
    /// The engine pulls in a UI toolkit or desktop service.
    EngineUsesUi { krate: String, dep: String },
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
                "{krate}: unknown workspace crate; name it <craft>-engine, <craft>-ui-egui, <craft>-macos-open, put it under apps/, or extend xtask/src/layers.rs"
            ),
            Violation::EngineUsesUi { krate, dep } => {
                write!(f, "{krate} (L4) depends on `{dep}`; the engine must not know about egui/eframe/winit/rfd/muda/arboard")
            }
            Violation::AppOnlyDep { krate, dep } => write!(f, "{krate} depends on `{dep}`; only apps/* may use eframe, winit, rfd, muda"),
            Violation::Upward { krate, dep } => {
                write!(f, "{krate} -> {dep}: dependencies must point down the layer table (engine <- ui-egui <- apps)")
            }
            Violation::DependsOnTooling { krate } => write!(f, "{krate} depends on xtask; tooling is not a library"),
        }
    }
}

/// Rank used for the direction check: a crate may only depend on strictly lower ranks.
fn rank(class: Class) -> u8 {
    match class {
        Class::Platform | Class::Engine => 0,
        Class::UiEgui => 1,
        Class::App => 2,
        Class::Tooling => 3,
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
                    Some(t) if class == Class::Platform || rank(t) >= rank(class) => {
                        out.push(Violation::Upward { krate: c.name.clone(), dep: d.name.clone() });
                    }
                    // Unknown dependencies are reported on their own entry.
                    _ => {}
                }
                continue;
            }
            match class {
                Class::Engine if forbidden_in_engine(&d.name) => out.push(Violation::EngineUsesUi { krate: c.name.clone(), dep: d.name.clone() }),
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
            c(&p("engine"), "/r/crates/engine/Cargo.toml", &[("serde", Normal, false), ("thiserror", Normal, false)]),
            c(&p("macos-open"), "/r/crates/macos-open/Cargo.toml", &[("objc2", Normal, false)]),
            c(
                &p("ui-egui"),
                "/r/crates/ui-egui/Cargo.toml",
                &[(&p("engine"), Normal, true), ("egui", Normal, false), ("egui_kittest", Dev, false), ("arboard", Normal, false)],
            ),
            c(
                prefix,
                "/r/apps/protocraft/Cargo.toml",
                &[
                    (&p("engine"), Normal, true),
                    (&p("ui-egui"), Normal, true),
                    ("eframe", Normal, false),
                    ("rfd", Normal, false),
                    ("muda", Normal, false),
                ],
            ),
            c(&p("cli"), "/r/apps/protocraft-cli/Cargo.toml", &[(&p("engine"), Normal, true)]),
            c("xtask", "/r/xtask/Cargo.toml", &[("serde_json", Normal, false)]),
        ]
    }

    #[test]
    fn baseline_graph_passes_for_any_craft_name() {
        for prefix in ["protocraft", "notecraft"] {
            let g = baseline(prefix);
            assert!(check(&g).is_empty(), "{prefix}: {:?}", check(&g));
        }
    }

    #[test]
    fn classification() {
        let g = baseline("protocraft");
        let classes: Vec<_> = g.iter().map(classify).collect();
        assert_eq!(
            classes,
            vec![Some(Class::Engine), Some(Class::Platform), Some(Class::UiEgui), Some(Class::App), Some(Class::App), Some(Class::Tooling)]
        );
        assert_eq!(classify(&c("protocraft-web", "/r/apps/protocraft-web/Cargo.toml", &[])), Some(Class::App));
        assert_eq!(classify(&c("protocraft-mystery", "/r/crates/mystery/Cargo.toml", &[])), None);
    }

    #[test]
    fn engine_must_not_use_ui_crates() {
        for dep in ["egui", "egui_extras", "eframe", "winit", "rfd", "muda", "arboard"] {
            let v = check(&[c("protocraft-engine", "/r/crates/engine/Cargo.toml", &[(dep, Normal, false)])]);
            assert!(matches!(&v[..], [Violation::EngineUsesUi { dep: d, .. }] if d == dep), "{dep}: {v:?}");
        }
        // Dev-dependencies count too: the engine's tests must stay headless.
        assert!(!check(&[c("protocraft-engine", "/r/crates/engine/Cargo.toml", &[("egui_kittest", Dev, false)])]).is_empty());
        assert!(check(&[c("protocraft-engine", "/r/crates/engine/Cargo.toml", &[("serde_json", Normal, false)])]).is_empty());
    }

    #[test]
    fn ui_egui_may_use_egui_but_not_a_window_host() {
        let ok = c(
            "protocraft-ui-egui",
            "/r/crates/ui-egui/Cargo.toml",
            &[("egui", Normal, false), ("egui_extras", Normal, false), ("arboard", Normal, false)],
        );
        assert!(check(&[ok]).is_empty());
        for dep in ["eframe", "winit", "rfd", "muda"] {
            let v = check(&[c("protocraft-ui-egui", "/r/crates/ui-egui/Cargo.toml", &[(dep, Normal, false)])]);
            assert!(matches!(&v[..], [Violation::AppOnlyDep { dep: d, .. }] if d == dep), "{dep}: {v:?}");
        }
    }

    #[test]
    fn platform_and_tooling_stay_clear_of_hosts() {
        assert!(!check(&[c("protocraft-macos-open", "/r/crates/macos-open/Cargo.toml", &[("winit", Normal, false)])]).is_empty());
        assert!(!check(&[c("xtask", "/r/xtask/Cargo.toml", &[("rfd", Normal, false)])]).is_empty());
    }

    #[test]
    fn upward_and_sideways_workspace_deps_flagged() {
        let mut g = baseline("protocraft");
        g[0].deps.push(Dep { name: "protocraft-ui-egui".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, dep }] if krate == "protocraft-engine" && dep == "protocraft-ui-egui"));

        let mut g = baseline("protocraft");
        g[2].deps.push(Dep { name: "protocraft".into(), kind: Dev, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "protocraft-ui-egui"));

        let mut g = baseline("protocraft");
        g[1].deps.push(Dep { name: "protocraft-engine".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "protocraft-macos-open"));

        let mut g = baseline("protocraft");
        g[0].deps.push(Dep { name: "protocraft-macos-open".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::Upward { krate, .. }] if krate == "protocraft-engine"));
    }

    #[test]
    fn nothing_depends_on_xtask_and_unknown_crates_are_errors() {
        let mut g = baseline("protocraft");
        g[4].deps.push(Dep { name: "xtask".into(), kind: Normal, workspace: true });
        assert!(matches!(&check(&g)[..], [Violation::DependsOnTooling { krate }] if krate == "protocraft-cli"));

        let v = check(&[c("protocraft-mystery", "/r/crates/mystery/Cargo.toml", &[])]);
        assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "protocraft-mystery"));
    }

    #[test]
    fn self_dev_dependency_ignored() {
        assert!(check(&[c("protocraft-engine", "/r/crates/engine/Cargo.toml", &[("protocraft-engine", Dev, true)])]).is_empty());
    }

    #[test]
    fn metadata_parsing() {
        let meta: Value = serde_json::from_str(
            r#"{"packages":[
                {"name":"protocraft-ui-egui","manifest_path":"/x/crates/ui-egui/Cargo.toml","dependencies":[
                    {"name":"protocraft-engine","kind":null,"path":"/x/crates/engine"},
                    {"name":"egui","kind":null},
                    {"name":"egui_kittest","kind":"dev"}]},
                {"name":"protocraft-engine","manifest_path":"/x/crates/engine/Cargo.toml","dependencies":[]}
            ]}"#,
        )
        .unwrap();
        let g = from_metadata(&meta).unwrap();
        assert_eq!(g.len(), 2);
        let ui = g.iter().find(|c| c.name == "protocraft-ui-egui").unwrap();
        assert!(ui.deps[0].workspace && !ui.deps[1].workspace);
        assert_eq!(ui.deps[2].kind, Dev);
        assert!(check(&g).is_empty());
    }
}
