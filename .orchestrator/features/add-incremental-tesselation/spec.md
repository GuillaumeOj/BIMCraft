# Incremental tessellation (GEO-05, CORE-08)

Issue: #69, part of the 3D viewport epic #67. Requirements: **GEO-05** (tessellation is
incremental: only changed elements are re-meshed), **CORE-08** (derived data goes through the
dependency graph, lazily and incrementally), with **GEO-06** (a view picks the representation it
needs) and **CORE-07** (change events) as inputs.

## 1. Problem

The viewport (#68, `render` crate, now on `main`) draws triangle meshes, but nothing turns model
geometry into triangles yet, and nothing keeps meshes current while the user edits. Re-meshing the
whole model after every command does not scale to large models (NFR-02). After a transaction, only
the elements whose geometry can have changed may be re-meshed.

## 2. Scope

In scope, backend only (Rust):

1. **Face triangulation in `geom`**: `Solid::tessellate` turns a valid B-rep solid (planar faces,
   holes allowed) into a triangle mesh.
2. **A new `scene` crate**: the meshes of the model's elements, kept current through the
   `core_model::derived::DependencyGraph` from the `core_txn::ChangeEvent` of each transaction,
   undo and redo. It re-meshes only stale elements and reports which ones it re-meshed.
3. ADRs, spec table update, and the developer docs that go with them.

Out of scope (later issues of the viewport epic):

- Wiring the scene into `host-desktop`, the API or the UI; uploading meshes to the `render`
  renderer. The scene's output feeds `render::MeshData::from_world` directly (positions as
  `geom::Point3`, normals converted to `f32`, `u32` indices), so the glue is a few lines when the
  viewport is wired.
- Real element generators (walls, slabs…): no entity of the core schema has a geometric
  representation yet. The scene takes generators as plug-ins; its tests register a fixture one.
- Attribute-level invalidation (re-meshing only when an attribute listed in a representation's
  `depends_on` changed): change events list objects, not attributes. Object-level granularity is
  enough for GEO-05.
- Curved faces, smooth normals, levels of detail beyond picking one representation.

No UI, so the `front`, `ui` and `ux` nodes have no task. **User docs: N/A (internal change)**, to
be stated in the pull request, as the issue says.

## 3. Design

### 3.1 `geom`: `Solid::tessellate`

```rust
/// A triangle mesh with one normal per vertex, flat-shaded (GEO-05).
pub struct TriangleMesh {
    pub positions: Vec<Point3>,
    pub normals: Vec<Vector3>,   // unit, outward, one per position
    pub indices: Vec<u32>,       // triangle list, 3 per triangle
}
impl TriangleMesh { pub fn triangle_count(&self) -> usize; }
impl Solid { pub fn tessellate(&self) -> Result<TriangleMesh, TessellationError>; }
```

- Each face is triangulated on its own, from its own vertices only (no Steiner points). A face
  with `n` vertices over all its rings and `h` holes gives exactly `n + 2h − 2` triangles.
- Vertices are duplicated per face so each carries the face's outward unit normal (flat shading,
  what the renderer expects).
- Triangles turn counter-clockwise seen from outside, like `FaceInput` rings.
- Algorithm: project the face on the coordinate plane that drops its dominant normal axis, bridge
  the holes into the outer ring, then clip ears, deciding every orientation and point-in-triangle
  test with the exact `orient2d` predicate. The owner may propose another algorithm in the plan
  (for example a constrained Delaunay triangulation without Steiner points) if it is more robust;
  whatever is chosen is recorded in **ADR 0021 (face triangulation)**.
- Bounded work: faces are bounded by the solid's own limits; the ADR states the worst-case cost.
- No panic: any face the algorithm cannot triangulate is a typed `TessellationError` naming the
  face index, never coordinates (SEC-03). For a solid that passed `Solid::validate`, this must
  not happen; tests and fuzzing check it.

Invariants checked by unit, property (`proptest`, seeded) and fuzz tests, on extrusions of random
profiles (with holes and arcs) and on boolean results:

- triangle count per face is `n + 2h − 2`, every index in range, no degenerate triangle;
- the triangles of a face cover exactly its area (sum of areas equals the face area within the
  tolerance policy) and each triangle's normal points like the face's;
- the mesh is closed and consistently oriented: by vertex position, every directed edge has
  exactly one reverse;
- the enclosed volume (divergence theorem) equals `Solid::volume`.

A fuzz target covers tessellation: a new `tessellation` target, or the `extrusion` and
`extruded_boolean` targets extended to tessellate their results and check the invariants, with
seeds.

### 3.2 New crate `scene`

Layer: the third layer of `scripts/crate-layers.txt` (with `georef`, `material`, `analysis`…),
depending on `core-model`, `core-txn`, `geom`. Listed in `scripts/wasm-crates.txt` and
`scripts/critical-crates.txt` (98 % lines). Added to the crate table of
`docs/specifications.md` §4: "`scene` | Element meshes derived from the model, kept current
incrementally (GEO-05) | wasm32-compatible". Core-crate rules apply (ARCH-02, ENG-04, no
`unwrap`/`panic`).

Public API (names may be refined in the owner's plan, behaviour may not):

```rust
/// Computes the solids of one representation of an element.
pub trait Generator {
    fn generate(&self, model: &Model, element: EntityId) -> Result<Generated, GeneratorError>;
}
pub struct Generated {
    pub solids: Vec<Solid>,
    /// Other objects the generator read (a layer set, an opening…), beyond the element itself
    /// and its type, which the scene always tracks.
    pub inputs: BTreeSet<EntityId>,
}
/// Generators by the path the schema names in `Representation::generator`, e.g. `wall::body`.
pub struct Generators { /* register(path, Box<dyn Generator>) */ }

/// Which representation the scene meshes (GEO-06): the entity's representation in `context`
/// with the greatest LOD not above `lod`.
pub struct View { pub context: String, pub lod: u16 }

pub struct Scene { /* … */ }
impl Scene {
    pub fn new(generators: Generators, view: View) -> Self;
    /// Meshes every element of `model` that has a matching representation (first build).
    pub fn rebuild(&mut self, model: &Model) -> SceneUpdate;
    /// Marks stale what `event` can have changed. Computes nothing (lazy).
    pub fn apply(&mut self, event: &ChangeEvent, model: &Model);
    /// Re-meshes the stale elements, and only them.
    pub fn update(&mut self, model: &Model) -> SceneUpdate;
    /// The current mesh of an element, re-meshing it first if it is stale.
    pub fn mesh(&mut self, model: &Model, element: EntityId) -> Option<&ElementMesh>;
}
pub struct ElementMesh { pub mesh: TriangleMesh /* all solids of the element merged */ }
/// What an update changed, for the viewport to upload, and for tests to count.
pub struct SceneUpdate {
    pub added: BTreeSet<EntityId>,
    pub changed: BTreeSet<EntityId>,
    pub removed: BTreeSet<EntityId>,
    pub failed: BTreeMap<EntityId, SceneError>,
}
impl SceneUpdate { /// Elements meshed by this update: added ∪ changed.
    pub fn remeshed(&self) -> usize; }
```

Dependencies, held in a `DependencyGraph`: the mesh of an element depends on the element's
object, on its type's object (`Model::type_of`), and on every object in `Generated::inputs`; the
dependencies are redeclared after each generation, so they follow the model.

`apply` invalidation rules, from the change event:

- a **modified** object makes stale every mesh that depends on it (itself if it is an element, the
  instances of a type, the elements whose generator read it);
- a **deleted** element loses its mesh (reported in `removed`); a deleted input makes its
  dependents stale;
- a **created** element with a matching representation is meshed by the next `update` (`added`);
- a **created or modified relationship** makes the meshes of its current participants stale
  (a new opening voiding a wall re-meshes the wall), since no generator can have read it before;
- everything else keeps its cached mesh.

Failures are per element: a generator or tessellation error puts the element in `failed`, leaves
it without a mesh, and does not stop the others. Error messages carry no coordinates or model
content (SEC-03). Iteration order is deterministic (`BTreeMap`/`BTreeSet`).

**ADR 0022 (incremental tessellation)** records the dependency keys, the invalidation rules, the
object-level granularity and the alternatives (re-mesh everything, attribute-level tracking,
recording reads through a model proxy).

### 3.3 Tests (the issue's acceptance criteria)

`scene` tests use a fixture namespace (walls with a type, a height and a thickness, a "body"
representation, a resource read by the generator, and a relationship) and a fixture generator
that extrudes a box. They drive a `core_txn::Document` and **count the re-meshed elements after
each edit**:

- first build of N walls re-meshes N; an edit of one wall's height re-meshes exactly 1, and the
  other meshes are the same values as before;
- an edit of a wall type re-meshes exactly its instances; an edit of an input resource re-meshes
  exactly the walls whose generator read it;
- a command that touches nothing geometric re-meshes 0; a second `update` with no new event
  re-meshes 0;
- create → `added` 1; delete → `removed` 1, re-meshed 0; undo and redo re-mesh exactly what
  they change;
- a new relationship re-meshes exactly its participants;
- an element whose generator fails is in `failed`, the others are meshed;
- property test (seeded): over random command sequences with undo/redo, after each step the
  incremental scene's meshes equal those of a scene rebuilt from scratch, and the re-meshed set is
  a subset of the elements the event can have reached.

Coverage: `geom` and `scene` ≥ 98 % lines. `cargo clippy -D warnings`, `cargo fmt`, `taplo`,
`typos`, `cargo deny`, architecture and wasm checks clean.

## 4. Tasks

| # | Node | Task | Depends on |
| --- | --- | --- | --- |
| 1 | back | `geom`: `Solid::tessellate`, `TriangleMesh`, `TessellationError`; unit, property and fuzz tests; ADR 0021; `geom` crate docs | — |
| 2 | back | `scene` crate: generators, view, dependency graph, invalidation from change events, `SceneUpdate`; re-mesh counting tests and the property test; ADR 0022; spec table, layers, critical and wasm lists | 1 |

Task 1 starts with a plan (algorithm choice). Both run on Opus (exact geometry, critical crates).

## 5. Notes for the integration

- The integration branch is behind `main` (which now has `render` and ADR 0020); it is rebased
  onto `main` before the pull request. Workspace `Cargo.toml` and `Cargo.lock` may conflict there.
- No new third-party dependency is expected.
- PR body: "User docs: N/A (internal change)", Refs #69, requirement IDs GEO-05, CORE-08.
