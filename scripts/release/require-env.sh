#!/usr/bin/env bash
# Fails closed (ENG-05): exits 1, naming each one, when any of the variables named as arguments is
# unset or empty, so a tag push never builds an unsigned release. The release workflow maps the
# secrets and variables of the `release` environment to these names (docs/dev/release.md).
set -eu
missing=0
for name in "$@"; do
  if [ -z "${!name:-}" ]; then
    echo "::error::$name is missing from the release environment (docs/dev/release.md)"
    missing=1
  fi
done
exit "$missing"
