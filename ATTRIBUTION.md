# Asset attribution

Every non-code asset in this repository (fonts, icons, images, sample files) is listed here with
its author, source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails if a file
under `assets/` is missing from this table, so the row is added in the same commit as the file.
Licence text files are listed too, so the table is the complete inventory of `assets/`.

**Policy (mandatory):** ProtoCraft, and every craft made from it, contains **no Microsoft, Adobe,
Avid or Autodesk iconography, images, templates, themes, clip art, fonts or other assets**. Every
asset is original work by the contributors or third-party material under an open licence (OSI
open source, public domain / CC0, or a Creative Commons licence that allows redistribution).
Screenshots of other vendors' software are never committed. A craft that needs more fonts adds
them through [storytold/craft-fonts](https://github.com/storytold/craft-fonts) (an optional
build input, `CRAFT_FONTS_DIR`), not to this folder.

Art drawn in code is original and has no file to list: the UI icon set
(`crates/ui-egui/src/icons.rs`), the sample document (`crates/engine/src/sample.rs`) and the
theme colours (`crates/ui-egui/src/theme.rs`).

The app icon is a placeholder for the baseline itself. A new craft replaces
`assets/app-icon/protocraft.svg` (renamed by `new-craft`) with its own drawing, reruns
`packaging/icons.sh`, and replaces the icon rows below.

| Asset | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/fonts/Inter-Regular.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font (copied from WordCraft, which carried it over from DesignCraft) |
| `assets/fonts/Inter-Medium.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font (copied from WordCraft, which carried it over from DesignCraft) |
| `assets/fonts/Inter-SemiBold.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font (copied from WordCraft, which carried it over from DesignCraft) |
| `assets/fonts/JetBrainsMono-Regular.ttf` | The JetBrains Mono Project Authors | https://github.com/JetBrains/JetBrainsMono | OFL-1.1 (`assets/fonts/OFL-JetBrainsMono.txt`) | Open-source typeface; monospace UI font (copied from WordCraft, which carried it over from DesignCraft) |
| `assets/fonts/OFL-Inter.txt` | (licence text) | upstream project | n/a | SIL Open Font License 1.1 text for Inter |
| `assets/fonts/OFL-JetBrainsMono.txt` | (licence text) | upstream project | n/a | SIL Open Font License 1.1 text for JetBrains Mono |
| `assets/app-icon/protocraft.svg` | ProtoCraft contributors | original artwork drawn by hand in SVG for this repository | MIT OR Apache-2.0 | Placeholder app icon: a rounded tile with a plinth of three bars and a block on top. Source of every rendered icon below |
| `assets/app-icon/protocraft-1024.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | 1024 px master of the full-bleed tile |
| `assets/app-icon/protocraft-macos-512.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | 512 px on the Apple icon grid (transparent margin); runtime window icon on macOS |
| `assets/app-icon/protocraft.icns` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | macOS bundle icon (16 to 512 px, 1x and 2x, Apple grid); copied into the .app by `packaging/macos/package.sh` |
| `assets/app-icon/protocraft.ico` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Windows icon (16, 20, 24, 32, 40, 48, 64, 128, 256 px), packed by `cargo xtask ico`; embedded by `apps/protocraft/build.rs` |
| `assets/app-icon/hicolor/16x16/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 16 px |
| `assets/app-icon/hicolor/24x24/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 24 px |
| `assets/app-icon/hicolor/32x32/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 32 px |
| `assets/app-icon/hicolor/48x48/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 48 px |
| `assets/app-icon/hicolor/64x64/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 64 px |
| `assets/app-icon/hicolor/128x128/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 128 px |
| `assets/app-icon/hicolor/256x256/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 256 px |
| `assets/app-icon/hicolor/512x512/apps/ai.storyteller.protocraft.png` | ProtoCraft contributors | rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 512 px |
| `assets/app-icon/hicolor/scalable/apps/ai.storyteller.protocraft.svg` | ProtoCraft contributors | copy of `assets/app-icon/protocraft.svg` made by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor scalable icon |
| `packaging/linux/ai.storyteller.protocraft.desktop` | ProtoCraft contributors | written for this repository | MIT OR Apache-2.0 | Linux desktop entry (see `packaging/README.md`) |

Rendered icon files: `packaging/icons.sh` writes every `assets/app-icon/` file listed above except
the SVG. Rerun it after changing the SVG; the rows stay valid because the paths do not change. A
craft that renames the icon (`new-craft` does) gets the rows renamed with it.
