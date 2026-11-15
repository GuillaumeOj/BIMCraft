#!/bin/sh
# Builds the viewport for the browser (ADR 0026): a release build of the `viewport` crate for
# wasm32-unknown-unknown, then wasm-bindgen's `web` bindings into ui/src/viewport/pkg/ (not
# committed). The UI imports that folder only through ui/src/viewport/.
# Usage: scripts/build-viewport.sh            builds, and prints the size of the module
# Needs wasm-bindgen-cli at the version of scripts/tool-versions.env (scripts/setup.sh checks it).
set -eu

root=$(git rev-parse --show-toplevel)
cd "$root"
# shellcheck source=scripts/tool-versions.env
. scripts/tool-versions.env

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "build-viewport: wasm-bindgen not found: cargo install --locked wasm-bindgen-cli@$WASM_BINDGEN_VERSION" >&2
  exit 1
fi
found=$(wasm-bindgen --version | cut -d' ' -f2)
if [ "$found" != "$WASM_BINDGEN_VERSION" ]; then
  echo "build-viewport: wasm-bindgen $found found, the viewport needs $WASM_BINDGEN_VERSION (the version of the wasm-bindgen crate in Cargo.lock): cargo install --locked wasm-bindgen-cli@$WASM_BINDGEN_VERSION" >&2
  exit 1
fi

out=ui/src/viewport/pkg
cargo build --package viewport --target wasm32-unknown-unknown --release --locked
rm -rf "$out"
wasm-bindgen --target web --typescript --out-dir "$out" --out-name viewport \
  target/wasm32-unknown-unknown/release/viewport.wasm

wasm="$out/viewport_bg.wasm"
printf 'viewport module: %s bytes, %s bytes gzipped\n' \
  "$(wc -c <"$wasm" | tr -d ' ')" "$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')"
