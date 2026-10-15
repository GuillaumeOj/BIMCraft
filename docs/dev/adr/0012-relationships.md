# 0012. Relationships

- Status: Accepted
- Requirements: CORE-03 (also CORE-01, CORE-05, CORE-06, SEC-03)
- Issues: [#24](https://github.com/GuillaumeOj/BIMCraft/issues/24) (epic [#20](https://github.com/GuillaumeOj/BIMCraft/issues/20))

## Context

CORE-03 asks for relationships as first-class objects (aggregation, containment, connection, voiding, assignment) that mirror the IFC `IfcRel*` entities. [ADR 0002](0002-schema-definition-format.md) already declares them in the schema: a relationship entity has attributes like any entity and a list of roles, each with allowed targets, a cardinality (`one` or `many`) and an `exclusive` flag. [ADR 0007](0007-attribute-tables.md) stores objects in attribute tables and leaves referential integrity to relationships and transactions.

Later work needs three things from relationships: queries in both directions (what does this storey contain, where is this wall contained), a deletion rule that never leaves a relationship with a missing end, and undo (#30).

## Decision

`core_model::Model` owns the `Store` and the relationships, so every rule that spans objects lives in one type.

- **Relationships are objects.** A relationship object has its own `EntityId` (a GUID, CORE-01) and its attributes in its entity's table. Its participants are kept in an index next to the tables, one list per role in schema order, with a reverse index from each participant to the roles it plays. `Model::insert` refuses relationship entities; `Model::relate` creates them with their roles.
- **Checked against the schema.** Every role is given once; a `one` role has exactly one participant and a `many` role at least one; participants exist, are allowed targets (by entity name or `kind:`), and appear once per relationship; an object plays an `exclusive` role in at most one relationship of that entity (a wall is contained in one storey, but can be assigned to many groups). Participants are added to and removed from `many` roles; a role is never left empty.
- **Queries both ways.** `Model::participants(relationship, role)` lists a role in insertion order; `Model::relationships_of(object)` lists the roles an object plays, sorted by relationship id then role, so results never depend on hash order.
- **Deletion rule.** Deleting an object removes it from every role it plays. A relationship left with an empty role is deleted too, and so on for relationships that relate it. No other object is deleted: deleting a storey deletes its containment but not the walls, and deleting a relationship deletes only the relationship.
- **Undo.** `Model::remove` returns a `Removal` listing every deleted object with its values, its roles and the position it held in each role it left; `Model::restore` puts it all back exactly, after checking that the model still allows it.
- Errors are typed (`ModelError`) and, like store errors, name entities and roles but never values or ids (SEC-03).

## Consequences

- The index is never iterated in hash order, so results stay deterministic.
- Deleting dependent objects, such as an opening with its host wall, is not a model rule: commands decide it (#30), and the model keeps the opening.
- Persistence (#35) stores roles separately from attribute values.
- Plugin relationship entities (PY-04) get the same checks with no extra code, as every rule is read from the schema.

## Alternatives considered

- **Roles as reference columns** in the relationship's table: persistence and undo for free, but writing a role through `Store::set` would bypass the reverse index and every check.
- **Relationships as attributes of participants** (a `contained_in` reference on each element): simple, but not first-class objects, so they carry no GUID or attributes and cannot be exported as `IfcRel*` entities.
- **Cascading deletion to participants** (delete the openings of a deleted wall): convenient for some cases, wrong for most (deleting a storey must not delete its walls); it belongs to commands, which know the user's intent.
- **Refusing to delete objects that take part in relationships**: safe, but every deletion would need the caller to dismantle relationships first.

## References

- [Specification §5](../../specifications.md#5-core-model-kernel), CORE-03.
- [ADR 0002](0002-schema-definition-format.md), relationship entities and roles; [ADR 0007](0007-attribute-tables.md), attribute tables.
