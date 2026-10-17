//! Windows only: embed the app icon and version info (VERSIONINFO) into `vizcraft.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `VIZCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/vizcraft.ico");
    println!("cargo:rerun-if-env-changed=VIZCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/vizcraft.ico")
        .set("ProductName", "VizCraft")
        .set("FileDescription", "VizCraft desktop app")
        .set("CompanyName", "VizCraft contributors")
        .set("LegalCopyright", "Copyright (c) 2026 VizCraft contributors. MIT OR Apache-2.0.")
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
