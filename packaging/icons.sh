#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/vizcraft.svg.
#
# Needs an SVG rasteriser: rsvg-convert (brew install librsvg) or resvg (brew install resvg /
# cargo install resvg). On a Mac without either, Quick Look (qlmanage) renders the SVG once at
# 1024 px and sips scales that master down to every other size; iconutil (ships with the Xcode
# command line tools) writes the .icns. The outputs are committed, so builds and packaging never
# need these tools; run this only after changing the SVG, and add a row to ATTRIBUTION.md for
# every file it writes.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
NAME="vizcraft"
SVG="$DIR/$NAME.svg"
ID="ai.storyteller.$NAME"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# render <svg> <size> <png>
if command -v rsvg-convert >/dev/null; then
  render() { rsvg-convert -w "$2" -h "$2" -o "$3" "$1"; }
elif command -v resvg >/dev/null; then
  render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }
elif command -v qlmanage >/dev/null && command -v sips >/dev/null; then
  # Quick Look renders one 1024 master per SVG (named <file>.png next to it in $TMP/ql) and
  # sips resamples it; sizes above 1024 would be upscaled, which nothing here asks for.
  mkdir -p "$TMP/ql"
  render() {
    local master="$TMP/ql/$(basename "$1").png"
    if [ ! -f "$master" ]; then
      qlmanage -t -s 1024 -o "$TMP/ql" "$1" >/dev/null 2>&1
      [ -f "$master" ] || { echo "error: qlmanage did not render $1" >&2; return 1; }
    fi
    if [ "$2" = 1024 ]; then
      cp "$master" "$3"
    else
      sips -s format png -z "$2" "$2" "$master" --out "$3" >/dev/null
    fi
  }
else
  echo "error: no SVG rasteriser found (brew install librsvg, or brew install resvg; macOS: qlmanage + sips)" >&2
  exit 1
fi

# The artwork is a full-bleed 512 tile (rx=112). Windows and Linux use it as is. macOS icons sit on
# Apple's grid: an 824/1024 body with a transparent margin, so widen the viewBox to 512*1024/824.
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62.136 -62.136 636.272 636.272"/' "$SVG" >"$MAC"
grep -q 'viewBox="-62.136' "$MAC" || { echo "error: expected viewBox=\"0 0 512 512\" in $SVG" >&2; exit 1; }

render "$SVG" 1024 "$DIR/$NAME-1024.png"
# Runtime window/Dock icon on macOS (embedded by apps/vizcraft); other platforms use hicolor 256.
render "$MAC" 512 "$DIR/$NAME-macos-512.png"

# Linux hicolor theme.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$SVG" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico (PNG-compressed entries, packed by the `ico` xtask).
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$SVG" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/$NAME.ico" "${ICO_PNGS[@]}")

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/$NAME.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/$NAME.icns" "$SET"
else
  echo "warning: iconutil not found (macOS only); $NAME.icns not regenerated" >&2
fi
echo "icons written to $DIR"
