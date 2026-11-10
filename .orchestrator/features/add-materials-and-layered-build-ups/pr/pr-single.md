## What and why

Epic #84: materials, a grade library and layered wall and slab build-ups. It closes the epic, its three sub-issues (#85, #86, #87), and the three stack-step issues that refine them (#244 refines #85, #245 refines #86, #246 refines #87). Everything is in one PR, at the user's choice; the three parts come in order in the history.

Closes #84
Closes #85
Closes #86
Closes #87
Closes #244
Closes #245
Closes #246
Requirements: MAT-01, MAT-02, MAT-03, CORE-03, CORE-04, CORE-09, API-02, UI-01, UI-04, UI-06, ENG-02, ENG-03, ENG-04, REP-05, SEC-03

### Material model (#85, #244; MAT-01)

- Schema: a new `resource` kind, enumerations (`enum:`), and sourced values (`sourced = true`) stored with their document and clause, or "entered by the user". Material values therefore persist, and save then reopen is tested in `crates/host-desktop/tests/material_persistence.rs`.
- New crate `material` (layer 3, critical, wasm). It has eight categories and these laws: linear elastic, concrete parabola-rectangle and bi-linear, steel bi-linear, timber elastic-brittle, and none. Each law reports its limit strains and initial modulus. Partial factors are given per annex (recommended, FR). A `MaterialOriginal` keeps the library value of an edited value, so it can be reset.
- `core-model` refuses to delete an object a required reference still targets; the API reports it as `in_use`.
- ADR 0018 (sourced values) and ADR 0019 (materials and layered build-ups). The IFC mapping table declares `IfcMaterial` and its property sets; IFC read and write of materials come with the element export (INT-07).

### Library and Materials dialog (#86, #245; MAT-02)

- `crates/material/data/*.json`, with 38 grades:
  - concrete (EN 1992-1-1) and reinforcement;
  - structural steel (EN 1993-1-1);
  - solid timber (EN 338:2016) and glulam (EN 14080);
  - masonry (EN 1996-1-1);
  - non-structural materials (Règles Th-Bât, Fascicule Matériaux, 2017).
- Every value cites its document and clause, with numbers only (REP-05). ENG-03 cross-checks the values against hand calculations. A structural engineer reviewed the values for the user: nine timber values were corrected, the non-structural values were re-sourced, wood-fibre board was removed, and mineral wool was split into rock wool and glass wool.
- The loader treats library files as hostile input, with property tests and a fuzz target.
- The `materials.*` API: `library`, `grade`, `list`, `get`, `add_from_library`, `create`, `update` and `delete`. Errors never echo model values.
- The Materials dialog (Ctrl/Cmd+Shift+M) lets the user add a grade from the library or create a custom material, see each value with its source, edit or reset values, rename, and delete with undo.

### Layered build-ups (#87, #246; MAT-03)

- A layer has a material, a thickness, a function (structure, substrate, insulation, air gap, membrane, finish), a priority from 1 to 5 and a core flag. The priority maps to IFC as (5 − p) × 25.
- The rules report every problem at once. The thermal resistance is a partial sum, R = d/λ, and the layers without data are counted. A material that layers use cannot be deleted. The mapping table declares `IfcMaterialLayerSet` and `IfcMaterialLayer` (priority, category); the totals and the layer order are checked with typed errors.
- The `buildups.*` API: `list`, `get`, `preview`, `create`, `update`, `duplicate` and `delete`. `preview` validates a draft without saving, so the UI computes no material maths (ENG-04).
- The Build-ups dialog (Ctrl/Cmd+Shift+B) has a list and a layer editor, a section preview, inline errors, and keyboard reorder with a live region.

## UI design

Boards 4.7–4.14 (Materials) and 3.8–3.20 (Build-ups) in the FigCraft design. The design review is in `.orchestrator/features/add-materials-and-layered-build-ups/design/review.md`. Its two must-fixes are done: "Manage materials…" is out of the native select (WCAG 3.2.2), and the membrane number is readable. These differences are accepted: findings 8, 11, 12, 15, 16 and 21.

## User documentation

- `docs/user/manage-project-materials.md` and `.fr.md`
- `docs/user/create-a-wall-or-slab-build-up.md` and `.fr.md`
- `docs/user/use-the-workspace-with-the-keyboard.md` (Ctrl+Shift+M, Ctrl+Shift+B)

## Known limits

- **Layer ids for the wall work** (epic #88), recorded in ADR 0019:
  - `geom::LayerId` is the layer's position, not its object id;
  - the derived ids of new layers can be reused after a delete.
- **Partial factors per annex** are flat attributes (`gamma_c`, `gamma_c_fr`, …), so adding an annex is a schema version. Enumerations are not checked across schema versions yet.
- **Speed:** `materials.list` is O(N·S), and Undo's history depth is found by walking the history.
- **Error reporting:**
  - an invalid layer name or description is reported as `invalid_name` with no index;
  - the totals show "—" until every layer has a material;
  - the range help doesn't say whether the bounds are included.
- **To check before a release** (`crates/material/data/README.md`):
  - the French annex partial factors;
  - that the 2017 Th-Bât edition is current;
  - the glass wool density (30 kg/m³) and specific heat (1030 J/(kg·K)).

## Definition of Done (AGENTS.md §2)

- [x] Unit tests added; coverage ≥ 98 % in the critical areas (material 99 %, api 99 %, ui/src/api 100 %)
- [x] E2E tests added (`ui/e2e/materials.spec.ts`, `ui/e2e/buildups.spec.ts`)
- [x] Bug fixes include a regression test
- [x] Format, lint, import order and type checks pass
- [x] No new dependency
- [x] End-user docs updated
- [x] Code review and simplification pass done
- [x] Commits follow Conventional Commits and are signed off (DCO)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
