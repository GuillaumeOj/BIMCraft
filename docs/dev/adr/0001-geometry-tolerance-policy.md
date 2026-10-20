# 0001. Geometry tolerance policy

- Status: Accepted
- Requirements: GEO-03, GEO-07 (also CORE-09, ENG-04)
- Issues: [#48](https://github.com/GuillaumeOj/BIMCraft/issues/48); amended for features one to a few tolerances wide, [#55](https://github.com/GuillaumeOj/BIMCraft/issues/55)

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

`LINEAR_TOLERANCE` is read only by the merge pass below and by the B-rep validity checks, which apply the distance rules above (planar faces, no edge, face or shell within the tolerance).

### Exactly one tolerance

**Within the tolerance** means a distance of at most `LINEAR_TOLERANCE` plus a fixed rounding allowance of 1 nm. Every test reads it the same way: two vertices merge, a vertex lies on a plane, two faces are coplanar, and the validity checks find an edge too short, a face too narrow, a shell too thin (on average) or a face not planar. So a feature exactly 1 µm wide is within the tolerance: a 1 µm gap closes, a 1 µm overlap is empty, a 1 µm edge merges.

The allowance is what makes this hold. A feature typed as 1 µm is computed as 1 µm give or take the rounding of its coordinates, so it falls on either side of a bare 1 µm bound depending on where it sits, and two tests computing the same distance differently could disagree: a face judged coplanar with a neighbour whose vertices do not merge with it leaves a face bent or open. 1 nm is hundreds of times the rounding of coordinates within the model extent, and a thousandth of the tolerance.

### Constructed geometry

Intersection points are computed in `f64` and rounded once. Each constructed vertex, edge and face records the input entities that generated it (for example "edge 12 of A crossing face 3 of B"). Later decisions about a constructed entity use exact predicates on its generators where possible, and its rounded coordinates only where they cannot. A face keeps the plane of the input face it comes from; its vertices lie within the tolerance of that plane.

Each vertex records the lines it was made on: the edge of an operand it lies on, such as "edge 12 of A crossing face 3 of B", and the line where the planes of two faces meet, for the points where either face's boundary meets the other's plane. A vertex merged with others lies on every line any of them was made on. Whether a vertex lies on a line is read from that record, never from its distance to the line: a vertex up to the tolerance off a line would bend it, and decisions taken on the straight line would no longer hold. An edge of an operand is split only at the points made on it; the planes of two faces at a small angle meet along a line whose points, each within the tolerance of both planes, may lie several tolerances apart across it. Whether the pieces of two lines of a face cross is then decided exactly on the pieces as split; a crossing within the tolerance of an end of one piece that lies within the other's span is that end, as the vertex merge would make it, and the other piece is split there. Pieces so bent are crossed again until none is split. Which stretches of the line two planes share lie in both faces is counted from the points where their boundaries meet it, a vertex on the plane taken once just above it and once just below, so that a stretch along either face's boundary counts. A piece lies inside its face if it lies on the inner side of the face's boundary as split. Pieces of faces of the two operands bounded by exactly the same vertices lie on each other, facing the same way if their boundaries run the same way round.

An edge crosses a plane where its ends lie on either side, beyond the tolerance; an end within the tolerance lies on the plane. An edge running along the plane at a shallow angle is the exception: if it crosses the plane farther than the tolerance from an end within the tolerance, that end takes the side it actually lies on and the crossing is kept. The decision depends only on the vertex, its edges and the plane, so every face around the vertex is cut the same way; otherwise one face's cut could stop at the vertex while its neighbour's runs on to the crossing, leaving the sliver between them open.

### Merge pass

Every boolean ends with the same deterministic clean-up. After the cut, its steps are local operations on the faces of the result, and each is applied only if every face it leaves still meets the rules the validity checks apply to a face on its own: planar within the tolerance, wider than it, with no edge within it. So **the merge pass never makes a valid result invalid**; where it cannot remove a feature within the tolerance without bending a face or breaking the solid apart, the operation fails with a typed error, `FeaturesTooClose`, never with an invalid solid:

1. **Coincident faces.** Faces of the two operands that are coplanar within the tolerance are first put onto one plane, before they are intersected: the face whose plane comes later in a fixed order (offset along the normal, then the normal) has its vertices projected onto the other's plane. The order depends only on the planes, so swapping the operands gives the same result. Vertices move by at most the tolerance; a corner lying within the tolerance of the planes around a corner of the other operand lands on that corner. Otherwise the faces around them would have to bend to join the two planes, and a face a few tolerances wide would not be flat. Faces that coincide with opposite orientation then cancel: both are removed. Between two lumps facing each other across a gap or overlap no thicker than the tolerance, the lumps fuse into one; on opposite sides of one lump, a lump or a void no thicker than the tolerance on average (near-zero volume, measured as the validity checks do) vanishes.
2. **Vertices.** Within each lump, vertices within `LINEAR_TOLERANCE` of each other are grouped with a union–find.
   - Edges whose endpoints fall in the same group disappear; edges that now share both endpoints merge.
   - Lumps that only touch along edges or at points keep their own vertices, so no non-manifold edge or vertex is created.
   - Each group is replaced by one representative: among its input vertices if it has any, otherwise among all its vertices, the one with the smallest coordinates (compared by x, then y, then z). The choice depends only on positions, so swapping the operands of a boolean gives the same result. Positions are never averaged, so repeated operations cannot make geometry drift.
   - If a group's bounding box is wider than `10 × LINEAR_TOLERANCE` (a chain of close points), the operation fails with a typed error instead of moving geometry silently.
   - If a merge bends a face of the result beyond the tolerance, as the validity checks measure planarity, the operation fails with `FeaturesTooClose`: straightening the face would move other vertices.
3. **Faces.** Adjacent faces with compatible orientation merge into one face when they are coplanar: always if they keep the same input plane, otherwise if they are coplanar within the tolerance, taken as one face and against the plane of one of them. A face a few tolerances wide, square to its neighbour, is within the tolerance of a plane fitted to both, but not of the neighbour's plane: it stays. Faces already merged are tested as a group, so a face a few tolerances wide lying within the tolerance of two neighbours that are not coplanar joins one of them at most, never both into one bent face. In every case the merged face must pass the validity checks, or the faces stay apart. A vertex left between two collinear edges goes if its two faces still pass them without it. A face whose boundary passes a vertex twice is split along the segment inside it that leaves the narrower part widest.
4. **Slivers.** Faces narrower than the tolerance, as the validity checks measure width (twice the area over the longest edge), are zipped shut where they can be: if the other vertices of such a face lie within the tolerance of its longest edge, in order along it, the face is removed and the face across that edge runs through those vertices instead. No vertex moves, and the surface moves by at most the tolerance. Where the face across also runs back along the sliver, the two lie back to back and cancel. A sliver no operation removes, such as one whose vertices zigzag across its longest edge, fails the operation with `FeaturesTooClose`. Booleans are **regularised**: the result is the closure of the interior, so lower-dimensional pieces (shared faces, edges or points) are dropped.

Candidate pairs are never found by comparing everything with everything: vertices go through an ordered grid of cells one tolerance wide (ordered, not hashed, so runs are reproducible), and faces are bucketed by the input plane they keep.

The B-rep validity checks (#50) then run on the result. A failure returns a typed error, never a panic. Since the merge pass leaves no face too narrow, bent or with too short an edge, a failure there points at faces that do not close into a solid: a fault of the cut, not of the tolerance.

### Degenerate cases

Each case below becomes a case of the boolean corpus (#55) with its expected volume and topology. "Cube" means a 1 m cube.

| Case | Operation | Expected result |
| --- | --- | --- |
| Coplanar faces | Union of two cubes sharing a face | One 2 × 1 × 1 m box: 8 vertices, 6 faces. The shared face disappears and the side faces merge. |
| Flush cutter | Box minus a cutter whose top is flush with the box top | No zero-thickness skin above the cut. |
| Near-coplanar, inside tolerance | Union of two cubes, the second shifted 0.5 µm away from the first | Same as coplanar faces: the gap is below the tolerance and closes. |
| Near-coplanar, at the tolerance | Same, shifted exactly 1 µm away | Same as coplanar faces: the gap is within the tolerance and closes. |
| Near-coplanar, outside tolerance | Same, shifted 2 µm away | Two separate lumps; the 2 µm gap is real and kept. |
| Touching edges | Two cubes sharing only an edge | Union: two separate lumps, no non-manifold edge. Intersection: empty. |
| Touching vertices | Two cubes sharing only a corner | Union: two separate lumps. Intersection: empty. |
| Near-zero volume | Box minus a cutter that leaves a 0.1 µm skin | The skin is removed. |
| Near-zero overlap | Intersection of two cubes overlapping by 0.5 µm, or by exactly 1 µm | Empty. |
| Features a few tolerances wide | Boxes on a 0.5 m grid, the second's corners moved by up to 4 µm in steps of 0.5 µm, each way | Union, difference and intersection are valid. Features wider than 1 µm stay; corners within the tolerance of each other's planes meet; an overlap no thicker than 1 µm on average, such as a stick 2 µm square, vanishes. |
| Micro-edge | A cut that would create a 0.3 µm edge | The edge's endpoints merge; no edge shorter than 1 µm remains. |

## Consequences

- Predicates, B-rep and booleans can be written and tested against a single rule. The corpus encodes it.
- Features smaller than 1 µm cannot be modelled. No construction use needs them.
- Operands whose features lie within a few tolerances of each other in ways the merge pass cannot resolve, such as a vertex of one 1.5 µm from a face of the other near a corner, get `FeaturesTooClose`: a typed error the caller can report, never a result that is silently wrong or invalid.
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
