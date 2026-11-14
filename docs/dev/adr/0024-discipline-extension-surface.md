# 0024. Discipline extension surface

- Status: Accepted
- Requirements: ARCH-03 (also ARCH-01, ARCH-02, ARCH-04)
- Issues: [#6](https://github.com/GuillaumeOj/BIMCraft/issues/6)

## Context

ARCH-03 asks that discipline crates depend only on the public extension interfaces of the core, the same ones offered to plugins; spec §8 adds that each discipline registers its entities, tools and rules through the same interfaces as third-party plugins. If a discipline can reach further than a plugin, the plugin interface stays untested by our own code and drifts away from what disciplines need.

The layering check (ARCH-04) lets a `disc-*` crate depend on any crate of a lower layer: `schema`, `core-model`, `core-txn`, `geom`, `persist`, `material`, `georef`, `analysis`, `docgen` and the `interop-*` crates. That is far wider than what a plugin can reach. A discipline could use `core_model::Store`, write project files through `persist`, call the IFC writer or drive the undo history directly. Nothing named the extension surface.

No `disc-*` crate exists yet, so the rule can be put in place before the first one lands, at no migration cost.

## Decision

**One façade crate, `core-ext`, checked at manifest level.**

`crates/core-ext` is the public extension interface of the core. It holds no logic: it re-exports, by name, the items a discipline or plugin may use, grouped in documented topic modules. Its rustdoc is the identified surface. It sits alone in a layer between the `georef material …` layer and the discipline layer (`scripts/crate-layers.txt`), is a core crate (ARCH-01, ARCH-02) and a critical crate.

`scripts/extension-surface.txt` lists the crates of the workspace a `disc-*` crate may depend on: `core-ext` and `platform` (the traits hosts implement, already an interfaces-only crate). `scripts/check-architecture.ts`, run by `scripts/check-all.sh repo` and so by CI, fails when:

- a crate whose package name starts with `disc-` has a path dependency, in any section (normal, dev, build, target-specific) and under any rename, that is not in the list;
- a `disc-*` crate depends on another `disc-*` crate, even a listed one: a plugin cannot link a discipline either;
- a listed crate does not exist in `crates/`;
- any source file of `core-ext` (`src/**/*.rs`, so an out-of-line module counts), comments stripped as Rust does, re-exports a glob (`pub use x::*`) or a whole crate (`pub use x;`, `pub use x as y;`, `pub use {x, y::Z};`, `pub use x::{self, …}`, `pub extern crate x;`), which would leak everything public in it. A braced group of named items (`pub use {x::A, y::B};`) is accepted.

Rust only lets a crate name what its manifest declares, so the manifest check covers every import without parsing `use` paths. Registry dependencies (serde, thiserror…) are out of scope: ARCH-03 is about the core, and `cargo deny` covers the rest.

`crates/core-ext/tests/surface.rs` names every re-exported item, so removing one breaks that test target: shrinking the surface is always deliberate. Growing it is a reviewed edit of `src/lib.rs` and the test.

### The surface

Selection principle: what a third-party plugin could legitimately touch. An extension declares entities, reads the model, changes it only through commands that run in a transaction, and computes with geometry, materials, units and the georeference. It never owns storage, history, files or other hosts' concerns.

| Module | Re-exports | Reasoning |
| --- | --- | --- |
| `schema` | The definition model (`Namespace`, `Entity`, `Attribute`, `Cardinality`, `EntityKind`, `Enumeration`, `EnumItem`, `IfcMapping`, `Placement`, `Presence`, `PropertySet`, `Representation`, `Role`, `RoleTarget`, `TypeDef`, `Value`), value types (`ValueType`, `QuantityKind`, `ValueTypeError`), validation (`validate`, `ValidationError`, `ValidationErrorKind`, `Item`, `Location`, `Owner`) with its size limits, and migration declarations (`Migration`, `Step`, `DeclaredStep`, `StoredAttribute`, `MigrationError`, `MAX_STEPS`) | An extension declares and versions its own entities. Left out: `stored_tables`, `StoredTable`, `Side` (the table layout of project files) and `resolve`, `converts` (how the persistence layer runs migrations). |
| `model` | `Model`, `ModelError`, `EntityId`, `IdSource`, `GlobalIdError`, `AttributeValue`, `Component`, `LibraryRef`, `ObjectState`, `Participation`, `Removal`, `Source`, `SourceError`, `SourcedValue`, `MAX_SOURCE_LEN` | What reading the model and writing commands take and return. `AttributeValue` is a plain value carried by commands. Left out: `Store`, `StoreError`, `RemovedEntity` (the attribute tables of ADR 0007) and the test-only `SeededIdSource`. |
| `units` | Every item of `core_model::units`: `Quantity`, `Unit`, `Dimension`, the unit enums and quantity aliases (`Length`, `Area`, …), `ProjectUnits`, the display and parsing types | Every engineering value an extension handles is a quantity. |
| `derived` | `DependencyGraph`, `DerivedError` | Disciplines compute derived data (geometry, analytical models) from the model (CORE-08). |
| `command` | `Command`, `ModelCommand`, `CommandError`, `Transaction`, `Origin`, `ChangeEvent` | An extension changes the model only through commands (CORE-11), which run in a transaction, and reacts to change events. Left out: `Document`, `HistoryError`, `HistoryEntry`, `replay`, `ReplayError`: the host owns the history and replays it, not the extension. |
| `geometry` | Every item `geom` exports at its root: points and vectors, profiles, paths, solids and their errors, location lines and points, tolerances and exact predicates | `geom` is pure computation on values, with no model state; disciplines build their representations with it. |
| `material` | Every item `material` exports at its root except `Source`, `SourceError` (already in `model`): materials, laws, properties, the grade library, layered build-ups, and the functions that read materials and build the commands that edit them | Materials and build-ups are shared by every discipline (MAT-02 asks that plugins extend the library), and the editing functions only read the model and return `ModelCommand`s. |
| `georef` | The georeference and map conversion values, the CRS registry, ellipsoids, projections and datum transformations, and `site::{read, find_site, SiteError}` | Read-only: an extension may need true north or projected coordinates. Left out: `site::write`, `site::clear` and the `Site` attribute names; the project's georeference is set through its own commands. |

Not re-exported at all: `persist`, the `interop-*` crates, `analysis`, `docgen`, `api`, `render` and the hosts. `platform` is depended on directly rather than re-exported, as it is already an interfaces-only crate.

The check sees crate re-exports, not module ones: `pub use core_model::units;` would pass it and leak a module. The surface test and review cover that case, and the façade names `units` items one by one.

A façade re-exports types, not a subset of their methods. `Model::store` returns the `&Store` that project files save (ADR 0016), so an extension holding a `Model` can still read the attribute tables, and clone them, without naming `Store`. It cannot write them back into the document, which only runs commands. Hiding `store` from extensions means changing `core-model`'s API, which `persist` relies on; that is left to a follow-up issue rather than this change.

How a discipline registers commands with `api::Registry`, a layer above, is settled when the first discipline lands; `core-ext` grows then.

### Coverage

`core-ext` compiles to no code, so `cargo llvm-cov` finds no file to measure and its 0/0 line ratio fails any `--fail-under-lines`. `scripts/check-all.sh coverage` treats `core-ext`, by name, the other way round: it runs its tests and then requires the report to hold no file at all, so the façade fails the gate the day it gains code that could go untested. Every other critical crate keeps the plain 98 % gate, so a crate whose coverage data went missing still fails. The test that guards the façade is `tests/surface.rs`, which fails to build when an item disappears.

## Consequences

- The extension surface is one file, `crates/core-ext/src/lib.rs`, and its rustdoc. A plugin host will bind to the same crate.
- A discipline that needs something the surface lacks has to grow `core-ext` in a reviewed change, which also grows what plugins get. That is the point: the two never drift apart.
- Disciplines cannot share code with each other; shared code moves down into a core crate and, if extensions need it, into `core-ext`.
- Two crates export different `Layer` and `LayerSet` types (geometric layers in `geom`, material layers in `material`); topic modules keep both reachable without clashes.

## Alternatives considered

- **An allowlist of crates and of modules or items per crate, with source scanning.** No new crate, but the check needs a Rust path parser in TypeScript (braced groups, `as` aliases, `self` and `super`, macros, re-exports such as `material` re-exporting `core_model::Source`) that is easy to fool, and the surface would not be something a plugin host can bind to.
- **A crate-only allowlist** (`schema`, `core-model`, `core-txn`, `geom`, `material`, `platform`). Trivial to check, but it exposes `Store` and everything else public in those crates, which does not meet "the same ones offered to plugins".

## References

- Specification: ARCH-03 (§4), the extension model (§8).
- [Issue #6](https://github.com/GuillaumeOj/BIMCraft/issues/6).
- ADR [0007](0007-attribute-tables.md) (attribute tables), [0015](0015-commands-and-transactions.md) (commands and transactions).
