# 0007. Attribute tables

- Status: Accepted
- Requirements: CORE-05 (also CORE-01, CORE-02, CORE-09, NFR-08, ARCH-01, SEC-03)
- Issues: [#23](https://github.com/GuillaumeOj/BIMCraft/issues/23) (epic [#20](https://github.com/GuillaumeOj/BIMCraft/issues/20))

## Context

[ADR 0002](0002-schema-definition-format.md) decided that every entity, core or plugin, lives in registry-driven attribute tables, and that the Rust types generated for core entities become typed views over them. CORE-05 asks for typed attribute tables for memory locality on large models: large models hold objects by the hundred thousand, and the browser build has at most 4 GB of memory (spec §16, risks). Analyses, views and exports mostly read one or two attributes of many objects, which a column layout serves best.

Later work builds on this layout: relationships (#24), types and overrides (#26), transactions and undo (#30), change events (#31) and project files (#35).

## Decision

`core_model::Store` holds every object of one schema namespace.

- **One table per entity, one column per attribute.** A table has an id list and, per attribute, a column of the attribute's Rust type: `bool`, `i64`, `f64` for reals and every quantity (SI, CORE-09), `Box<str>` for text and enumerations, `EntityId` for references, `Box<[AttributeValue]>` for lists. Each column is a dense vector plus a presence bitmap, not a vector of `Option`s: an optional length then takes 8 bytes and 1 bit instead of 16 bytes, an optional reference 16 bytes and 1 bit instead of 32.
- **Addressed by GUID.** A hash map from `EntityId` to (table, row) gives constant-time access. Deleting an object moves the last row of its table into the gap, so tables stay dense with no tombstones. The map is never iterated, so its random hash order never reaches results.
- **Instance attributes only.** Tables hold `instance` and `type_overridable` attributes (CORE-02). `required` and defaults apply to `instance` attributes; a `type_overridable` value on an instance is an optional override, whose default and value come from the type. `type` attributes are stored on type objects, which #26 adds.
- **Checked writes.** Every value is checked against the attribute's value type, lists item by item, and reals must be finite. A failed operation changes nothing. Error messages never repeat values or ids (SEC-03).
- **Undo-friendly results.** `set` returns the previous value and `remove` returns the removed object, which `insert` puts back unchanged; transactions (#30) record these.
- **Memory is measured.** `Store::heap_bytes` estimates the heap the store holds; a unit test keeps a 100,000-object model under a per-object budget, and [the baseline](../performance-baselines.md) records the measurement for NFR-08.

## Consequences

- Reading one attribute of every object touches only that column.
- References are not checked for existence: import order and relationships make dangling references legitimate for a while. Relationships (#24) and transactions (#30) own referential integrity.
- One namespace per store. Plugin namespaces (PY-04) will need tables keyed by namespace and entity; the change stays inside `Store`.
- Moving rows on delete means a row number is not stable; only `EntityId` is. Anything that keeps a row number must not outlive a deletion.
- `schema-gen` will emit typed views over these tables in place of plain structs (ADR 0002).

## Alternatives considered

- **One struct or map per object** (`HashMap<EntityId, BTreeMap<String, Value>>`): simple, but every object costs several allocations and a scan of one attribute walks the whole model.
- **Columns of `Option<T>`**: simpler code, twice the memory for optional reals and references.
- **A `BTreeMap` index**: deterministic order and no hash-flooding concern, but logarithmic lookups with poorer locality on the main access path. The hash map uses the standard library's hasher, keyed at random where the platform provides randomness, so ids crafted in a hostile file cannot force collisions there.
- **Tombstones on delete**: stable row numbers, but tables grow with deletions and every scan must skip holes.

## References

- [Specification §5](../../specifications.md#5-core-model-kernel), NFR-08 in §14.
- [ADR 0002](0002-schema-definition-format.md), "One store for every namespace".
