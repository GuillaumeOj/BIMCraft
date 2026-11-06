# 0018. Sourced values

- Status: Accepted
- Requirements: REP-02, REP-05, MAT-01, MAT-04 (also CORE-04, CORE-09, CORE-11, SEC-03)
- Issues: [#85](https://github.com/GuillaumeOj/BIMCraft/issues/85), [#244](https://github.com/GuillaumeOj/BIMCraft/issues/244)

## Context

REP-05 asks that national annex and design-code values be entered as data with their sources, REP-02 that every reported value trace back to its code clause, and MAT-04 that environmental product data keep their sources. #244 met this for materials with a convention: every sourced property `X` has a companion text property `XSource` holding `"<document>, <clause>"`, and the `material` crate pairs them up when it reads a material back.

The convention has costs that grow with every sourced value:

- Each sourced value is defined twice in the schema, and the copies drift (their docs were copy-pasted).
- The rule "a value has a source" is only checked when the `material` crate reads, so the API, plugins and IFC import can write a value without its source, or a source without its value.
- The source format is invisible to the schema, the JSON Schema and the reference pages.
- IFC property sets cannot hold the companion properties, so material values live in `BIMCraft_*` sets instead of the IFC sets they belong to.

National annex parameters, partial factors and EPD data will all need the same thing.

## Decision

**A `sourced` key on attributes and properties.** `sourced = true` on an `[[attribute]]` or `[[property]]` says every value it holds comes with its source.

- Allowed on `bool`, `integer`, `real`, `quantity:<kind>`, `text` and `enum:<Name>`; not on `ref:` and `list:` values (a reference is not data from a document; a list would need one source per item).
- A sourced attribute has no `default`, since a default has no source; it may be `required` or optional.
- The canonical model gets `Attribute::sourced: bool`. Validation rejects it on the value types and with the presence above.

**One source type.** `core_model::Source` is a document and a clause, for example `EN 1992-1-1:2004` and `Table 3.1`. Both parts are trimmed, non-empty, at most 200 characters and free of control characters, so a source never carries standards text by accident (REP-05). The `material` crate's own `Source` is replaced by it.

**Sourced values are attribute values.** `AttributeValue` gains `Sourced(Box<SourcedValue>)`, where `SourcedValue` holds the plain value and its `Source`; `AttributeValue::sourced` builds one and `AttributeValue::plain` reads the value without its source.

- A sourced attribute or property accepts only `Sourced` values whose inner value has the declared type; any other attribute rejects them. A value and its source are therefore written, read, cleared, undone and serialised together, through the paths that already exist: `insert`, `set`, `attach_property_set`, `set_property`, every `ModelCommand` and the history journal (CORE-11).
- New typed errors: `StoreError::MissingSource` (a plain value written to a sourced attribute) and `StoreError::UnexpectedSource` (a sourced value written elsewhere). Messages name the attribute, never the value or the source (SEC-03).
- Attribute tables keep one typed column per attribute (ADR 0007); a sourced attribute's column also keeps a source per row, so scans of the values stay as fast as before.

**Generated code and documents.**

- JSON Schema (API-02): a sourced field is an object `{ "value": …, "source": { "document": …, "clause": … } }`, described once as `$defs/Source`.
- Rust: a sourced field has type `Sourced<T>`; the emitter writes `Sourced` and `Source` once when the namespace uses them.
- Reference pages: a "Sourced" mark in the attribute and property tables.
- Snapshots: `sourced` is written only when true, so earlier snapshots read unchanged.

**Project files.** A sourced entity attribute is stored as its value column plus `<name>@document` and `<name>@clause` `TEXT` columns (ADR 0016). Property-set values are not saved yet (#35); they follow the same layout when they are.

**Migrations.** Turning `sourced` on or off changes what stored values mean, so the schema history reports it (`MigrationError::SourcedChanged`) unless a custom step takes over the entity. Renaming or removing a sourced attribute renames or drops its source columns with it.

**IFC.** Export will write a sourced property's value to its IFC property and the source to that property's `Specification`, so sourced values can live in IFC-named property sets. Import reads it back. This lands with the IFC mapper, not here.

**Materials.** The `<Name>Source` companion properties, `Property::source_name` and the read errors that pair them (`MissingSource`, `SourceWithoutValue`, `InvalidSource`) are removed. The material property sets declare their properties `sourced = true`. Core schema version 2 is not released yet, so no migration is needed.

## Consequences

- One definition per sourced value; the model guarantees value and source together for every writer, including plugins and IFC import.
- `AttributeValue` grows one variant, so every exhaustive `match` on it (store columns, project-file codec, commands) handles it. Values are slightly larger in memory only for sourced attributes.
- Readers that only want the number unwrap the `Sourced` value; `Model::quantity_property` and similar helpers do so.
- Renaming material property sets to IFC names becomes possible; it is left to the IFC mapper's work.

## Alternatives considered

- **Keep the companion `<Name>Source` properties** (the #244 convention): twice the definitions, the pairing is checked only by one crate, and IFC sets cannot hold them.
- **A separate source map keyed by object and attribute**, outside the attribute tables: a second storage path for persistence, undo and the API to keep in step, against ADR 0002's "one store".
- **A `source` text value type** written next to the value by convention: still two attributes per value, and nothing ties them together.
- **Sources as relationship objects** (a `Source` entity referenced by values): heavier for a document and a clause, and a reference per value would cost far more memory than two short texts.

## References

- [Specification](../../specifications.md): REP-02, REP-05, MAT-01, MAT-04.
- [ADR 0002](0002-schema-definition-format.md), [ADR 0007](0007-attribute-tables.md), [ADR 0013](0013-property-sets.md), [ADR 0015](0015-commands-and-transactions.md), [ADR 0016](0016-project-file-layout.md).
