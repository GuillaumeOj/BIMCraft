#!/bin/sh
# Stamps HEAD as reviewed so the pre-push hook lets it through (AGENTS.md §1).
# Run only after the user has run a code review AND a simplification pass on these changes.
set -eu

if [ -n "$(git status --porcelain)" ]; then
  echo "mark-reviewed: working tree is not clean; commit or stash first so the stamp matches what is pushed." >&2
  exit 1
fi

head=$(git rev-parse HEAD)
printf '%s\n' "$head" > "$(git rev-parse --git-dir)/bimcraft-review-stamp"
echo "Marked $head as reviewed. A push of this exact commit is now allowed."
