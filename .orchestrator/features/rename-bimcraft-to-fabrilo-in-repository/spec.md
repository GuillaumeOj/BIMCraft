# Rename BIMCraft to Fabrilo in repository files, dev docs and tooling

Issue: #388 (epic #380). Follows [ADR 0028](../../../docs/dev/adr/0028-rename-to-fabrilo.md). Requirement IDs: none.
User docs: N/A (repository and developer files only).

## Goal

The repository's own files name the product Fabrilo. After this feature, outside this feature's own records,
`git grep -i bimcraft` on the in-scope paths below matches only:

- links to `github.com/GuillaumeOj/BIMCraft` (the repository is not renamed yet; ADR 0028 keeps them until it is);
- `docs/dev/adr/0028-rename-to-fabrilo.md` (the record of the change);
- the §16.2 entry of `docs/specifications.md` (already done by #381);
- the historical orchestrator records under `.orchestrator/features/` (see "Decisions").

## Decisions

1. **Earlier ADRs are renamed, not left alone.** The issue says to leave older ADRs, but ADR 0028 (merged after
   the issue was written) decides otherwise: "Issue #388 updates the name in records 0001 to 0027 in place, and
   they keep their status." ADR 0028 wins. In ADRs 0001–0027, rename the product name in prose and tables, and
   old identifiers by the ADR 0028 table (`BIMCraft_SurveyPoint` → `Fabrilo_SurveyPoint`,
   `BIMCraft_Material*` → `Fabrilo_Material*`, `BIMCraft_LibraryValues` → `Fabrilo_LibraryValues`,
   `BIMCraft_*` → `Fabrilo_*`). Status lines and decisions do not change.
2. **Repository links stay.** Every `https://github.com/GuillaumeOj/BIMCraft/...` URL and `REPO=GuillaumeOj/BIMCraft`
   stays as is: the repository has not been renamed and GitHub would redirect anyway. A later change updates them
   once the owner renames the repository.
3. **Project board.** `docs/dev/project-management.md` describes the board as it should be: it is named
   **Fabrilo** (title and `gh project create --title Fabrilo`). Renaming the real board is an owner task (#380).
   The `scripts/push-pr.test.ts` fixtures use `Fabrilo` as the project name; they run against a fake `gh`, so
   nothing depends on the real board's name.
4. **Orchestrator records are left as they are.** `.orchestrator/features/*` are historical working records; the
   old-name guard (#389) allowlists `.orchestrator/features/`. No guard exists yet, so there is no allowlist to edit
   in this feature; #389 must include that path.
5. **No other wording changes.** Replace the name only; do not rewrite sentences, except where grammar needs it
   (for example "BIMCraft's" → "Fabrilo's").

## Scope (files that mention the old name on `main` today)

The integration branch is behind `main`, which already renamed `docs/dev/api.md`, `docs/dev/ui-design.md`,
`docs/dev/desktop.md` prose, `scripts/build-icons.ts` and the workflows. Do not touch those: the list of files is
`git grep -il bimcraft origin/main -- <paths>`, not the working tree.

- Top level: `README.md`, `LICENSE` ("Copyright (c) 2026 The Fabrilo contributors"), `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `SECURITY.md`, `AGENTS.md`, the `.gitignore` comment.
- `.github/ISSUE_TEMPLATE/bug.yml` ("Fabrilo version and operating system"). No other template or workflow on
  `main` mentions the name.
- `.claude/hooks/session-start.sh`, `.claude/skills/new-feature/SKILL.md`.
- `scripts/push-pr.test.ts` fixtures.
- `docs/dev/`: `project-management.md`, `quality.md`, `schema-versions.md`, and ADRs 0001–0027 (`desktop.md` holds
  only a repository URL, which stays).

Out of scope: crates, UI, fuzz, schema, the user guide and the reference book (other sub-issues), the FigCraft
design file (owner task), the guard (#389).

## Tasks

| Id | Node | Files |
| --- | --- | --- |
| T1 | front | Top-level files, `.github/`, `.claude/`, `scripts/push-pr.test.ts` |
| T2 | back | `docs/dev/` (guides and ADRs 0001–0027) |

The two tasks share no file and run in parallel.

## Checks

- Each task: `git grep -n -i bimcraft -- <its files>` shows only repository URLs; `scripts/check-all.sh repo`
  (typos, links, formatting) passes; T1 also runs `bun test scripts/push-pr.test.ts`.
- Lead, after both merges: rebase the integration branch onto `main` (conflicts possible in `docs/dev/quality.md`
  and `docs/dev/schema-versions.md`, which `main` also changed), then on the rebased tree
  `git grep -il bimcraft -- . ':!.orchestrator'` lists only the allowed files above, `scripts/check-all.sh repo` and
  `bun test scripts/` pass.
- Commits: `docs(...)` or `chore(...)` only, never `feat`/`fix` or `wip`; subject ≤ 100 characters; signed off with
  the noreply address; `Refs #388`.

## Acceptance

- No BIMCraft left in the in-scope files except the allowed cases above.
- `scripts/check-all.sh repo` and `bun test scripts/` pass; CI is green.
- The pull request states "User docs: N/A (repository and developer files)".
