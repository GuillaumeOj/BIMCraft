---
name: ship
description: Prepare local commits for a push to the remote and see them through CI — targeted checks, the Definition of Done, the user's code review, simplification and review stamp, then the push and CI follow-up. Use when the user says the work is ready, asks to push, open a PR, or "ship" it, or when CI fails on a pull request. Never pushes without the user's OK and stamp.
---

# Ship: prepare a push and get CI green

CI is the gate: it runs every check, so do not run the whole suite locally. Follow AGENTS.md §1–§3a.

1. **Clean tree.** `git status` must be clean. If not, ask the user whether to commit (signed off, Conventional Commits) or stash.
2. **Targeted checks.** Run `scripts/check-all.sh repo` and the narrow checks for the areas the commits touch (AGENTS.md §3a). Run a full section only in the cases §3a lists. Fix every failure at its cause; never lower thresholds, disable lints or skip tests. Commit fixes.
3. **Definition of Done.** For the commits not yet on the remote (`git log @{upstream}..HEAD`, or since `main` without an upstream), check and report each item:
   - spec requirement IDs referenced where relevant;
   - unit tests for the changed behaviour, written for the coverage thresholds (90 % global, 98 % critical areas; CI measures them);
   - e2e test for any new or changed main user flow;
   - user docs in `docs/user/` for user-facing changes, linked from `SUMMARY.md`, or a stated reason why not;
   - no new dependency without justification; all versions exact.

   List anything missing and fix it (or ask) before going on.
4. **Review gate.** Tell the user, in one short message, to run **`/code-review`** and then **`/simplify`** on these changes. Apply and commit the fixes they accept, and rerun the targeted checks they affect.
5. **Hand over.** Once the user confirms both passes are done, give them the exact `bun scripts/push-pr.ts` command for their branch: it stamps, pushes and opens or updates the pull request. Fill in a Conventional Commits `--title`, a `--body-file` drafted from `.github/pull_request_template.md` (write it inside the repository, uncommitted, or in a path the user can open), the requirement labels and `--assignee @me`. For a stack, use `--stack` with one `--branch <name> --title … --body-file …` group per branch. You may check the command with `--dry-run`; the user runs it for real, because the stamp asks them to confirm the reviews at a terminal.
6. **Or push for them.** If the user would rather you push, have them run `scripts/mark-reviewed.sh`. Once they have stamped and told you to push, run `git push -u origin <branch>` and, for a new pull request, `gh pr create --draft --title … --body-file …`. Never force-push; after a rebase, hand over `push-pr.ts --force` instead.
7. **Track CI.** Watch the pull request with `gh pr checks <number> --watch`. For a failed job, read `gh run view <run-id> --log-failed`, reproduce it with the narrowest local check, fix the cause and commit. Then go back to step 4 for the new commits: review, stamp, push on the user's OK. Stop when every check is green, and tell the user the pull request is ready to be marked ready for review.
