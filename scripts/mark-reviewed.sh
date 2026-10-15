#!/bin/sh
# Stamps HEAD as reviewed so the pre-push hook lets it through (AGENTS.md §1).
# Run only after the user has run a code review AND a simplification pass on these changes.
# The confirmation must be typed at a terminal, so an agent's shell cannot stamp.
set -eu

if [ -n "$(git status --porcelain)" ]; then
  echo "mark-reviewed: working tree is not clean; commit or stash first so the stamp matches what is pushed." >&2
  exit 1
fi

if [ ! -t 0 ]; then
  echo "mark-reviewed: run this in a terminal; the review confirmation must be typed by the user." >&2
  exit 1
fi

head=$(git rev-parse HEAD)
printf 'Have /code-review and /simplify (or equivalent reviews) been run on %s? [y/N] ' "$(git rev-parse --short HEAD)"
read -r answer || answer=
case $answer in
  [yY] | [yY][eE][sS]) ;;
  *)
    echo "mark-reviewed: not stamped; review the changes first." >&2
    exit 1
    ;;
esac

printf '%s\n' "$head" > "$(git rev-parse --git-dir)/bimcraft-review-stamp"
echo "Marked $head as reviewed. A push of this exact commit is now allowed."
