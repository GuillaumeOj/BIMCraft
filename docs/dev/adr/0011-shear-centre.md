# 0011. Shear centre of arbitrary profiles

- Status: Accepted
- Requirements: STR-14, GEO-13 (also ENG-03, ENG-04, SEC-03)
- Issues: [#187](https://github.com/GuillaumeOj/BIMCraft/issues/187), [#189](https://github.com/GuillaumeOj/BIMCraft/issues/189); builds on [ADR 0008](0008-arc-faceting.md) and [ADR 0010](0010-location-lines.md)

## Context

STR-14 lets a column or beam sit on its shear centre: the point a transverse load must pass through to bend the member without twisting it. For doubly symmetric sections, the shear centre is the centroid. For channels, angles, tees and the custom profiles of GEO-12, it is not. For a channel, it even lies outside the section.

A caller can supply a shear centre from a section catalogue, but a profile drawn by the user has none. The kernel must compute it for any valid `Profile`: lines and arcs, holes included, whatever the wall thickness.

## Decision

### Definition

The kernel uses **Trefftz's definition**, based on the Saint-Venant warping function ψ of the section:

- ψ satisfies ∇²ψ = 0 in the section and ∂ψ/∂n = y nₓ − x n_y on its boundary, with the origin at the centroid.
- Rotating about a point (a, b) changes the warping function to ψ + a y − b x.
- The shear centre is the point for which this function is orthogonal to x and y over the section.

That orthogonality gives a 2 × 2 linear system. Its terms are the second moments I_xx = ∫y², I_yy = ∫x² and I_xy = ∫xy, and the warping products I_xψ = ∫xψ and I_yψ = ∫yψ:

- a = (I_xψ I_xy − I_yy I_yψ) / (I_xx I_yy − I_xy²)
- b = (I_xx I_xψ − I_xy I_yψ) / (I_xx I_yy − I_xy²)

The result does not depend on Poisson's ratio, so it is a pure geometric property. For thin-walled open sections, it equals the shear centre of classical thin-walled theory.

### Solving for ψ

ψ is solved with the **finite element method on linear triangles**:

1. **Facet the profile** as for extrusion ([ADR 0008](0008-arc-faceting.md)).
2. **Choose the target element size.** It is h = √(4A / (√3 · 4000)) for a profile of area A, which aims at about 4 000 elements (`TARGET_ELEMENTS`) whatever the units or proportions.
3. **Place the nodes.** Every boundary edge is split into pieces no longer than h/2. Interior nodes follow a triangular lattice of spacing h, and a lattice node is kept only if it lies inside the section and at least h/2 from its boundary.
4. **Triangulate.** The nodes are triangulated by incremental Delaunay insertion (Bowyer–Watson) with the exact predicates of ADR 0001. A boundary piece missing from the triangulation is split at its midpoint until every piece is an edge, which makes the mesh conforming. Triangles are then classified inside or outside by crossing boundary edges from a vertex of the enclosing triangle.
5. **Solve.** The stiffness matrix and the load vector are assembled, with fᵢ = ∫ ∇Nᵢ · (y, −x) dA. That form needs no boundary integral. One node is pinned, and the system is solved by conjugate gradients with a Jacobi preconditioner. The solve stops at a residual of 10⁻¹² relative to the load (`RELATIVE_RESIDUAL`).
6. **Integrate.** The integrals are evaluated exactly over the linear elements, and the shear centre is shifted back from the centroid.

Everything uses IEEE basic operations and the square root, which are correctly rounded, and `libm` for the rest (`hypot` for edge lengths, the faceting and inside tests of ADR 0008 and ADR 0005, per ADR 0003), all in a fixed order. So the result is bit-identical on every target (ENG-04).

### Limits

- **Size:** a mesh larger than 50 000 nodes (`MAX_MESH_NODES`), or an interior lattice of more than 2 × 10⁶ points (`MAX_LATTICE_POINTS`) or costing more than 2 × 10⁸ point-to-edge tests (`MAX_LATTICE_TESTS`), fails with a typed error rather than running for minutes. So does a boundary that cannot be recovered within that budget, which can happen at very sharp corners. A solve that does not converge also fails with a typed error.
- **Accuracy:** about 1 % of the section depth on the test sections. That is well below what positioning a member needs. The computed point can be stored as a catalogue value and does not need recomputing.

## Consequences

- `CardinalPoint::ShearCentre` resolves for every profile, with no caller input.
- A shear-centre resolution costs a mesh and a solve: milliseconds natively. Element code should compute it once per profile and keep it.
- The same mesh and solver give the torsion constant J and the warping constant, which STR-02 will need. That work is left for later.
- A different mesh density changes the results in their last digits. The target element count is part of the stored-result contract, like `ARC_CHORD_TOLERANCE`.

## Alternatives considered

- **Thin-walled theory along a midline.** It is exact for the sections it applies to and cheap. But it needs a midline that a general profile does not have, it is wrong for stocky sections, and it cannot handle closed cells without extra theory. It is kept as the test reference.
- **Harmonic-polynomial Ritz method without a mesh.** It needs no mesher, but it converges poorly at re-entrant corners such as flange-to-web junctions. It also needs extra multivalued terms for sections with holes.
- **Boundary element method.** It is accurate with only a boundary mesh, but it needs singular integrals and a dense solver. It is no simpler than a planar mesher.
- **Quadratic triangles.** They are more accurate per node, but double the code paths. At 4 000 linear elements, the error is already far below what matters for positioning.

## References

- Specification: STR-14, GEO-13, ENG-03, ENG-04.
- [ADR 0001](0001-geometry-tolerance-policy.md): exact predicates. [ADR 0008](0008-arc-faceting.md): arc faceting. [ADR 0010](0010-location-lines.md): location points.
- E. Trefftz (1935), "Über den Schubmittelpunkt in einem durch eine Einzellast gebogenen Balken", ZAMM 15.
- W. D. Pilkey, *Analysis and Design of Elastic Beams*, Wiley, 2002, chapter 6 (warping function and shear centre by finite elements).
- S. P. Timoshenko and J. M. Gere, *Theory of Elastic Stability*, 2nd ed., 1961, for thin-walled reference values.
