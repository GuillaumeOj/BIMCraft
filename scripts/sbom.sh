#!/bin/sh
# Writes a CycloneDX SBOM (one JSON file per workspace crate) into the given directory (SEC-05).
# The release workflow attaches these files to every published release.
# Usage: scripts/sbom.sh <out-dir>
set -eu

[ $# -eq 1 ] || { echo "usage: scripts/sbom.sh <out-dir>" >&2; exit 2; }
out=$1

root=$(git rev-parse --show-toplevel)
mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$root"

# --target all: the SBOM lists the dependencies of every platform, not only the runner's.
# fuzz/ is a separate workspace and never shipped, so it is left out.
cargo cyclonedx --all-features --format json --spec-version 1.5 --target all --quiet
mv crates/*/*.cdx.json "$out/"
echo "SBOM written to $out"
