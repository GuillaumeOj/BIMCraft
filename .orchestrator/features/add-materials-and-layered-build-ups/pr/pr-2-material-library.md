## What and why

A library of common European grades, and the Materials dialog: add a grade from the library or create a custom material, see each value with its source, edit or reset it, rename, and delete with undo. Second of three stacked PRs for epic #84, on top of #85.

Closes #86
Refs #84, #245
Requirements: MAT-02, UI-01, UI-04, UI-06, ENG-02, ENG-03, REP-05, SEC-03

- `crates/material/data/*.json`: concrete (EN 1992-1-1), reinforcement, structural steel (EN 1993-1-1), solid timber and glulam (EN 338, EN 14080), masonry (EN 1996-1-1), and non-structural materials. Every value cites its document and clause, with numbers only (REP-05). ENG-03 cross-checks against hand calculations. The loader treats library files as hostile input, with property tests and a fuzz target.
- The `materials.*` API: `library`, `grade`, `list`, `get`, `add_from_library`, `create`, `update` and `delete`. Errors never echo model values (SEC-03).
- UI: the Materials dialog (Ctrl/Cmd+Shift+M), EN/FR strings, unit and e2e tests (`ui/e2e/materials.spec.ts`).
- The library values were checked by a structural engineer for the user. The corrections are at the top of the stack (#87 PR), with commits marked Refs #86.

## UI design

Boards 4.7–4.14 in the FigCraft design. Where the code differs from them, and why: `.orchestrator/features/add-materials-and-layered-build-ups/design/review.md` (findings 16–21).

## User documentation

- `docs/user/manage-project-materials.md` (EN) and `.fr.md` (FR)
- `docs/user/use-the-workspace-with-the-keyboard.md` lists Ctrl+Shift+M

## Known limits

- `materials.list` reads each material's users (O(N·S)): a referrer count in the core would make it linear.
- The range help reads "Between min and max" without saying whether the bounds are included.
- Undo's history depth is found by walking the history; a depth field in `history.state` would avoid it.

## Definition of Done (AGENTS.md §2)

- [x] Unit tests added; coverage ≥ 98 % in the critical areas (material 98 %, api 99 %, ui/src/api 100 %)
- [x] E2E test added (`ui/e2e/materials.spec.ts`, run by CI; local ports are blocked in the sandbox)
- [x] Bug fixes include a regression test
- [x] Format, lint, import order and type checks pass (biome, tsc, clippy, fmt)
- [x] No new dependency
- [x] End-user docs updated
- [x] Code review and simplification pass done
- [ ] Commits signed off (DCO): two `.orchestrator/` planning commits (a942e0f, 10f8f1b) have no sign-off; see the stack notes

🤖 Generated with [Claude Code](https://claude.com/claude-code)
