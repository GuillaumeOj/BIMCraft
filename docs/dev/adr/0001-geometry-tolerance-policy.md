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

`LINEAR_TOLERANCE` is read only by the merge pass below and by the B-rep validity checks, which apply the distance rules above (planar faces, no edge, face or shell within the tolerance). The cut of a boolean, between the two, takes no decision by it: it decides on the operands' coordinates as they are (constructed geometry, below).

### Exactly one tolerance

**Within the tolerance** means a distance of at most `LINEAR_TOLERANCE` plus a fixed rounding allowance of 1 nm. Every test reads it the same way: an edge collapses, two faces are coplanar, and the validity checks find an edge too short, a face too narrow, a shell too thin (on average) or a face not planar. So a feature exactly 1 µm wide is within the tolerance: a 1 µm gap closes, a 1 µm overlap is empty, a 1 µm edge collapses.

The allowance is what makes this hold. A feature typed as 1 µm is computed as 1 µm give or take the rounding of its coordinates, so it falls on either side of a bare 1 µm bound depending on where it sits, and two tests computing the same distance differently could disagree: a face judged coplanar with a neighbour whose edge between them does not collapse leaves a face bent or open. 1 nm is hundreds of times the rounding of coordinates within the model extent, and a thousandth of the tolerance.

### Constructed geometry

Intersection points are computed in `f64` and rounded once. Each constructed vertex, edge and face records the input entities that generated it (for example "edge 12 of A crossing face 3 of B"). Later decisions about a constructed entity use exact predicates on its generators where possible, and its rounded coordinates only where they cannot. A face keeps the plane of the input face it comes from; its vertices lie on that plane up to rounding.

The cut decides on the operands' own coordinates: no vertex is merged with another, and no vertex moves, before the faces are cut and their pieces classified. Every decision there is exact or reads the rounding allowance alone, never the tolerance, so two decisions cannot disagree about a feature within the tolerance; the merge pass removes such features afterwards, each by a checked operation.

- **Planes.** Faces put onto one plane (merge pass, step 1) share its number. Each vertex records the planes it lies on: an operand vertex those of its faces, and of any face whose plane it lies within the rounding allowance of; a constructed vertex those of the lines it was made on. Three planes meet at one point unless they share a line, so vertices made on three of the same planes are one point, as are vertices within the rounding allowance of each other: they are one vertex, however rounding placed them. Only where the three planes share a line do two such vertices lie farther apart than the tolerance, and they stay apart. Below the allowance, two computations of one point cannot be told apart. Two faces lie in one plane only if every vertex of one lies on the other's plane so, never because they lie within the tolerance of each other: a face within the tolerance of two planes a shallow angle apart goes onto one of them (merge pass, step 1) and lies across the other.
- **Sides.** A vertex lies on a plane if it records it; otherwise it lies on the side of the plane its distance says, however near. An edge crosses a plane where its ends lie on either side. Every face around a vertex then sees it on the same side, so their cuts agree, and a vertex a micrometre from a plane leaves a feature a micrometre wide for the merge pass, never a cut bent through it.
- **Lines.** Each vertex records the lines it was made on: the edge of an operand it lies on, such as "edge 12 of A crossing face 3 of B", and the line where the planes of two faces meet, for the points where either face's boundary meets the other's plane. Whether a vertex lies on a line is read from that record, never from its distance to the line. An edge of an operand is split only at the points made on it. An edge whose ends both lie on the plane of a face of the other operand runs along the line that plane shares with the edge's own face, so the points that line holds between the edge's ends and within the rounding allowance of it, such as where the other face's boundary crosses it, are made on the edge; where the two planes meet at a small angle, points within the allowance of both lie up to micrometres across the line they share, and only those within the allowance of the edge lie on it. Whether the pieces of two lines of a face cross is then decided exactly on the pieces as split, in the face's projection: a crossing is the vertex already made on three of the planes of the two lines if there is one, such as an end of one of the pieces, which then splits the other. Pieces so bent are crossed again until none is split. Which stretches of the line two planes share lie in both faces is counted from the points where their boundaries meet it, a vertex on the plane taken once just above it and once just below, so that a stretch along either face's boundary counts. A piece lies inside its face if it lies on the inner side of the face's boundary as split. Pieces of faces of the two operands bounded by exactly the same vertices lie on each other, facing the same way if their boundaries run the same way round.

### Merge pass

Every boolean ends with the same deterministic clean-up. Apart from step 1, which puts coincident faces onto one plane before the cut, its steps run after the pieces are classified, as local operations on the faces of the result, and each is applied only if every face it leaves still meets the rules the validity checks apply to a face on its own: planar within the tolerance, wider than it, with no edge within it. So **the merge pass never makes a valid result invalid**; where it cannot remove a feature within the tolerance without bending a face or breaking the solid apart, the operation fails with a typed error, `FeaturesTooClose`, never with an invalid solid:

1. **Coincident faces.** Faces of the two operands that are coplanar within the tolerance are first put onto one plane, before they are intersected: the face whose plane comes later in a fixed order (offset along the normal, then the normal) has its vertices projected onto the other's plane. The order depends only on the planes, so swapping the operands gives the same result. Vertices move by at most the tolerance; a corner lying within the tolerance of the planes around a corner of the other operand lands on that corner. Otherwise the faces around them would have to bend to join the two planes, and a face a few tolerances wide would not be flat. A vertex lying on a plane both operands share, through a face of its own that keeps its plane, moves along that plane onto the other, if that is no farther than the tolerance, and along the other's normal otherwise: off the shared plane, its face there would be cut into triangles a fraction of a degree off the other operand's face, whose planes nearly share a line. The other faces around a moved vertex keep their planes all the same, as the cut needs every vertex of a face to lie on its plane up to rounding: one whose vertices moved off it is cut along a collar, a line across it near them, as far from each as its shortest edge is long (at most a third of the edge). The strip between becomes triangles, each on a plane of its own, cut off one by one with the shortest new edge first, so they are as well shaped as the strip allows; the rest keeps its plane, the collar's points being made on the planes of the faces along their edges that stay flat beyond them. Cutting off a triangle at the vertices beside a moved one instead would leave triangles as thin as a long face, whose planes, three of them meeting near a tip, nearly share a line. A face with no vertex left on its plane, or whose strip does not fit inside it, stays whole. Faces that coincide with opposite orientation then cancel: both are removed. Between two lumps facing each other across a gap or overlap no thicker than the tolerance, the lumps fuse into one; on opposite sides of one lump, a lump or a void no thicker than the tolerance on average (near-zero volume, measured as the validity checks do) vanishes.
2. **Vertices.** After classification, in each lump, edges no longer than `LINEAR_TOLERANCE` collapse: the vertices they join are replaced by one of them in every face around.
   - Lumps that only touch along edges or at points get their own vertices first, so no collapse joins them and no non-manifold edge or vertex is created.
   - A group of vertices joined by such edges collapses onto one representative: an input vertex if it has any, otherwise the one with the smallest coordinates (compared by x, then y, then z). The choice depends only on positions, so swapping the operands of a boolean gives the same result. Positions are never averaged, so repeated operations cannot make geometry drift.
   - A collapse is checked before it is applied: every face it changes must vanish or be left without a fault of its own the validity checks find (bent beyond the tolerance, narrower than it) that it did not have, and the faces along every edge it changes must still pair around it, one side each way next to each other. Edges that now share both endpoints merge only so, and only where the lump, touching itself along the merged edge, can still be pulled apart there: giving each fan of faces around an end its own vertex must leave each pair of faces along the edge with ends of its own, as the validity checks accept no edge carried by more than two faces. A face along the edge lying flat with one running the other way merges with it (step 3), the edge then in the middle of the merged face, so that pair is left out. A group that cannot collapse whole collapses edge by edge where that holds, either end staying, the preferred one first; groups and edges are taken in order of position.
   - Faces a collapse leaves back to back, vertex for vertex with opposite orientation, cancel: both are removed.
   - If a group's bounding box is wider than `10 × LINEAR_TOLERANCE` (a chain of close points), the operation fails with a typed error, `MergeChainTooWide`, instead of moving geometry silently.
   - An edge within the tolerance that no collapse removes, such as one along a wedge thinner than the tolerance whose other edges are longer than it, is left to the steps below; if it remains, the operation fails with `FeaturesTooClose`.
3. **Faces.** Adjacent faces with compatible orientation merge into one face when they are coplanar: always if they keep the same input plane, otherwise if they are coplanar within the tolerance, taken as one face and against the plane of one of them. A face a few tolerances wide, square to its neighbour, is within the tolerance of a plane fitted to both, but not of the neighbour's plane: it stays. Faces already merged are tested as a group, so a face a few tolerances wide lying within the tolerance of two neighbours that are not coplanar joins one of them at most, never both into one bent face. In every case the merged face must pass the validity checks, or the faces stay apart. A vertex left between two collinear edges goes if its two faces still pass them without it. Either change moves the surface by up to the tolerance, which can leave a lump just thicker than the tolerance on average thinner than that; it is then not made, so a lump the checks would accept is not dropped as too thin. A face whose boundary passes a vertex twice is split along the segment inside it that leaves the narrower part widest. Where no segment inside it joins the two parts, they only touch at that vertex: each becomes a face, and lumps that touched only through that face get their own copies of the vertex, as in step 2.
4. **Slivers.** Faces narrower than the tolerance, as the validity checks measure width (twice the area over the longest edge), are zipped shut where they can be: if the other vertices of such a face lie within the tolerance of its longest edge, in order along it, the face is removed and the face across that edge runs through those vertices instead. No vertex moves, and the surface moves by at most the tolerance. Where the face across also runs back along the sliver, the two lie back to back and cancel. A sliver that does not zip whole, such as a strip whose ends overhang each other along its longest edge, is split along a diagonal if both parts then zip, each onto the face across its own longest edge. A sliver no operation removes, such as one with a vertex beyond the tolerance of the plane of the face across, fails the operation with `FeaturesTooClose`. Booleans are **regularised**: the result is the closure of the interior, so lower-dimensional pieces (shared faces, edges or points) are dropped.

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
| Micro-edge | A cut that would create a 0.3 µm edge | The edge collapses; no edge shorter than 1 µm remains. |

## Consequences

- Predicates, B-rep and booleans can be written and tested against a single rule. The corpus encodes it.
- Features smaller than 1 µm cannot be modelled. No construction use needs them.
- Operands whose features lie within a few tolerances of each other in ways the merge pass cannot resolve, such as a vertex of one 1.5 µm from a face of the other near a corner, get `FeaturesTooClose`: a typed error the caller can report, never a result that is silently wrong or invalid.
- What a boolean may move follows from these rules, and the `extruded_boolean` fuzz target checks no more: a vertex moves by at most the tolerance, or across a group of collapsed vertices at most ten tolerances wide along each axis; the volume, by at most the tolerance times the operands' area (moving the surface by the tolerance sweeps no more, and a lump that vanishes is no thicker than the tolerance on average). Rounding of constructed points is six orders of magnitude below either bound.
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
