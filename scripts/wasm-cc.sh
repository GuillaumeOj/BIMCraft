#!/bin/sh
# Prints a C compiler that can target wasm32-unknown-unknown, or fails.
# The SQLite library behind the persist crate is C code compiled for the browser (ADR 0005); older
# Apple clang releases lack the WebAssembly backend, so Homebrew LLVM is tried after `clang`.
set -eu

if [ -n "${CC_wasm32_unknown_unknown:-}" ]; then
  echo "$CC_wasm32_unknown_unknown"
  exit 0
fi

candidates=clang
if command -v brew >/dev/null 2>&1; then
  for formula in llvm llvm@18; do
    prefix=$(brew --prefix "$formula" 2>/dev/null || true)
    [ -n "$prefix" ] && candidates="$candidates $prefix/bin/clang"
  done
fi

for cc in $candidates; do
  if echo 'int x;' | "$cc" --target=wasm32-unknown-unknown -x c -c -o /dev/null - >/dev/null 2>&1; then
    echo "$cc"
    exit 0
  fi
done
echo "wasm-cc: no clang with the WebAssembly backend found; install LLVM (macOS: brew install llvm)" >&2
exit 1
