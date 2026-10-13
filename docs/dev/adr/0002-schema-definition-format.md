# 0002. Schema definition format

- Status: Accepted
- Requirements: spec §5.1; CORE-02, CORE-03, CORE-04, CORE-09, CORE-10, GEO-06, API-02, PY-01, PY-04, PY-07, PERS-01, PERS-04, ARCH-01, ARCH-02
- Issues: [#15](https://github.com/GuillaumeOj/BIMCraft/issues/15) (epic [#14](https://github.com/GuillaumeOj/BIMCraft/issues/14)), [#16](https://github.com/GuillaumeOj/BIMCraft/issues/16), [#17](https://github.com/GuillaumeOj/BIMCraft/issues/17), [#18](https://github.com/GuillaumeOj/BIMCraft/issues/18), [#19](https://github.com/GuillaumeOj/BIMCraft/issues/19), [#36](https://github.com/GuillaumeOj/BIMCraft/issues/36), [#142](https://github.com/GuillaumeOj/BIMCraft/issues/142)

## Context

The specification makes the schema the single source of truth (§5.1): each entity declares its attributes, property sets, allowed relationships, representations and IFC mapping, and generators produce the Rust types, serialization, the API JSON Schema, Python type stubs, the IFC mapping code and the reference documentation. Every later piece of Phase 0 — generators, persistence, the IFC round-trip — reads these definitions, which makes their format the most expensive decision to change later (§15).

Two requirements reach beyond the core team. Plugins written in Python, running in their own process (PY-02), can add schema entities (PY-04) that carry versions and migrations like the core schema (PY-07). Project and component files must open from every earlier schema version (PERS-01, PERS-04). The format must therefore serve both the build of the core and the runtime loading of plugin definitions.

Decision drivers, most important first:

1. **One source for core and plugins** (§5.1, PY-04): a plugin author declares entities in the same format as core developers, so the API, JSON Schema and documentation treat both alike.
2. **Versions and migrations are part of the definition** (PERS-01, PERS-04, PY-07).
3. **Every output can be generated** from it: Rust types, serialization, JSON Schema (API-02), Python stubs (PY-01), IFC mapping (CORE-10), reference documentation (COM-04).
4. **Readable and reviewable**: a schema change is a small, obvious diff that domain experts (architects, engineers) can review, not only Rust developers.
5. **Runtime loading works everywhere** (ARCH-01, ARCH-02): the parser and validator that load plugin definitions build for `wasm32-unknown-unknown` and do no I/O of their own.
6. **Cheap tooling**: formatting, editor completion and validation without building and maintaining our own.
7. **Safe on hostile input** (AGENTS.md §8): plugin definitions are untrusted, so parsing must be bounded and simple.

## Decision

**Schema definitions are TOML files** (option B), one file per entity, property set or relationship kind, parsed into one canonical model that every generator and the runtime registry share.

### Layout

```text
schema/
  core/
    namespace.toml            # namespace name and current schema version
    entities/wall.toml        # one entity (with its type, see CORE-02)
    entities/beam.toml
    property-sets/Pset_WallCommon.toml
    migrations/0002.toml      # from version 1 to version 2
    snapshot.json             # canonical model of the current version, generated and committed
```

A plugin ships the same tree, with its own namespace, inside its package.

### Processing

1. **Parse** each file into typed Rust structures with `serde`; unknown keys are errors.
2. **Validate** the whole namespace: unique names, existing references (types, property sets, relationship targets), known IFC entities and predefined types, known quantities and units, `since` not above the namespace version, size and nesting limits.
3. **Build the canonical model**: one sorted, normalised description of the namespace. Same input, same output.
4. **Emit**, at build time, for the core namespace: Rust types and serialization ([#16](https://github.com/GuillaumeOj/BIMCraft/issues/16)), JSON Schema ([#17](https://github.com/GuillaumeOj/BIMCraft/issues/17)), the IFC mapping table ([#18](https://github.com/GuillaumeOj/BIMCraft/issues/18)), reference documentation ([#19](https://github.com/GuillaumeOj/BIMCraft/issues/19)) and Python stubs ([#142](https://github.com/GuillaumeOj/BIMCraft/issues/142)).
5. **Load at runtime**, for plugin namespaces: steps 1–3 run when a plugin is installed or a file using it is opened, and the result is registered in the same schema registry as the core. Plugin entities are stored in generic attribute tables keyed by the registry; no Rust code is generated for them.

Steps 1–3 must build for wasm32 and take file contents, not paths (ARCH-01, ARCH-02); only the step 4 emitters are build-time only.

### File format

Every definition file starts with `format`, the version of the definition file format itself, so the format can evolve independently of the schema it describes.

| Key | Content |
| --- | --- |
| `[entity]` | `name`, `kind` (`element`, `spatial`, `relationship`, …), `doc`, `since`, optional `deprecated` (version), `property_sets`: names of the property sets it carries (CORE-04) |
| `[type]` | For elements with types (CORE-02): the type entity's `name` and `doc` |
| `[ifc]` | `entity` and `predefined_type` for the occurrence; `type_entity` for the type (CORE-10) |
| `[[attribute]]` | `name`, `value` (`bool`, `integer`, `real`, `text`, `quantity`, `enum`, `ref:<Entity>`, `list` with `items`), `quantity` and `unit` for quantities (CORE-09: storage in SI, `unit` is the SI unit), `required` or `default`, `on` (`type` or `instance`), `override` (`allowed` or `forbidden`) for type attributes, `since`, `doc` |
| `[[relationship]]` | `kind` (`aggregation`, `containment`, `connection`, `voiding`, `assignment`), the entity's `role`, allowed `targets`, `cardinality` (CORE-03) |
| `[[representation]]` | `lod`, `context` (`body`, `axis`, `footprint`, `analytical`…), the `generator` function that computes it, and `depends_on`, the attributes it reads (GEO-06) |

`depends_on` is the dependency graph's input (CORE-08) and tells IFC export which parameters are geometric (INT-07).

A property set file has `[property_set]` (`name`, `applies_to`, `since`) and `[[property]]` entries with the same keys as attributes. User-defined property sets (CORE-04) are model data that follow this same structure, but they live in the project file, not in `schema/`.

### Worked example: a wall

```toml
# schema/core/entities/wall.toml
format = 1

[entity]
name = "Wall"
kind = "element"
doc = "A vertical element that bounds or subdivides spaces, built from layers."
since = 1
property_sets = ["Pset_WallCommon"]

[type]
name = "WallType"
doc = "A named wall build-up: layers, materials and default properties."

[ifc]
entity = "IfcWall"
predefined_type = "STANDARD"
type_entity = "IfcWallType"

[[attribute]]
name = "layers"
value = "list"
items = "ref:MaterialLayer"
on = "type"
override = "forbidden"
since = 1
doc = "Layers from the exterior face to the interior face."

[[attribute]]
name = "axis"
value = "ref:Curve2D"
on = "instance"
required = true
since = 1
doc = "Location line in the storey plane."

[[attribute]]
name = "height"
value = "quantity"
quantity = "length"
unit = "m"
on = "type"
default = 2.5
override = "allowed"
since = 1

[[attribute]]
name = "base_offset"
value = "quantity"
quantity = "length"
unit = "m"
on = "instance"
default = 0.0
since = 1
[[relationship]]
kind = "containment"
role = "contained"
targets = ["Storey"]
cardinality = "one"

[[relationship]]
kind = "voiding"
role = "host"
targets = ["Opening"]
cardinality = "many"

[[relationship]]
kind = "connection"
role = "joined"
targets = ["Wall", "Slab", "Beam", "Column"]
cardinality = "many"

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

```toml
# schema/core/entities/beam.toml
format = 1

[entity]
name = "Beam"
kind = "element"
doc = "A horizontal or near-horizontal linear structural member."
since = 1
property_sets = ["Pset_BeamCommon"]

[type]
name = "BeamType"
doc = "A named beam section and material."

[ifc]
entity = "IfcBeam"
predefined_type = "BEAM"
type_entity = "IfcBeamType"

[[attribute]]
name = "profile"
value = "ref:Profile"
on = "type"
required = true
override = "forbidden"
since = 1

[[attribute]]
name = "material"
value = "ref:Material"
on = "type"
required = true
override = "allowed"
since = 1

[[attribute]]
name = "axis"
value = "ref:Curve3D"
on = "instance"
required = true
since = 1

[[attribute]]
name = "roll"
value = "quantity"
quantity = "plane_angle"
unit = "rad"
on = "instance"
default = 0.0
since = 1
[[relationship]]
kind = "containment"
role = "contained"
targets = ["Storey"]
cardinality = "one"

[[relationship]]
kind = "connection"
role = "joined"
targets = ["Column", "Beam", "Wall", "Slab"]
cardinality = "many"

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

### Worked example: a property set

Property names follow the IFC property set; descriptions are our own words (REP-05).

```toml
# schema/core/property-sets/Pset_WallCommon.toml
format = 1

[property_set]
name = "Pset_WallCommon"
applies_to = ["Wall"]
since = 1

[[property]]
name = "IsExternal"
value = "bool"
doc = "True when the wall separates the inside from the outside."

[[property]]
name = "LoadBearing"
value = "bool"
doc = "True when the wall carries loads other than its own weight."

[[property]]
name = "FireRating"
value = "text"
doc = "Fire resistance class as written in the project's regulation."

[[property]]
name = "ThermalTransmittance"
value = "quantity"
quantity = "thermal_transmittance"
unit = "W/(m2.K)"
```

### Versioning and migrations

- Each **namespace** — `core`, or `plugin:<plugin-id>` — has one integer schema version in its `namespace.toml`, increased by one for every released change.
- Entities, attributes and properties record the version that introduced them (`since`) and, once removed from use, the version that deprecated them (`deprecated`).
- A **project or component file** stores the schema version of every namespace it uses (PERS-01, PERS-04).
- Each version step has a **migration file**, `migrations/NNNN.toml`, migrating from version `NNNN - 1` to `NNNN`. It lists declarative steps, applied in order:

```toml
# schema/core/migrations/0002.toml
format = 1
from = 1
to = 2

[[step]]
op = "rename_attribute"
entity = "Wall"
from = "base_offset"
to = "bottom_offset"

[[step]]
op = "add_attribute"
entity = "Beam"
attribute = "camber"
value = 0.0

[[step]]
op = "custom"
function = "migrations::v2::split_wall_layers"
doc = "Splits composite layers into one layer per material."
```

  Declarative operations: `add_attribute` (with a value), `remove_attribute`, `rename_attribute`, `rename_entity`, `change_unit`, `add_property_set`, `remove_property_set`. Anything else uses `custom`, naming a Rust function for the core or a Python function (`module:function`) for a plugin, which runs in the plugin process (PY-02).
- Opening a file runs every migration from the file's version to the current one, namespace by namespace, in one transaction; the file on disk is only rewritten on save.
- **CI** regenerates `snapshot.json` and fails when the definitions changed but the namespace version did not, or when the version changed without a matching migration file ([#36](https://github.com/GuillaumeOj/BIMCraft/issues/36)). The same check is offered to plugin authors in the plugin SDK.
- A file using a plugin namespace whose plugin is missing, or older than the file, still opens: that plugin's data is kept unchanged and read-only, and the user is told which plugin is needed.

### Plugin-defined entities

- A plugin's manifest names its schema directory; its namespace is `plugin:<plugin-id>`, and every name it defines is qualified by it (`plugin:acme-timber/Truss`), so plugins never collide with the core or with each other.
- Plugins can add entities, property sets and relationships, attach their property sets to core entities, and use core entities as relationship targets. They cannot change core attributes, IFC mappings or migrations.
- Plugin entities map to IFC like core ones (CORE-10); when no specific IFC entity fits, the mapping uses `IfcBuildingElementProxy` or `IfcProxy` with a predefined type, and the round-trip test is the plugin author's.
- Plugin definitions are untrusted input (AGENTS.md §8, PY-08): the loader caps file size, number of entities, attributes and steps, name length and nesting depth, rejects unknown keys, and reports every error with the file and line.

## Consequences

- ➕ One definition serves the core, plugins, the API, IFC export and the documentation; nothing is declared twice.
- ➕ Schema changes are short, reviewable diffs; versions and migrations sit next to the definitions they migrate, and CI can check that none is missing.
- ➕ Contributors and plugin authors reuse tools they know: TOML, `taplo` for formatting, and a published JSON Schema of the definition format for editor completion and validation.
- ➖ Mistakes in definitions are reported by our validator, not by the Rust compiler; good error messages with file and line are a requirement of the parser, not a nicety.
- ➖ TOML's arrays of tables make long definitions verbose; one file per entity keeps each file short.
- ➖ The schema parser, validator and canonical model are runtime code (plugin loading), so they fall under the wasm32 rule and the 98 % coverage of critical areas, and need fuzz targets like other parsers (ENG-02).
- ➖ Representations and custom migrations name functions by string; the generator must check that each name resolves, or the build fails.
- New dependencies — `toml` and `serde` with derive — are justified in the pull request that adds them ([#16](https://github.com/GuillaumeOj/BIMCraft/issues/16)).

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
| YAML | Compact nesting; widely known | Implicit typing pitfalls (`NO` becomes `false`, `1.10` becomes `1.1`); indentation errors; the main Rust implementation, `serde_yaml`, is unmaintained; anchors and aliases make hostile input costly (“billion laughs”) |
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

- [#16](https://github.com/GuillaumeOj/BIMCraft/issues/16) Generate Rust entity types: parser, validator, canonical model and the Rust emitter.
- [#17](https://github.com/GuillaumeOj/BIMCraft/issues/17) Generate the API JSON Schema; also publish the JSON Schema of the definition format itself for `taplo` and editors.
- [#18](https://github.com/GuillaumeOj/BIMCraft/issues/18) Generate the IFC mapping table from the `[ifc]` sections.
- [#19](https://github.com/GuillaumeOj/BIMCraft/issues/19) Generate the reference documentation from `doc` fields.
- [#36](https://github.com/GuillaumeOj/BIMCraft/issues/36) Migration framework and the CI snapshot check.
- [#142](https://github.com/GuillaumeOj/BIMCraft/issues/142) Generate the Python client and stubs.
- Specification: §4 lists the `schema` crate as *build-time only*, but loading plugin definitions needs the parser and validator at runtime. A `spec` issue should split the row into a wasm32-compatible schema model (parser, validator, registry) and build-time generators, and state in §5.1 that plugin entities are loaded at runtime rather than compiled.
