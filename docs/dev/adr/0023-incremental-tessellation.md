# 0023. Incremental tessellation

- Status: Accepted
- Requirements: GEO-05, CORE-08 (also GEO-06, CORE-07, ARCH-01, ARCH-02, ENG-04, SEC-03)
- Issues: [#69](https://github.com/GuillaumeOj/BIMCraft/issues/69), builds on [ADR 0014](0014-components-types-instances.md), [ADR 0015](0015-commands-and-transactions.md) and [ADR 0022](0022-face-triangulation.md)

## Context

The viewport draws triangle meshes (#68) and `Solid::tessellate` makes them from solids (ADR 0022), but nothing computes the solids of the model's elements or keeps their meshes current. Meshing the whole model after every command does not scale (NFR-02): after a transaction, an undo or a redo, only the elements whose geometry can have changed may be meshed again (GEO-05), through the dependency graph of `core-model`, lazily and incrementally (CORE-08).

What the model offers:

- a schema entity lists its geometric representations: a level of detail, a context (`body`, `axis`, `footprint`…), the path of a generator, and the attributes it reads (GEO-06);
- every transaction, undo and redo gives a `ChangeEvent` listing the objects created, modified and deleted, as net states. A change to a type lists the type, not its instances; a change to a relationship lists the relationship (CORE-07, ADR 0015);
- `DependencyGraph<K, V>` marks stale what depends on a changed node and recomputes lazily, but it cannot call a function that both computes a value and declares the dependencies it found while doing so.

No entity of the core schema has a representation yet, and no generator exists. The scene therefore takes generators as plug-ins.

## Decision

### A `scene` crate

`scene` is a core crate in the layer of `georef` and `material`: it depends on `core-model`, `core-txn`, `geom` and `schema`, never on `render` (a higher layer). It builds for `wasm32-unknown-unknown`, uses no platform API, and is critical (98 % lines).

- A `Generator` computes the solids of one element's representation from the model, and the objects it read beyond the obvious ones (`Generated::inputs`). Generators are registered by the path the schema names in `Representation::generator`, such as `wall::body`; a closure is a generator.
- A `View` says which representation is meshed: in its context, the greatest level of detail not above its own (the last declared on a tie). An entity with none is not meshed.
- A `Scene` meshes every such element once (`rebuild`), marks stale what an event can have changed (`apply`, which computes nothing), meshes the stale elements and only them (`update`), and meshes one element on demand (`mesh`).

### Dependency keys

The graph has two kinds of node: `Mesh(element)` and `Object(id)`, an object of the model. The mesh of an element depends on:

1. its own object;
2. its type, if it has one;
3. every relationship it takes part in;
4. every object its own and its type's attributes refer to, in lists and sourced values too;
5. every object its generator lists in `Generated::inputs`.

The dependencies are declared again after every generation, so they follow the model: an element that now reads another finish stops depending on the former one.

Items 2 to 4 are known from the model alone, whether or not the generator succeeds or gets as far as saying what it read. Item 4 matters when a generator fails: a wall that refers to a finish that does not exist yet must be meshed when the finish is created, and a generator that fails on the missing object cannot say that it wanted it. Item 5 covers what no attribute says: a layer set found through a relationship, a shared resource.

After a failure the generator's own list is unknown: the element keeps the objects it read the last time it succeeded, so fixing one of them meshes it again.

### Invalidation

`apply` follows the event:

- an object **created, modified or deleted** makes stale every mesh that depends on it: the element itself, the instances of a modified or deleted type, the elements that took part in a relationship that changed or went, the elements that read an input;
- a **deleted** element loses its mesh and its dependencies;
- a **created** element with a matching representation is meshed by the next update;
- a **created or modified relationship** also makes stale the meshes of its current participants, since no generator can have read it before. Participants that left are reached through the dependency on the relationship, so the model never has to be asked about objects that are gone;
- everything else keeps its cached mesh.

An element that is stale is meshed once, however many events marked it.

### What an update says

`SceneUpdate` is the net effect since the previous update, compared with what the viewport was last told: `added` (a mesh it had none for), `changed` (a new mesh), `removed` (deleted) and `failed` (no mesh, and why). `failed` replaces any previous mesh: an element that had one and fails loses it, and the viewport must drop it. An element deleted and recreated between two updates, as when an undo follows a delete, is `changed`, not removed and added. `mesh` may mesh a stale element before the update; the update still reports it, and the generator does not run again.

### Failures are per element

A generator error, a solid that cannot be tessellated (`TessellationError`, which ADR 0022 allows for some valid boolean results), a representation whose generator is not registered, and merged meshes with more vertices than 32-bit indices address (2³², checked with an injectable limit in tests) put the element in `failed` with a `SceneError` and leave the others alone. The index limit is not the renderer's: the viewport glue meets `render`'s own cap when it uploads. Errors name solids and faces, never coordinates or model content (SEC-03). Ordered maps and sets make every result deterministic.

### Cost

An edit costs the walk of the graph from the changed objects, linear in the stale meshes, plus one generation and tessellation per stale element. An update with nothing stale costs nothing. Declaring dependencies reads the element's state: linear in its attributes, once per element meshed.

### Tests

Re-meshing is counted twice after each kind of edit: from what the update reports, and from the generator calls that really ran, so a lazy `mesh` or a hidden recomputation cannot pass. A seeded property test runs random commands with undo and redo and checks, after each step and again with several events between updates, that the viewport state equals a scene built from scratch, that each element is generated once exactly when it is reported, and that only elements the event can have reached are meshed again.

## Consequences

- Editing one wall meshes one wall; editing a type meshes its instances; editing a finish meshes the walls that read it; a command that changes nothing geometric meshes nothing.
- Granularity is the object. A change to an attribute no generator reads still meshes the element: change events list objects, not attributes, and `Representation::depends_on` is not used. Attribute-level tracking is a later refinement if profiling asks for it.
- Adding a participant to a relationship meshes every participant of it, a containment in a storey included. Cheap generators make this acceptable; a generator that needs the relationship can read it, and one that does not pays one mesh per participant.
- Elements that depend on elements (a wall cut by an opening) read the other element through `inputs` or through a relationship. Nothing needs the graph to order them: a mesh depends on objects, not on other meshes.
- A deleted object that other objects still refer to is a missing input, not a panic: the generator fails and the element is `failed` until the object is back.
- The viewport glue (a later issue) converts `ElementMesh` into `render`'s mesh data and applies updates.

## Alternatives considered

- **Mesh the whole model after every command.** Correct and trivial, but linear in the model for every edit, which NFR-02 rules out.
- **Attribute-level tracking through `Representation::depends_on`.** Meshes fewer elements, but change events list objects, not attributes, so it needs a new event and attributes read through the type's inheritance (`inheriting_instances`). The schema's lists would also have to stay true of the generators that read them. Left for when profiling asks.
- **Recording what a generator reads through a model proxy.** Exact and without a list to keep true, but every read goes through a wrapper that records it, and nothing says what a generator reads through a type or a relationship, which the scene tracks anyway. It makes generators depend on the scene.
- **Storing the meshes in the graph.** `DependencyGraph::get` takes a function from the values of the dependencies, but the dependencies of a mesh are only known once its generator ran. The graph keeps what is stale; the scene keeps the meshes.
- **Looking up the former participants of a deleted or changed relationship in the event.** Events carry no former state and the objects may be gone. The dependency of each participant on the relationship does it without asking.
- **Failing the whole update on the first error.** One broken element would blank the viewport.

## References

- Specification: §6 (GEO-05, GEO-06), CORE-07, CORE-08, NFR-02, SEC-03.
- [ADR 0014](0014-components-types-instances.md): types and instances. [ADR 0015](0015-commands-and-transactions.md): commands, transactions and change events. [ADR 0022](0022-face-triangulation.md): face triangulation.
- Issues: [#69](https://github.com/GuillaumeOj/BIMCraft/issues/69), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67).
