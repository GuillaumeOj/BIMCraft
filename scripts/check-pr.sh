#!/bin/sh
# Checks a range of commits the way CI checks a pull request.
# Usage: scripts/check-pr.sh <base-ref> [head-ref]
#   - every commit: Conventional Commit subject + DCO sign-off (same rules as .githooks/commit-msg)
#   - user docs rule (AGENTS.md §6): a feat/fix touching crates/ or ui/src/ must also touch docs/user/,
#     unless NO_USER_DOCS=true (CI sets it from the "no-user-docs" PR label).
set -eu

base=$1
head=${2:-HEAD}
root=$(git rev-parse --show-toplevel)
msg=$(mktemp)
trap 'rm -f "$msg"' EXIT

failed=0
for sha in $(git rev-list "$base..$head"); do
  git log -1 --format=%B "$sha" > "$msg"
  if ! "$root/.githooks/commit-msg" "$msg"; then
    echo "  in commit $(git log -1 --format='%h %s' "$sha")" >&2
    failed=1
  fi
done

user_facing=$(git log --format=%s "$base..$head" | grep -E '^(feat|fix)(\(|!|:)' || true)
code_changed=$(git diff --name-only "$base...$head" -- crates ui/src || true)
docs_changed=$(git diff --name-only "$base...$head" -- docs/user || true)
if [ -n "$user_facing" ] && [ -n "$code_changed" ] && [ -z "$docs_changed" ] && [ "${NO_USER_DOCS:-false}" != "true" ]; then
  echo "check-pr: feat/fix commits change product code but docs/user/ is untouched." >&2
  echo "          Add end-user documentation, or label the PR 'no-user-docs' and explain why in its description." >&2
  failed=1
fi

exit "$failed"
