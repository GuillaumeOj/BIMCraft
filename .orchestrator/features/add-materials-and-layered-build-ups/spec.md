# Materials and layered build-ups — feature spec

Epic [#84](https://github.com/GuillaumeOj/BIMCraft/issues/84). Requirements: MAT-01, MAT-02, MAT-03 (`docs/specifications.md` §8.1), with REP-05 (no standards text), ENG-03 (validation against hand calculations), ENG-04 (deterministic maths). Milestone: Phase 1 – MVP.

## Goal

A project holds **materials**: generic objects with a behaviour law and properties per design code (concrete C25/30 to EN 1992-1-1, steel S355 to EN 1993-1-1, timber GL24h to EN 1995-1-1). A **library** ships the common European grades as data, each value citing its source clause; users and plugins can extend it. A project also holds **build-ups**: ordered layers for walls and slabs, each layer with a material, a thickness, a function and a join priority. The wall, slab and join elements come later (epic #88, GEO-11, ARC-04, ARC-30); this epic gives them tested data to build on.

## Delivery: one stacked PR per sub-issue

| # | Sub-issue | Requirements | What it delivers | User docs |
| --- | --- | --- | --- | --- |
| 1 | [#85](https://github.com/GuillaumeOj/BIMCraft/issues/85) Material model with behaviour laws per design code | MAT-01 | The `resource` entity kind, the new `materials` crate (categories, design codes, property definitions, behaviour laws), the `Material` schema entity, ADR 0018 | N/A (internal change) |
| 2 | [#86](https://github.com/GuillaumeOj/BIMCraft/issues/86) Library of common European material grades | MAT-02 | Library data with sources, the loader and its extension point, the `materials.*` API, the Materials dialog in the UI, e2e test, user guide EN + FR | `docs/user/manage-project-materials.md` (+ `.fr.md`) |
| 3 | [#87](https://github.com/GuillaumeOj/BIMCraft/issues/87) Layered build-ups with join priorities | MAT-03 | The `BuildUp` and `BuildUpLayer` schema entities and their rules, the `buildups.*` API, the build-up editor in the UI, e2e test, user guide EN + FR | `docs/user/create-a-wall-or-slab-build-up.md` (+ `.fr.md`) |

On the integration branch the tasks are merged in stack order, so each sub-issue ends at a known commit. When the feature is done, the lead names the three boundary commits in the PR request; the stack is pushed with `bun scripts/push-pr.ts --stack`, which links each PR to its issue (`Closes #85` / `#86` / `#87`), the `Phase 1 – MVP` milestone and the BIMCraft project. Each commit references its issue and requirement IDs (`feat(materials): … (MAT-01, Refs #85)`). PR 1 says "User docs: N/A (internal change)".

`.orchestrator/` does not ship (earlier user decision): the lead removes this feature's folder in a final commit before the PR request.

## Decisions

### D1. Where materials live: a `materials` crate (layer 3)

- New crate `crates/materials`, in layer 3 beside `georef` and `analysis` (`scripts/crate-layers.txt`). It depends on `core-model`, `core-txn`, `libm`, `thiserror`; `analysis` and the discipline crates will depend on it, never the reverse (ARCH-04).
- It is a core crate: listed in `scripts/critical-crates.txt` (≥ 98 % lines) and `scripts/wasm-crates.txt`, starts with `#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]`, does no I/O (ARCH-02), uses `libm` for powers and roots (ENG-04).
- New dependencies: none expected. Parsing the library data may use `toml` or `serde_json`, already workspace dependencies; the commit body justifies the choice (wasm build, size).

### D2. Storage: schema entities of a new `resource` kind

Materials and build-ups are shared definitions, not elements and not spatial structure. IFC models them as resources (`IfcMaterial`, `IfcMaterialLayerSet`, `IfcMaterialLayer`), which have no GlobalId and no placement.

- Add a fourth entity kind, **`resource`**, to the schema (`crates/schema`, ADR 0002 lists kinds as an open set). Resources take part in no relationship by kind, have no `[type]` and no representation. Validation, generators (JSON Schema, IFC mapping table, reference pages, Python stubs) and their tests are updated. ADR 0002 gets a short amendment note pointing to ADR 0018.
- Storing them as schema entities gives undo/redo (CORE-06), change events (CORE-07), persistence (ADR 0016) and the generic `model.*` API with no new core machinery, as the `Site` did for georeferencing (ADR 0017).
- Each schema change bumps the core namespace version with its snapshot, following `docs/dev/schema-versions.md` (one bump per PR that changes the schema, so each PR is valid on its own), adds the files to `CORE_SCHEMA` in `crates/host-desktop/src/document.rs`, and regenerates `api.schema.json`, `docs/dev/ifc-mapping.md` and the reference pages (`UPDATE_SNAPSHOTS=1 cargo test -p schema-gen`).
- **ADR 0018 "Materials and layered build-ups"** records D2–D6: the kind, the attribute layout, how sources and overrides are kept, priorities and their IFC mapping (`IfcMaterial`, `IfcMaterialProperties`/`Pset_Material*`, `IfcMaterialLayerSet`, `IfcMaterialLayer.Priority` and `.Category`). The IFC read/write of materials is out of scope (no element export yet); only the mapping table entries are declared.

T1 is created `plan_first`: the backend confirms or amends the `resource` kind and the attribute layout in its plan before coding.

### D3. Material model and behaviour laws (#85, MAT-01)

- **Category**: `concrete`, `reinforcement`, `steel`, `timber`, `masonry`, `insulation`, `finish`, `generic`. Non-structural categories exist so build-ups can use plaster, render, EPS, mineral wool and the like.
- **Design code**: `EN 1992-1-1` (concrete, reinforcement), `EN 1993-1-1` (steel), `EN 1995-1-1` (timber, with EN 338 and EN 14080 for grades), `EN 1996-1-1` (masonry), or none for non-structural materials. National annexes: **Recommended** and **France** (NF EN …/NA). The values that differ by annex (partial factors γc, γs, γM0, γM1, γM2, γM, αcc) are stored per annex.
- **Properties** are defined in code as a fixed catalogue per category: key, quantity kind (SI, CORE-09), whether required. Examples: concrete `fck`, `fck_cube`, `fcm`, `fctm`, `ecm`, `eps_c2`, `eps_cu2`, `n`; steel `fy`, `fu`, `e`, `g`, `nu`; reinforcement `fyk`, `k`, `eps_uk`, `es`; timber `fm_k`, `ft0_k`, `fc0_k`, `fv_k`, `e0_mean`, `e0_05`, `g_mean`, `rho_k`, `rho_mean`; every category: `density`, optional `thermal_conductivity` (λ), `specific_heat` (MAT-03 thermal values optional). The exact list is settled in T1's plan and ADR 0018.
- **Behaviour laws**, pure functions in `materials`, each giving stress from strain (characteristic and design), the limit strains and the tangent modulus at zero:
  - concrete: parabola-rectangle and bi-linear for section design, linear elastic (Ecm);
  - reinforcement: bi-linear with horizontal or inclined top branch;
  - structural steel: elastic–perfectly plastic, optionally with a hardening branch;
  - timber: linear elastic to the strength, brittle in tension, plastic in compression;
  - non-structural and masonry: linear elastic, or none (no structural use).
- **ENG-03**: every law is validated against hand calculations written in the tests (with the arithmetic shown in comments), including points on each branch and at each limit strain, and property-based tests (monotonicity, continuity at branch ends, odd symmetry where it applies) with the fixed-seed `proptest_config()`. No standards text in code, data or docs (REP-05): clauses are cited by number only.
- Errors are typed (`thiserror`), never panic on bad input, and never contain model values (SEC-03).

### D4. Library (#86, MAT-02)

- **Contents**: concrete C12/15 to C50/60; reinforcement B500A, B500B, B500C; structural steel S235, S275, S355, S420, S460 (thickness bands as in the source); solid timber C16, C24, C30; glulam GL24h, GL28h, GL32h; masonry: one or two common unit/mortar combinations; non-structural: plaster, mineral render, EPS, mineral wool, wood-fibre board, brick, concrete block, timber cladding. About 40 entries.
- **Data, not code**: library files under `crates/materials/library/` (one per family), embedded with `include_str!` and parsed at first use. Each value carries its **source**: document and clause or table (`EN 1992-1-1 Table 3.1`, `NF EN 1992-1-1/NA 2.4.2.4`). Values that no standard gives (typical λ of a finish) cite their real public source; a value with no source is not shipped. A `README.md` next to the files records the editions used and the rule "numbers and clause numbers only, no text" (REP-05).
- **ENG-03 cross-checks**: tests recompute the derived values that the codes define by formula (for example fcm from fck, Ecm from fcm, fctm from fck) and compare them with the data, within the rounding the source uses.
- **Extension**: a library is built from sources; `Library::with_source(name, contents)` adds a user or plugin source after validating it as hostile input (bounded size and count, unknown keys rejected, finite values in range, duplicate keys reported). Plugin wiring comes with the plugin host (PY-04); this epic ships the tested entry point. **Users** extend the library through project materials: create a material from a grade (values copied, as types copy component defaults in ADR 0014, so a library update never silently changes a project) or from scratch, and edit any value.
- **Sources of project values**: a project material keeps its library key; each value shows the library source, or "Entered by user" once edited. Resetting a value restores the library value and source. How this is stored (for example a list of overridden keys) is fixed in T1's plan and ADR 0018.
- **Deleting** a material used by a build-up layer is refused with a typed `in_use` error carrying the count of users.

### D5. Build-ups (#87, MAT-03)

- **`BuildUp`** (resource, IFC `IfcMaterialLayerSet`): `name`, `usage` (`wall` or `slab`), `layers` (`list:ref:BuildUpLayer`, exterior to interior for walls, top to bottom for slabs, ADR 0010), `description`.
- **`BuildUpLayer`** (resource, IFC `IfcMaterialLayer`): `material` (`ref:Material`, required), `thickness` (length), `function` (`structure`, `substrate`, `insulation`, `air_gap`, `membrane`, `finish`), `priority` (1 highest … 5 lowest), `core` (bool), `name` (optional). The layer's entity id is the stable **LayerId** of ADR 0010: reordering, renaming or resizing never changes it.
- **Join priority**: 1 to 5, as in board 3.3. Defaults by function: structure 1, substrate 2, insulation 3, air gap 3, membrane 4, finish 5; the user may override. At a junction, a layer of higher priority (lower number) runs through layers of lower priority; equal priorities meet. Joins themselves are GEO-11 (epic #88); ADR 0018 fixes the IFC mapping of the priority.
- **Rules**, checked by the `buildups.*` commands and by a `materials` validation function that the future wall and slab code reuses: at least one layer; at most 32 layers; thickness > 0, except a membrane, which may be 0; core layers contiguous; air gaps reference an air or generic material. Violations return typed errors carrying the layer index, never values (SEC-03).
- **Derived values** returned by the API: total thickness; per layer, the thermal resistance d/λ when the material has λ; the sum of layer resistances when every layer has one (surface resistances and U-values belong to the thermal module).
- **Deleting** a build-up deletes its layers in the same undo step. Wall and slab types (epic #88) will reference build-ups; until then nothing else references them.
- **Location line** (ARC-30) belongs to the wall type and is out of scope; the editor leaves room for it (board 3.3).

### D6. User interface (#86, #87)

- **Materials dialog** (design concept: board 4.6 "Material library"): the project materials, a search, the library grades to add, the detail of one material grouped by tabs (Mechanical, Thermal; Graphics and Environmental are not built yet: MAT-04 and DOC work), each value with its unit and source, editing a value, resetting it, a read-only description of the behaviour law, deleting with the `in_use` message.
- **Build-ups** (design concept: board 3.3 "Wall type — layers and location line", without the location line): a list of wall and slab build-ups, then an editor: a layer table (position, function, material, thickness, join priority, core), add / move up / move down / remove, total thickness, thermal resistance when known, a schematic section preview, one undoable Apply.
- **Entry points**: designed by UX. Each dialog opens from the shell, has a keyboard shortcut, and the shortcut is documented (UI-03).
- UI-01: only through the API client (`ui/src/api/`). UI-04: every string in `en.json` and `fr.json`. UI-06: labels, full keyboard use (including reordering layers without a pointer), WCAG AA contrast. Lengths in millimetres in the UI, metres in the core (ADR 0004); stresses in MPa, moduli in GPa or MPa, density in kg/m³, λ in W/(m·K); locale decimal separators handled in the UI.
- **Design file**: the design file is no longer in the repository (it moved out on main). Designers edit the project's shared design file `/Users/guillaume/Library/CloudStorage/GoogleDrive-guillaume.ojardias@gmail.com/My Drive/BIMCraft.figcraft` (a project file: live, unversioned, shared by every agent; re-read before changing, change only your boards, commit nothing of it). Never edit or recreate `docs/BIMCraft.figcraft`; if a stale copy exists in a worktree, ignore it. New boards go after the existing ones: materials on page `4 · Structure modelling` (4.7 onward), build-ups on page `3 · Architecture modelling` (3.8 onward); boards 3.3 and 4.6 stay as concepts. Light and dark boards, `Mode`/`Core` variables, `Brand guide` components reused, French strings in the boards. PNG exports and a `README.md` go in `.orchestrator/features/add-materials-and-layered-build-ups/design/`. Deviations from the design are listed in the PR.

## Out of scope

Walls, slabs, wall and slab types, joins and their geometry (epic #88, GEO-11, ARC-04), the location line (ARC-30), IFC import/export of materials (INT-07, with the element export), environmental data (MAT-04), U-values and thermal checks (thermal module), graphics/hatches of materials (documentation work), member checks that use the laws (ANA-06), a project-wide choice of national annex, plugin host wiring (PY-04).

## Team and tasks

| Task | Node | Sub-issue | Depends on |
| --- | --- | --- | --- |
| T1 `resource` kind, `materials` crate (categories, codes, property catalogue, behaviour laws, ENG-03 tests), `Material` entity, schema version bump, ADR 0018 — plan first | back | #85 | — |
| T2 Library data with sources and README, loader, `Library::with_source`, cross-check tests, `materials.*` API (contract), protocol schema and generated TS types | back | #86 | T1 |
| T3 UX flows for the Materials dialog and the build-up list and editor (`ux-brief.md`) | ux | #86, #87 | — |
| T4 UI design of the Materials dialog in the shared design file (page 4, boards 4.7+), PNG exports | ui | #86 | T3 |
| T5 Materials dialog, API helpers, i18n EN/FR, unit and e2e tests, user guide EN/FR | front | #86 | T2, T4 |
| T6 `BuildUp` and `BuildUpLayer` entities, rules, schema version bump, `buildups.*` API, protocol schema and TS types | back | #87 | T2 |
| T7 UI design of the build-up list and editor (page 3, boards 3.8+), PNG exports | ui | #87 | T4 |
| T8 Build-up list and editor, API helpers, i18n EN/FR, unit and e2e tests, user guide EN/FR | front | #87 | T5, T6, T7 |

Merge order: T1 (end of PR 1); T2, T4, T5 (end of PR 2); T6, T7, T8 (end of PR 3). T3 produces `.orchestrator/` notes only and can merge any time.

## Definition of Done (per task, AGENTS.md §2)

- `materials`, `schema`, `schema-gen`, `api` are critical: ≥ 98 % lines (`cargo llvm-cov -p materials --fail-under-lines 98`). UI: ≥ 98 % lines and branches in `ui/src/api/`, ≥ 90 % elsewhere.
- Targeted checks only (AGENTS.md §3a): `cargo test`/`clippy` on the crates touched, `cargo fmt --all --check`, the `wasm` section for the new core crate, `bun scripts/check-architecture.ts` for the new crate, `scripts/check-all.sh repo` before each commit; `bunx vitest`/`biome`/`tsc` and the Playwright spec for the UI.
- Conventional Commits, signed off, with the issue (`Refs #85`…) and requirement IDs. No `wip` subjects: orchestrated agents cannot reword commits.
- Each task is reviewed by its owner (`/code-review` with fixes, then `/simplify`) before it is reported done.

## Amendment A1 (lead, 2026-11-06): port the existing work, then extend it

The epic already has an implementation on local branches `claude/244-material-objects` (049946f, bcc65b5, f043f25, d661647, 22c21c5), `claude/245-european-grades` (3940bbe) and `claude/246-layer-sets` (02dbe20), written for issues #244–#246, which refine #85–#87, on a main 118 commits old. The user left the choice to the lead. Decision: **port it, don't rewrite it**, then extend it to this spec. It supersedes D1–D5 where they differ:

- **Crate name `material`** (singular, as in the existing work, like `geom` and `georef`). Read `materials` as `material` everywhere above.
- **Kept from the port:** the `resource` kind, schema **enumerations** (closed sets are `enum:` types, not text), property-set docs, **sourced values** (`sourced = true`, `core_model::Source`), and the `material` crate with its laws, library loader (`Library::builtin`, `Library::extend`) and layer sets on `geom::LayerSet`.
- **Renumbering:** the ported ADR "sourced values" becomes **0018**; "Materials and layered build-ups" becomes **0019**. Core schema snapshots follow the Site's 0002: one new version per PR that changes the schema.
- **Gate: values must persist.** Property-set values are not written to the project file yet (ADR 0013, #35). Before T1's code is accepted, a material saved, closed and reopened must keep every value and source. If the ported design stores values in property sets that do not persist, T1 either stores them as sourced attributes or makes those property sets persist. It is the backend's choice, stated in its revised plan.
- **Extensions still owed, per this spec:**
  - T1: "Entered by user" sources and reset with the library original kept (D4); national annexes and partial factors (D3); the 8 categories; the concrete bilinear, timber elastic-brittle and `none` laws; and the limit-strain and initial-modulus queries.
  - T2: the library grows from 13 grades to about 40 (D4), and the `materials.*` API.
  - T6: `usage`, `function` and `description`; priority 1–5 with defaults by function, mapped to IFC 0–100 in ADR 0019; and the `buildups.*` API. The existing entity names `MaterialLayerSet` / `MaterialLayer` are kept instead of `BuildUp` / `BuildUpLayer` (IFC names); the API stays `buildups.*`.
- **Commits:** ported commits keep their subject and body, with the references rewritten to this stack's issue (`Refs #85` for T1, `#86` for T2, `#87` for T6) and `#244`/`#245`/`#246` kept as `Refs`. Each ends with the sign-off. The user docs `choose-a-material.md` (EN/FR) from 3940bbe are merged into T5's page `manage-project-materials.md`, so that only one page remains.

## Stack boundaries (lead log)

- PR 1 (#85, MAT-01): ends at `219f8bc` (merge of T1, task 273).
- PR 2 gate (user decision): the user checks the library values written from memory (EN 338 C18/C22/C27/C35, EN 14080 GL28h/GL32h, the French annex partial factors, the EN ISO 10456 values of the 8 non-structural materials, and the cited law clauses) before the PR request. Lead reminds the user at PR time.
- PR 2 (#86, MAT-02): ends at `dca3b3e` (merge of T5, task 277; T2 merged before it).
- PR 3 (#87, MAT-03): ends at `186cee7` (merge of T8, task 280), plus the design review fixes if any.
- Library value fixes (task 302, T10, after the user's engineer review in `library-review.md`) land at the top of PR 3, with commits marked Refs #86. The PR 3 body says so.
- PR 3 also carries the design review (task 301, `design/review.md`) and its fixes (task 303, T11; commits marked Refs #87, the Materials ones Refs #86). Integration HEAD after both merges: `976536a`.
- PR-time fix: commits 3b9df09, b6c0f19, 4c1debd, 13ead78, a942e0f, 10f8f1b (lead) and ba0d987, 81fcfca (ui) have no `Signed-off-by:`. They touch only `.orchestrator/`, which does not ship. Drop or sign them when the stack is cut, before CI's commit check.
