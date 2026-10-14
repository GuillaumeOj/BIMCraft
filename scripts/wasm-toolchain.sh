#!/bin/sh
# Prints shell exports for a C compiler and archiver that target wasm32-unknown-unknown, or fails.
# Usage: eval "$(scripts/wasm-toolchain.sh)"
# The SQLite library behind the persist crate is C code compiled for the browser (ADR 0005). Both
# tools must come from LLVM: Apple clang may lack the WebAssembly backend, and without `llvm-ar`
# the build falls back to the system `ar`, which silently drops wasm objects (Apple) or writes no
# symbol index (GNU), so the build passes but the archive cannot be linked.
set -eu

quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
emit() {
  echo "CC_wasm32_unknown_unknown=$(quote "$1"); export CC_wasm32_unknown_unknown"
  echo "AR_wasm32_unknown_unknown=$(quote "$2"); export AR_wasm32_unknown_unknown"
}

if [ -n "${CC_wasm32_unknown_unknown:-}" ] && [ -n "${AR_wasm32_unknown_unknown:-}" ]; then
  emit "$CC_wasm32_unknown_unknown" "$AR_wasm32_unknown_unknown"
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
  echo 'int x;' | "$cc" --target=wasm32-unknown-unknown -x c -c -o /dev/null - >/dev/null 2>&1 || continue
  # clang answers with a full path only when llvm-ar sits among its own tools.
  ar=$("$cc" -print-prog-name=llvm-ar 2>/dev/null || true)
  case "$ar" in */*) [ -x "$ar" ] || [ -x "$ar.exe" ] || continue ;; *) continue ;; esac
  emit "$cc" "$ar"
  exit 0
done
echo "wasm-toolchain: no clang with the WebAssembly backend and llvm-ar found; install LLVM (macOS: brew install llvm)" >&2
exit 1
