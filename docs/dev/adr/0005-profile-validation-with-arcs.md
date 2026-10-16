# 0005. Profile validation with arcs

- Status: Accepted
- Requirements: GEO-01, GEO-03 (also ENG-04, SEC-03)
- Issues: [#49](https://github.com/GuillaumeOj/BIMCraft/issues/49), builds on [ADR 0001](0001-geometry-tolerance-policy.md)

## Context

GEO-01 requires 2D profiles made of lines and circular arcs, with holes. A profile must be closed and must not intersect itself, and every later operation (extrusion, booleans) relies on that.

ADR 0001 makes every decision on stored coordinates exact, with orient2d and orient3d, and reads `LINEAR_TOLERANCE` only in the merge pass. That covers straight edges. Whether an arc crosses or touches another edge cannot be decided with those predicates: the circle's points are not stored coordinates, and an exact answer needs arithmetic on square roots. ADR 0001 leaves curve tolerances to a later record; this is the part profiles need now.

## Decision

### Arcs

An arc is stored as its start, end, centre and turning direction (counterclockwise or clockwise). The radius is the distance from centre to start; the end must be at the same distance within `LINEAR_TOLERANCE`, otherwise the arc is rejected. An arc whose end equals its start is a full circle, so a round hole is one edge. Arcs with a radius within the tolerance, with distinct ends at the same angle or within the tolerance of each other, or lying all within the tolerance of their ends (their middle is that close to an end), are rejected as degenerate: the checks between consecutive edges ignore points that close to their common end, so they could miss such an arc retracing or crossing its neighbour.

### Validity

A profile is an outer loop and any number of holes. It is valid when:

- every coordinate is finite and inside the model extent of ADR 0001;
- every loop is closed **exactly**: each edge ends where the next one starts, with equal coordinates and no tolerance;
- a full circle is a loop on its own: in a longer loop it would pinch the loop at its start;
- no line has zero length;
- no two edges meet, except consecutive edges at their common point:
  - **two lines**: decided exactly with orient2d, as ADR 0001 requires. Consecutive lines are rejected only when the second goes back along the first;
  - **a pair involving an arc**: decided in floating point and **rejected when the edges come within `LINEAR_TOLERANCE`**, read as ADR 0001 reads it everywhere: at most the tolerance, up to rounding, so edges exactly 1 µm apart are rejected. The distance is the minimum over the points where it can be smallest (ends, crossings, points along common normals). For consecutive edges, points within the tolerance of their common point are ignored, so a line tangent to an arc is valid. Edges leaving their common point in the same direction, so closely that the turn alone keeps them within the tolerance along the shorter one, form a cusp and meet: their sliver starts in the ignored zone, where no point of it may be found;
- every hole lies inside the outer loop and outside the other holes.

The arc rule fails safe: an edge that comes within 1 µm of another is reported as an intersection, never accepted as a sliver. Since loops are then at least 1 µm apart wherever an arc is involved, and exactly disjoint otherwise, testing one vertex per hole decides nesting reliably in floating point.

Loops are stored with the outer loop counterclockwise and holes clockwise, whatever the input direction. Area and centroid are computed in closed form (triangles and circular segments) from the stored loops, so equal profiles give bit-identical results.

Errors identify the loop and the edges by position and never carry coordinates (SEC-03). Validation compares every pair of edges, so a profile is limited to 4,096 edges.

## Consequences

- `LINEAR_TOLERANCE` is now also read by profile validation, for arcs only, besides the merge pass and the B-rep validity checks named in ADR 0001.
- Two arcs, or an arc and a line, at 1 µm or less from each other cannot be part of the same profile. No construction use needs that.
- Straight-edged profiles keep the exact semantics of ADR 0001: two lines 1 nm apart are valid.
- Larger profiles (above 4,096 edges) need a spatial index in the pairwise check before the limit can be raised.

## Alternatives considered

- **Exact rational arithmetic for arcs.** Fully consistent with ADR 0001, but slow and much more code, with numbers growing with every square root eliminated. The 1 µm gap gives the same guarantees in practice.
- **Three-point arcs** (start, a point on the arc, end), as IFC4 `IfcArcIndex`. Always consistent and lets incircle decide some cases exactly, but authoring and DXF/IFC parametric arcs carry a centre; the radius check costs one comparison.
- **Bulge arcs** (start, end, tan(sweep/4)), as DXF polylines. Compact, but the circle is derived, so nothing about it is exact either.
- **The same tolerance for two lines.** Simpler to state, but it contradicts ADR 0001 for straight edges, where exact decisions are cheap.

## References

- Specification: §6 (GEO-01, GEO-03), ENG-04, SEC-03.
- [ADR 0001](0001-geometry-tolerance-policy.md): tolerance policy.
- Issues: profiles [#49](https://github.com/GuillaumeOj/BIMCraft/issues/49), epic [#46](https://github.com/GuillaumeOj/BIMCraft/issues/46), fuzzing [#56](https://github.com/GuillaumeOj/BIMCraft/issues/56).
