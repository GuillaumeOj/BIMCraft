# Fail the repository check when the old name BIMCraft reappears

Issue: #389 (epic #380). Follows [ADR 0028](../../../docs/dev/adr/0028-rename-to-fabrilo.md). Requirement IDs: none.
User docs: N/A (repository tooling only).

## Goal

`scripts/check-all.sh repo` (so CI) and the pre-commit hook fail when "bimcraft", in any case, appears in a tracked
file's content or path outside a short allowlist. The failure names each file and line and points to ADR 0028 and
its identifier table.

## Decisions

1. **ADR 0028, not 0025.** The issue says "ADR 0025", but 0025 to 0027 were taken by viewport records; the rename
   record is `docs/dev/adr/0028-rename-to-fabrilo.md`. The allowlist and the message use 0028.
2. **Earlier ADRs are not allowlisted.** ADR 0028 decided that #388 renames ADRs 0001–0027 in place, so once #388
   is merged they only hold repository URLs, which rule 3 covers.
3. **Allowlist** (`scripts/old-name-allowlist.txt`, one entry per line, `#` comments, each entry with a reason
   comment). An entry is `<path>` (a file, or a directory when it ends with `/`), or `<path> | <phrase>`: then a
   line of that file is allowed only if it contains `<phrase>` (literal, case-sensitive). The path `*` means any
   file. The initial list, and nothing else:

   | Entry | Why |
   | --- | --- |
   | `docs/dev/adr/0028-rename-to-fabrilo.md` | the record of the change |
   | `.orchestrator/` | historical feature records (left untouched by #388; this feature's own records too) |
   | `docs/specifications.md \| Changed from BIMCraft on 2026-11-17` | the §16.2 decision entry |
   | `docs/user/introduction.md \| was called BIMCraft until November 2026` | the name-change note in the user guide |
   | `crates/interop-ifc/tests/georef.rs \| "BIMCraft_SurveyPoint"` | the test that the old property set is foreign (ADR 0028 "No compatibility") |
   | `* \| GuillaumeOj/BIMCraft` | repository URLs and `REPO=` until the owner renames the repository (ADR 0028); remove then |

   No lock file holds the name (`ui/bun.lock` and `fuzz/Cargo.lock` were renamed), so none is listed.
   The phrase form keeps big files such as the specification guarded everywhere else.
4. **What is scanned.** `git ls-files` (tracked files of the working tree, so staged new files count) — both the
   path and the content. Binary files (a NUL byte in the first 8 KiB) are checked by path only. Files listed in
   the allowlist that no longer exist, or entries that match nothing, fail the check too ("stale allowlist entry"),
   so the list can only shrink deliberately and never silently carries dead entries.
5. **Pre-commit.** `.githooks/pre-commit` runs `bun scripts/check-old-name.ts <staged files>` (bun present, else the
   usual `skip`). With file arguments the script checks only those files and skips the stale-entry rule.
6. **No other old identifiers.** The guard looks for `bimcraft` only. `BIMC` (the old file header) is too short
   and common to match safely; the persist tests already pin the new header.

## Script contract

`scripts/check-old-name.ts`, same style as `check-pinned-deps.ts` / `check-architecture.ts`: pure exported
functions (parse the allowlist, find hits in a `{path, content}` list, find stale entries) plus a `main(argv,
root)` that reads the repository, prints and returns an exit code. Output on failure, one line per hit, then the
pointer:

```text
check-old-name: the old name BIMCraft appears outside the allowlist:
  README.md:3: BIMCraft takes a construction project…
  crates/foo/BIMCraft.txt: (file name)
Use Fabrilo and the identifiers in the table of docs/dev/adr/0028-rename-to-fabrilo.md.
A justified exception goes in scripts/old-name-allowlist.txt.
```

Tests (`scripts/check-old-name.test.ts`, `bun test`, temporary repositories as in `check-architecture.test.ts`):
a file containing `BIMCraft`, `bimcraft`, `BIMCRAFT` fails; a path containing it fails; each allowlist form
(file, directory, phrase, `*` phrase) allows exactly what it should and no more (another line of the same file
still fails); a stale entry fails; binary content is ignored; the message names ADR 0028; and `main` passes on a
clean tree. The suite runs in `(cd scripts && bun test)`, already part of `repo`.

## Wiring and docs

- `scripts/check-all.sh`: `bun scripts/check-old-name.ts` in `check_repo`, and "old name" in its step title.
- `.githooks/pre-commit`: rule 5.
- `docs/dev/quality.md`: a short paragraph (or line in the existing checks list) naming the guard, why it exists,
  where the allowlist lives and the ADR 0028 pointer. ADR 0028 is not edited: it already names "the allowlist of
  the guard in issue #389", and accepted records are not rewritten.
- AGENTS.md §9 table: one row "Old name guard | `bun scripts/check-old-name.ts`".

## Tasks

| Id | Node | Files |
| --- | --- | --- |
| T1 | back | `scripts/check-old-name.ts`, its test, `scripts/old-name-allowlist.txt`, `scripts/check-all.sh`, `.githooks/pre-commit`, `docs/dev/quality.md`, `AGENTS.md` |

One task: the whole change is ~200 lines of one script. `front` reviews it.

## Order and checks

- The integration branch is behind `main`, and #388's pull request (top-level files, `docs/dev`) is not merged yet,
  so the check cannot pass on the repository until both are in. T1 builds and tests the script with temporary
  repositories; the allowlist is the table above.
- Lead, at integration: rebase onto `main` once #388 is merged, then `bun scripts/check-old-name.ts` passes on the
  tree, `scripts/check-all.sh repo` passes, and a scratch file containing "BIMCraft" makes it fail.
- Commits: `chore(...)` or `docs(...)` only, never `feat`/`fix` or `wip`; subject ≤ 100 characters; signed off;
  `Refs #389`.

## Acceptance

- The check fails on a test file containing "BIMCraft" and passes on the repository.
- `scripts/check-all.sh repo` runs it; CI is green.
- `docs/dev/quality.md` lists the check.
- The pull request states "User docs: N/A (tooling)".
