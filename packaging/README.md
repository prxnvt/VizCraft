# Packaging

This folder holds what turns the built binaries into something people install: the icon
script and the macOS bundle template today; the macOS package script, and the Linux and Windows
packaging, arrive with Cascade 3 of the [roadmap](../ROADMAP.md), adapted from WordCraft's.

| File | Job |
|---|---|
| `icons.sh` | Renders every icon size from `assets/app-icon/protocraft.svg`: the 1024 master, the macOS-grid 512, the Linux hicolor set, the `.icns` (and the `.ico` once an `ico` xtask exists). Needs `rsvg-convert` or `resvg`, and `iconutil` on macOS. Outputs are committed and listed in `ATTRIBUTION.md`, so builds never need these tools. |
| `macos/Info.plist.in` | The bundle's `Info.plist` with `@VERSION@`, `@SHORT_VERSION@` and `@BUILD_SHA@` placeholders for `package.sh` to fill. |
| `macos/package.sh` (Cascade 3) | Builds the release binaries, assembles `ProtoCraft.app`, signs it (ad hoc by default), optionally notarises, and writes a DMG plus a zip of the CLI. |
| `macos/entitlements.plist` (Cascade 3) | Hardened-runtime entitlements; empty on purpose (pure Rust, no JIT, not sandboxed). |

## The macOS bundle

`package.sh` assembles this layout under `$CARGO_TARGET_DIR/macos-package/`:

```
ProtoCraft.app/
  Contents/
    Info.plist              from macos/Info.plist.in with the placeholders substituted
    PkgInfo                 "APPL????"
    MacOS/
      ProtoCraft             the apps/protocraft binary, renamed to CFBundleExecutable
    Resources/
      ProtoCraft.icns        assets/app-icon/protocraft.icns, renamed to CFBundleIconFile
      OFL-<family>.txt      licences of craft-fonts fonts, only when built with CRAFT_FONTS_DIR
```

The executable and the icon carry the display name because `CFBundleExecutable` and
`CFBundleIconFile` in the template say `ProtoCraft`; the build artefacts keep their lowercase
crate names until they are copied in. The bundle id is `ai.storyteller.protocraft`; the
preferences folder `~/Library/Application Support/ProtoCraft/` is derived from the display name
by the app, not from the bundle.

Signing is inside out: the executable first, then the bundle, both with the hardened runtime and
the entitlements file; the final signature is not `--deep`. With no `MACOS_SIGN_IDENTITY` the
script signs ad hoc, which is what a local build needs and what Gatekeeper on other Macs rejects.
Notarisation runs only with a real identity and `APPLE_ID`, `APPLE_PASSWORD` and
`APPLE_TEAM_ID` set.

The version comes from `[workspace.package] version` in the root `Cargo.toml`, read by
`packaging/env.sh` (also Cascade 3); `PROTOCRAFT_VERSION` overrides it and `PROTOCRAFT_BUILD_SHA`
is the commit baked into the binaries and the plist.

## What the local installer expects

`~/Workspace/crafts/rebuild.sh` builds every app folder in the crafts workspace and installs it
into `/Applications`, in place, so Dock pins survive. For a craft to work with it, the repository
must provide:

| Requirement | Why |
|---|---|
| An executable `packaging/macos/package.sh` | `rebuild.sh` only considers folders that have one, and runs it as `DIST=<repo>/target/dist packaging/macos/package.sh --arch aarch64`. The script must accept `--arch` and honour `DIST`. |
| `packaging/macos/Info.plist.in` with `CFBundleName` | `rebuild.sh` reads the name with PlistBuddy to know what the installed app is called (`ProtoCraft.app`) and whether it is running (`pgrep -x ProtoCraft`). |
| A `<CFBundleName>BuildCommit` key in the installed `Info.plist` | `rebuild.sh` compares it with the checkout's `HEAD` to skip apps that are up to date. The template's `ProtoCraftBuildCommit` is filled from `@BUILD_SHA@`; keep the key's prefix equal to `CFBundleName` after a rename. |
| The bundle at `target/macos-package/<CFBundleName>.app` | That is where `rebuild.sh` picks the built app up from, so `package.sh` must write its work directory under `CARGO_TARGET_DIR` (which `rebuild.sh` leaves at the default `target/`). |
| A signature that passes `codesign --verify --strict` | The installer verifies the copy in `/Applications` and rolls back to the previous build if it fails; ad-hoc signing passes. |
| `CFBundleShortVersionString` | Printed in the installer's success line. |
| Building without a signing identity | `rebuild.sh` unsets `MACOS_SIGN_IDENTITY`, `MACOS_KEYCHAIN`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`, so the script's defaults must produce an ad-hoc signed, non-notarised app. |
| Building with `CRAFT_FONTS_DIR` set | `rebuild.sh` exports it pointing at `crafts/craft-fonts`; the build may use it (extra fonts) and must not fail on it. |
| A git checkout | The installer uses `git status` and `git rev-parse HEAD` in the folder; `--update` rebases on `origin`. |

The installer also sets `CARGO_BUILD_JOBS` to leave cores free, writes the build log to
`crafts/.build-logs/<folder>.log`, and keeps the previously installed app in
`crafts/.previous-builds/`. Nothing in the craft needs to know about those.

## After `new-craft`

The rename changes `ProtoCraft` to the new display name in the template (and therefore
`CFBundleName`, `CFBundleExecutable`, `CFBundleIconFile` and the `BuildCommit` key), the bundle
id to `ai.storyteller.<name>` and the icon file names. What a craft still has to do by hand:

- Draw its own icon SVG and rerun `icons.sh`.
- Declare the document types it opens (`CFBundleDocumentTypes`, and `UTExportedTypeDeclarations`
  for its own format) and set `LSApplicationCategoryType`.
- Check `NSHumanReadableCopyright`.
