# 0014. Components, types and instances

- Status: Accepted
- Requirements: CORE-02 (also CORE-01, CORE-04, CORE-05, CORE-08, INT-07, INT-08, SEC-03)
- Issues: [#26](https://github.com/GuillaumeOj/BIMCraft/issues/26) (epic [#20](https://github.com/GuillaumeOj/BIMCraft/issues/20))

## Context

CORE-02 separates three things (spec §16.3): a **component**, a reusable parametric definition of an element such as a hinged door; a **type**, a named set of parameter values made from a component, such as a 90 × 215 cm door; and an **instance**, a placement of a type in the project. An instance inherits from its type and may override only the values the schema allows. [ADR 0002](0002-schema-definition-format.md) declares this per attribute and property with `on = "instance"`, `"type"` or `"type_overridable"`; [ADR 0007](0007-attribute-tables.md) stores only the instance side so far. IFC export (INT-07) writes types with their component's library reference, and import (INT-08) may meet types with no component.

Authoring parametric components (constraints, geometry, GEO-12) comes later; this decision covers the data model they plug into.

## Decision

- **Components** are kept by the `Model`, outside the attribute tables: the entity they build (which must have a `[type]`), a library reference (id and version, for INT-07 and INT-08) and default values for type attributes, checked against the type's columns. They share the id space of stored objects, so no object and component have the same GUID. A component is removed only once no type is made from it.
- **Types are objects.** `Store` has one table per entity type, named after it (`WallType`), holding `type` and `type_overridable` attributes with their required and default rules. `Model::create_type` makes a type from a component: values not given come from the component's defaults, then the schema's. The defaults are copied, so a later component version does not silently change existing types; rebuilding types from a new version is a command (INT-08). A type with no component, such as a static type from IFC, is stored with `Model::insert` under the type's name.
- **Instances** are objects of the entity, linked to at most one type of that entity with `Model::assign_type`; untyped instances are allowed. The model indexes the links both ways.
- **Resolution on read.** `Model::get` and `Model::property` return an instance's own value, else its type's, else the default: `instance` values come from the instance, `type` values from the type, and `type_overridable` values from the instance's override when it has one. Since nothing is copied to instances, a change on the type reaches every instance that does not override it; `Model::inheriting_instances` lists them, so derived data that reads the value can be invalidated (CORE-08).
- **Overrides are checked.** Writing a `type` attribute or property on an instance fails with `ModelError::NotOverridable`; reading or writing an `instance` one on a type fails with `ModelError::NotOnType`. One resolver serves attributes and properties alike. Clearing an override brings the type's value back.
- **Property sets** (ADR 0013) have the same two sides: a set applies to the types of the entities it applies to, and a type holds its `type` and `type_overridable` properties.
- **Deletion** extends the rule of ADR 0012: deleting a type leaves its instances untyped, and `Model::restore` relinks the type, its instances and its component.

## Consequences

- Reading an inherited value costs one extra lookup; storing it once per type keeps memory proportional to types, not instances.
- Commands (#30) decide what deleting a type means for the user (retype the instances, or delete them); the model never deletes instances on its own.
- Persistence (#35) stores components and the instance-to-type and type-to-component links next to the tables.
- IFC export (INT-07) finds each type's component and library reference with `Model::component_of`.

## Alternatives considered

- **Copying type values into instances** on creation and on every type change: faster reads, but every type change rewrites many rows, and telling an override from a copy needs extra state.
- **Types as a relationship** (`IfcRelDefinesByType`): mirrors IFC, but the schema does not declare one, and a type link is too central to resolution to go through generic roles. IFC export can still write the relationship.
- **Components as objects in the attribute tables**: no table fits their open set of parameter defaults, and their name would have to be invented beside the schema's entities and types.
- **Requiring a component for every type**: would make static IFC types impossible (INT-08).

## References

- [Specification §5](../../specifications.md#5-core-model-kernel) (CORE-02), §16.3 (glossary), INT-07 and INT-08.
- [ADR 0002](0002-schema-definition-format.md), placement; [ADR 0007](0007-attribute-tables.md), attribute tables; [ADR 0012](0012-relationships.md), deletion; [ADR 0013](0013-property-sets.md), property sets.
