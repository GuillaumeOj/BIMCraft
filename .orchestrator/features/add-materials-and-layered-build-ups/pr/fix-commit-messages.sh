#!/bin/sh
# Fixes the commit messages CI's "Commits and user docs" check refuses on PR #317, and cuts the stack.
# Only messages change: trees, authors and the merge topology stay as they are.
#   - adds Signed-off-by to the eight .orchestrator/ commits that lack it;
#   - shortens the subject of ef0bd31 to 100 characters or fewer.
# Run it in the integration worktree, then push with the command in ship.md (with --force).
set -eu
cd "$(git rev-parse --show-toplevel)"
branch=orch/add-materials-and-layered-build-ups/integration
test "$(git branch --show-current)" = "$branch" || { echo "check out $branch first" >&2; exit 1; }
git fetch origin main

# First-parent positions of the stack boundaries (merges of T1 and T5); filter-branch keeps them.
pos() { git rev-list --first-parent HEAD | grep -n "^$(git rev-parse "$1")" | cut -d: -f1; }
p1=$(pos 219f8bc); p2=$(pos dca3b3e)
t1=$(git rev-parse 219f8bc^{tree}); t2=$(git rev-parse dca3b3e^{tree}); th=$(git rev-parse HEAD^{tree})
signoff="Signed-off-by: $(git config user.name) <$(git config user.email)>"
export signoff

FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch -f --msg-filter '
  msg=$(cat)
  case "$GIT_COMMIT" in
    ef0bd31*) msg=$(printf "%s\n" "$msg" | sed "1s/.*/fix(api): report a wrong law, count every original, one annex naming (MAT-02, Refs #86)/") ;;
  esac
  case "$msg" in
    "Merge "*) printf "%s\n" "$msg" ;;
    *) printf "%s\n" "$msg" | git interpret-trailers --if-exists doNothing --trailer "$signoff" ;;
  esac
' -- origin/main..HEAD

b1=$(git rev-parse "HEAD~$((p1 - 1))"); b2=$(git rev-parse "HEAD~$((p2 - 1))")
test "$(git rev-parse "$b1^{tree}")" = "$t1" && test "$(git rev-parse "$b2^{tree}")" = "$t2" \
  && test "$(git rev-parse HEAD^{tree})" = "$th" || { echo "tree mismatch: restore with git reset --hard refs/original/refs/heads/$branch" >&2; exit 1; }
git branch -f materials-1-model "$b1"
git branch -f materials-2-library "$b2"

failed=0
for sha in $(git rev-list origin/main..HEAD); do
  git log -1 --format=%B "$sha" > "$(git rev-parse --git-dir)/CHECK_MSG"
  .githooks/commit-msg "$(git rev-parse --git-dir)/CHECK_MSG" || { echo "  in $(git log -1 --format='%h %s' "$sha")" >&2; failed=1; }
done
rm -f "$(git rev-parse --git-dir)/CHECK_MSG"
test "$failed" = 0 && echo "All commit messages pass. materials-1-model=$b1 materials-2-library=$b2. Now run the push-pr.ts command of ship.md with --force."
exit "$failed"
