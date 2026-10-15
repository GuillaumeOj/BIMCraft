# 0002. Schema definition format

- Status: Accepted
- Requirements: spec §5.1; CORE-02, CORE-03, CORE-04, CORE-09, CORE-10, GEO-06, API-02, PY-01, PY-04, PY-07, PERS-01, PERS-04, ARCH-01, ARCH-02
- Issues: [#15](https://github.com/GuillaumeOj/BIMCraft/issues/15) (epic [#14](https://github.com/GuillaumeOj/BIMCraft/issues/14)), [#16](https://github.com/GuillaumeOj/BIMCraft/issues/16), [#17](https://github.com/GuillaumeOj/BIMCraft/issues/17), [#18](https://github.com/GuillaumeOj/BIMCraft/issues/18), [#19](https://github.com/GuillaumeOj/BIMCraft/issues/19), [#36](https://github.com/GuillaumeOj/BIMCraft/issues/36), [#142](https://github.com/GuillaumeOj/BIMCraft/issues/142), [#153](https://github.com/GuillaumeOj/BIMCraft/issues/153)

## Context

The specification makes the schema the single source of truth (§5.1): each entity declares its attributes, property sets, allowed relationships, representations and IFC mapping, and generators produce the Rust types, serialization, the API JSON Schema, Python type stubs, the IFC mapping code and the reference documentation. Every later piece of Phase 0 — generators, persistence, the IFC round-trip — reads these definitions, which makes their format the most expensive decision to change later (§15).

Two requirements reach beyond the core team. Plugins written in Python, running in their own process (PY-02), can add schema entities (PY-04) that carry versions and migrations like the core schema (PY-07). Project and component files must open from every earlier schema version (PERS-01, PERS-04). The format must therefore serve both the build of the core and the runtime loading of plugin definitions.

Decision drivers, most important first:

1. **One source for core and plugins** (§5.1, PY-04): a plugin author declares entities in the same format as core developers, so the API, JSON Schema and documentation treat both alike.
2. **Versions and migrations are part of the definition** (PERS-01, PERS-04, PY-07).
3. **Every output can be generated** from it: Rust types, serialization, JSON Schema (API-02), Python stubs (PY-01), IFC mapping (CORE-10), reference documentation (COM-04).
4. **Readable and reviewable**: a schema change is a small, obvious diff that domain experts (architects, engineers) can review, not only Rust developers.
5. **Runtime loading works everywhere** (ARCH-01, ARCH-02): the validator and registry that load plugin definitions build for `wasm32-unknown-unknown` and do no I/O of their own.
6. **Cheap tooling**: formatting, editor completion and validation without building and maintaining our own.
7. **Safe on hostile input** (AGENTS.md §8): plugin definitions are untrusted, so parsing must be bounded and simple.

## Decision

**Schema definitions are TOML files** (option B), one file per entity or property set, built into one canonical model that every generator, the runtime registry and the migration check share.

### Layout

```text
schema/
  core/
    namespace.toml                   # namespace name and current schema version
    entities/wall.toml               # an element and its type (CORE-02)
    entities/beam.toml
    relationships/containment.toml   # a relationship entity (CORE-03)
    property-sets/Pset_WallCommon.toml
    migrations/0002.toml             # from version 1 to version 2
    snapshots/0001.json              # canonical model of each released version,
    snapshots/0002.json              #   generated and committed
```

A plugin's source tree has the same layout, with its own namespace.

### Processing

1. **Parse** each TOML file into typed Rust structures with `serde`; unknown keys are errors.
2. **Build the canonical model**: one sorted, normalised description of the namespace, with defaults and derived names filled in. Same input, same output.
3. **Validate** the canonical model: unique names, map keys equal to the names they index, existing references, known IFC entities and predefined types, unique IFC mapping keys, known quantities, size and nesting limits.
4. **Emit**, at build time, for the core namespace: Rust types and serialization ([#16](https://github.com/GuillaumeOj/BIMCraft/issues/16)), JSON Schema ([#17](https://github.com/GuillaumeOj/BIMCraft/issues/17)), the IFC mapping table ([#18](https://github.com/GuillaumeOj/BIMCraft/issues/18)), reference documentation ([#19](https://github.com/GuillaumeOj/BIMCraft/issues/19)) and Python stubs ([#142](https://github.com/GuillaumeOj/BIMCraft/issues/142)).
5. **Bundle**, for a plugin: the plugin SDK's build command, which wraps the same Rust code, writes the snapshots and migrations into one `schema.json` shipped with the plugin.

At runtime the host only loads canonical models: the generated one for the core, the bundle for each plugin. A bundle is untrusted, so it goes through step 3 when the plugin is installed or updated; the validated model is then cached by plugin id, version and content hash, and opening a file only compares namespace versions. Step 3 and the registry build for wasm32 and take contents, not paths (ARCH-01, ARCH-02); TOML parsing and the emitters are build-time only.

**One store for every namespace.** All entities, core and plugin, live in the registry-driven attribute tables (CORE-05). The Rust types generated for core entities are typed views over those tables, not a second storage model, so persistence, undo, the API, migrations and IFC export each have one code path. Data from a missing plugin stays in its tables, simply without the plugin's code.

### File format

Every definition file starts with `format`, the version of the definition file format itself, so the format can evolve independently of the schema it describes. Versions of the schema are not written in definitions: they come from the snapshots (see [Versioning and migrations](#versioning-and-migrations)).

| Key | Content |
| --- | --- |
| `[entity]` | `name`, `kind` (`element`, `spatial`, `relationship`, …), `doc`, optional `deprecated` (version), `property_sets` (CORE-04) |
| `[type]` | For elements with types (CORE-02): `doc`. The type is named `<Entity>Type`, unless `name` overrides it |
| `[ifc]` | `entity` and `predefined_type` (CORE-10); for `USERDEFINED`, `object_type`. The IFC type entity is `<entity>Type`, unless `type_entity` overrides it |
| `[[attribute]]` | `name`, `value`, `required` or `default`, `on`, `doc` |
| `[[role]]` | Relationship entities only: `name`, allowed `targets` (entity names, or `kind:<kind>` for every entity of a kind), `cardinality` in one relationship, and `exclusive` when an object can play that role in only one such relationship |
| `[[representation]]` | `lod`, `context` (`body`, `axis`, `footprint`, `analytical`…), the `generator` function that computes it, and `depends_on`, the attributes it reads (GEO-06) |

- **Value types**: `bool`, `integer`, `real`, `text`, `enum:<Name>`, `quantity:<kind>` (`quantity:length`, `quantity:plane_angle`…), `ref:<Entity>`, `list:<value type>`. Quantities are stored in the SI unit of their kind (CORE-09), so no unit is written.
- **Placement** (CORE-02): `on = "instance"` (the default), `"type"` (fixed by the type) or `"type_overridable"` (set by the type, overridable per instance).
- `depends_on` is the dependency graph's input (CORE-08) and tells IFC export which parameters are geometric (INT-07).

A property set file has `[property_set]` (`name`, `applies_to`) and `[[property]]` entries with the same keys as attributes, placement included. User-defined property sets (CORE-04) are model data with this same structure, but they live in the project file, not in `schema/`.

Allowed relationships are declared once, on the relationship entity, not on each participant. An element therefore takes part in containment, connections or voids through its `kind`, and a plugin element joins core relationships without changing any core file.

### Worked example: a wall

```toml
# schema/core/entities/wall.toml
format = 1

[entity]
name = "Wall"
kind = "element"
doc = "A vertical element that bounds or subdivides spaces, built from layers."
property_sets = ["Pset_WallCommon"]

[type]
doc = "A named wall build-up: layers, materials and default properties."

[ifc]
entity = "IfcWall"
predefined_type = "STANDARD"

[[attribute]]
name = "layers"
value = "list:ref:MaterialLayer"
on = "type"
doc = "Layers from the exterior face to the interior face."

[[attribute]]
name = "axis"
value = "ref:Curve2D"
required = true
doc = "Location line in the storey plane."

[[attribute]]
name = "height"
value = "quantity:length"
on = "type_overridable"
default = 2.5

[[attribute]]
name = "base_offset"
value = "quantity:length"
default = 0.0

[[representation]]
lod = 100
context = "footprint"
generator = "wall::footprint"
depends_on = ["axis", "layers"]

[[representation]]
lod = 200
context = "body"
generator = "wall::body_envelope"
depends_on = ["axis", "layers", "height", "base_offset"]

[[representation]]
lod = 300
context = "body"
generator = "wall::body_layers"
depends_on = ["axis", "layers", "height", "base_offset"]
```

### Worked example: a beam

Two representations share a level of detail in different contexts: the 3D body and the analytical bar used by solvers.

```toml
# schema/core/entities/beam.toml
format = 1

[entity]
name = "Beam"
kind = "element"
doc = "A horizontal or near-horizontal linear structural member."
property_sets = ["Pset_BeamCommon"]

[type]
doc = "A named beam section and material."

[ifc]
entity = "IfcBeam"
predefined_type = "BEAM"

[[attribute]]
name = "profile"
value = "ref:Profile"
on = "type"
required = true

[[attribute]]
name = "material"
value = "ref:Material"
on = "type_overridable"
required = true

[[attribute]]
name = "axis"
value = "ref:Curve3D"
required = true

[[attribute]]
name = "roll"
value = "quantity:plane_angle"
default = 0.0

[[representation]]
lod = 100
context = "axis"
generator = "beam::axis"
depends_on = ["axis"]

[[representation]]
lod = 300
context = "body"
generator = "beam::swept_profile"
depends_on = ["axis", "profile", "roll"]

[[representation]]
lod = 300
context = "analytical"
generator = "beam::analytical_bar"
depends_on = ["axis", "profile", "material"]
```

### Worked example: a relationship

Walls and beams are contained in a storey because they are elements; neither file has to say so.

```toml
# schema/core/relationships/containment.toml
format = 1

[entity]
name = "Containment"
kind = "relationship"
doc = "Places elements in the spatial structure, such as a storey."

[ifc]
entity = "IfcRelContainedInSpatialStructure"

[[role]]
name = "structure"
targets = ["kind:spatial"]
cardinality = "one"

[[role]]
name = "elements"
targets = ["kind:element"]
cardinality = "many"
exclusive = true
```

### Worked example: a property set

Property names follow the IFC property set; descriptions are our own words (REP-05).

```toml
# schema/core/property-sets/Pset_WallCommon.toml
format = 1

[property_set]
name = "Pset_WallCommon"
applies_to = ["Wall"]

[[property]]
name = "IsExternal"
value = "bool"
on = "type_overridable"
doc = "True when the wall separates the inside from the outside."

[[property]]
name = "LoadBearing"
value = "bool"
on = "type_overridable"
doc = "True when the wall carries loads other than its own weight."

[[property]]
name = "FireRating"
value = "text"
on = "type_overridable"
doc = "Fire resistance class as written in the project's regulation."

[[property]]
name = "ThermalTransmittance"
value = "quantity:thermal_transmittance"
on = "type_overridable"
```

### Versioning and migrations

- Each **namespace** — `core`, or `plugin:<plugin-id>` — has one integer schema version in its `namespace.toml`, increased by one for every released change.
- Each released version has a committed **snapshot**, `snapshots/NNNN.json`: the canonical model of that version. The version that introduced each entity, attribute or property is read from the snapshots, not written by hand, and shown in the generated documentation.
- `deprecated` marks an item as discouraged from a given version. A deprecated item still loads and stays in the API for the deprecation period (API-06); removing it is a schema change like any other.
- A **project or component file** stores the schema version of every namespace it uses (PERS-01, PERS-04).
- **Migration files**: `migrations/NNNN.toml` migrates from version `NNNN - 1` to `NNNN`; the file name carries both versions. Additions and removals are inferred from the difference between the two snapshots, an added attribute taking its `default`. Authors write only what a difference cannot tell, in order:

```toml
# schema/core/migrations/0002.toml
format = 1

[[step]]
op = "rename_attribute"
entity = "Wall"
old_name = "base_offset"
new_name = "bottom_offset"

[[step]]
op = "custom"
entities = ["Wall"]
function = "migrations::v2::split_wall_layers"
doc = "Splits composite layers into one layer per material."
```

  Declarative operations: `rename_attribute`, `rename_entity`, `change_value` (new value type, with a conversion factor) and `set_value` (for an added required attribute without a default). Anything else uses `custom`, naming the entities it handles and a Rust function for the core or a Python function (`module:function`) for a plugin. A plugin's custom step runs in the plugin process (PY-02) and is called once per step with every affected object, not once per object.
- Opening a file runs every migration from the file's version to the current one, namespace by namespace, in one transaction; the file on disk is only rewritten on save.
- Migrations are resolved at build time into explicit steps (renames, additions, value changes, values for new required attributes, custom code, removals), run in that order with removals last, so applying them to a file needs no older schema. [ADR 0016](0016-project-file-layout.md#migrations) describes how each step changes the file's tables, and [schema-versions.md](../schema-versions.md) how to add a version.
- **CI** ([#36](https://github.com/GuillaumeOj/BIMCraft/issues/36)) applies migration `N` to snapshot `N - 1` and compares the result with snapshot `N`. Any difference not covered by a step, or by a `custom` step for the entities it names, fails the build, as does a definition change without a new version. The plugin SDK runs the same check.
- A file using a plugin namespace whose plugin is missing, or older than the file, still opens: that plugin's data is kept unchanged and read-only, and the user is told which plugin is needed.

### Plugin-defined entities

- A plugin's manifest names its bundled `schema.json`; its namespace is `plugin:<plugin-id>`, and every name it defines is qualified by it (`plugin:acme-timber/Truss`), so plugins never collide with the core or with each other.
- Plugins can add entities, property sets and relationships, and attach their property sets to core entities. They cannot change core definitions, IFC mappings or migrations.
- **IFC mapping keys are unique across the registry**: no two entities, in any namespaces, may share an IFC entity, predefined type and `object_type`. The registry checks this when a namespace is registered, so import always knows which entity to rebuild. When no specific IFC entity fits, a plugin uses `predefined_type = "USERDEFINED"` with an `object_type` of its own; every mapping, core or plugin, has a round-trip test (CORE-10).
- Plugin bundles are untrusted input (AGENTS.md §8, PY-08): the loader caps their size, the number of entities, attributes and steps, name length and nesting depth, rejects unknown keys, and reports every error with its location.

## Consequences

- ➕ One definition serves the core, plugins, the API, IFC export and the documentation; nothing is declared twice.
- ➕ Schema changes are short, reviewable diffs. History is derived from snapshots, so authors only write the migration steps a machine cannot infer, and CI checks that the steps actually turn one version into the next.
- ➕ Contributors and plugin authors reuse tools they know: TOML, `taplo` for formatting, and a published JSON Schema of the definition format for editor completion and validation.
- ➕ The runtime never parses TOML: it loads one validated, cached model per namespace.
- ➖ Mistakes in definitions are reported by our validator, not by the Rust compiler; good error messages with file and line are a requirement of the parser, not a nicety.
- ➖ TOML's arrays of tables make long definitions verbose; one file per entity keeps each file short.
- ➖ The validator and registry are runtime code that reads untrusted bundles, so they fall under the wasm32 rule and the critical-area coverage and fuzzing rules (AGENTS.md §5, ENG-02).
- ➖ Representations and custom migrations name functions by string; the generator must check that each name resolves, or the build fails.
- `toml`, `serde` and `serde_json` arrive with [#16](https://github.com/GuillaumeOj/BIMCraft/issues/16), justified as AGENTS.md §4 requires.

## Alternatives considered

### A. Rust macros or a Rust DSL

Entities are Rust structs with derive or attribute macros (`#[entity(ifc = "IfcWall")]`); generators run on the macro input or on reflection data.

- ➕ Compile-time checking, rename refactoring and IDE support for core entities at no extra cost.
- ➕ No separate parser for the core.
- ➖ Python plugins cannot write Rust, so plugin entities need a second format: two sources of truth, contradicting §5.1 and driver 1.
- ➖ Non-Rust outputs (JSON Schema, Python stubs, documentation) need the macro to export its model anyway, which is a data format in disguise.
- ➖ Migrations expressed in macros are hard to read and hard to check for completeness.
- ➖ Schema diffs are mixed with Rust syntax, so domain experts cannot review them easily.

### B. A data file format (chosen: TOML)

Definitions are plain files parsed into one canonical model, from which each output is generated. Candidates:

| Format | For | Against |
| --- | --- | --- |
| **TOML** | Widely known (Cargo, Python packaging); comments; explicit types, no implicit conversions; `taplo` is already pinned in the repository and formats and validates TOML against a JSON Schema, with editor completion | Deeply nested data is verbose (arrays of tables); no references or reuse inside a file |
| YAML | Compact nesting; widely known | Implicit typing pitfalls (`1.10` becomes `1.1`; in YAML 1.1 parsers such as PyYAML, the Python default, `NO` becomes `false`); indentation errors; the main Rust implementation, `serde_yaml`, is unmaintained; anchors and aliases make hostile input costly (“billion laughs”) |
| KDL | Node syntax fits nested definitions well | Little known; new parser dependency and new editor tooling to adopt |
| RON | Maps directly onto Rust types | Rust-specific, so foreign to plugin authors; weak editor support |
| JSON | Universal; JSON Schema validation | No comments; noisy to read and review |

- ➕ One format for core and plugins (driver 1); every output is a generator over the same canonical model (driver 3).
- ➕ Migrations are data, so their completeness can be checked in CI (driver 2).
- ➖ Errors in definitions surface at generation time, not in the Rust compiler; the generator must report them with file and line.

### C. A dedicated language

An EXPRESS-like language with its own grammar, parser and tools.

- ➕ The most expressive and concise; could reuse ideas from IFC's own EXPRESS schema.
- ➖ We would build and maintain the parser, formatter, error messages, editor support and documentation, before writing any generator.
- ➖ Another language for plugin authors to learn.
- ➖ The expressiveness is not needed: the definitions are declarative data, and behaviour (geometry, rules) lives in code.

## References

- [#16](https://github.com/GuillaumeOj/BIMCraft/issues/16) Generate Rust entity types: parser, canonical model, validator and the Rust emitter.
- [#17](https://github.com/GuillaumeOj/BIMCraft/issues/17) Generate the API JSON Schema; also publish the JSON Schema of the definition format itself for `taplo` and editors.
- [#18](https://github.com/GuillaumeOj/BIMCraft/issues/18) Generate the IFC mapping table from the `[ifc]` sections.
- [#19](https://github.com/GuillaumeOj/BIMCraft/issues/19) Generate the reference documentation from `doc` fields and the snapshots.
- [#36](https://github.com/GuillaumeOj/BIMCraft/issues/36) Migration framework, per-version snapshots and the CI migration check.
- [#142](https://github.com/GuillaumeOj/BIMCraft/issues/142) Generate the Python client and stubs.
- [#153](https://github.com/GuillaumeOj/BIMCraft/issues/153) Specification: §4 lists the `schema` crate as *build-time only*, but the validator and registry run at runtime. The issue splits the row into a wasm32-compatible schema model (validator, registry) and build-time parsing and generators, and states in §5.1 that plugin entities are loaded at runtime rather than compiled.
