# Rename BIMCraft to Fabrilo across the user guide (EN and FR)

Issue [#387](https://github.com/GuillaumeOj/BIMCraft/issues/387), sub-issue of the epic [#380](https://github.com/GuillaumeOj/BIMCraft/issues/380). Requirement: UI-04. Decision record: [ADR 0028 "Rename to Fabrilo"](../../../docs/dev/adr/0028-rename-to-fabrilo.md) (the issue says "ADR 0025"; the record was numbered 0028 when it merged).

This feature changes the end-user guide only (`docs/user/`): no code, no tests, no UI, no API. There is no `api-contract.md` or `ux-brief.md`, because there is no interface between the two tasks and no design work.

## 1. Rules

- Every "BIMCraft" in `docs/user/` becomes "Fabrilo", in English and French. The product name does not change in French. French grammar stays correct after the swap (the name is used without an article, like before; check agreements such as "BIMCraft la calcule" still read well).
- Built-in material names shown to users start with `fabrilo/` instead of `bimcraft/`. Today no page in `docs/user/` contains `bimcraft/` (checked with `git grep`), so the material pages only need the product-name swap. If a page gains one before merge, rename it too.
- Plain language, no internal jargon (AGENTS.md §6). Don't reword anything else: this is a rename, not an edit pass.
- File names do not change: none contains the old name, and the interface links to pages by file name.

## 2. Deliverables

| File | Change |
| --- | --- |
| `docs/user/book.toml` | `title = "Fabrilo User Guide"`, `authors = ["Fabrilo contributors"]` |
| `docs/user/SUMMARY.md` | "Install BIMCraft" → "Install Fabrilo" |
| `docs/user/introduction.md` | Name swap, plus the name-change note (section 3) |
| English pages: `getting-started.md`, `save-and-open-a-project.md`, `recover-unsaved-work.md`, `change-the-interface-language.md`, `use-the-workspace-with-the-keyboard.md`, `create-a-wall-or-slab-build-up.md`, `manage-project-materials.md`, `read-and-enter-coordinates.md`, `set-up-the-project-location.md`, `navigate-the-3d-view.md`, `place-a-block.md`, `select-and-move-elements.md` | Name swap |
| French pages: `create-a-wall-or-slab-build-up.fr.md`, `manage-project-materials.fr.md`, `read-and-enter-coordinates.fr.md`, `set-up-the-project-location.fr.md`, `navigate-the-3d-view.fr.md`, `place-a-block.fr.md`, `select-and-move-elements.fr.md` | Name swap |

The issue's list predates PR #370: `navigate-the-3d-view`, `place-a-block` and `select-and-move-elements` (EN and FR) also mention the old name and are included.

## 3. Name-change note

On `introduction.md`, right after the first paragraph:

> **New name.** Fabrilo was called BIMCraft until November 2026. It is the same product: older links, screenshots and videos may still show the old name.

There is no French introduction page today, so there is no French note. Creating one is out of scope.

This note is the only place in `docs/user/` where "BIMCraft" remains.

## 4. Out of scope

- Pages from open PR #378 (`getting-started.fr.md`, `update-bimcraft.md`, `update-bimcraft.fr.md`, the install guide). They are not on `main`. ADR 0028 says #378 adopts the new name itself, or is renamed after it merges. If #378 merges before this pull request, the rebase brings those pages in and we rename them then (including `update-bimcraft*.md` → `update-fabrilo*.md` if the interface's links allow).
- Links to `github.com/GuillaumeOj/BIMCraft` (none in `docs/user/` today): they stay until the owner renames the repository (#388, #389).
- The interface, the desktop app, the developer docs and the old-name guard: issues #384 to #389.

## 5. Tasks

Split by file so the branches never conflict.

1. **back** — English: `book.toml`, `SUMMARY.md`, `introduction.md` (with the note) and every English page of section 2.
2. **front** — French: every `.fr.md` page of section 2.

Each owner cross-reviews the other's branch (back ↔ front) for the French grammar and the note's wording.

## 6. Acceptance

- `git grep -i bimcraft -- docs/user` matches only the note on `introduction.md`.
- `mdbook build docs/user` passes; `scripts/check-all.sh repo` (typos, links) passes.
- Commits: Conventional Commits (`docs(user): …`), subject ≤ 100 characters, `Signed-off-by: Guillaume <7831777+GuillaumeOj@users.noreply.github.com>`, never `wip`. Each references `Refs #387`.
- The pull request says "User docs: this change is the user docs" and closes #387.
- CI is green.
