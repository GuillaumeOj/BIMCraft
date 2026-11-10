# Command API

Every client, the UI, plugins, AI sessions, reaches the core through the `api` crate: every user-visible operation is a named, typed, versioned command or query (API-01). Background: [ADR 0015](adr/0015-commands-and-transactions.md) (commands and history).

## Commands and queries

A `Descriptor` holds a stable dotted name (`model.set`; the `rpc.` area is reserved for JSON-RPC), a semantic version, a one-sentence summary, the JSON Schema of its parameters and result (a `Signature`), and its handler. `Session::registry()` builds the registry of the built-ins:

| Names | What they do |
| --- | --- |
| `model.<tag>` | One per `core_txn::ModelCommand` variant, named after its serde tag; the parameters are the variant's fields, and unknown fields are rejected, in batches too. |
| `history.undo`, `history.redo` | Undo and redo; they take an empty object. |
| `model.get_object`, `model.list_objects`, `model.property_sets`, `history.state` | Read-only queries. `model.list_objects` pages by id with an `after` cursor, `history.state` by position with `start`; both return `next`, take 100 items by default and `MAX_PAGE_SIZE` (1000) at most. History entries name the method that ran, such as `model.create`. |
| `georef.list_crs`, `georef.get`, `georef.convert` | Read-only georeferencing queries: the CRSs a project can use (current, projected by default), the project's georeference with its derived true north, convergence and scale, and conversion of up to `MAX_PAGE_SIZE` points between local, projected and geographic coordinates (radians and metres). |
| `georef.set`, `georef.clear` | Set or remove the georeference on the project's `Site`. Each runs as one `model.batch`, so it is one undo step and the history names it `model.batch`; `georef.clear` on a project that is not georeferenced changes nothing and records nothing. |
| `materials.library`, `materials.grade` | Read-only queries on the library of European grades: a page of grades by search text and category (`total` is the number of matches), and one grade with its law and its sourced values. A grade is known by a stable key such as `en1992-1-1/C25/30`. |
| `materials.list`, `materials.get` | Read-only queries on the project's materials: the list by name, and one material with every property its category gives (value, source, library value to reset to, range, whether required), its law and how many objects use it. |
| `materials.add_from_library`, `materials.create`, `materials.update`, `materials.delete` | Add a material with the values of a grade, create a custom one (every value entered by the user), rename it and edit or reset its values (`null` resets), or delete one that nothing uses. Each runs as one undo step; the new object's id comes from the caller, as for `model.create`. |
| `buildups.list`, `buildups.get`, `buildups.preview` | Read-only queries on the wall and slab build-ups: the list by usage, one build-up with its layers, their priorities and thermal resistance, and a draft checked without saving, which returns its totals and every error at once. |
| `buildups.create`, `buildups.update`, `buildups.duplicate`, `buildups.delete` | Create a build-up with its layers, replace its layers (those named keep their identity), copy it, or delete it with its layers. Each runs as one undo step; the new build-up's id comes from the caller, and the ids of new layers derive from it. |

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

## JSON-RPC messages

Clients send JSON-RPC 2.0 messages (API-03) to an `api::jsonrpc::Server`, which wraps a `Session`. The method is the command or query name, the params its parameters (an empty object when left out). The transport passes the client's `Origin` with each message; a message cannot claim one.

- **Limits.** A message is measured before it is parsed: more than `Limits::max_bytes` (1 MiB), or arrays and objects nested deeper than `max_depth` (64), is refused with an Invalid Request error.
- **Batches** are supported, up to `max_batch` (100) requests. Each request runs on its own, in order, and each command is its own undo step; use `model.batch` for one atomic step. An empty or oversized batch gets a single Invalid Request error.
- **Notifications.** A request without an `id` gets no response, even when it fails. After each command that changes the model, the server emits a `model.changed` notification whose params are the `ChangeEvent`.

| Code | Meaning |
| --- | --- |
| -32700 | Not JSON. |
| -32600 | Not a request, or too large, too deep, an empty or oversized batch. |
| -32601 | Unknown method. |
| -32602 | Invalid params; `data.detail` says what the decoder rejected. |
| -32603 | Internal error. |
| -32000 | The model rejected the command. |
| -32001, -32002 | Nothing to undo, nothing to redo. |
| -32003 | No object with this id. |

A georeference call that the model refuses (-32000) adds `data.reason`, a stable `snake_case` string the UI maps to a message: `unknown_crs`, `not_projected`, `invalid_scale`, `outside_domain` (with `data.index`, the first bad point of a conversion), `not_georeferenced`, `multiple_sites` and `missing_site_id`. `georef.set` takes the id of the `Site` to create, like `model.create` takes its id; it is required only when the project has no `Site`. `georef.get`, `georef.convert` and `georef.set` accept a deprecated CRS such as 25838; `georef.list_crs` leaves it out.

A materials call that the model refuses (-32000) adds `data.reason`: `unknown_grade`, `not_found`, `duplicate_name`, `invalid_name`, `unknown_property` (with `data.key`), `invalid_value` (with `data.key` when one property is at fault), `required_property` (with `data.key`) and `in_use` (with `data.used_by`). `materials.get`, `update` and `delete` of an object that is not a material answer `not_found` this way (-32000), not -32003. A generic `model.delete` of an object that other objects still refer to through a required reference is refused the same way, with `data.reason` `in_use` and `data.count`. The values of a material (ADR 0019) are checked together, so `fy` and `fu` can be raised in one `materials.update`; a refused call changes nothing and a call that changes nothing records nothing.

A build-up call that the model refuses (-32000) adds `data.reason` and, when a layer is at fault, `data.index` (from 0): `not_found`, `invalid_name`, `duplicate_name`, `no_layers`, `too_many_layers`, `invalid_thickness`, `invalid_priority`, `duplicate_layer`, `core_not_contiguous`, `too_thick`, `unknown_material`, `invalid_material` and `unknown_layer`. `buildups.preview` lists the same codes, all of them, in `errors`.

Error messages never hold model content (SEC-03). The `jsonrpc` fuzz target (`fuzz/`) feeds arbitrary bytes to the server.

## Transports

A transport carries messages between a client and a server (API-04). The client's end implements `api::Transport`: `send` takes one message and returns the response, or `None` for notifications, and `notifications` takes the `model.changed` notifications received since the last call. Calls are blocking and carry text, so the trait builds for `wasm32-unknown-unknown` too; an asynchronous transport adapts to it in its tests.

`api::InProcess` is the transport to a server in the same process, for tests, the browser build and the desktop host. Each transport sets the `Origin` of its client. The desktop window reaches its document over Tauri's IPC, which passes the same suite: see [desktop.md](desktop.md). In the UI, `ui/src/api/` holds the typed client; its types are generated from `schema/api/protocol.schema.json` by `bun scripts/generate-api-types.ts`, so regenerate them, and commit `ui/src/api/protocol.gen.ts`, whenever the schema changes (`scripts/check-all.sh repo` fails until then).

### The conformance suite

Every transport must pass `api::conformance::run`, behind the `conformance` feature. It covers commands and their notifications, queries, every error class, the size and depth limits, undo and redo, batches and notification-only requests. Each case runs on a fresh server on `conformance::namespace()`, which the suite hands to the function that connects a client:

```rust
// In the transport's tests, with `api = { workspace = true, features = ["conformance"] }`.
assert_eq!(api::conformance::run(|server| my_transport::connect(server)), Ok(()));
```

`run` returns the failed cases, each with its reason, rather than panicking at the first one.
