---
name: ship
description: Prepare local commits for a push to the remote — run all checks, verify the Definition of Done, and walk the user through code review, simplification and the review stamp. Use when the user says the work is ready, asks to push, open a PR, or "ship" it. Never pushes by itself.
---

# Ship: prepare a push

You never push. This skill gets the branch to a state where **the user** can push. Follow AGENTS.md §1–§2.

1. **Clean tree.** `git status` must be clean. If not, ask the user whether to commit (signed off, Conventional Commits) or stash.
2. **Full checks.** Run `scripts/check-all.sh`. Fix every failure at its cause; never lower thresholds, disable lints or skip tests. Commit fixes.
3. **Definition of Done.** For the commits not yet on the remote (`git log @{upstream}..HEAD`, or since `main` without an upstream), check and report each item:
   - spec requirement IDs referenced where relevant;
   - unit tests and coverage thresholds (90 % global, 98 % critical areas);
   - e2e test for any new or changed main user flow;
   - user docs in `docs/user/` for user-facing changes, linked from `SUMMARY.md`, or a stated reason why not;
   - no new dependency without justification; all versions exact.

   List anything missing and fix it (or ask) before going on.
4. **Review gate.** Tell the user, in one short message, to run **`/code-review`** and then **`/simplify`** on these changes. Apply and commit the fixes they accept, then re-run step 2.
5. **Hand over.** Once the user confirms both passes are done, give them the exact `bun scripts/push-pr.ts` command for their branch and stop: it stamps, pushes and opens or updates the pull request. Fill in a Conventional Commits `--title`, a `--body-file` drafted from `.github/pull_request_template.md` (write it inside the repository, uncommitted, or in a path the user can open), the requirement labels and `--assignee @me`. For a stack, use `--stack` with one `--branch <name> --title … --body-file …` group per branch. You may check the command with `--dry-run`; the user runs it for real, because the stamp asks them to confirm the reviews at a terminal. If the user only wants the stamp, give them `scripts/mark-reviewed.sh` to run.
