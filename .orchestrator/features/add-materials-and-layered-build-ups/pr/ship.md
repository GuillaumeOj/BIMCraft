# Shipping the three stacked PRs

Lead cannot create branches (the sandbox refuses to lock refs), so the stack is cut by hand from the integration branch, in the main checkout. The first-parent boundaries:

| PR | Branch | Ends at | Closes |
| --- | --- | --- | --- |
| 1 | `materials-1-model` | `219f8bc` (merge of T1) | #85 |
| 2 | `materials-2-library` | `dca3b3e` (merge of T5) | #86 |
| 3 | `materials-3-buildups` | integration HEAD | #87 |

```sh
git branch materials-1-model 219f8bc
git branch materials-2-library dca3b3e
git switch -c materials-3-buildups orch/add-materials-and-layered-build-ups/integration
D=.orchestrator/features/add-materials-and-layered-build-ups/pr
bun scripts/push-pr.ts --stack --assignee @me --milestone "Phase 1 – MVP" --project BIMCraft \
  --branch materials-1-model    --title "feat(material): material model with behaviour laws and sourced values (MAT-01)" --body-file $D/pr-1-material-model.md \
  --branch materials-2-library  --title "feat(material): grade library and the Materials dialog (MAT-02)"                  --body-file $D/pr-2-material-library.md \
  --branch materials-3-buildups --title "feat(material): layered wall and slab build-ups (MAT-03)"                         --body-file $D/pr-3-layered-build-ups.md
```

push-pr.ts links each PR to the issue it closes ("Closes #85/#86/#87"), and to that issue's milestone and project, the upper PRs of the stack included. `--milestone` and `--project` are given anyway: drop them if the issues already carry them.

Eight `.orchestrator/`-only commits have no `Signed-off-by:`. CI doesn't check the trailer; only the local `commit-msg` hook does, and the orchestrator's commits bypass it.
- PR 1: 3b9df09, b6c0f19, 4c1debd, 13ead78, ba0d987, 81fcfca.
- PR 2: a942e0f, 10f8f1b.
These commits only add planning files under `.orchestrator/`, so lead recommends shipping them as they are. Signing them would mean rewriting a history that contains merge commits (the stack boundaries are merges), and a plain `rebase --exec` would flatten it.