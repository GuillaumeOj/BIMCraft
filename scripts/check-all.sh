#!/bin/sh
# Runs the project checks (AGENTS.md §9). CI calls the same sections, so local and remote never drift.
# Usage: scripts/check-all.sh [section...]   sections: repo rust coverage wasm fuzz ui e2e docs (default: all)
# A section is skipped only when the code it checks does not exist yet.
# CHECK_CRATES="a b" limits the Rust sections to those crates (set by .githooks/pre-push from
# scripts/affected.ts); unset, they cover the whole workspace, as in CI.
set -eu

root=$(git rev-parse --show-toplevel)
cd "$root"

step() { printf '\n==> %s\n' "$1"; }
require() {
  command -v "$1" >/dev/null 2>&1 && return 0
  echo "check-all: '$1' is required. Run scripts/setup.sh to see how to install it." >&2
  exit 1
}
has_rust() { ls crates/*/Cargo.toml >/dev/null 2>&1; }
has_ui() { [ -f ui/package.json ]; }
has_fuzz() { [ -f fuzz/Cargo.toml ]; }
in_scope() { [ -z "${CHECK_CRATES:-}" ] || case " $CHECK_CRATES " in *" $1 "*) true ;; *) false ;; esac; }
packages() { if [ -n "${CHECK_CRATES:-}" ]; then printf -- '--package %s ' $CHECK_CRATES; else echo --workspace; fi; }
existing_crates() { while read -r crate; do [ -f "crates/$crate/Cargo.toml" ] && echo "$crate"; done < "$1"; }

check_repo() {
  step "Repository: spelling, TOML, secrets, pinned versions, crate layering"
  require bun; require typos; require taplo; require gitleaks
  typos
  taplo fmt --check
  gitleaks git --no-banner --redact
  bun scripts/check-pinned-deps.ts
  bun scripts/check-architecture.ts
  (cd scripts && bun test)
  if command -v actionlint >/dev/null 2>&1; then actionlint; fi
}

check_rust() {
  has_rust || { step "Rust: no crates yet, skipped"; return; }
  step "Rust: format, lint, tests, dependency audit"
  require cargo-deny
  cargo fmt --all --check
  cargo clippy $(packages) --all-targets --all-features --locked -- -D warnings
  cargo test $(packages) --all-features --locked
  cargo deny --locked check
}

check_coverage() {
  has_rust || { step "Rust coverage: no crates yet, skipped"; return; }
  step "Rust coverage: 90 % workspace (full runs only), 98 % critical crates in scope"
  require cargo-llvm-cov
  # The 90 % gate is a workspace total, so a run scoped by CHECK_CRATES cannot check it.
  [ -n "${CHECK_CRATES:-}" ] ||
    cargo llvm-cov --workspace --all-features --locked --fail-under-lines 90 --lcov --output-path lcov.info
  for crate in $(existing_crates scripts/critical-crates.txt); do
    in_scope "$crate" || continue
    cargo llvm-cov --package "$crate" --all-features --locked --fail-under-lines 98
  done
}

check_wasm() {
  has_rust || { step "wasm32: no crates yet, skipped"; return; }
  step "wasm32: core crates build for the browser (ARCH-01)"
  for crate in $(existing_crates scripts/wasm-crates.txt); do
    in_scope "$crate" || continue
    cargo build --package "$crate" --target wasm32-unknown-unknown --all-features --locked
  done
  step "wasm32: geometry results match the native golden output (ENG-04)"
  require wasmtime
  # wasm32 aborts on panic, which hides captured output; --nocapture keeps the diverging value.
  cargo test --package geom --test golden --target wasm32-wasip1 --locked -- --nocapture
}

# Bounded and seeded, so a run is reproducible and fits in CI; longer runs: docs/dev/quality.md.
check_fuzz() {
  has_fuzz || { step "Fuzzing: no fuzz targets yet, skipped"; return; }
  step "Fuzzing: parsers survive hostile input (ENG-02)"
  require cargo-fuzz; require cargo-deny
  (
    # Inside fuzz/, rustup picks the nightly pinned in fuzz/rust-toolchain.toml.
    cd fuzz
    cargo fmt --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo deny --locked check
    for target in $(cargo fuzz list --fuzz-dir .); do
      corpus=$(mktemp -d)
      cargo fuzz run --fuzz-dir . "$target" "$corpus" "seeds/$target" -- \
        -runs="${FUZZ_RUNS:-200000}" -seed=1 -timeout=10
      rm -rf "$corpus"
    done
  )
}

check_ui() {
  has_ui || { step "UI: no package.json yet, skipped"; return; }
  step "UI: lint, format, imports, types, unit tests + coverage, audit"
  require bun
  (
    cd ui
    bun install --frozen-lockfile
    bunx biome ci .
    bunx tsc --noEmit
    bunx vitest run --coverage
    bun audit
  )
}

check_e2e() {
  has_ui || { step "E2E: no UI yet, skipped"; return; }
  step "UI: end-to-end tests"
  (cd ui && bun install --frozen-lockfile && bunx playwright test)
}

check_docs() {
  step "User documentation builds"
  require mdbook
  mdbook build docs/user
}

sections=${*:-repo rust coverage wasm fuzz ui e2e docs}
for section in $sections; do
  case "$section" in
    repo|rust|coverage|wasm|fuzz|ui|e2e|docs) "check_$section" ;;
    *) echo "check-all: unknown section '$section'" >&2; exit 2 ;;
  esac
done

printf '\nAll checks passed: %s\n' "$sections"
