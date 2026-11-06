# Materials and build-ups — UX brief

For `ux` (T3), then `ui` (T4, T7). Read `spec.md` (D3–D6) and `api-contract.md` first. Users are architects and structural engineers, French first in practice (UI-04: boards carry French strings, about 30 % longer than English).

## Starting points in the design file

Open `docs/BIMCraft.figcraft` (repository copy, Git LFS; see `docs/dev/ui-design.md`):

- Page `4 · Structure modelling`, board **4.6 "Material library"**: a modal with a searchable list on the left and the selected material's properties (value, source) in tabs on the right. A concept, not a spec.
- Page `3 · Architecture modelling`, board **3.3 "Wall type — layers and location line"**: a layer table (#, function, material, thickness, join priority), Add / Move up / Move down / Remove, total, a schematic section preview, Apply. The location line part is **out of scope** here (ARC-30, later); the build-up editor must be reusable inside that future wall-type editor.
- Page `2 · Site and georeferencing`, boards 2.4–2.13 and `Brand guide › 8 · Form and dialog components`: the latest dialog patterns (field errors, server error banner, inline confirmation, focus ring). Reuse them.

## Flows to design

### F1. Materials (#86)

1. Open the Materials dialog from the shell (entry point + keyboard shortcut to propose; UI-03).
2. Browse the **project materials** (empty state for a new project: explain and offer "Add from library").
3. **Add from library**: search or filter by category the ~40 grades (C25/30, S355, B500B, GL24h, EPS…), see a grade's values before adding, add it (optionally renamed). Adding the same grade twice needs a distinct name.
4. **Create a custom material**: name + category, then values.
5. **Read a material**: tabs Mechanical and Thermal (Graphics and Environmental not built yet: hide them or show them disabled, your call, say why). Each row: property name with its symbol, value with unit, source ("EN 1992-1-1 · Table 3.1", or "Entered by user"). Partial factors show both the recommended value and the French national annex value. The behaviour law appears read-only with its source; a small stress–strain sketch is welcome but optional (no API for curve points; decide whether it is worth asking the backend for one).
6. **Edit a value**, see it marked as entered by user, **reset** it to the library value. Validation errors per field (`invalid_value`, `required_property`) and a server error banner.
7. **Rename**, **delete** (inline confirmation; `in_use` → explain that N build-up layers use it, and that it cannot be deleted until they change material).

### F2. Build-ups (#87)

1. Open the build-up list from the shell (entry point + shortcut; may share a "Types and materials" area with F1 — your call).
2. List wall and slab build-ups (filter by usage), empty state, create (wall or slab), duplicate, rename, delete (inline confirmation).
3. **Edit a build-up**: layer table — position, function, material (picker over project materials, with a way to reach F1 to add one), thickness in mm, join priority 1–5 (default from function, shown as default until overridden), core flag. Add, move up, move down, remove; **keyboard-only reordering** (UI-06) and its announcement for screen readers.
4. Total thickness; thermal resistance per layer and summed when every material has λ ("—" otherwise, with why).
5. Schematic section preview: exterior/interior (wall) or top/bottom (slab) labelled, core layers marked, selected layer highlighted, "schematic, not to scale".
6. Errors: `no_layers`, `invalid_thickness`, `core_not_contiguous` (point at the layers), `unknown_material`; server error banner. One **Apply** = one undo step; Cancel discards.
7. Explain join priority in plain language in the dialog (a short help text or tooltip): "When two walls meet, layers with a higher priority (1) run through layers with a lower priority (5)."

## Deliverables

- **ux (T3)**: `design/ux-notes.md` in this folder: the flows above as numbered steps with every state (empty, filled, editing, field error, server error, confirmation, in use), focus order and shortcuts, the English copy of every string, and open questions. Low-fidelity wireframes in the design file are optional; if you add them, put them on a scratch page you delete before handing off, or in the boards' area of `ui`, and tell `ui`.
- **ui (T4)**: boards 4.7+ on page `4 · Structure modelling` for F1, light and dark, French strings, new components in `Brand guide` if any, contrast checked with `ui/src/shell/contrast.ts`, PNG exports and `design/README.md` mapping boards to images (as `.orchestrator/features/georeferencing/design/README.md` does).
- **ui (T7)**: boards 3.8+ on page `3 · Architecture modelling` for F2, same rules, added to the same README.
