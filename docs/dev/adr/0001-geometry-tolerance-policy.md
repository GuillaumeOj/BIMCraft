# 0001. Geometry tolerance policy

- Status: Accepted
- Requirements: GEO-03, GEO-07 (also CORE-09, ENG-04)
- Issues: [#48](https://github.com/GuillaumeOj/BIMCraft/issues/48)

## Context

GEO-03 requires booleans that are robust on planar faces, "using exact predicates and an explicit tolerance policy". GEO-07 requires model coordinates in double precision. The kernel is written from scratch, and robust booleans are the project's highest technical risk (specification §16).

Floating-point kernels fail in two ways. If every comparison uses an epsilon, decisions become inconsistent: a point can be "on" a plane for one test and "above" it for the next, and the topology built from those answers is invalid. If no tolerance is used at all, booleans produce slivers, micro-edges and faces a few ulps thick that break later operations and make results depend on operand order.

This record fixes, before the predicates (#47), the B-rep (#50) and the booleans are written, which decisions are exact, which use a tolerance, what its value is and how near-coincident features are merged.

## Decision

### Units and range

- The kernel works in **metres** (SI, CORE-09) and stores every coordinate as **`f64`** (GEO-07). Display units are a UI concern and never reach the kernel.
- Modelling happens in the project's local engineering frame, near the origin (specification §7). Georeferenced or projected coordinates never enter the kernel.
- Coordinates must lie within the **model extent**: `MODEL_EXTENT = 1e4` m, i.e. every component in [−10 km, +10 km]. This covers any building and most bridges. Input outside the extent is rejected with a typed error, never clamped.

At 10 km the spacing between adjacent `f64` values is 2⁻³⁹ m ≈ 1.8 × 10⁻¹² m, more than five orders of magnitude below the tolerance chosen below. That margin absorbs the rounding of constructed points.

### Exact decisions (no tolerance)

Every decision about the relative position of **stored coordinates** is made with exact adaptive predicates (orient2d, orient3d, #47). This covers side of a line or plane, point in polygon, the sign of segment–segment and segment–plane crossings, and collinearity or coplanarity of input vertices.

A predicate result is a sign. It is never compared against an epsilon, and no code computes "almost zero" determinants.

### The linear tolerance

- One constant: **`LINEAR_TOLERANCE = 1e-6` m (1 µm)**, defined once in the geometry crate.
- It is **not user- or project-settable**, so results are identical on every target and for every user (ENG-04).
- 1 µm is far below construction precision (about 0.1 mm) and below anything a drawing shows, so users never see it.
- There is **no angular tolerance**. Deviations are always measured as distances: a face is planar if all its vertices lie within the tolerance of its plane; two planes are coincident over a face if the face's vertices lie within the tolerance of the other plane.

`LINEAR_TOLERANCE` is read only by the merge pass below.

### Constructed geometry

Intersection points are computed in `f64` and rounded once. Each constructed vertex, edge and face records the input entities that generated it (for example "edge 12 of A crossing face 3 of B"). Later decisions about a constructed entity use exact predicates on its generators where possible, and its rounded coordinates only where they cannot. A face keeps the plane of the input face it comes from; its vertices lie within the tolerance of that plane.

### Merge pass

Every boolean ends with the same deterministic clean-up:

1. **Coincident faces.** Faces that coincide within the tolerance with opposite orientation cancel: both are removed. Between two lumps facing each other across a gap or overlap thinner than the tolerance, the lumps fuse into one; on opposite sides of one lump, a solid thinner than the tolerance (near-zero volume) vanishes.
2. **Vertices.** Within each lump, vertices closer than `LINEAR_TOLERANCE` are grouped with a union–find.
   - Edges whose endpoints fall in the same group disappear; edges that now share both endpoints merge.
   - Lumps that only touch along edges or at points keep their own vertices, so no non-manifold edge or vertex is created.
   - Each group is replaced by one representative: among its input vertices if it has any, otherwise among all its vertices, the one with the smallest coordinates (compared by x, then y, then z). The choice depends only on positions, so swapping the operands of a boolean gives the same result. Positions are never averaged, so repeated operations cannot make geometry drift.
   - If a group's bounding box is wider than `10 × LINEAR_TOLERANCE` (a chain of close points), the operation fails with a typed error instead of moving geometry silently.
3. **Faces.** Adjacent faces with compatible orientation merge into one face when they are coplanar: always if they keep the same input plane, otherwise if they are coplanar within the tolerance.
4. **Slivers.** Faces narrower than the tolerance everywhere are removed. Booleans are **regularised**: the result is the closure of the interior, so lower-dimensional pieces (shared faces, edges or points) are dropped.

Candidate pairs are never found by comparing everything with everything: vertices go through a spatial hash with cells one tolerance wide, and faces are bucketed by the input plane they keep.

The B-rep validity checks (#50) then run on the result. A failure returns a typed error, never a panic.

### Degenerate cases

Each case below becomes a case of the boolean corpus (#55) with its expected volume and topology. "Cube" means a 1 m cube.

| Case | Operation | Expected result |
| --- | --- | --- |
| Coplanar faces | Union of two cubes sharing a face | One 2 × 1 × 1 m box: 8 vertices, 6 faces. The shared face disappears and the side faces merge. |
| Flush cutter | Box minus a cutter whose top is flush with the box top | No zero-thickness skin above the cut. |
| Near-coplanar, inside tolerance | Union of two cubes, the second shifted 0.5 µm away from the first | Same as coplanar faces: the gap is below the tolerance and closes. |
| Near-coplanar, outside tolerance | Same, shifted 2 µm away | Two separate lumps; the 2 µm gap is real and kept. |
| Touching edges | Two cubes sharing only an edge | Union: two separate lumps, no non-manifold edge. Intersection: empty. |
| Touching vertices | Two cubes sharing only a corner | Union: two separate lumps. Intersection: empty. |
| Near-zero volume | Box minus a cutter that leaves a 0.1 µm skin | The skin is removed. |
| Near-zero overlap | Intersection of two cubes overlapping by 0.5 µm | Empty. |
| Micro-edge | A cut that would create a 0.3 µm edge | The edge's endpoints merge; no edge shorter than 1 µm remains. |

## Consequences

- Predicates, B-rep and booleans can be written and tested against a single rule. The corpus encodes it.
- Features smaller than 1 µm cannot be modelled. No construction use needs them.
- Models larger than 10 km in any direction need splitting into separate local frames. A future linear-infrastructure need would require a new record.
- Changing `LINEAR_TOLERANCE` or `MODEL_EXTENT` changes stored results.
- The rendering half of GEO-07 (camera-relative origin, `f32` on the GPU, VP-04) is a viewport concern and is out of scope here (#68).
- Curved surfaces (GEO-09) will need tolerances for curve and surface approximation; they will be decided in their own record.

## Alternatives considered

- **Epsilon comparisons everywhere.** Simple, but decisions contradict each other near degeneracies and topology becomes invalid. This is the failure mode GEO-03 exists to prevent.
- **Exact rational or arbitrary-precision arithmetic for all geometry.** Always consistent, but much slower, and coordinate sizes grow with every chained operation. Exactness is kept for decisions, where it is cheap thanks to adaptive predicates.
- **Snapping all coordinates to a fixed integer grid.** Makes every decision exact, but limits the extent, loses precision on arc points and moves user input.
- **A user- or project-settable tolerance.** Breaks determinism and makes the same file give different results in different projects.
- **A separate angular tolerance.** Its meaning depends on feature size; distance bounds over the model extent cover the same cases consistently.

## References

- Specification: §6 (geometry kernel, GEO-03, GEO-07 and the 1,000-case acceptance target), §7 (local engineering frame), ENG-04.
- Issues: epic [#46](https://github.com/GuillaumeOj/BIMCraft/issues/46), exact predicates [#47](https://github.com/GuillaumeOj/BIMCraft/issues/47), B-rep [#50](https://github.com/GuillaumeOj/BIMCraft/issues/50), boolean corpus [#55](https://github.com/GuillaumeOj/BIMCraft/issues/55), viewport [#68](https://github.com/GuillaumeOj/BIMCraft/issues/68), deterministic maths [#9](https://github.com/GuillaumeOj/BIMCraft/issues/9).
- J. R. Shewchuk, *Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates*, Discrete & Computational Geometry 18, 1997.
