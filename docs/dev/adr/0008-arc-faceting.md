# 0008. Arc faceting for planar solids

- Status: Accepted
- Requirements: GEO-02, GEO-03, GEO-09 (also ENG-04, SEC-03)
- Issues: [#51](https://github.com/GuillaumeOj/BIMCraft/issues/51), builds on [ADR 0001](0001-geometry-tolerance-policy.md) and [ADR 0005](0005-profile-validation-with-arcs.md)

## Context

GEO-02 requires solids made by extruding a profile, and profiles have circular arcs (GEO-01). Extruding an arc gives a cylindrical face, but the B-rep only has planar faces: GEO-03 asks for robust booleans on planar faces, and curved surfaces (GEO-09) are scheduled for later. ADR 0001 leaves the tolerance for approximating curves to its own record. This is that record, for arcs.

## Decision

### Chord tolerance

Before a profile becomes a solid, each arc is replaced by a chain of straight chords. One constant bounds the approximation: **`ARC_CHORD_TOLERANCE = 1e-3` m (1 mm)**, the largest distance between an arc and its chords (the sagitta). Like `LINEAR_TOLERANCE`, it is fixed, never user- or project-settable (ENG-04).

1 mm keeps a 1 m diameter column to 50 faces and a 20 m radius curved wall to about 315 faces per full turn, while a facet stays below what a 1:20 drawing shows.

### Chords

An arc of radius `r` sweeping the angle `θ` is cut into `n` chords of equal angle, the smallest count whose sagitta `r (1 − cos(θ / 2n))` is within the tolerance:

`n = ⌈ |θ| / (2 acos(1 − s / r)) ⌉`, with `s = ARC_CHORD_TOLERANCE`, the argument of `acos` clamped to −1 for radii below `s / 2`.

`n` is at least 1, and at least 3 for a full circle, so a round hole stays a ring. The first and last points are the arc's stored ends, so faceted loops stay exactly closed; the points between are computed with `libm` (ADR 0003), identical on every target.

### Validation again

Chords lie inside the arc's circle and can move a loop by up to the tolerance, more than the 1 µm gap ADR 0005 keeps around arcs. The faceted loops are therefore validated again as a straight-edged profile, with the exact checks of ADR 0001. A profile whose faceting would make loops meet, or that exceeds `MAX_PROFILE_EDGES` once faceted, is **rejected with a typed error, never repaired**. Faceting stops as soon as the edge limit is passed, so hostile input cannot make it allocate millions of points.

## Consequences

- An extruded solid is a faceted solid: its volume is the faceted volume. For a convex arc it is smaller than the true volume by at most arc length × tolerance × height (for a 1 m diameter column, about 0.3 %). Quantity take-off of extruded elements should use the profile's exact area times the height rather than the solid's volume.
- Two loops less than 1 mm apart where one of them is an arc may be a valid profile but fail to extrude. No construction use needs that.
- True cylindrical faces arrive with GEO-09 and a record that supersedes this one for the cases it covers.
- Changing `ARC_CHORD_TOLERANCE` changes stored results.

## Alternatives considered

- **Reject arcs until GEO-09.** Simpler, but round columns, round openings and curved walls could not be modelled before the curved-surface work.
- **Chords that straddle the arc** (vertices outside the circle, so the area is preserved). Keeps volumes closer, but moves the stored end points off the arc and breaks the exact closure of loops.
- **A count of segments per arc, or per full turn.** Simple, but the error grows with the radius: 32 segments leave a 2 cm gap on a 20 m wall.
- **A smaller tolerance (0.1 mm).** Triples the face count of every round element and slows booleans for a difference nobody sees.

## References

- Specification: §6 (GEO-01, GEO-02, GEO-03, GEO-09), ENG-04.
- [ADR 0001](0001-geometry-tolerance-policy.md): tolerance policy. [ADR 0003](0003-deterministic-math-library.md): deterministic maths. [ADR 0005](0005-profile-validation-with-arcs.md): profile validation with arcs.
- Issues: extrusion [#51](https://github.com/GuillaumeOj/BIMCraft/issues/51), sweep and revolution [#74](https://github.com/GuillaumeOj/BIMCraft/issues/74), epic [#46](https://github.com/GuillaumeOj/BIMCraft/issues/46).
