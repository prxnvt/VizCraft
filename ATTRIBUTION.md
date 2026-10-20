# Asset attribution

Every non-code asset in this repository (fonts, icons, images, sample files) is listed here with
its author, source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails if a file
under `assets/` is missing from this table, so the row is added in the same commit as the file.
Licence text files are listed too, so the table is the complete inventory of `assets/`.

**Policy (mandatory):** VizCraft contains **no Microsoft, Adobe, Avid or Autodesk iconography,
images, templates, themes, clip art, fonts or other assets**. Every asset is original work by the
contributors or third-party material under an open licence (OSI open source, public domain / CC0,
or a Creative Commons licence that allows redistribution). Diagram stencils are drawn in code and
original; vendor product and brand icons (cloud provider icons, logos) are never included, even
under a licence that seems to allow it. Screenshots of other vendors' software are never
committed. Extra fonts come through [storytold/craft-fonts](https://github.com/storytold/craft-fonts)
(an optional build input, `CRAFT_FONTS_DIR`), not into this folder.

Art drawn in code is original and has no file to list: the UI icon set
(`crates/ui-egui/src/icons.rs`), the sample document (`crates/engine/src/sample.rs`) and the
theme colours (`crates/ui-egui/src/theme.rs`).

| Asset | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/fonts/Inter-Regular.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font |
| `assets/fonts/Inter-Medium.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font |
| `assets/fonts/Inter-SemiBold.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; UI font |
| `assets/fonts/Inter-Italic.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter (release v4.0, `extras/ttf/Inter-Italic.ttf`) | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface; the italic face for PNG and PDF export (usvg does not synthesise oblique) |
| `assets/fonts/JetBrainsMono-Regular.ttf` | The JetBrains Mono Project Authors | https://github.com/JetBrains/JetBrainsMono | OFL-1.1 (`assets/fonts/OFL-JetBrainsMono.txt`) | Open-source typeface; monospace UI font |
| `assets/fonts/OFL-Inter.txt` | (licence text) | upstream project | n/a | SIL Open Font License 1.1 text for Inter |
| `assets/fonts/OFL-JetBrainsMono.txt` | (licence text) | upstream project | n/a | SIL Open Font License 1.1 text for JetBrains Mono |
| `assets/app-icon/vizcraft.svg` | prxnvt | original artwork drawn by hand in SVG for this repository | MIT OR Apache-2.0 | VizCraft app icon: a deep violet rounded tile holding a tiny flowchart (a cream rounded box, an amber diamond and a coral circle joined by two elbow connectors with arrowheads). Source of every rendered icon below |
| `assets/app-icon/vizcraft-1024.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | 1024 px master of the full-bleed tile with the VizCraft flowchart icon |
| `assets/app-icon/vizcraft-macos-512.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | 512 px the VizCraft flowchart icon on the Apple icon grid (transparent margin); runtime window icon on macOS |
| `assets/app-icon/vizcraft.icns` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | macOS bundle icon with the VizCraft flowchart icon (16 to 512 px, 1x and 2x, Apple grid); copied into the .app by `packaging/macos/package.sh` |
| `assets/app-icon/vizcraft.ico` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Windows icon with the VizCraft flowchart icon (16, 20, 24, 32, 40, 48, 64, 128, 256 px), packed by `cargo xtask ico`; embedded by `apps/vizcraft/build.rs` |
| `assets/app-icon/hicolor/16x16/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 16 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/24x24/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 24 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/32x32/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 32 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/48x48/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 48 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/64x64/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 64 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/128x128/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 128 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/256x256/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 256 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/512x512/apps/io.github.prxnvt.vizcraft.png` | prxnvt | rendered from `assets/app-icon/vizcraft.svg` by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor icon, 512 px: the VizCraft flowchart icon |
| `assets/app-icon/hicolor/scalable/apps/io.github.prxnvt.vizcraft.svg` | prxnvt | copy of `assets/app-icon/vizcraft.svg` made by `packaging/icons.sh` | MIT OR Apache-2.0 | Linux hicolor scalable icon: the VizCraft flowchart icon |
| `assets/dictionaries/en-US.txt` | Kevin Atkinson and the SCOWL contributors (Spell Checker Oriented Word Lists) | http://wordlist.aspell.net/ , release 2020.12.07, https://github.com/en-wl/wordlist | MIT-like permissive licence (`assets/dictionaries/SCOWL-Copyright.txt`) | American English word list for spelling checks: SCOWL size 60, the `english-*`, `american-*` words, upper and contractions lists at sizes 10 to 60 (no abbreviations, no proper-name lists, no British, Canadian or Australian variants), reduced to lower-case ASCII words and apostrophes, sorted and de-duplicated. The licence grants use, copy, modify and distribute for any purpose without fee, provided the copyright and permission notice appear in all copies; the third-party notices in that file cover the public-domain Moby, 12Dicts and UK frequency sources and the WordNet licence. Read from the SCOWL distribution itself |
| `assets/dictionaries/SCOWL-Copyright.txt` | (licence text) | the `Copyright` file of the SCOWL 2020.12.07 distribution | n/a | Verbatim SCOWL copyright and permission notice that must accompany the word list |
| `packaging/linux/io.github.prxnvt.vizcraft.desktop` | prxnvt | written for this repository | MIT OR Apache-2.0 | Linux desktop entry (see `packaging/README.md`) |

Rendered icon files: `packaging/icons.sh` writes every `assets/app-icon/` file listed above except
the SVG. Rerun it after changing the SVG; the rows stay valid because the paths do not change.
