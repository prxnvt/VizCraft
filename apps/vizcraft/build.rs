//! Records the git commit the binary was built from (`VIZCRAFT_GIT_COMMIT`, for the performance
//! trace's session header; `unknown` outside a checkout), and on Windows embeds the app icon and
//! version info (VERSIONINFO) into `vizcraft.exe`, so it shows in Explorer, the taskbar, the
//! Start menu and Alt-Tab.
//!
//! On every other target the resources step does nothing. A missing resource compiler is a
//! warning, so a cross-compile from macOS or Linux still links, unless `VIZCRAFT_REQUIRE_WINRES=1`
//! turns it into an error (for release builds).

/// `git rev-parse` as text, or `None` when git or the checkout is missing.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/vizcraft.ico");
    println!("cargo:rerun-if-env-changed=VIZCRAFT_REQUIRE_WINRES");
    // Rebuild when HEAD moves (a worktree's HEAD lives wherever git says it does).
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=VIZCRAFT_GIT_COMMIT={commit}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/vizcraft.ico")
        .set("ProductName", "VizCraft")
        .set("FileDescription", "VizCraft desktop app")
        .set("CompanyName", "prxnvt")
        .set("LegalCopyright", "Copyright (c) 2026 prxnvt. MIT OR Apache-2.0.")
        .set("OriginalFilename", "vizcraft.exe")
        .set("InternalName", "vizcraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("VIZCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=vizcraft.exe built without icon/version resources: {e}");
    }
}
