#!/bin/sh
# Prints shell exports for a C compiler and archiver that target wasm32-unknown-unknown, or fails.
# Usage: eval "$(scripts/wasm-toolchain.sh)"
# The SQLite library behind the persist crate is C code compiled for the browser (ADR 0006). Both
# tools must come from LLVM: Apple clang may lack the WebAssembly backend, and only llvm-ar
# archives wasm objects correctly (.cargo/config.toml defaults to it, from PATH).
set -eu

# Values already exported take priority, as they do for cargo.
if [ -n "${CC_wasm32_unknown_unknown:-}" ] && [ -n "${AR_wasm32_unknown_unknown:-}" ]; then
  exit 0
fi

# Prints "clang llvm-ar" when $1 compiles for wasm32 and has llvm-ar among its own tools.
probe() {
  echo 'int x;' | "$1" --target=wasm32-unknown-unknown -x c -c -o /dev/null - >/dev/null 2>&1 || return 1
  ar=$("$1" -print-prog-name=llvm-ar 2>/dev/null) || return 1
  case "$ar" in */*) [ -x "$ar" ] || [ -x "$ar.exe" ] || return 1 ;; *) return 1 ;; esac
  printf "CC_wasm32_unknown_unknown='%s'; export CC_wasm32_unknown_unknown\n" "$1"
  printf "AR_wasm32_unknown_unknown='%s'; export AR_wasm32_unknown_unknown\n" "$ar"
}

probe clang && exit 0
# Homebrew's LLVM is keg-only, so it is not on PATH by default.
if brew=$(brew --prefix 2>/dev/null); then
  for formula in llvm llvm@18; do
    probe "$brew/opt/$formula/bin/clang" && exit 0
  done
fi
echo "wasm-toolchain: no clang with the WebAssembly backend and llvm-ar found; install LLVM (macOS: brew install llvm)" >&2
exit 1
