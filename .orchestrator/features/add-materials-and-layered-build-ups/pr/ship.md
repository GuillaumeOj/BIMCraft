# Shipping the three stacked PRs

Lead cannot create branches or rewrite commits (the sandbox refuses to lock refs), so you run these two steps in the integration worktree, where the branch of PR #317 is checked out.

| PR | Branch | Ends at (before step 1) | Closes |
| --- | --- | --- | --- |
| 1 | `materials-1-model` | `219f8bc` (merge of T1) | #85 |
| 2 | `materials-2-library` | `dca3b3e` (merge of T5) | #86 |
| 3 | `orch/add-materials-and-layered-build-ups/integration` (PR #317) | HEAD | #87 |

## 1. Fix the commit messages and cut the stack

CI's "Commits and user docs" job runs `.githooks/commit-msg` on every commit, and nine commits fail it:
- eight `.orchestrator/` commits have no `Signed-off-by:`: 3b9df09, b6c0f19, 4c1debd, 13ead78, ba0d987, 81fcfca (PR 1) and a942e0f, 10f8f1b (PR 2);
- the subject of ef0bd31 is 111 characters, over the limit of 100.

```sh
cd /Users/guillaume/dev/BIMCraft.worktrees/add-materials-and-layered-build-ups/integration
sh .orchestrator/features/add-materials-and-layered-build-ups/pr/fix-commit-messages.sh
```

The script uses `git filter-branch --msg-filter`, so only the messages change:
- it signs off the commits that lack a sign-off, as `git config user.name/email`;
- it shortens the ef0bd31 subject;
- it checks that the trees of HEAD and of both boundaries are unchanged;
- it creates `materials-1-model` and `materials-2-library` on the new boundary commits;
- it runs the commit-msg hook over every commit.

If anything looks wrong: `git reset --hard refs/original/refs/heads/orch/add-materials-and-layered-build-ups/integration`.

## 2. Push the stack and open PRs 1 and 2

```sh
D=.orchestrator/features/add-materials-and-layered-build-ups/pr
bun scripts/push-pr.ts --only materials-1-model,materials-2-library,orch/add-materials-and-layered-build-ups/integration \
  --force --assignee @me --milestone "Phase 1 – MVP" --project BIMCraft \
  --branch materials-1-model --label no-user-docs \
    --title "feat(material): material model with behaviour laws and sourced values (MAT-01)" --body-file $D/pr-1-material-model.md \
  --branch materials-2-library \
    --title "feat(material): grade library and the Materials dialog (MAT-02)" --body-file $D/pr-2-material-library.md \
  --branch orch/add-materials-and-layered-build-ups/integration \
    --title "feat(material): layered wall and slab build-ups (MAT-03)" --body-file $D/pr-3-layered-build-ups.md
```

- `--force` is needed because step 1 rewrote the history: push-pr.ts pushes with `--force-with-lease --force-if-includes`.
- PR 1 changes no user page (the model has no UI yet), so it carries the `no-user-docs` label the CI check reads. Its description explains why.
- #317 is the top of the stack. push-pr.ts finds it open and updates it: base `materials-2-library`, with PR 3's title and description. PRs 1 and 2 open below it.
- Each PR gets its "Closes #85/#86/#87", and the milestone and project of that issue, the upper PRs included.
