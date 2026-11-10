# Solids by sweep and revolution — feature spec

Issue [#74](https://github.com/GuillaumeOj/BIMCraft/issues/74), part of the epic "Geometry for MVP elements" ([#46](https://github.com/GuillaumeOj/BIMCraft/issues/46)). Requirement: **GEO-02** (`docs/specifications.md` §6): "Solids MUST be creatable by extrusion, by sweep along a path, and by revolution." Extrusion exists (`Solid::extrude`, #51); this feature adds the other two. Milestone: Phase 1 – MVP.

## Goal

Two new constructors in the geometry kernel (`crates/geom`, a critical crate):

- `Solid::sweep`: moves a `Profile` along a 3D path of lines and circular arcs (curved beams, handrails, wall sweeps, later the bars of GEO-08).
- `Solid::revolve`: turns a `Profile` around an axis in its plane, by a partial or full turn (round bases, domes, rings, turned members).

Both return a valid `Solid` (planar faces, checked by `Solid::from_faces`) or a typed error. Volumes match hand calculations.

## Delivery

- **One pull request**, closing #74. Commits reference `GEO-02` and `Refs #74`, are Conventional Commits, signed off. No `wip` subjects: orchestrated agents cannot reword commits.
- **Internal change**: no UI, no API command, no schema change. The PR states "User docs: N/A (internal change: kernel constructors with no user-facing command yet)".
- Only the `back` node works on this feature. There is no design, UX or frontend work.
- `.orchestrator/` does not ship (earlier user decision): the lead removes this feature's folder in a final commit before the PR request.

## Decisions

### D1. Planar faces, faceted curves (ADR 0008 extended)

The B-rep has planar faces only (GEO-03; curved surfaces are GEO-09, Later). As for extrusion, arcs become chords within `ARC_CHORD_TOLERANCE` (1 mm). Here the faceting has two sources:

- **Profile arcs**: faceted with `Profile::faceted()`, as extrusion does.
- **The curve traced by the motion**: a path arc, or the circle of a revolution. Profile points far from the path or from the axis move on a larger circle than the path itself. The step count is therefore computed with the **largest radius any profile point travels on**: R + d for a path arc of radius R and a profile reaching d from the path, r_max for a revolution. So no point of the solid is more than 1 mm from the exact swept shape. The formula is ADR 0008's: `n = ⌈ |θ| / (2 acos(1 − s / r)) ⌉`, with `libm`, and these minimums:
  - path arc: n ≥ 1, and each step turns by at most 90°;
  - revolution: each step turns by at most 90° (so a full turn has at least 4 steps).
- The total face count is bounded (a constant such as `MAX_SOLID_FACES`, chosen by the backend and justified in the ADR). Counting stops as soon as the bound is passed, so hostile input cannot allocate millions of faces. Exceeding it is a typed error.

Between two consecutive sections of a sweep or a revolution, the side face of a profile edge is a trapezoid: its two section edges are parallel. So every side face is planar by construction.

The backend writes **ADR 0020 "Sweeps and revolutions"**. It records D1 to D5, the constants and the rejected alternatives. It updates ADR 0008's "Consequences" with a pointer to it.

### D2. Revolution (`Solid::revolve`)

- **Input**: the `Profile` in the plane z = 0 (as for extrusion), an axis in that plane (a 2D point and a 2D direction, not zero), and an angle in radians, 0 < angle ≤ 2π. A positive angle turns by the right-hand rule around the axis direction. The result is in model coordinates: the profile at angle 0 is the input profile, in z = 0.
- **The profile lies on one side of the axis.** Points on the axis are allowed (cones, solid cylinders, spheres): a vertex on the axis is one vertex of the solid, not one per step; a profile edge along the axis gives no face. A faceted vertex within `LINEAR_TOLERANCE` of the axis counts as on it (ADR 0001 merge). A profile that crosses the axis is rejected.
- **Partial turn**: two cap faces (the profile at the start and end angles), one shell. Each hole adds a tunnel from cap to cap: genus = number of holes.
- **Full turn** (angle = 2π exactly): no caps, the last section is the first one. A profile that does not touch the axis gives a ring (genus 1). A profile that touches it gives a solid of genus 0. Each profile hole gives a void shell (a closed ring-shaped cavity, genus 1, negative volume).
- An angle so close to 2π that the end cap comes within `LINEAR_TOLERANCE` of the start cap, at the largest radius, is rejected (no "almost closed" solid).
- **Volume** (hand calculation): Pappus for the exact shape, V = A · θ · r̄, with r̄ the distance from the axis to the profile's centroid. The faceted solid has, for n steps of angle α = θ/n, V = n · sin α · A_f · r̄_f, where A_f and r̄_f are those of the faceted profile. Tests check both: the faceted formula to rounding, Pappus within the ADR 0008 bound.

### D3. Sweep (`Solid::sweep`)

- **Path**: a new public `Path` type, validated at construction like `Profile`. It is a chain of 3D segments, each ending exactly where the next one starts:
  - `PathSegment::Line { start, end }`;
  - `PathSegment::Arc { start, through, end }`: the circular arc from `start` to `end` through `through`. Three distinct, non-collinear points (within tolerance) fix the arc's plane, radius and sense, with no ambiguity. Full circles cannot be given this way.
- Path validation rejects, with a typed `PathError`: no segment, too many segments, non-finite or out-of-extent coordinates, a gap between segments, zero-length lines, degenerate arcs (collinear or coincident points), and **closed paths** (last end = first start; see Out of scope).
- **Profile placement**: the profile's origin is at the path's start point, in the plane normal to the path's start tangent t. The caller gives a `reference` vector: the profile's y axis points along the part of `reference` normal to t, and its x axis is y × t, so (x, y, t) is right-handed. A `reference` parallel to t (within tolerance), zero or not finite is rejected. For a beam, `reference` is usually world z.
- **Frame along the path: rotation-minimizing (parallel transport).** Along a line the frame does not turn. Along an arc it turns with the tangent, around the arc's normal. At a corner it turns by the smallest rotation that takes the incoming tangent to the outgoing one. A path in one plane therefore keeps the profile's orientation relative to that plane, and a beam path in a vertical plane keeps the profile upright. A fixed-reference mode (IFC `IfcFixedReferenceSweptAreaSolid`) is out of scope.
- **Corners** (tangent discontinuities, and the joints between the chords of a faceted path arc) are **mitre joints**: the section there is the profile projected onto the plane that bisects the corner. Turns of 180° (the path doubles back) are rejected.
- **Ends**: two cap faces, the profile at the start and at the end. One shell, genus = number of profile holes.
- **No self-intersection.** `Solid::from_faces` does not check that faces avoid each other, so the sweep must:
  - *locally*: on a path arc of radius R, every profile point stays less than R − `LINEAR_TOLERANCE` from the path, measured towards the arc's centre (otherwise the inner side folds over itself); at every corner and along every straight piece, each profile vertex must keep a length greater than `LINEAR_TOLERANCE` between the two mitre planes of the piece. Both conditions are linear in the profile point, so checking the faceted profile's vertices is exact.
  - *globally*: two pieces of the path that are not neighbours must stay more than 2 · d_max apart (d_max: the largest distance from the path to a profile point). This test is conservative: it may reject a legal sweep whose distant parts come close without touching. It is sound, simple and bounded (O(pieces²), with the piece count bounded). The ADR records the trade-off.
- **Volume** (hand calculation): for a path of straight pieces with mitres, V = A_f × the length of the polyline traced by the profile's centroid (each piece contributes A_f times its centroid length between its mitre planes). When the path goes through the centroid, V = A_f × path length. A straight path perpendicular to the profile gives the same solid volume as `Solid::extrude`. For a path arc, the faceted result is checked against the step formula; the exact torus segment (Pappus) is checked within the ADR bound.

### D4. Errors and security (SEC-03, AGENTS.md §8)

- New `thiserror` enums: `PathError` (path validation), `SweepError` and `RevolutionError` (each wraps `ProfileError` for faceting and `BrepError` for the final check, like `ExtrusionError`). No variant carries coordinates; segments and loops are identified by index, as in `ProfileError`.
- Every input is hostile: NaN, infinities, subnormals, out-of-extent values, zero vectors, huge face counts. No panic on any input; `unwrap`/`expect`/`panic` are banned in non-test code.

### D5. Determinism and placement in the code (ENG-04, ARCH-01/02)

- Trigonometry with `libm` only. No platform maths, no `std::time`, no allocation proportional to unbounded input.
- New modules under `crates/geom/src/brep/` (for instance `revolve.rs`, `sweep.rs`) and `crates/geom/src/path/` for the `Path` type. A shared helper for the step count (ADR 0008's `chord_count`, generalised to a given radius) is factored once and used by profile faceting, sweep and revolution.
- Public items documented (`missing_docs`); the crate docs in `crates/geom/src/lib.rs` describe the two constructors.
- `crates/geom/tests/golden.rs`: new cases for a sweep and a revolution (vertices and volume bits), so a target that rounds differently fails. Regenerate `golden.txt` natively with `BIMCRAFT_BLESS=1` and check the wasm run (`scripts/check-all.sh wasm`) still passes.

## Tests (AGENTS.md §5, ENG-02)

- **Unit tests** next to the code, with hand calculations written in the test comment, at least:
  - revolution: a rectangle off the axis, full turn → hollow cylinder (tube), faceted formula and Pappus; a rectangle with one side on the axis → solid cylinder (no face for that side, genus 0); a right triangle with a vertex on the axis → cone; a half disc on the axis → sphere; a partial turn (90°) with caps; a profile with a hole, partial (tunnel, genus 1) and full (void shell, negative volume); every error case.
  - sweep: straight path = extrusion volume; an L-shaped path with a mitre (V = A × centroid length); a quarter-circle path (torus segment, step formula and Pappus bound); a 3D path (horizontal then inclined, e.g. a stair handrail), checking the profile stays upright; a profile with a hole (genus 1); the frame rule (parallel transport) on a planar path; every rejection (closed path, 180° turn, arc radius smaller than the profile, mitres crossing on a short piece, distant pieces too close, reference parallel to the tangent, too many faces).
  - path validation: every `PathError`.
- **Property-based tests** (`proptest`, fixed seed `proptest_config()` like the other `crates/geom/tests/*_properties.rs`): random profiles and axes/paths in the model extent give a valid solid or a typed error, never a panic; accepted results have the expected shell count and genus and a volume matching the faceted formula.
- **Fuzz targets** in `fuzz/` (`sweep`, `revolution`), with seeds, registered in `fuzz/Cargo.toml` and in the fuzz docs (`docs/dev/quality.md#fuzzing`). Same invariants as the property tests.
- **Coverage**: `cargo llvm-cov -p geom --all-features --fail-under-lines 98`; each uncovered line justified.
- **Checks**: `cargo test -p geom`, `cargo clippy -p geom --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, `scripts/check-all.sh fuzz` (targets changed), `scripts/check-all.sh wasm` (golden file), `scripts/check-all.sh repo` before each commit. Zero warnings.

## Out of scope

- **Closed paths** (a sweep around a loop, giving a ring): rejected with a typed error. A ring in one plane is a full revolution. Closed sweeps need the frame to close up after the loop, which parallel transport does not guarantee on a 3D path. Later issue.
- Fixed-reference frames, twist along the path, scaling or tapering along the path.
- Paths made of splines, clothoids or any curve other than lines and circular arcs.
- True curved faces (GEO-09).
- Elements using these solids (walls, beams, rebar), IFC export of `IfcRevolvedAreaSolid` / `IfcSweptAreaSolid` variants, API commands, UI. They come with the elements that need them.

## Tasks

| # | Node | Task | Depends on |
| --- | --- | --- | --- |
| T1 | back | Revolution: `Solid::revolve`, `RevolutionError`, the shared step-count helper, ADR 0020 (D1–D5 for both constructors), ADR 0008 pointer, unit, property and golden tests, fuzz target, crate docs. Plan first. | — |
| T2 | back | Sweep: the `Path` type and `PathError`, `Solid::sweep`, `SweepError`, the self-intersection checks, unit, property and golden tests, fuzz target, crate docs, ADR 0020 completed if the plan changed anything. Plan first, Opus. | T1 |

T1 goes first because it is smaller and puts the shared helper and the ADR in place; T2 builds on both. Both touch `lib.rs`, `golden.txt` and `fuzz/Cargo.toml`, so they run one after the other, not in parallel.

## Definition of Done (per task, AGENTS.md §2)

Requirement ID in commits; tests as above; ≥ 98 % lines on `geom`; clippy, fmt, typos, taplo clean with zero warnings; `cargo deny check` clean, no new dependency; commits signed off, Conventional Commits, `Refs #74`; owner's `/code-review` with fixes then `/simplify` before reporting done.
