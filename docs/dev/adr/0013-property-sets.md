# 0013. Property sets

- Status: Accepted
- Requirements: CORE-04, CORE-09 (also CORE-02, CORE-05, PY-04, SEC-03)
- Issues: [#25](https://github.com/GuillaumeOj/BIMCraft/issues/25) (epic [#20](https://github.com/GuillaumeOj/BIMCraft/issues/20))

## Context

CORE-04 asks for properties grouped in property sets, with typed values and units, and for custom sets that users and plugins define. [ADR 0002](0002-schema-definition-format.md) defines built-in sets in the schema (`[property_set]` with `applies_to` and `[[property]]` entries that have the same keys as attributes) and says user-defined sets are model data with the same structure. Values must carry units (CORE-09), and invalid values must be rejected with typed errors.

## Decision

- **One mechanism.** A custom set is a `schema::PropertySet`, like a built-in one. `Model::define_property_set` adds it to the model's copy of the namespace and runs `schema::validate` on the result, so a custom set obeys every rule a built-in set does (names, limits, value types, entities in `applies_to`); a rejected set leaves nothing behind. Built-in and custom sets then share the same storage, API and checks.
- **Storage.** Each set has attribute tables of the same `Table` type entity tables use (ADR 0007), with one row per object that carries the set. The instance table holds `instance` and `type_overridable` properties, with the same required, default and optional rules as attributes; `type` properties belong to type objects, which have a table of their own ([ADR 0014](0014-components-types-instances.md)).
- **Carrying a set.** An object can carry a set when its entity is in `applies_to`. It starts carrying it with `attach_property_set` (several values at once, required ones included) or with its first `set_property`, and stops with `detach_property_set`. Reading a property never attaches: an object without a value reads the property's default.
- **Units.** Quantities are stored as SI reals, like quantity attributes. `quantity_property` and `set_quantity_property` read and write them as `Quantity<U>` values, after checking that the property's quantity kind matches the dimension of `U`, so a length is never written into an area.
- **Typed errors.** Unknown or non-applicable sets, unknown properties, type properties written on an instance, repeated or missing values, values of the wrong type and non-finite numbers each have a `ModelError` variant. Messages name sets and properties, never values or ids (SEC-03).
- **Deletion.** A deleted object takes its property values with it; `Model::restore` puts them back.

## Consequences

- Plugins (PY-04) and users add sets at run time with the same guarantees as the core schema. Removing or changing a custom set, and saving custom sets in the project file (#35), come later.
- Validating the whole namespace for each new set costs time proportional to the schema; sets are defined rarely, so simplicity wins.
- Rows are keyed by object id in a hash map that is never iterated, so results stay deterministic.

## Alternatives considered

- **A separate format or code path for custom sets**: duplicate rules that would drift apart, contradicting "one mechanism".
- **Properties as a map per object** (`HashMap<EntityId, BTreeMap<String, Value>>`): simple, but loses the typed columns and memory layout of ADR 0007.
- **Storing quantities with their unit**: every value would carry a unit tag, doubling its size; the schema already fixes the kind, and the kind fixes the SI unit.

## References

- [Specification §5](../../specifications.md#5-core-model-kernel), CORE-04, CORE-09.
- [ADR 0002](0002-schema-definition-format.md), property sets; [ADR 0004](0004-units-and-quantities.md), quantities; [ADR 0007](0007-attribute-tables.md), attribute tables.
