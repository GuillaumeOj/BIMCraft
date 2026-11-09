## What and why

Layered build-ups for walls and slabs. A build-up is an ordered list of layers. Each layer has a material, a thickness, a function (structure, substrate, insulation, air gap, membrane, finish), a priority from 1 to 5 and a core flag. The editor shows the total thickness and the thermal resistance live. Third of three stacked PRs for epic #84, on top of #86.

Closes #87
Refs #84, #246
Requirements: MAT-03, UI-01, UI-04, UI-06

- `material`: build-ups with usage, functions and default priorities, mapped to IFC priority as (5 − p) × 25. Rules report every problem at once: no layers, more than 32 layers, thickness, a core that isn't contiguous, too thick, unknown material, or an air gap whose material is not generic. The thermal resistance is a partial sum, R = d/λ, and the layers without data are counted. A material that layers use cannot be deleted.
- The `buildups.*` API: `list`, `get`, `preview`, `create`, `update`, `duplicate` and `delete`. `preview` validates a draft without saving, so the UI never computes material maths (ENG-04).
- UI: the Build-ups dialog (Ctrl/Cmd+Shift+B) with list and layer editor, a section preview, inline errors, keyboard reorder with a live region, and EN/FR strings. Unit tests and an e2e test (`ui/e2e/buildups.spec.ts`).
- Design review fixes for both dialogs (`design/review.md`). The two must-fixes: "Manage materials…" moves out of the native select (WCAG 3.2.2), and the membrane number is readable again.
- **Library value fixes for #86** (commits marked Refs #86), after the engineer's review:
  - nine EN 338:2016 timber values corrected;
  - the non-structural values re-sourced to Règles Th-Bât, Fascicule Matériaux (2017), with corrected conductivities;
  - wood-fibre board removed (no default in that document);
  - mineral wool split into rock wool and glass wool.

## UI design

Boards 3.8–3.20 in the FigCraft design. Accepted differences: `design/review.md` findings 8, 11, 12, 15.

## User documentation

- `docs/user/create-a-wall-or-slab-build-up.md` (EN) and `.fr.md` (FR)
- `docs/user/manage-project-materials(.fr).md` updated for the library changes

## Known limits

- Layer ids for the wall work (epic #88), recorded in ADR 0019:
  - `geom::LayerId` is the layer's position, not its object id;
  - the derived ids of new layers can be reused after a delete.
- An invalid layer name or build-up description is reported as `invalid_name` without the layer index.
- The totals show "—" until every layer has a material.
- To check before a release (`crates/material/data/README.md`):
  - the French annex partial factors;
  - that the 2017 Th-Bât edition is current;
  - the glass wool density (30 kg/m³) and specific heat (1030 J/(kg·K)).

## Definition of Done (AGENTS.md §2)

- [x] Unit tests added; coverage ≥ 98 % in the critical areas (material 99 %, api 99 %, ui/src/api 100 %)
- [x] E2E test added (`ui/e2e/buildups.spec.ts`, run by CI)
- [x] Bug fixes include a regression test
- [x] Format, lint, import order and type checks pass
- [x] No new dependency
- [x] End-user docs updated
- [x] Code review and simplification pass done
- [x] Commits signed off (DCO)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
