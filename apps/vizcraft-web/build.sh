#!/bin/sh
# Build the VizCraft web shell into dist/web without trunk or wasm-pack.
#
#   apps/vizcraft-web/build.sh [--debug]
#
# Steps: cargo build for wasm32-unknown-unknown, then the wasm-bindgen CLI (the version that
# matches the wasm-bindgen crate in Cargo.lock) writes the JS glue and the .wasm next to a copy
# of index.html. Serve dist/web with any static file server, for example
# `python3 -m http.server -d dist/web 8772`. If the wasm-bindgen CLI is missing the script says
# how to install it and exits 1; `trunk build --release` from this directory works too.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
target_dir=${CARGO_TARGET_DIR:-$root/target}
profile=release
flag=--release
if [ "${1:-}" = "--debug" ]; then
  profile=debug
  flag=
fi
dist=$root/dist/web

if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  echo "build.sh: the wasm32 target is missing: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  want=$(grep -A1 '^name = "wasm-bindgen"$' "$root/Cargo.lock" | sed -n 's/^version = "\(.*\)"/\1/p')
  echo "build.sh: the wasm-bindgen CLI is not installed. Install the version Cargo.lock uses:" >&2
  echo "    cargo install wasm-bindgen-cli --version ${want:-<see Cargo.lock>}" >&2
  echo "or build with trunk instead: cd apps/vizcraft-web && trunk build --release" >&2
  exit 1
fi

(cd "$root" && cargo build $flag --target wasm32-unknown-unknown -p vizcraft-web)
wasm=$target_dir/wasm32-unknown-unknown/$profile/vizcraft-web.wasm
rm -rf "$dist"
mkdir -p "$dist"
wasm-bindgen "$wasm" --out-dir "$dist" --out-name vizcraft-web --target web --no-typescript
if command -v wasm-opt >/dev/null 2>&1 && [ "$profile" = release ]; then
  wasm-opt -Oz -o "$dist/vizcraft-web_bg.wasm" "$dist/vizcraft-web_bg.wasm"
fi
# index.html without the trunk directive, plus the two-line module loader trunk would inject.
sed -e '/data-trunk/d' \
    -e 's|</body>|  <script type="module">import init from "./vizcraft-web.js"; init();</script>\n</body>|' \
    "$here/index.html" > "$dist/index.html"
echo "built $dist ($(du -sh "$dist" | cut -f1)); serve it with: python3 -m http.server -d $dist 8772"
