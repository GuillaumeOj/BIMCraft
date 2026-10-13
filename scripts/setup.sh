#!/bin/sh
# One-time setup for contributors and coding agents: enables shared git hooks and checks tools.
set -eu

root=$(git rev-parse --show-toplevel)
cd "$root"
# shellcheck source=scripts/tool-versions.env
. scripts/tool-versions.env

bun_install="curl -fsSL https://bun.sh/install | bash -s bun-v$BUN_VERSION"

git config core.hooksPath .githooks
echo "Git hooks enabled (.githooks)."

missing=0
check() {
  if command -v "$1" >/dev/null 2>&1; then
    printf '  ok       %s\n' "$1"
  else
    printf '  MISSING  %s  ->  %s\n' "$1" "$2"
    missing=1
  fi
}

echo "Tools:"
check rustup  "https://rustup.rs (the toolchain in rust-toolchain.toml is installed automatically)"
check bun     "$bun_install"
check cargo-deny "cargo install --locked cargo-deny@$CARGO_DENY_VERSION"
check cargo-llvm-cov "cargo install --locked cargo-llvm-cov@$CARGO_LLVM_COV_VERSION"
check taplo   "cargo install --locked taplo-cli@$TAPLO_VERSION"
check typos   "cargo install --locked typos-cli@$TYPOS_VERSION"
check gitleaks "https://github.com/gitleaks/gitleaks/releases/tag/v$GITLEAKS_VERSION"
check actionlint "https://github.com/rhysd/actionlint/releases/tag/v$ACTIONLINT_VERSION"
check mdbook  "cargo install --locked mdbook@$MDBOOK_VERSION"

if command -v bun >/dev/null 2>&1 && [ "$(bun --version)" != "$BUN_VERSION" ]; then
  echo "  WARNING  bun $(bun --version) found, project uses $BUN_VERSION: $bun_install" >&2
fi

if [ "$missing" -ne 0 ]; then
  echo "Install the missing tools above, then re-run scripts/setup.sh." >&2
  exit 1
fi
echo "Setup complete."
