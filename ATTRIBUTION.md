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

Rendered icon files: `packaging/icons.sh` writes `assets/app-icon/protocraft-1024.png`,
`protocraft-macos-512.png`, `protocraft.icns`, `protocraft.ico` (once an `ico` xtask exists) and the
Linux hicolor set `assets/app-icon/hicolor/<size>/apps/ai.storyteller.protocraft.png` for the
sizes 16, 24, 32, 48, 64, 128, 256 and 512, plus `hicolor/scalable/apps/ai.storyteller.protocraft.svg`.
The rasteriser was not installed when the baseline was made, so none of them is committed yet.
Whoever runs the script adds one row per file in the same commit, with author "ProtoCraft
contributors", source "rendered from `assets/app-icon/protocraft.svg` by `packaging/icons.sh`" and
licence MIT OR Apache-2.0, in the table above.
