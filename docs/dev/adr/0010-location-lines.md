# 0010. Location lines and location points

- Status: Accepted
- Requirements: ARC-30, STR-14, GEO-13, INT-10 (also ARCH-04, ENG-04, SEC-03, MAT-03)
- Issues: [#187](https://github.com/GuillaumeOj/BIMCraft/issues/187), [#188](https://github.com/GuillaumeOj/BIMCraft/issues/188), [#190](https://github.com/GuillaumeOj/BIMCraft/issues/190), related to [#87](https://github.com/GuillaumeOj/BIMCraft/issues/87) and [#104](https://github.com/GuillaumeOj/BIMCraft/issues/104); builds on [ADR 0001](0001-geometry-tolerance-policy.md) and [ADR 0002](0002-schema-definition-format.md)

## Context

A wall is drawn along a line and a beam or column along an axis. Which line of the section that is matters. It decides where a new wall lands, where its body stays when its type changes, how a flip mirrors it, and how it exchanges with IFC. ARC-30 asks for a wall location line on any layer face, any layer centre, the core centre or a custom offset. STR-14 asks for a cardinal point on a profile, plus an offset. GEO-13 asks the kernel to resolve both into an offset of the section. INT-10 maps them to IFC.

The wall, column and beam elements do not exist yet. The `disc-arch` and `disc-struct` crates are planned, but there is no code for them. The resolution rules must still be fixed now, so that the elements and the IFC mapping build on one tested definition. The design is Penpot board 3.3, "Wall type — layers and location line".

## Decision

### Where it lives

The types and the resolution live in **`geom`**, module `location`. They are pure arithmetic on a section, and `geom` already owns profiles. `geom` sits in layer 2 with `core-model`. The discipline crates (layer 4) and `interop-ifc` (layer 3) depend on it, never the reverse (ARCH-04).

They are **hand-written Rust, not schema-defined**. ADR 0002 keeps behaviour and geometry in code and persistent model data in the schema. When the wall-type and member entities arrive, their schema definitions will store the choice: the layer id, the position or face, and the offset. Their code will convert it into these types. The kernel works in metres (ADR 0004); the UI shows millimetres.

### Layer sets and stable snap points

A layer set lists layers from the **exterior face to the interior face**. Each layer has a thickness, a core flag and a **`LayerId`**. The caller assigns the id, and it never changes when layers are reordered, renamed or resized.

A choice refers to layers only by id, never by index:

| Choice | Meaning | Distance from the exterior face |
| --- | --- | --- |
| `CoreCentre` (default) | Middle of the span from the first core layer to the last | `(start of first core + end of last core) / 2`; middle of the wall when no layer is core |
| `WallFace(Exterior / Interior)` | A face of the whole wall | `0` / total thickness |
| `Snap { layer, ExteriorFace / Centre / InteriorFace }` | A face or the centre of one layer | start, start + t/2, start + t of that layer |
| `Custom { layer, face, offset }` | An offset from one face of one layer, positive towards the interior | that face + offset |

The snap-point list holds every face once and every centre, from exterior to interior: 2n + 1 points for n layers. A face shared by two layers is listed as the exterior face of the inner layer. Resolving still accepts the interior face of the outer layer and gives the same distance.

Names follow the design: "Layer 3 — Structural core · centre". The number is the layer's current 1-based position. The function name comes from the caller. The kernel returns structured data, and the UI localises it (UI-04).

When the referenced layer is gone, or a custom offset no longer fits the section, resolution **falls back to the core centre and reports why**. It never panics. A custom offset may overshoot the section by at most `LINEAR_TOLERANCE`; it is then clamped, which absorbs rounding in the sums.

### Sign conventions

**Walls.** In the wall's local frame, x runs along the drawing direction and y points to its left. The location line is the wall axis, at y = 0. Let d be the resolved distance of the location line from the exterior face:

- **Not flipped:** the exterior face is at y = −d and the layers run towards +y. A wall drawn counterclockwise around a building therefore has its exterior outside.
- **Flipped:** the section is mirrored about the axis. The exterior face is at y = +d and the layers run towards −y. Flipping negates the offset and reverses the sense, so a double flip is exactly the identity.

**Changing the location line of a type** keeps placed walls where they are (ARC-30). Their axis moves sideways, along y, by d_new − d_old when the wall is not flipped and by d_old − d_new when it is flipped. **Changing layer thicknesses** keeps the axis: the body is resolved again around it.

**Profiles.** Profile x is the member's local y (horizontal, across the section). Profile y is its local z (up). For a beam, local x runs along the member from start to end. For a column, it runs up. The cardinal point P comes from the profile's tight bounding box (arcs included), its centroid, or its shear centre. The member offset (Δy, Δz) is added to it. The section is translated by (Δy − P.x, Δz − P.y), so P lands at (Δy, Δz) from the axis.

The caller may give a known shear centre (from a section catalogue, or computed earlier). Otherwise the kernel computes it from the profile ([ADR 0011](0011-shear-centre.md)). It may lie outside the section, as for a channel. When it cannot be computed, resolution fails with a typed error rather than guessing.

Resolution uses only addition, subtraction, multiplication and division, so it is bit-identical on every target (ENG-04). Errors carry indices or codes, never dimensions (SEC-03).

### IFC mapping (INT-10)

| BIMCraft | IFC 4.3 |
| --- | --- |
| Layer set of a wall | `IfcMaterialLayerSetUsage` with `LayerSetDirection = AXIS2` (`AXIS3` for slabs) |
| Not flipped / flipped | `DirectionSense = POSITIVE` / `NEGATIVE` |
| Distance d of the location line from the exterior face | `OffsetFromReferenceLine = −d` (POSITIVE) or `+d` (NEGATIVE) |
| Cardinal points 1–9 (bottom-left to top-right, row by row) | `CardinalPoint` 1–9 |
| Centroid / shear centre | `CardinalPoint` 10 / 15 |
| Member offset (Δy, Δz) | Translation of the swept area's 2D position relative to the cardinal point |
| Tapered members | `ReferenceExtent` where it applies, once tapering exists |

**Export** writes the resolved offset as an exact `f64`, so the geometry round-trips without loss. The user's choice (layer id, position, anchor face) is written in a BIMCraft property set, so a BIMCraft file rebuilds the same choice.

**Import without that property set** maps the offset back to the core centre or a snap point within `LINEAR_TOLERANCE`. Otherwise it maps it to a custom offset from the exterior face of the first layer, at exactly the imported distance.

**Cardinal points 11–14 and 16–19** ("in line with" the centroid or shear centre) map to the nearest bounding-box point on the same side: 11/16 to bottom centre, 12/17 to middle left, 13/18 to middle right, 14/19 to top centre. The import report (INT-02) lists them.

## Consequences

- The wall and member tools can use one tested resolver. So can the IFC importer and exporter, before any element exists.
- Layer ids are mandatory. A wall-type editor must keep them stable when layers are reordered, and it must never derive them from positions.
- A slab uses the same layer set, with its exterior face at the top.
- Resolving a `ShearCentre` location point costs a finite-element solve (ADR 0011), so element code should compute it once per profile and keep it.
- Changing these sign conventions changes stored results and the IFC output.

## Alternatives considered

- **Snap points by index** (as the list is shown). Simple, but reordering layers silently moves every wall type's location line.
- **Schema-defined location types now.** ADR 0002 keeps behaviour in code, and there is no wall-type entity yet to hang them on. The stored fields arrive with that entity.
- **Location line in `core-model`.** The resolution needs profiles and the tolerance policy, which are in `geom`. Putting it in `core-model` would duplicate them or invert the layering.
- **Exterior on the right of the drawing direction.** It matches some authoring tools, but it needs `DirectionSense = NEGATIVE` for unflipped walls. IFC readers handle that case less consistently.
- **Approximating the shear centre by the centroid.** Wrong for channels and angles, which are exactly the profiles where users pick it.

## References

- Specification: ARC-30, STR-14, GEO-13, INT-10, MAT-03, GEO-11, ENG-04.
- [ADR 0001](0001-geometry-tolerance-policy.md): tolerance policy. [ADR 0002](0002-schema-definition-format.md): schema definition format. [ADR 0004](0004-units-and-quantities.md): units.
- IFC 4.3: `IfcMaterialLayerSetUsage`, `IfcMaterialProfileSetUsage`, `IfcCardinalPointReference`.
- FigCraft design, page "3 · Architecture modelling", board 3.3 "Wall type — layers and location line".
