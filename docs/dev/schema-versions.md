# Schema and file format versions

Project files must open in every later version of BIMCraft (PERS-01). Two versions protect them, and CI fails when a change to either lands without a migration. Background: [ADR 0002](adr/0002-schema-definition-format.md#versioning-and-migrations) (schema history) and [ADR 0016](adr/0016-project-file-layout.md) (what a file holds).

| Version | What it covers | Where it lives | Checked by |
| --- | --- | --- | --- |
| Schema version of a namespace | Entities and attributes, stored as one table per entity | `schema/<namespace>/namespace.toml`, `snapshots/`, `migrations/` | `cargo test -p schema-gen` (`tests/history.rs`) |
| File format version | The tables a file keeps about itself (`project_property`, `schema_namespace`) | `FORMAT_VERSION` and `FORMAT_MIGRATIONS` in `crates/persist/src/lib.rs` | `cargo test -p persist` (`tests/migrations.rs`) |

## Change the schema

1. Edit the definitions under `schema/core/` as usual.
2. Raise `version` in `schema/core/namespace.toml` by one, to `N`.
3. If stored data changes in a way a difference cannot tell, write `schema/core/migrations/NNNN.toml`:

   ```toml
   format = 1

   [[step]]
   op = "rename_attribute"   # also rename_entity, change_value, set_value, custom
   entity = "Wall"
   old_name = "base_offset"
   new_name = "bottom_offset"
   ```

   | `op` | Keys | When |
   | --- | --- | --- |
   | `rename_entity` | `old_name`, `new_name` | An entity is renamed. References to it follow. |
   | `rename_attribute` | `entity`, `old_name`, `new_name` | An attribute is renamed. |
   | `change_value` | `entity`, `attribute`, `value`, `factor` (default 1) | An attribute changes value type. Numbers become reals or quantities (reals are multiplied by `factor`), text and enumerations convert into each other, references keep their id, lists convert item by item. |
   | `set_value` | `entity`, `attribute`, `value` | Objects without a value get this one, for an attribute that becomes required. |
   | `custom` | `entities`, `function`, `doc` | Anything else. The function runs on the file's tables; the host passes it to `persist` in `Schema::custom_steps`. |

   Added and removed entities and attributes need no step: they are inferred from the snapshots, an added attribute taking its default.
4. Run `UPDATE_SNAPSHOTS=1 cargo test -p schema-gen` to write `snapshots/NNNN.json`, then `cargo test -p schema-gen`. Existing snapshots are never rewritten. The same run regenerates the files derived from the definitions, `schema/core/api.schema.json` and the reference pages in `docs/reference/schema/`, which date each item from the snapshots; commit them with the change.

The check reports what is missing: definitions that differ from the last snapshot ("raise the version"), a value type change without `change_value`, a new required attribute without `set_value`, or a step naming something that does not exist.

Steps run in this order: declared renames, inferred additions, the other declared steps as written, inferred removals. Custom code can therefore still read what is about to be removed.

## Change the file format

1. Change `create_format` in `crates/persist/src/lib.rs`.
2. Raise `FORMAT_VERSION` and append a function to `FORMAT_MIGRATIONS` that turns the previous format into the new one.
3. Run `UPDATE_SNAPSHOTS=1 cargo test -p persist --test migrations` to write `format-NNNN.sql` and `format-NNNN.bim` in `crates/persist/tests/fixtures/`. The fixtures of earlier versions stay: the test opens each of them with the new code.

## Fixtures

`crates/persist/tests/fixtures/` holds one file per version, written once and never rewritten, because they stand for files users already have:

- `format-NNNN.bim`: a project of each file format version, and `format-NNNN.sql`, the tables of the current one.
- `sample-NNNN.bim`: a model of each version of a sample namespace defined in the test, which exercises every kind of migration step.

The core namespace gets its own fixtures once it stores entities; until then its history is checked by `schema-gen` alone.
