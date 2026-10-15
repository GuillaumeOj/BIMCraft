# Command API

Every client, the UI, plugins, AI sessions, reaches the core through the `api` crate: every user-visible operation is a named, typed, versioned command or query (API-01). Background: [ADR 0015](adr/0015-commands-and-transactions.md) (commands and history).

## Commands and queries

A `Descriptor` holds a stable dotted name (`model.set`; the `rpc.` area is reserved for JSON-RPC), a semantic version, a one-sentence summary, the JSON Schema of its parameters and result (a `Signature`), and its handler. `Session::registry()` builds the registry of the built-ins:

| Names | What they do |
| --- | --- |
| `model.<tag>` | One per `core_txn::ModelCommand` variant, named after its serde tag; the parameters are the variant's fields, and unknown fields are rejected, in batches too. |
| `history.undo`, `history.redo` | Undo and redo; they take an empty object. |
| `model.get_object`, `model.list_objects`, `model.property_sets`, `history.state` | Read-only queries. `model.list_objects` pages by id with an `after` cursor, `history.state` by position with `start`; both return `next`, take 100 items by default and `MAX_PAGE_SIZE` (1000) at most. History entries name the method that ran, such as `model.create`. |

A command runs with the caller's `Origin`, which the history records (API-05), and reports a `ChangeEvent`. A query only borrows the session, so it cannot change the model. A failed call leaves the model and its history unchanged.

## Add a command or query

1. Write the handler with typed input (`Deserialize`) and output (`Serialize`), and its `Signature` from the helpers in `api::schema` (`object`, `array_of`, `nullable`, `def("EntityId")`…).
2. Register it in `crates/api/src/commands.rs` or `queries.rs`. A breaking change to its input or output is a new major version registered beside the old one (API-06).
3. Run `UPDATE_SNAPSHOTS=1 cargo test -p api --test protocol_schema`, review the diff of `schema/api/protocol.schema.json` and commit it.

## The protocol schema

`schema/api/protocol.schema.json` (API-02, JSON Schema draft 2020-12, `$id` `urn:bimcraft:api`) is generated from the registry. `cargo test -p api` fails when it is stale.

| Path | Content |
| --- | --- |
| `$defs/commands/<name>`, `$defs/queries/<name>` | One entry per command or query: `properties.name` and `properties.version` (`const`), `properties.params` and `properties.result`. Older versions, once there are any, are listed as `<name>@<version>`. |
| `$defs/EntityId`, `AttributeValue`, `NameValue`, `Origin`, `ChangeEvent`, `ModelCommand` | The definitions the entries share. |
| `$defs/entities` | The entity schema, `schema/core/api.schema.json`, embedded unchanged. |

The schemas are written by hand next to each handler. Tests check every serialised command, result and change event against them, so they cannot drift from the serde forms.
