# Architecture decision records

An architecture decision record (ADR) captures one significant technical decision: the problem, the choice, its consequences and the options rejected.

## Rules

- One decision per file, named `NNNN-kebab-case-title.md` with the next free four-digit number. Numbers are never reused.
- Status is one of **Proposed**, **Accepted** or **Superseded by NNNN** (linking the newer record).
- A record is written as Accepted in the pull request that introduces it: merging that pull request is the acceptance. Use Proposed only for a record merged before its decision is final.
- Accepted records are not rewritten. To change a decision, write a new record that supersedes the old one and update the old one's status line only.
- Name the requirement IDs from the [specification](../../specifications.md) and link the issues the decision affects.
- Add the record to the index below in the same pull request.

## Template

Copy [`_template.md`](_template.md) and follow its comments.

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-geometry-tolerance-policy.md) | Geometry tolerance policy | Accepted |
| [0002](0002-schema-definition-format.md) | Schema definition format | Accepted |
| [0003](0003-deterministic-math-library.md) | Deterministic math library | Accepted |
| [0004](0004-units-and-quantities.md) | Units and quantities | Accepted |
| [0005](0005-profile-validation-with-arcs.md) | Profile validation with arcs | Accepted |
| [0006](0006-sqlite-binding.md) | SQLite binding | Accepted |
| [0007](0007-attribute-tables.md) | Attribute tables | Accepted |
| [0008](0008-arc-faceting.md) | Arc faceting for planar solids | Accepted |
| [0009](0009-ifc-entity-classes-from-express.md) | IFC entity classes from the EXPRESS schema | Accepted |
| [0010](0010-location-lines.md) | Location lines and location points | Accepted |
| [0011](0011-shear-centre.md) | Shear centre of arbitrary profiles | Accepted |

One decision predates this folder and stays in the specification: the web UI over an all-Rust UI ([specification §13](../../specifications.md#13-ui-and-rendering), "Decision record").
