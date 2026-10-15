#!/bin/sh
# Fetches the public IFC sample corpus (INT-04) listed in crates/interop-ifc/tests/corpus/manifest.txt
# into a directory (default: target/ifc-corpus), at the pinned revision, and checks every file
# against its SHA-256. Files already there with the right checksum are not downloaded again.
# The tests never touch the network: they read the directory this script fills (IFC_CORPUS_DIR).
# Usage: scripts/fetch-ifc-corpus.sh [directory]
set -eu

root=$(git rev-parse --show-toplevel)
manifest="$root/crates/interop-ifc/tests/corpus/manifest.txt"
dir=${1:-"$root/target/ifc-corpus"}
mkdir -p "$dir"

sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
directive() { sed -n "s/^$1 //p" "$manifest"; }

repository=$(directive repository)
revision=$(directive revision)
[ -n "$repository" ] && [ -n "$revision" ] || {
  echo "fetch-ifc-corpus: $manifest needs a repository and a revision" >&2
  exit 1
}

grep -E '^[0-9a-f]{64}  ' "$manifest" | while IFS= read -r line; do
  checksum=${line%%  *}
  path=${line#*  }
  file="$dir/$(basename "$path")"
  [ -f "$file" ] && [ "$(sha256 "$file")" = "$checksum" ] && continue
  url="https://raw.githubusercontent.com/$repository/$revision/$(printf '%s' "$path" | sed 's/ /%20/g')"
  curl -fsSL --retry 3 -o "$file.part" "$url"
  actual=$(sha256 "$file.part")
  if [ "$actual" != "$checksum" ]; then
    rm -f "$file.part"
    echo "fetch-ifc-corpus: $path has SHA-256 $actual, expected $checksum" >&2
    exit 1
  fi
  mv "$file.part" "$file"
  echo "fetched $path"
done
