# Georeferencing — feature spec

Epic [#79](https://github.com/GuillaumeOj/BIMCraft/issues/79). Requirements: GEOREF-01, GEOREF-02, GEOREF-03, GEOREF-04 (`docs/specifications.md` §7). Milestone: Phase 1 – MVP.

## Goal

Each project is placed on Earth through a projected coordinate reference system (CRS). Modelling stays in the local engineering frame near the origin (ADR 0001: projected coordinates never enter the geometry kernel). The link between the two follows IFC 4.3 (`IfcMapConversion`, `IfcProjectedCRS`). Transformations are written from scratch in Rust, CRS definitions are data taken from the EPSG registry, and users can read and enter coordinates in both systems.

## Delivery: one stacked PR per sub-issue

The four sub-issues ship as a stack of four pull requests, in this order. Each one builds on the one below it.

| # | Sub-issue | Requirements | What it delivers | User docs |
| --- | --- | --- | --- | --- |
| 1 | [#80](https://github.com/GuillaumeOj/BIMCraft/issues/80) Project CRS, survey point, base point and map conversion | GEOREF-01, GEOREF-02 | The new `georef` crate: georeference data, the map conversion (local ↔ projected), the `Site` schema entity that stores the data, and the IFC round-trip of `IfcMapConversion` and `IfcProjectedCRS` | N/A (internal) |
| 2 | [#81](https://github.com/GuillaumeOj/BIMCraft/issues/81) Transverse Mercator, Lambert Conformal Conic and Helmert shifts | GEOREF-03 | Projection maths: ellipsoids, geographic ↔ geocentric, TM, LCC (1SP and 2SP), 7-parameter Helmert, meridian convergence and point scale factor. Within 1 mm of PROJ on a committed test grid | N/A (internal) |
| 3 | [#82](https://github.com/GuillaumeOj/BIMCraft/issues/82) Load CRS definitions from the EPSG registry | GEOREF-03 | The MVP CRS set as embedded data extracted from the EPSG dataset, a registry that parses it, typed errors for unknown codes, and the redistribution terms checked and recorded | N/A (internal) |
| 4 | [#83](https://github.com/GuillaumeOj/BIMCraft/issues/83) Read and pick coordinates in local and projected systems | GEOREF-04 | The `georef.*` API commands and queries, the "Project location" dialog and the "Coordinates" panel in the UI, end-to-end test, user guide page (EN + FR) | `docs/user/set-up-the-project-location.md` (+ FR) |

On the integration branch, the tasks of each sub-issue are merged in stack order, so each sub-issue ends at a known commit. When the feature is done, the lead names those four boundary commits in the PR request, and the user pushes the stack (`bun scripts/push-pr.ts --stack`). Each commit references its issue and requirement IDs. The three backend PRs say "User docs: N/A (internal change)".

## Decisions

### D1. Where the georeference is stored: a `Site` entity

The schema defines no entity yet, and no project-settings store goes through transactions. The georeference is therefore stored as attributes of a new spatial schema entity, `Site` (`schema/core/entities/site.toml`, kind `spatial`, IFC mapping `IfcSite`). This entity is the first one in the schema. In return, undo/redo (CORE-06), change events (CORE-07), persistence (attribute tables, ADR 0016), the generic `model.*` API and the IFC mapping table (CORE-10) all work with no new core machinery.

- A project has at most one `Site` for now. The `georef.set` command creates it when it is missing. Reading a georeference from a model with more than one `Site` returns a typed error.
- The `Site` keeps only the georeference attributes below. Later work (address, spatial structure) adds attributes through normal schema migrations.
- Add the entity to `CORE_SCHEMA` in `crates/host-desktop/src/document.rs`, bump the schema version and snapshot it (`docs/dev/schema-versions.md`), and regenerate the reference pages and `docs/dev/ifc-mapping.md`.

| Attribute | Value | Default | Meaning |
| --- | --- | --- | --- |
| `crs_epsg` | `integer` | unset | EPSG code of the projected CRS (e.g. 2154). Unset: the project is not georeferenced |
| `base_point_eastings` | `quantity:length` | 0 | Easting of the project base point, which is the origin of the local frame |
| `base_point_northings` | `quantity:length` | 0 | Northing of the project base point |
| `base_point_height` | `quantity:length` | 0 | Orthogonal height of the project base point |
| `grid_rotation` | `quantity:plane_angle` | 0 | Angle from the CRS grid easting axis to the local x-axis, counter-clockwise. It sets `XAxisAbscissa`/`XAxisOrdinate` |
| `scale` | `real` | 1.0 | Map conversion scale, finite and > 0 |
| `survey_point_eastings` / `_northings` / `_height` | `quantity:length` | unset | Optional survey point: a known point (benchmark), given in projected coordinates. Its local position is derived |

The `georef` crate turns these attributes into the typed `Georeference` and back. The data model and the maths live in `georef` (layer 3). It reads the model through `core-model` (layer 2). Nothing below layer 3 knows about CRSs.

### D2. Map conversion (GEOREF-02)

The map conversion follows the semantics of IFC 4.3 ADD2 `IfcMapConversion`. Local point (x, y, z) in metres, θ = `grid_rotation`, s = `scale`:

- E = s·(x·cos θ − y·sin θ) + E₀, N = s·(x·sin θ + y·cos θ) + N₀, H = z + H₀ (the backend checks in the EXPRESS schema and its documentation whether s applies to z and follows IFC). The inverse is exact up to rounding.
- Grid rotation is what is stored. **True north** is derived from it with the meridian convergence of the projection at the base point (sub-issue 2) and is shown read-only. GEOREF-02's "rotation (true north)" is met by storing the grid rotation and deriving true north.
- Trigonometry uses `libm` (ENG-04). Angles are converted with `x * (PI / 180.0)`.

### D3. IFC round-trip (#80)

The model-to-IFC export does not exist yet, so the mapping is scoped to the georeference. In `interop-ifc`, which may depend on `georef` because both are in layer 3, a `Georeference` is written as `IfcProjectedCRS` (Name `EPSG:<code>`) and `IfcMapConversion` (SourceCRS = the project's 3D model `IfcGeometricRepresentationContext`, TargetCRS = the `IfcProjectedCRS`), and read back. The round trip read → write → read is lossless, and file → model → file keeps unrelated instances through `preserve` (INT-03). Imports validate everything (SEC: hostile input): non-finite numbers, zero scale, unknown CRS names and dangling references are reported, never panicking.

The survey point has no IFC 4.3 attribute. The backend proposes its mapping in **ADR 0017 "Georeference storage and IFC mapping"**: a BIMCraft property set on `IfcSite`, or the `IfcSite` placement. It goes in the plan of task 1. The constraint: BIMCraft → IFC → BIMCraft must not lose it, and it must not change the meaning of `IfcMapConversion` for other tools.

### D4. Projections (#81)

- Transverse Mercator: the Poder/Engsager extended Krüger series that PROJ uses (`tmerc`, default algorithm), which stays accurate well beyond a UTM zone. Lambert Conformal Conic: 2SP for Lambert 93 and CC42–CC50, 1SP supported. Helmert: 7 parameters, position-vector and coordinate-frame conventions, through geocentric coordinates. All forward and inverse.
- Also computed: meridian convergence and point scale factor, used for true north and as information in the UI.
- Accuracy: within 1 mm of PROJ on a committed test grid covering each MVP CRS over its area of use (and the full UTM zone width). The reference values are a committed golden file with its provenance (PROJ version, command), generated by a script under `crates/georef/tests/` or `scripts/`. Tests never call PROJ (deterministic, no network). PROJ is not installed on this machine: the backend tries to install it (`brew install proj`); if it cannot, it asks the lead, who asks the user to generate the file.
- Deterministic across targets (ENG-04): `libm` only, plus a golden test of exact bits in the style of `crates/geom/tests/golden.rs`. Property-based tests (forward∘inverse round trip, monotonicity) with the fixed-seed `proptest_config()`.

### D5. EPSG registry (#82)

- MVP set (spec §7): WGS 84 (4326), Lambert 93 (2154), CC42–CC50 (3942–3950), UTM zones on ETRS89 (25828–25838) and WGS 84 (32601–32660, 32701–32760). It includes their base geographic CRSs, datums, ellipsoids, prime meridians and, where EPSG defines one, the Helmert transformation to WGS 84.
- The definitions are a data file extracted from the EPSG dataset (for example from PROJ's `proj.db`, which embeds it), embedded with `include_str!` and parsed at first use. No CRS parameters are hard-coded in Rust. An extraction script and a `README.md` next to the data record the source, the dataset version, the retrieval date, the SHA-256 and the terms, in the style of `crates/interop-ifc/schema/README.md`.
- **Redistribution terms**: the backend reads the EPSG Dataset Terms of Use and records them. If they forbid or restrict embedding the subset in an MIT-licensed project, the lead asks the user before any data is committed.
- Unknown codes, unsupported projection methods and malformed data return typed `thiserror` errors. Messages never contain model values (SEC-03).

### D6. Coordinates in the UI (#83)

- **Project location dialog**: pick the CRS from the registry (search by code or name), enter the base point coordinates, the grid rotation, the scale and the optional survey point. It shows the derived true north and the geographic position of the base point. It applies the whole georeference as one undoable step (`georef.set`) or removes it (`georef.clear`).
- **Coordinates panel**: enter a point in local, projected or geographic coordinates and read it in the other two systems (`georef.convert`). Each value can be copied.
- **Scope limit, "pick"**: there is no viewport renderer yet, so picking a point in a view with the pointer cannot be built. In this feature, "pick" means entering a point in either system. The panel is written so that a future viewport can push the hovered or clicked point into it. Pointer picking is left to the viewport work, and the PR says so.
- UI-01: the UI goes through the API client (`ui/src/api/`) only. UI-04: every string goes into `en.json` and `fr.json`. UI-06: labels, full keyboard use, WCAG AA contrast. UI-03: the dialog and the panel can be opened by a keyboard shortcut and the command palette if one exists; otherwise the shortcut is documented.
- Numbers: lengths in metres with mm precision. Angles in degrees, or DMS through the core's angle formats. Geographic coordinates on the CRS's own geodetic datum, labelled with its name (e.g. "RGF93 v1"). Locale decimal separators are handled in the UI (ADR 0004).

## Out of scope

Vertical datums (GEOREF-05), grid shifts such as NTv2 and geoid grids (spec §7.4, V1), terrain, alignments, context data, sun position (GEOREF-06 to 09), CRS-to-CRS reprojection of imported site data, pointer picking in a viewport, and the general model-to-IFC export beyond the georeference.

## Team and tasks

| Task | Node | Sub-issue | Depends on |
| --- | --- | --- | --- |
| T1 `georef` crate, `Site` entity, map conversion, IFC round-trip, ADR 0017 (plan first) | back | #80 | — |
| T2 Projections and Helmert, PROJ test grid | back | #81 | T1 |
| T3 EPSG registry data, loader, terms recorded | back | #82 | T2 |
| T4 UX flows for the location dialog and the coordinates panel | ux | #83 | — |
| T5 UI design in Penpot, frames exported to `.orchestrator/features/georeferencing/design/` | ui | #83 | T4 |
| T6 `georef.*` API commands and queries, protocol schema, generated TS types | back | #83 | T3 |
| T7 Location dialog and coordinates panel, i18n EN/FR, unit and e2e tests, user guide EN/FR | front | #83 | T5, T6 |

Merge order on the integration branch: T1, T2, T3 (boundaries of the first three PRs), then T5, T6, T7 (fourth PR). The design work (T4, T5) runs in parallel with the backend.

## Definition of Done (per task, AGENTS.md §2)

- `georef` and `interop-ifc` are critical: ≥ 98 % line coverage (`cargo llvm-cov -p georef --fail-under-lines 98`). The UI meets ≥ 98 % lines and branches in `ui/src/api/` and ≥ 90 % elsewhere.
- Targeted checks only (AGENTS.md §3a): `cargo test/clippy -p georef -p interop-ifc`, `cargo fmt --all --check`, the `wasm` section for the new core crate, `scripts/check-all.sh repo` before each commit, and `bunx vitest`/`biome`/`tsc` for the UI.
- New dependencies: none expected. Any addition is justified in the commit body (AGENTS.md §4).
- Conventional Commits, signed off (`-s`), with the issue (`Refs #80`…) and the requirement IDs.
- Each task is reviewed by its owner (`/code-review` with fixes, then `/simplify`) before it is reported done.
