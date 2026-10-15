# 0016. Project file layout

- Status: Accepted
- Requirements: PERS-01, PERS-02 (also CORE-05, CORE-12, ARCH-01, ARCH-02, SEC-03)
- Issues: [#35](https://github.com/GuillaumeOj/BIMCraft/issues/35), [#36](https://github.com/GuillaumeOj/BIMCraft/issues/36) (epic: SQLite project files)

## Context

[ADR 0006](0006-sqlite-binding.md) made the project file a SQLite database opened by the host and handed to the `persist` crate, which owns what is inside. [ADR 0007](0007-attribute-tables.md) keeps the model in memory as one table per entity and one typed column per attribute. The file must now hold that model, record which schema version wrote it (PERS-01), survive an interrupted save (PERS-02), and later allow partial loading (CORE-12). Every file is untrusted input (AGENTS.md §8).

## Decision

### Two kinds of version

- **File format version**, in `PRAGMA user_version`: the layout of the tables the file keeps about itself (`project_property`, `schema_namespace`). `open_project` refuses a newer one; an older one is brought up to date by a chain of format migrations.
- **Schema version of each namespace**, one row per namespace in `schema_namespace (name, version)`, as [ADR 0002](0002-schema-definition-format.md) requires. It governs the entity tables of that namespace. `open_model` and `save_model` refuse a newer one.

`PRAGMA application_id` marks the database as a BIMCraft project, as before.

### One table per entity and per type

Each entity of a namespace has a `STRICT` table named `<namespace>/<Entity>` (for example `core/Wall`), and each entity with a type one named after the type (`core/WallType`), mirroring the attribute tables in memory:

- `id BLOB PRIMARY KEY`: the 16-byte GUID, big-endian.
- One nullable column per attribute the table's objects store, in declaration order: `instance` and `type_overridable` attributes for instances, `type` and `type_overridable` ones for type objects (CORE-02, [ADR 0014](0014-components-types-instances.md)). `NULL` is an absent value.

| Value type | Column type | Stored as |
| --- | --- | --- |
| `bool` | `INTEGER` | 0 or 1 |
| `integer` | `INTEGER` | the integer |
| `real`, `quantity:<kind>` | `REAL` | the SI value (CORE-09) |
| `text`, `enum:<Name>` | `TEXT` | UTF-8 |
| `ref:<Entity>` | `BLOB` | 16-byte GUID, big-endian |
| `list:<type>` | `BLOB` | the encoding below |

Lists use a compact encoding without type tags, because the attribute's value type says what each item is: a `u32` item count, then each item — `bool` as one byte, `integer` and `real` as 8 little-endian bytes, `text` as a `u32` byte length and UTF-8, `ref` as 16 bytes, a nested list the same way recursively. All lengths are little-endian.

Identifiers are always quoted in SQL, although schema names are already validated ASCII.

### Saving and opening

- `save_model` writes the whole model in **one transaction**: it migrates a file from an earlier version, creates the namespace's tables the first time, empties them, inserts every object and records the namespace version. SQLite's rollback journal makes the transaction atomic, so an interrupted save leaves the previous content (PERS-02).
- `open_model` reads every table back through `Store::insert`, which checks every value against the schema. It runs in a transaction that is never committed, so opening never changes the file, even when it migrates it or the file did not hold the namespace yet.
- Before reading or writing, the columns of each table (`pragma_table_info`: name, declared type, primary key) must match the namespace exactly. Statements name their columns, so the order of columns in a table does not matter: migrations append the columns they add.

### Migrations

Both versions migrate inside the transaction that reads or writes the file, so a file is rewritten in the newer version only when it is saved ([ADR 0002](0002-schema-definition-format.md#versioning-and-migrations)).

- **File format**: `FORMAT_MIGRATIONS[i]` turns format `i + 1` into `i + 2`.
- **Schema**: the host passes a `Schema`: the current namespace, the resolved migration to each later version (`schema_gen::History`), and the functions its `custom` steps name. A resolved migration lists every change explicitly, so no older schema is needed at run time. Each step becomes table changes: `ALTER TABLE … RENAME` for renames, `CREATE TABLE` or `ADD COLUMN` (filled with the default) for additions, `DROP TABLE` or `DROP COLUMN` for removals, a rewritten column for value changes, `UPDATE … WHERE … IS NULL` for `set_value`, and a call for `custom`.
- A step that does not fit the file (a missing table or column) means the file does not hold the version it claims: `Corrupt`. A missing migration or custom function is reported as such.
- Committed fixtures of every file format version and of every version of a sample namespace are opened by the tests; `docs/dev/schema-versions.md` explains how to add a version.

### Hostile files

Nothing read from a file is trusted. `SQLITE_NOTADB` is `NotAProject`; `SQLITE_CORRUPT`, a table whose columns differ from the namespace, a value of the wrong SQL type, a boolean other than 0 or 1, an id or reference that is not 16 bytes, invalid UTF-8, a malformed list blob or trailing bytes are `Corrupt`. Values the schema refuses (a missing required value, a non-finite real, a duplicate id) are `Model`. List decoding is bounded by the validated value type (`MAX_LIST_DEPTH`) and by the blob length, and never allocates more than the blob could hold. Errors never carry values, ids or paths (SEC-03). The `project_values` fuzz target feeds arbitrary bytes to every blob-encoded column.

## Consequences

- The file mirrors the in-memory layout: reading one attribute of many objects is one column scan, and loading part of a model (CORE-12) can select tables and rows.
- A schema change changes the tables, so migrations alter tables (`RENAME`, `ADD COLUMN`, `DROP COLUMN`) as well as rows.
- Saving rewrites every row. Incremental saves from transaction change events (CORE-07) can replace this later without changing the layout.
- SQLite stores a `REAL` with no fractional part as an integer on disk, so `-0.0` in a real column reads back as `0.0`. The two compare equal and no building quantity depends on the sign of zero. Values inside list blobs keep their exact bits.
- Plugin namespaces get tables of their own, prefixed by their namespace, without touching the core's.

## Alternatives considered

- **Generic rows**: one `object (id, entity)` table and one `attribute_value (object, attribute, value)` table. The layout never changes with the schema, so migrations only touch rows, but every attribute read becomes a lookup in one large table, the file no longer mirrors the attribute tables, and partial loading needs extra indexes. Rejected for locality and partial loading.
- **One serialized blob per object** (JSON or a binary format): simple to write, but SQLite can no longer query attributes, partial loading and migrations must decode every object, and the format needs its own versioning.
- **JSON for lists**: readable with SQLite's JSON functions, but a JSON round trip of reals is not exact without extra care, and it is larger.

## References

- Specification: §5.3, PERS-01, PERS-02, CORE-05, CORE-12, SEC-03.
- [ADR 0002](0002-schema-definition-format.md), [ADR 0006](0006-sqlite-binding.md), [ADR 0007](0007-attribute-tables.md).
- SQLite documentation: "STRICT Tables", "Datatypes In SQLite" (REAL storage), "Atomic Commit In SQLite".
