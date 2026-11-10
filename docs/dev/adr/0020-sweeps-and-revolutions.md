# 0020. Sweeps and revolutions

- Status: Accepted
- Requirements: GEO-02, GEO-03, GEO-09 (later), ENG-04
- Issues: sweep and revolution [#74](https://github.com/GuillaumeOj/BIMCraft/issues/74), epic [#46](https://github.com/GuillaumeOj/BIMCraft/issues/46)

## Context

GEO-02 asks for solids by extrusion, by sweep along a path and by revolution. Extrusion exists (`Solid::extrude`, [ADR 0008](0008-arc-faceting.md)). A sweep moves a `Profile` along a 3D path of lines and circular arcs (curved beams, handrails, wall sweeps); a revolution turns it around an axis of its plane (round bases, domes, rings, turned members).

The B-rep has planar faces only (GEO-03); true curved faces are GEO-09, Later. So both constructors facet what they trace, as extrusion facets arcs. Two new questions arise: how finely to facet a curve that no input arc describes (the circle a point travels on), and what a solid must avoid so that `Solid::from_faces`, which checks each face and edge but not that faces avoid each other, never accepts a solid that crosses itself.

## Decision

This record covers both constructors, `Solid::revolve` and `Solid::sweep`, so that they share their rules.

### D1. Planar faces, faceted curves

- **Profile arcs** become chords with `Profile::faceted()`, within `ARC_CHORD_TOLERANCE` (1 mm), as for extrusion.
- **The curve traced by the motion** (the circle of a revolution, an arc of a sweep path) is also cut into chords within 1 mm, with the **largest radius any profile point travels on**: `r_max` for a revolution (the farthest profile vertex from the axis), `R + d` for a path arc of radius `R` with the profile reaching `d` from the path. A point nearer the axis travels on a smaller circle, so its sagitta is smaller still: no point of the solid is more than 1 mm from the shape the faceted profile sweeps.
- **One formula.** ADR 0008's `n = ⌈ |θ| / (2 acos(1 − s / r)) ⌉` is a single helper, `step_count(radius, sweep, max_step, minimum)`, used by profile faceting (no step limit), by revolution and by sweep. It uses `libm` (ADR 0003) and returns a whole-valued `f64`, so callers compare it with their budget before converting it.
- **Steps are at most 90°** (`max_step = π/2`), so a full turn has at least 4 steps, however small the radius: below half the tolerance a single chord could otherwise span half a turn, which gives faces that flip over. A sweep along a path arc takes at least one step.
- **Side faces are planar by construction.** Between two sections of a revolution, a profile edge sweeps a trapezoid (its two section edges are parallel, both perpendicular to the axis); an edge with a vertex on the axis sweeps a triangle. Between two sections of a sweep joined by mitres the same holds.
- **Face budget.** A solid has at most `MAX_SOLID_FACES = 65 536` faces (16 × `MAX_PROFILE_EDGES`, 4096). The count is computed from the step count and the number of profile edges *before* any face is built, so hostile input cannot allocate millions of them; exceeding it is a typed error (`TooManyFaces`). The bound keeps the memory (a few megabytes) and the validation of one solid, which is linear in its faces, small and predictable, and it admits every ordinary element: a unit sphere takes 36 × 71 = 2 556 faces, a ring of 20 m radius with a rectangular section 4 × 280 = 1 120. At a radius of 1 m (71 steps) a full turn admits a profile of up to 922 edges; at 14 km from the axis, near the limit of the model extent (about 8 300 steps), one of up to 7. The limit is a safety net, not a design capacity, and raising it later only admits more inputs.

### D2. Revolution (`Solid::revolve`)

- **Input**: the `Profile` in the plane z = 0, an axis in that plane (`RevolutionAxis`: a 2D point and a non-zero `Vector2` direction), and an angle in radians, 0 < angle ≤ 2π. A positive angle turns by the right-hand rule around the axis direction. The result is in model coordinates; the start section is the input profile bit for bit.
- **The profile lies on one side of the axis.** A vertex within the tolerance of the axis (`WITHIN_TOLERANCE`, ADR 0001) is **on** the axis: it is a single vertex of the solid, not one per step, and a profile edge between two vertices on the axis gives no face. An edge with one end on the axis gives triangles (cones), with both ends off it quadrilaterals. A profile with vertices on both sides is rejected (`ProfileCrossesAxis`).
- **Partial turn**: two end faces (the profile at the start and end angles) and one shell; each profile hole is a tunnel from end face to end face, so genus = number of holes.
- **Full turn** (angle exactly 2π): no end faces, the last section is the first. A profile away from the axis gives a ring (genus 1); one that touches it, a solid of genus 0; each profile hole gives a closed ring-shaped cavity, a void shell of genus 1 with negative volume.
- **Pinched axis vertex.** In a full turn, a vertex on the axis whose two profile edges both leave the axis would be the tip of two cones: the faces around it form two circles, not one fan, and the solid touches itself at a point. It is rejected (`PinchedOnAxis`). A vertex next to an edge along the axis is fine. In a partial turn the two end faces join the circles into one fan, so the same profile is accepted. In a full turn, a vertex between two edges along the axis lies in no face and is left out of the solid.
- **No almost closed solid.** An angle past a half turn whose end faces come within the tolerance of each other at `r_max` is rejected (`NearlyFullTurn`); an angle of exactly 2π is the full turn.
- **Volume**: Pappus for the exact shape, V = A · θ · r̄ (r̄: distance of the profile's centroid to the axis). For n steps of α = θ/n the faceted solid has V = n · sin α · A_f · r̄_f, with A_f and r̄_f those of the faceted profile: each step is a trapezoid prism, whose volume is sin α times the profile's first moment about the axis. Tests check the faceted formula to rounding and Pappus within the bound below.

### D3. Sweep (`Solid::sweep`)

- **Path**: a new public `Path` type, validated at construction like `Profile`: a chain of 3D segments, each ending exactly where the next begins, either `Line { start, end }` or `Arc { start, through, end }` (three distinct non-collinear points fix the plane, radius and sense; full circles cannot be given this way). Validation rejects an empty path, too many segments, non-finite or out-of-extent coordinates, gaps, zero-length lines, degenerate arcs and **closed paths**.
- **Placement**: the profile's origin is at the path's start, in the plane normal to the start tangent `t`. A caller-given `reference` vector gives the profile's y axis (its part normal to `t`), and the x axis is y × t, so (x, y, t) is right-handed. A `reference` parallel to `t`, zero or not finite is rejected.
- **Frame: rotation-minimizing (parallel transport).** Along a line the frame does not turn; along an arc it turns with the tangent around the arc's normal; at a corner by the smallest rotation taking the incoming tangent to the outgoing one. A beam in a vertical plane keeps its profile upright. A fixed-reference frame (`IfcFixedReferenceSweptAreaSolid`) is out of scope.
- **Corners** (tangent discontinuities, and the joints between the chords of a faceted path arc) are **mitre joints**: the section there is the profile projected onto the plane that bisects the corner. A turn of 180° is rejected.
- **Ends**: two end faces; one shell; genus = number of profile holes.
- **No self-intersection**, which `Solid::from_faces` does not check. *Locally*: on a path arc of radius R every profile point stays more than `LINEAR_TOLERANCE` inside R, measured towards the centre; at every corner and along every straight piece each profile vertex keeps a length greater than `LINEAR_TOLERANCE` between the two mitre planes of the piece. Both conditions are linear in the profile point, so checking the faceted profile's vertices is exact. *Globally*: two pieces of the path that are not neighbours stay more than 2 · d_max apart (d_max: the largest distance from the path to a profile point). This test is **conservative**: it may reject a legal sweep whose distant parts come close without touching. It is sound, simple, and bounded (O(pieces²), the piece count being bounded); a tighter exact test would need face-pair intersection and is left to a later decision.
- **Volume**: for straight pieces with mitres, V = A_f × the length of the polyline traced by the profile's centroid. A straight path perpendicular to the profile gives the volume of `Solid::extrude`. For a path arc the faceted result follows the step formula, and the exact torus segment (Pappus) is checked within the bound below.
- **Closed paths** (a sweep around a loop) are rejected: a ring in one plane is a full revolution, and parallel transport does not close up the frame after a loop on a 3D path.

### D4. Errors and security

New `thiserror` enums: `RevolutionError` (here), and for the sweep `PathError` and `SweepError`; each constructor's error wraps `ProfileError` (faceting) and `BrepError` (the final check) like `ExtrusionError`. No variant carries coordinates (SEC-03); loops and segments are named by index. Every input is hostile: NaN, infinities, subnormals, out-of-extent values, zero vectors, huge counts. No input may panic.

### D5. Determinism and placement

Trigonometry through `libm` only (ADR 0003): no platform maths, no `std::time`, no allocation proportional to unbounded input (ENG-04, ARCH-01/02). The constructors live in `crates/geom/src/brep/` (`revolve.rs`, `sweep.rs`) and the `Path` type in `crates/geom/src/path/`. Golden values (`tests/golden.txt`) include revolutions and sweeps, and run natively and under WebAssembly.

### The approximation bound

The faceted profile is within 1 mm of the exact profile, and the sweep of the faceted profile with chorded steps is within 1 mm of the exact sweep of the faceted profile. The solid is therefore within **about 2 mm** of the exact shape, worst case where both extremes meet (a profile arc point farthest from the axis at the middle of a step). Volumes follow: the faceted solid lies inside the exact one by at most 2 × `ARC_CHORD_TOLERANCE` times the area of the exact curved surface it replaces (a tube of radii 1 and 2 m, 1 m high: 6π m² × 2 mm ≈ 38 L out of 9.42 m³; tests use this bound). The tests assert it.

2 mm is acceptable because it is well below what a model at the level of detail of building elements carries, and because the alternative costs: face count grows as 1/√tolerance, so 0.5 mm for each source would raise the face count of every round element by 1.4 and slow the booleans for a gap nobody sees. Changing `ARC_CHORD_TOLERANCE` changes stored results (ADR 0008) and the golden file.

## Consequences

- Revolved and swept solids are faceted solids: their volume is the faceted volume. Quantity take-off of such elements should use Pappus' exact value from the section's area and centroid (as for extrusion).
- A caller needing a closed sweep builds a revolution, or two sweeps.
- A profile of more than a few edges at the farthest radii of the model extent cannot be fully revolved: see the face budget.
- True cylindrical, conical and toroidal faces arrive with GEO-09 and a record that supersedes this one for the cases it covers.
- The sweep's global self-intersection test may refuse legal solids; rejection is a typed error, never a repair.

## Alternatives considered

- **Reject partial support until GEO-09.** Round bases, domes and curved rails could not be modelled before the curved-surface work.
- **Steps from the profile's centroid radius** (or from the path's radius alone). A profile point farther out then travels on a circle whose chords exceed the tolerance, by an amount that grows with the profile's size: a 1 m wide section on a 2 m radius arc would be off by centimetres.
- **A fixed number of steps per turn.** Simple, but the error grows with the radius (ADR 0008).
- **Steps without the 90° limit.** For radii below half the tolerance one chord would span a half turn.
- **Merging coincident axis vertices after building** one copy per step. It would build `n` degenerate faces per axis edge and rely on the merge pass of ADR 0001 to remove them; building the right topology directly is simpler and exact.
- **Accepting a profile that crosses the axis** by splitting it. A cut at the axis is a boolean-style operation with its own tolerance questions, and building elements do not need it.
- **Auto-mirroring a profile on the right of the axis.** Rejected: the right-hand rule is stated for the axis direction, and the profile at angle 0 stays the input profile.
- **An angle above 2π**, or a negative angle. A negative angle is the same as the reversed axis; more than a full turn would overlap itself.
- **Closed sweeps** and **fixed-reference frames**: out of scope for GEO-02, later issues.
- **Exact global self-intersection tests for sweeps.** Needs face-pair intersection and a policy for touching faces; the conservative distance test is sound and cheap.

## References

- Specification: §6 (GEO-02, GEO-03, GEO-09), ENG-04.
- [ADR 0001](0001-geometry-tolerance-policy.md): tolerance policy. [ADR 0003](0003-deterministic-math-library.md): deterministic maths. [ADR 0008](0008-arc-faceting.md): arc faceting.
- Issues: sweep and revolution [#74](https://github.com/GuillaumeOj/BIMCraft/issues/74), epic [#46](https://github.com/GuillaumeOj/BIMCraft/issues/46).
