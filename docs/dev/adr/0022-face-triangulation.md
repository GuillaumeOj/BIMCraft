# 0022. Face triangulation

- Status: Accepted
- Requirements: GEO-05 (also GEO-03, ENG-02, ENG-04, SEC-03)
- Issues: [#69](https://github.com/GuillaumeOj/BIMCraft/issues/69), builds on [ADR 0001](0001-geometry-tolerance-policy.md) and [ADR 0008](0008-arc-faceting.md)

## Context

The viewport draws triangle meshes; the kernel's solids are bounded by planar faces with holes (GEO-03). `Solid::tessellate` turns a solid into a mesh, face by face, and the scene re-runs it for every element an edit touches (GEO-05), so it must be fast on the faces extrusions make (caps of up to `MAX_PROFILE_EDGES` = 4096 vertices) and must never panic or loop on any valid solid.

A face is planar, and its rings are simple and apart, only within the tolerance (ADR 0001): validation checks planarity within `LINEAR_TOLERANCE`, but not that rings avoid each other, which the booleans answer as they build. Fuzzing the booleans found valid solids whose faces cross themselves at the micrometre scale, where an operand was itself a few micrometres across:

- a sliver 25 m long whose end crosses itself about 1 µm across, which no triangulation into counterclockwise triangles exists for;
- holes 7 to 30 µm wide poking up to 0.34 µm out of their face, or whose own ring crosses itself;
- a micro face folded flat onto the corner of its neighbour, so a diagonal of one runs along an edge of the other.

## Decision

### Output

A `TriangleMesh` of positions, normals and `u32` indices. Each face is triangulated on its own, from its own vertices only (no Steiner points): `n` vertices over all its rings and `h` holes give exactly `n + 2h − 2` triangles. Vertices are repeated per face, each carrying the face's outward unit normal (its Newell area vector, normalised), as flat shading expects. Triangles turn counterclockwise seen from outside, like `FaceInput` rings. A face that cannot be triangulated is `TessellationError::Untriangulable { face }`, more than 2³² positions `TooManyVertices { face }`: indices, never coordinates (SEC-03).

### Projection

The face is projected on the coordinate plane that drops the largest component of its area vector, the two others ordered so the outer ring stays counterclockwise. Dropping a coordinate is exact, so the projected points are valid input for the exact `orient2d` (validation already bounds every coordinate), and every orientation, containment and crossing test below is exact.

### Hole bridging

Holes are taken from the one whose farthest vertex `m` (by first, then second coordinate) lies farthest. Candidates on the ring merged so far are tried by increasing distance from `m`, which only orders the search; a bridge is accepted only if it lies strictly inside the angles at both ends and meets no edge of any ring except at its own ends, all decided exactly. Splicing the hole in duplicates `m` and its partner, so each hole adds two nodes. Two holes may bridge to the same outer vertex; its copies are told apart by their edges, never by a tolerance.

### Ear clipping

The single ring left is clipped ear by ear. A node is an ear when its triangle turns strictly counterclockwise, no other node at a distinct position lies in the closed triangle, and no edge leaving a copy of a corner enters it. Ear status is tested afresh at each visit, never cached. A whole pass with no ear ends the strict attempt; exact faces never get past it.

### Faces simple only within the tolerance

When strict clipping fails, as on the faces above:

1. **Search**, for rings of at most `MAX_SEARCH_NODES` = 256 nodes: dynamic programming over all triangulations of the ring (as for the minimum-weight triangulation of a polygon) keeps only triangles that turn strictly counterclockwise or are **slivers**, no wider (twice the area over the longest side) than `MERGE_CHAIN_WIDTH`, 10 µm, the reach of the merge pass, and of distinct points; it minimises the number of slivers, then their area. The triangles of any triangulation of a ring add up, with their signs, to its winding number, so when all turn counterclockwise they cover a simple ring exactly once; a sliver is used only where the ring leaves no other choice.
2. **Relaxed clipping**, for larger rings, in place of strict clipping, which it follows step by step until no node is an ear: it then cuts the first sliver of three distinct points along the ring.
3. Otherwise, a small ring the search finds nothing for, or a large one left with no sliver, the face is `Untriangulable`. A defect wider than a sliver, such as a hole outside its face, which validation does not catch either, ends here.

### Cost

For a ring of `N` nodes (`n + 2h` for a face):

- strict clipping: each test is O(N) behind an exact bounding-box filter, at most N tests between two clips, so O(N³) in the worst case and O(N²) when ears are found as the walk reaches them; a convex cap is close to O(N). A cap of 3,300 vertices with ten round holes takes about 40 ms (release build);
- bridging: O(N) per hole when the nearest candidate is visible, the usual case, O(N²) per hole at worst;
- search: O(N³) time with N ≤ 256, at most about 2.8 × 10⁶ triangle tests, and N² ≤ 65,536 table entries;
- memory: O(N) per face outside the search; output: Σ n_f positions and normals, 3 Σ (n_f + 2h_f − 2) indices.

### Invariants and tests

Unit, property (`proptest`, seeded) and fuzz tests (`extrusion`, `extruded_boolean`) check on every mesh: the triangle count of each face; indices within the face; triangles strictly counterclockwise in the face's projection, slivers excepted; the triangles' area vectors adding up to the face's (which holds even off the plane, where the triangles' areas would not); vertex normals equal to the face's unit normal; by position, every directed edge used as many times as its reverse; and the enclosed volume, by the divergence theorem, equal to `Solid::volume` within rounding plus `LINEAR_TOLERANCE` times the area, what faces off their plane may sweep.

Two contracts: extrusions, exact, must always succeed, without slivers, every directed edge with exactly one reverse. Boolean results may have slivers and edges used more than once (where a folded micro face lies on another), and a face may fail only if two of its ring edges that share no vertex come within twice `MERGE_CHAIN_WIDTH` of each other.

## Consequences

- Every exact face triangulates exactly, from its own vertices, with no rounding in any decision.
- On faces simple only within the tolerance, the mesh may hold slivers turning the wrong way and triangles overlapping by up to `MERGE_CHAIN_WIDTH`: invisible at any display scale, and the mesh stays closed and consistently oriented, which the renderer and the scene need.
- An element whose solid has an untriangulable face has no mesh; the scene reports it as failed rather than drawing part of it.
- **Follow-up.** The defects come from the booleans: their merge pass leaves faces whose rings cross or come within its reach of themselves, and folded micro faces. It should reject such results with `FeaturesTooClose`, or repair them, and validation should check that faces are simple; tessellation could then drop its search and relaxed paths.

## Alternatives considered

- **Constrained Delaunay triangulation without Steiner points.** Better-shaped triangles and the same count, but incremental insertion, constraint recovery and flips are about three times the code with more degenerate paths, and shape does not matter to flat shading. It needs simple input too, so it would not handle the faces above either.
- **Monotone decomposition or a sweep.** O(N log N), but needs simple, exactly planar input and a robust event order; the caps it would speed up already take milliseconds.
- **Fan triangulation.** Correct for convex faces only.
- **Floating-point ear clipping with heuristics (as earcut).** Fast and forgiving, but its decisions round, and it gives no guarantee to test against.
- **Restarting the walk from other nodes when clipping fails.** It fixed some faces found by fuzzing and not others; the search decides whether any acceptable triangulation exists.
- **Failing every face that strict clipping cannot finish.** Simpler, but drops valid elements from the view for micrometre defects.
- **A triangulation crate.** A new dependency (AGENTS.md §4) for a few hundred lines, which would also have to decide with the kernel's exact predicates to give the same answer on every target (ENG-04).

## References

- Specification: §6 (GEO-03, GEO-05), ENG-02, ENG-04, SEC-03.
- [ADR 0001](0001-geometry-tolerance-policy.md): tolerance policy and merge pass. [ADR 0008](0008-arc-faceting.md): arc faceting.
- G. H. Meisters, "Polygons have ears", *American Mathematical Monthly* 82 (1975). D. Eberly, "Triangulation by ear clipping" (2002). J. O'Rourke, *Computational Geometry in C*, 2nd ed., ch. 1 (diagonals and cones).
- Issue [#69](https://github.com/GuillaumeOj/BIMCraft/issues/69), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67).
