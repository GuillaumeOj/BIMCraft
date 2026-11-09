# Shipping PR #317

The user's decision: the epic ships as **one PR, #317**, not as a stack.

## 1. Fix the commit messages (user)

CI's "Commits and user docs" job runs `.githooks/commit-msg` on every commit, and nine commits fail it:
- eight `.orchestrator/` commits have no `Signed-off-by:`: 3b9df09, b6c0f19, 4c1debd, 13ead78, ba0d987, 81fcfca, a942e0f, 10f8f1b;
- the subject of ef0bd31 is 111 characters, over the limit of 100.

```sh
cd /Users/guillaume/dev/BIMCraft.worktrees/add-materials-and-layered-build-ups/integration
sh .orchestrator/features/add-materials-and-layered-build-ups/pr/fix-commit-messages.sh
```

The script uses `git filter-branch --msg-filter`, so only messages change:
- it signs off the commits that lack it;
- it shortens the ef0bd31 subject;
- it checks that the tree of HEAD is unchanged;
- it runs the commit-msg hook over every commit.

If anything looks wrong: `git reset --hard refs/original/refs/heads/orch/add-materials-and-layered-build-ups/integration`.

## 2. Push (orchestrator)

Lead calls `request_pr`, and the orchestrator force-pushes the branch with lease.

## 3. Update the description of #317 (user)

```sh
cd /Users/guillaume/dev/BIMCraft.worktrees/add-materials-and-layered-build-ups/integration
gh pr edit 317 \
  --title "feat(material): materials, grade library and layered build-ups (MAT-01, MAT-02, MAT-03)" \
  --body-file .orchestrator/features/add-materials-and-layered-build-ups/pr/pr-single.md \
  --milestone "Phase 1 – MVP" --add-project BIMCraft --add-assignee @me
```

The description says "Closes" for #84, #85, #86, #87, #244, #245 and #246. #317 targets `main`, so GitHub links those issues and closes them when it merges. `--add-project` needs a token with the `project` scope (`gh auth refresh -s project`).
