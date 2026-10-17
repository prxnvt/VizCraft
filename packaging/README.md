# Packaging

This folder holds what turns the built binaries into something people install: the icon
script, the macOS bundle and its package script, and the Linux desktop entry. Windows has no
script yet; see [Windows](#windows). The macOS parts are adapted from WordCraft's.

| File | Job |
|---|---|
| `icons.sh` | Renders every icon size from `assets/app-icon/vizcraft.svg`: the 1024 master, the macOS-grid 512, the Linux hicolor set, the `.icns` and the `.ico` (through `cargo xtask ico`). Uses `rsvg-convert` or `resvg` when installed; on a Mac without either it falls back to Quick Look (`qlmanage`, one 1024 render per SVG) and `sips` for the smaller sizes, and `iconutil` writes the `.icns`. Outputs are committed and listed in `ATTRIBUTION.md`, so builds never need these tools. |
| `env.sh` | Sourced by the package scripts: `ROOT`, `VERSION` (from `[workspace.package]`), `DIST`, `VIZCRAFT_BUILD_SHA`, `CARGO_TARGET_DIR`, and the helpers that copy licences (including the craft-fonts ones) into a package. |
| `macos/Info.plist.in` | The bundle's `Info.plist` with `@VERSION@`, `@SHORT_VERSION@` and `@BUILD_SHA@` placeholders for `package.sh` to fill. Declares the `.craft.json` document type (`LSHandlerRank` Owner, with its exported UTI). |
| `macos/package.sh` | Builds the release binaries, assembles `VizCraft.app`, signs it (ad hoc by default), optionally notarises, and writes a DMG plus a zip of the CLI. `--arch universal|aarch64|x86_64`, `--skip-build`, `--no-dmg`. |
| `macos/entitlements.plist` | Hardened-runtime entitlements; empty on purpose (pure Rust, no JIT, not sandboxed). |
| `linux/ai.storyteller.vizcraft.desktop` | Freedesktop launcher entry; see [Linux](#linux). |

The `ico` xtask (`cargo xtask ico <out.ico> <in.png>...`, `xtask/src/ico.rs`) packs square PNGs of
at most 256 px into a Windows `.ico` with PNG-compressed entries; `icons.sh` calls it.
`apps/vizcraft/build.rs` embeds that `.ico` and a VERSIONINFO block into `vizcraft.exe` with
`winresource` when the target is Windows and does nothing elsewhere.

## The macOS bundle

`package.sh` assembles this layout under `$CARGO_TARGET_DIR/macos-package/`:

```
VizCraft.app/
  Contents/
    Info.plist              from macos/Info.plist.in with the placeholders substituted
    PkgInfo                 "APPL????"
    MacOS/
      VizCraft             the apps/vizcraft binary, renamed to CFBundleExecutable
    Resources/
      VizCraft.icns        assets/app-icon/vizcraft.icns, renamed to CFBundleIconFile
      OFL-<family>.txt      licences of craft-fonts fonts, only when built with CRAFT_FONTS_DIR
```

The executable and the icon carry the display name because `CFBundleExecutable` and
`CFBundleIconFile` in the template say `VizCraft`; the build artefacts keep their lowercase
crate names until they are copied in. The bundle id is `ai.storyteller.vizcraft`; the
preferences folder `~/Library/Application Support/VizCraft/` is derived from the display name
by the app, not from the bundle.

Signing is inside out: the executable first, then the bundle, both with the hardened runtime and
the entitlements file; the final signature is not `--deep`. With no `MACOS_SIGN_IDENTITY` the
script signs ad hoc, which is what a local build needs and what Gatekeeper on other Macs rejects.
Notarisation runs only with a real identity and `APPLE_ID`, `APPLE_PASSWORD` and
`APPLE_TEAM_ID` set.

The version comes from `[workspace.package] version` in the root `Cargo.toml`, read by
`packaging/env.sh`; `VIZCRAFT_VERSION` overrides it and `VIZCRAFT_BUILD_SHA` (default: `git
rev-parse HEAD`) is the commit written into the plist as `VizCraftBuildCommit`.

`package.sh` keeps the names in two places only: `CRATE=vizcraft` at the top (the GUI crate; the
CLI is `$CRATE-cli`) and `CFBundleName` in `Info.plist.in`, which it reads with PlistBuddy for the
bundle, executable and icon names. It refuses to package when `CFBundleExecutable` or
`CFBundleIconFile` differ from `CFBundleName`. `cargo xtask new-craft` renames all of them.

`CARGO_TARGET_DIR` is honoured when set (the bundle then lands under it); unset, it is
`<repo>/target`. To build and check a bundle by hand:

```sh
packaging/macos/package.sh --arch aarch64 --no-dmg
plutil -lint target/macos-package/VizCraft.app/Contents/Info.plist
codesign --verify --deep --strict --verbose=2 target/macos-package/VizCraft.app
target/macos-package/VizCraft.app/Contents/MacOS/VizCraft --sample
```

## What the local installer expects

`~/Workspace/crafts/rebuild.sh` builds every app folder in the crafts workspace and installs it
into `/Applications`, in place, so Dock pins survive. For a craft to work with it, the repository
must provide:

| Requirement | Why |
|---|---|
| An executable `packaging/macos/package.sh` | `rebuild.sh` only considers folders that have one, and runs it as `DIST=<repo>/target/dist packaging/macos/package.sh --arch aarch64`. The script must accept `--arch` and honour `DIST`. |
| `packaging/macos/Info.plist.in` with `CFBundleName` | `rebuild.sh` reads the name with PlistBuddy to know what the installed app is called (`VizCraft.app`) and whether it is running (`pgrep -x VizCraft`). |
| A `<CFBundleName>BuildCommit` key in the installed `Info.plist` | `rebuild.sh` compares it with the checkout's `HEAD` to skip apps that are up to date. The template's `VizCraftBuildCommit` is filled from `@BUILD_SHA@`; keep the key's prefix equal to `CFBundleName` after a rename. |
| The bundle at `target/macos-package/<CFBundleName>.app` | That is where `rebuild.sh` picks the built app up from, so `package.sh` must write its work directory under `CARGO_TARGET_DIR` (which `rebuild.sh` leaves at the default `target/`). |
| A signature that passes `codesign --verify --strict` | The installer verifies the copy in `/Applications` and rolls back to the previous build if it fails; ad-hoc signing passes. |
| `CFBundleShortVersionString` | Printed in the installer's success line. |
| Building without a signing identity | `rebuild.sh` unsets `MACOS_SIGN_IDENTITY`, `MACOS_KEYCHAIN`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`, so the script's defaults must produce an ad-hoc signed, non-notarised app. |
| Building with `CRAFT_FONTS_DIR` set | `rebuild.sh` exports it pointing at `crafts/craft-fonts`; the build may use it (extra fonts) and must not fail on it. |
| A git checkout | The installer uses `git status` and `git rev-parse HEAD` in the folder; `--update` rebases on `origin`. |

The installer also sets `CARGO_BUILD_JOBS` to leave cores free, writes the build log to
`crafts/.build-logs/<folder>.log`, and keeps the previously installed app in
`crafts/.previous-builds/`. Nothing in the craft needs to know about those.

### Adding a craft to the installer

`rebuild.sh` has no fixed app list and no dry run: it builds every folder directly under
`~/Workspace/crafts/` that has an executable `packaging/macos/package.sh`, and finds the app's
name in that folder's `packaging/macos/Info.plist.in`. Nothing in `rebuild.sh` changes for a new
craft. To add one:

1. Check the craft out as `~/Workspace/crafts/<folder>` (a git clone or worktree; the folder name
   is what `./rebuild.sh <folder>` takes, so use the crate name, e.g. `vizcraft`).
2. Optionally add a row to the table in `~/Workspace/crafts/README.md` (`| vizcraft | VizCraft |
   (baseline) |`); it is documentation only.
3. Run `./rebuild.sh --list` to see it, then `./rebuild.sh <folder>` to build and install it.

The baseline itself satisfies every row of the table above; the checks in
[The macOS bundle](#the-macos-bundle) are what the installer does after `package.sh`.

## After `new-craft`

The rename changes `VizCraft` to the new display name in the template (and therefore
`CFBundleName`, `CFBundleExecutable`, `CFBundleIconFile` and the `BuildCommit` key), the bundle
id to `ai.storyteller.<name>` and the icon file names. What a craft still has to do by hand:

- Draw its own icon SVG and rerun `icons.sh`.
- Replace the sample `.craft.json` document type (`CFBundleDocumentTypes` and the matching
  `UTExportedTypeDeclarations` entry) with the types it opens, keeping `LSHandlerRank` on each
  (`Owner` for its own format, `Alternate` for formats other apps own), and set
  `LSApplicationCategoryType`. Mirror the change in the Linux desktop entry's `MimeType=` and in
  `build.rs`'s `FileDescription`.
- Check `NSHumanReadableCopyright`.

## Linux

There is no Linux package script yet. The pieces a `.deb`/`.rpm` (nfpm) or Flatpak needs are in
place and follow the freedesktop layout, so a package only has to copy them:

| Source | Installed as |
|---|---|
| `target/release/vizcraft`, `target/release/vizcraft-cli` | `/usr/bin/vizcraft`, `/usr/bin/vizcraft-cli` |
| `packaging/linux/ai.storyteller.vizcraft.desktop` | `/usr/share/applications/ai.storyteller.vizcraft.desktop` |
| `assets/app-icon/hicolor/<size>/apps/ai.storyteller.vizcraft.png` (16 to 512) | `/usr/share/icons/hicolor/<size>/apps/` |
| `assets/app-icon/hicolor/scalable/apps/ai.storyteller.vizcraft.svg` | `/usr/share/icons/hicolor/scalable/apps/` |

The desktop entry's `Icon=` and `StartupWMClass=` are the bundle id, which is also the hicolor
file name; the app must set its window class to the same id for the taskbar to pick the icon up.
A real craft replaces `GenericName`, `Comment`, `Categories`, `Keywords` and `MimeType` (and ships
a `<id>.mime.xml` under `/usr/share/mime/packages/` when the format is its own, plus an AppStream
`<id>.metainfo.xml` for software centres). After installing, run `update-desktop-database` and
`gtk-update-icon-cache /usr/share/icons/hicolor`. WordCraft's `packaging/linux/` (nfpm.yaml,
postinst.sh, flatpak manifests) is the reference when the script is written.

## Windows

There is no Windows package script yet. What exists:

- `assets/app-icon/vizcraft.ico` (16 to 256 px) and `apps/vizcraft/build.rs`, which embeds it
  and the VERSIONINFO (product name, description, copyright) into `vizcraft.exe` through
  `winresource`. A cross-compile from macOS or Linux without a resource compiler only warns;
  `VIZCRAFT_REQUIRE_WINRES=1` makes that an error for release builds.
- `cargo build --release --target x86_64-pc-windows-msvc -p vizcraft -p vizcraft-cli` on a
  Windows runner produces the two executables; a zip of them with the licences is the minimum
  release artefact.

An installer (WiX `.wxs` + `signtool`, as in WordCraft's `packaging/windows/`) registers the
`.craft.json` extension and the Start menu entry; it is not part of the baseline.
